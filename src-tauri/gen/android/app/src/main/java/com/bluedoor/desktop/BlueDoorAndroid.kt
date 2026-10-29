package com.bluedoor.desktop

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Matrix
import android.media.ExifInterface
import android.graphics.pdf.PdfRenderer
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.TextRecognizer
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import java.io.File
import java.io.IOException
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlin.math.max
import kotlin.math.min

/**
 * Android-only facilities used by the Rust pipeline. It never receives
 * document text from the WebView: Rust calls it while importing or exporting a
 * document that already belongs to the native session.
 */
object BlueDoorAndroid {
  private const val MAX_FILE_BYTES = 10L * 1024 * 1024
  private const val MAX_TEXT_BYTES = 60 * 1024
  private const val MAX_PAGES = 30
  private const val MAX_PIXELS = 2000

  private val importCancelled = java.util.concurrent.atomic.AtomicBoolean(false)
  @Volatile var sourceUri: Uri? = null
  @Volatile private var appContext: Context? = null

  fun init(context: Context) {
    appContext = context.applicationContext
  }

  private fun context(): Context =
    appContext ?: throw IllegalStateException("BlueDoor aún no se ha inicializado.")

  /** Copies a system picker URI into the private cache and returns its path. */
  @JvmStatic
  fun copyUriToCache(uriString: String): String = copyUri(uriString, MAX_FILE_BYTES)

  @JvmStatic
  fun copyVaultUriToCache(uriString: String): String = copyUri(uriString, 4L * 1024 * 1024 * 1024)

  private fun copyUri(uriString: String, maxBytes: Long): String {
    importCancelled.set(false)
    val context = context()
    val uri = Uri.parse(uriString)
    val name = safeName(displayName(context, uri), context.contentResolver.getType(uri))
    val directory = File(context.cacheDir, "imports").apply { mkdirs() }
    val destination = File(directory, "${UUID.randomUUID()}-$name")
    try {
      val input = context.contentResolver.openInputStream(uri)
        ?: throw IOException("No se pudo abrir el documento.")
      input.use { source ->
        destination.outputStream().use { output ->
          val buffer = ByteArray(128 * 1024)
          var total = 0L
          while (true) {
            checkImport()
            val read = source.read(buffer)
            if (read < 0) break
            total += read
            if (total > maxBytes) {
              throw IOException(if (maxBytes == MAX_FILE_BYTES) "Selecciona un documento de hasta 10 MB." else "La copia del Vault supera 4 GB.")
            }
            output.write(buffer, 0, read)
          }
        }
      }
      return destination.absolutePath
    } catch (error: Exception) {
      destination.delete()
      throw error
    }
  }

  /** Runs ML Kit text recognition locally on an image or a PDF. */
  @JvmStatic
  fun ocr(path: String): String {
    importCancelled.set(false)
    val file = File(path)
    if (!file.isFile) throw IOException("No se pudo abrir el documento.")
    val recognizer = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
    try {
      val text =
        if (file.name.lowercase().endsWith(".pdf")) ocrPdf(file, recognizer)
        else {
          val bitmap = decodeImage(file)
          try { ocrBitmap(bitmap, recognizer) } finally { bitmap.recycle() }
        }
      return text.trim()
    } finally {
      recognizer.close()
    }
  }

  private fun checkImport() { if (importCancelled.get()) throw IOException("Extracción cancelada; no se ha completado la protección.") }
  @JvmStatic
  fun cancelImport(ignored: String) { importCancelled.set(true) }

  @JvmStatic
  fun rememberSource(value: String) { sourceUri = if (value.isEmpty()) null else Uri.parse(value) }

  @JvmStatic
  fun writeDocument(payload: String) {
    val data = org.json.JSONObject(payload)
    val path = File(data.getString("path"))
    val uri = Uri.parse(data.getString("uri"))
    val output = context().contentResolver.openOutputStream(uri, "wt") ?: throw IOException("No se pudo abrir el destino elegido.")
    output.use { target -> path.inputStream().use { it.copyTo(target) } }
  }

  @JvmStatic
  fun previewSource(ignored: String) {
    if (sourceUri == null) throw IOException("Vuelve a seleccionar el documento para ver la fuente.")
    context().startActivity(Intent(context(), SourcePreviewActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
  }

  /** Opens the system share sheet with the reviewed PDF. */
  @JvmStatic
  fun sharePdf(path: String) {
    val context = context()
    val file = File(path)
    if (!file.isFile) throw IOException("No se pudo preparar el PDF para compartir.")
    val uri =
      FileProvider.getUriForFile(
        context,
        "${context.packageName}.fileprovider",
        file,
      )
    val send =
      Intent(Intent.ACTION_SEND).apply {
        type = "application/pdf"
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
    val chooser =
      Intent.createChooser(send, "Compartir documento protegido").apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
    context.startActivity(chooser)
  }

  private fun ocrPdf(file: File, recognizer: TextRecognizer): String {
    val descriptor =
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    descriptor.use { fd ->
      PdfRenderer(fd).use { renderer ->
        val builder = StringBuilder()
        if (renderer.pageCount > MAX_PAGES) throw IOException("El PDF supera 30 páginas; divídelo. No se analizará parcialmente.")
        val pages = renderer.pageCount
        for (index in 0 until pages) {
          checkImport()
          renderer.openPage(index).use { page ->
            val scale = min(3.0, MAX_PIXELS.toDouble() / max(page.width, page.height))
            val width = max(1, (page.width * scale).toInt())
            val height = max(1, (page.height * scale).toInt())
            val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            try {
              bitmap.eraseColor(Color.WHITE)
              page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
              builder.append(ocrBitmap(bitmap, recognizer)).append("\n\n")
            } finally {
              bitmap.recycle()
            }
          }
          if (builder.toString().toByteArray(Charsets.UTF_8).size >= MAX_TEXT_BYTES) throw IOException("El OCR supera el límite de texto; divide el documento. No se truncará.")
        }
        return builder.toString()
      }
    }
  }

  private fun decodeImage(file: File): Bitmap {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.absolutePath, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
      throw IOException("No se pudo leer la imagen del documento.")
    }
    var sample = 1
    while (max(bounds.outWidth, bounds.outHeight) / sample > MAX_PIXELS) sample *= 2
    val options =
      BitmapFactory.Options().apply {
        inSampleSize = sample
        inPreferredConfig = Bitmap.Config.ARGB_8888
      }
    val bitmap = BitmapFactory.decodeFile(file.absolutePath, options)
      ?: throw IOException("No se pudo leer la imagen del documento.")
    val orientation = runCatching { ExifInterface(file.absolutePath).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }.getOrDefault(ExifInterface.ORIENTATION_NORMAL)
    return orientBitmap(bitmap, orientation)
  }

  /** Applies every EXIF orientation, including mirrored camera images. */
  fun orientBitmap(bitmap: Bitmap, orientation: Int): Bitmap {
    val matrix = Matrix()
    when (orientation) {
      ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> matrix.setScale(-1f, 1f)
      ExifInterface.ORIENTATION_ROTATE_180 -> matrix.setRotate(180f)
      ExifInterface.ORIENTATION_FLIP_VERTICAL -> matrix.setScale(1f, -1f)
      ExifInterface.ORIENTATION_TRANSPOSE -> { matrix.setRotate(90f); matrix.postScale(-1f, 1f) }
      ExifInterface.ORIENTATION_ROTATE_90 -> matrix.setRotate(90f)
      ExifInterface.ORIENTATION_TRANSVERSE -> { matrix.setRotate(270f); matrix.postScale(-1f, 1f) }
      ExifInterface.ORIENTATION_ROTATE_270 -> matrix.setRotate(270f)
      else -> return bitmap
    }
    val oriented = Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true)
    if (oriented !== bitmap) bitmap.recycle()
    return oriented
  }

  private fun ocrBitmap(bitmap: Bitmap, recognizer: TextRecognizer): String {
    checkImport()
    val image = InputImage.fromBitmap(bitmap, 0)
    val result = Tasks.await(recognizer.process(image), 60, TimeUnit.SECONDS)
    checkImport()
    return result.text
  }

  private fun displayName(context: Context, uri: Uri): String? {
    if (uri.scheme == "content") {
      context.contentResolver
        .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        ?.use { cursor ->
          if (cursor.moveToFirst()) return cursor.getString(0)
        }
    }
    return uri.lastPathSegment
  }

  private fun safeName(name: String?, mime: String?): String {
    val fallback = "documento"
    var value =
      (name ?: fallback)
        .replace(Regex("[\\\\/:*?\"<>|\\u0000-\\u001f]"), "_")
        .trim()
        .take(80)
        .ifEmpty { fallback }
    if (!value.contains('.')) {
      value += extensionForMime(mime)
    }
    return value
  }

  private fun extensionForMime(mime: String?): String =
    when (mime) {
      "application/pdf" -> ".pdf"
      "text/plain", "text/markdown" -> ".txt"
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document" -> ".docx"
      "image/png" -> ".png"
      "image/jpeg" -> ".jpg"
      "image/tiff" -> ".tiff"
      "image/heic", "image/heif" -> ".heic"
      else -> ".bin"
    }
}
