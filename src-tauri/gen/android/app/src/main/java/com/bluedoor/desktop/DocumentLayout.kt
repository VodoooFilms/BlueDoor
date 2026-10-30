package com.bluedoor.desktop

import android.graphics.*
import android.graphics.pdf.PdfDocument
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.text.*
import android.text.style.*
import android.util.Base64
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.math.*

/** Native immutable page rasters. Protected pages are flattened after erasure;
 * the source pixels and OCR text are never embedded in the exported PDF. */
object DocumentLayout {
  private const val MAX_PAGES = 60
  // A4 in PostScript points. Images carry no page geometry, so they are fitted
  // to A4 preserving their aspect ratio instead of using pixels as points.
  private const val A4_WIDTH = 595.28
  private const val A4_HEIGHT = 841.89
  private fun encoded(bitmap: Bitmap): String = ByteArrayOutputStream().use {
    bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
    Base64.encodeToString(it.toByteArray(), Base64.NO_WRAP)
  }
  private fun rect(box: Rect, scale: Double, scaleY: Double = scale) = JSONArray(listOf(box.left / scale, box.top / scaleY, box.width() / scale, box.height() / scaleY))
  private fun checked(action: () -> String): String = try { action() } catch (e: Exception) {
    "ERROR:${e.message ?: "No se pudo conservar el formato del documento."}"
  }
  fun extract(path: String): String = checked {
    val file = File(path)
    if (!file.isFile || file.length() > 10L * 1024 * 1024) throw IOException("Selecciona un documento de hasta 10 MB.")
    val recognizer = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
    val scanner = BarcodeScanning.getClient()
    val text = StringBuilder()
    var bytes = 0
    var imageBytes = 0
    var lineNumber = 0
    val pages = JSONArray()
    fun append(value: String) { text.append(value); bytes += value.toByteArray(Charsets.UTF_8).size }
    fun page(bitmap: Bitmap, width: Double, height: Double) {
      BlueDoorAndroid.checkImport()
      val scale = bitmap.width / width
      val scaleY = bitmap.height / height
      val input = InputImage.fromBitmap(bitmap, 0)
      val result = Tasks.await(recognizer.process(input), 60, TimeUnit.SECONDS)
      val glyphs = JSONArray()
      for (block in result.textBlocks) for (line in block.lines) {
        for (element in line.elements) {
          val box = element.boundingBox ?: throw IOException("Texto sin coordenadas; no se puede proteger parcialmente.")
          val clipped = Rect(max(0, box.left), max(0, box.top), min(bitmap.width, box.right), min(bitmap.height, box.bottom))
          if (clipped.isEmpty) throw IOException("Texto fuera de la página.")
          val start = bytes
          append(element.text)
          glyphs.put(JSONObject().put("start", start).put("end", bytes).put("line", lineNumber)
            .put("rect", rect(clipped, scale, scaleY)).put("fontSize", max(1.0, clipped.height() / scaleY * 0.8))
            .put("fontName", "sans-serif").put("color", JSONArray(listOf(0, 0, 0))).put("confidence", 1.0))
          append(" ")
        }
        append("\n"); lineNumber++
      }
      append("\n")
      if (bytes > 60 * 1024) throw IOException("El OCR supera el límite de texto; divide el documento.")
      val qr = JSONArray()
      for (code in Tasks.await(scanner.process(input), 60, TimeUnit.SECONDS)) {
        val box = code.boundingBox ?: throw IOException("Código sin coordenadas; revisión bloqueada.")
        val padded = Rect(max(0, box.left - 3), max(0, box.top - 3), min(bitmap.width, box.right + 3), min(bitmap.height, box.bottom + 3))
        qr.put(rect(padded, scale, scaleY))
      }
      val image = encoded(bitmap)
      imageBytes += image.length
      if (imageBytes > 80 * 1024 * 1024) throw IOException("El documento supera el límite de vista local; divídelo.")
      pages.put(JSONObject().put("width", width).put("height", height).put("imageBase64", image)
        .put("glyphs", glyphs).put("source", "ocr").put("qrRegions", qr))
      BlueDoorAndroid.checkImport()
    }
    try {
      if (file.extension.lowercase() == "pdf") {
        ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
          PdfRenderer(fd).use { renderer ->
            if (renderer.pageCount !in 1..MAX_PAGES) throw IOException("El PDF supera $MAX_PAGES páginas; usa la importación como texto o divídelo.")
            for (i in 0 until renderer.pageCount) renderer.openPage(i).use { source ->
              val scale = min(3.0, 2000.0 / max(source.width, source.height))
              val bitmap = Bitmap.createBitmap(max(1, (source.width * scale).toInt()), max(1, (source.height * scale).toInt()), Bitmap.Config.ARGB_8888)
              try {
                bitmap.eraseColor(Color.WHITE)
                source.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                page(bitmap, source.width.toDouble(), source.height.toDouble())
              } finally { bitmap.recycle() }
            }
          }
        }
      } else {
        val bitmap = BlueDoorAndroid.decodeImage(file)
        val (pageWidth, pageHeight) = pageBox(bitmap.width, bitmap.height)
        try { page(bitmap, pageWidth, pageHeight) } finally { bitmap.recycle() }
      }
      JSONObject().put("text", text.toString()).put("pages", pages).toString()
    } finally { recognizer.close(); scanner.close() }
  }

  private fun box(array: JSONArray) = RectF(array.getDouble(0).toFloat(), array.getDouble(1).toFloat(),
    (array.getDouble(0) + array.getDouble(2)).toFloat(), (array.getDouble(1) + array.getDouble(3)).toFloat())
  private fun pageBox(width: Int, height: Int): Pair<Double, Double> {
    val aspect = width.toDouble() / height.toDouble()
    return if (aspect <= 1.0) Pair(A4_HEIGHT * aspect, A4_HEIGHT) else Pair(A4_WIDTH, A4_WIDTH / aspect)
  }
  private fun paint(area: JSONObject) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    textSize = area.getDouble("fontSize").toFloat()
    typeface = Typeface.create(area.optString("fontName", "sans-serif"), Typeface.NORMAL)
    val rgb = area.getJSONArray("color")
    color = Color.rgb((rgb.getDouble(0) * 255).toInt(), (rgb.getDouble(1) * 255).toInt(), (rgb.getDouble(2) * 255).toInt())
  }
  private fun fits(area: JSONObject): Boolean {
    if (area.getString("alias").isEmpty()) return true
    val p = paint(area); val r = box(area.getJSONArray("rect"))
    return p.measureText(area.getString("alias")) <= r.width() && p.descent() - p.ascent() <= r.height()
  }
  fun checkAliases(payload: String): String = checked {
    val areas = JSONArray(payload); val failed = mutableSetOf<Int>()
    for (i in 0 until areas.length()) { val a = areas.getJSONObject(i); if (!fits(a)) failed.add(a.getInt("entityIndex")) }
    JSONArray(failed.toList()).toString()
  }
  private fun eraseParagraph(bitmap: Bitmap, canvas: Canvas, overlay: JSONObject, sx: Float, sy: Float) {
    val erased = box(overlay.getJSONArray("eraseRect")); val target = box(overlay.getJSONArray("rect"))
    val regions = overlay.getJSONArray("eraseRegions")
    if (regions.length() == 0) throw IOException("Texto original sin coordenadas.")
    val allowed = (0 until regions.length()).map { box(regions.getJSONArray(it)).apply { inset(-0.7f, -0.7f) } }
    val background = bitmap.getPixel(((erased.left - 2) * sx).toInt().coerceIn(0, bitmap.width - 1), ((erased.top - 2) * sy).toInt().coerceIn(0, bitmap.height - 1))
    val union = RectF(erased).apply { union(target) }
    val left = floor(union.left * sx).toInt().coerceAtLeast(0)
    val top = floor(union.top * sy).toInt().coerceAtLeast(0)
    val right = ceil(union.right * sx).toInt().coerceAtMost(bitmap.width)
    val bottom = ceil(union.bottom * sy).toInt().coerceAtMost(bitmap.height)
    for (y in top until bottom) for (x in left until right) {
      val px = (x + .5f) / sx; val py = (y + .5f) / sy
      if (!erased.contains(px, py) && !target.contains(px, py)) continue
      if (allowed.any { it.contains(px, py) }) continue
      val color = bitmap.getPixel(x, y)
      if (abs(Color.red(color) - Color.red(background)) > 8 || abs(Color.green(color) - Color.green(background)) > 8 || abs(Color.blue(color) - Color.blue(background)) > 8)
        throw IOException("El área contiene una imagen, una línea o contenido existente. Ajusta los bordes para conservarlo.")
    }
    // OCR bounds can miss antialiased edge pixels. Erase an opaque,
    // padded paragraph rather than reproducing a sampled paper colour.
    erased.inset(-1.5f, -1.5f)
    canvas.drawRect(erased, Paint().apply { color = Color.WHITE })
    verifyErased(bitmap, erased, sx, sy)
  }
  private fun verifyErased(bitmap: Bitmap, bounds: RectF, sx: Float, sy: Float) {
    // Check before drawing replacement text: every covered pixel must be white.
    val left = ceil(bounds.left * sx).toInt().coerceIn(0, bitmap.width)
    val top = ceil(bounds.top * sy).toInt().coerceIn(0, bitmap.height)
    val right = floor(bounds.right * sx).toInt().coerceIn(0, bitmap.width)
    val bottom = floor(bounds.bottom * sy).toInt().coerceIn(0, bitmap.height)
    for (y in top until bottom) for (x in left until right) {
      if (bitmap.getPixel(x, y) != Color.WHITE)
        throw IOException("No se pudo comprobar el borrado; exportación bloqueada.")
    }
  }
  private fun rich(canvas: Canvas, overlay: JSONObject) {
    val bounds = box(overlay.getJSONArray("rect")); val paragraphs = overlay.getJSONArray("paragraphs")
    var y = bounds.top
    for (i in 0 until paragraphs.length()) {
      val paragraph = paragraphs.getJSONObject(i); val runs = paragraph.getJSONArray("runs")
      val text = SpannableStringBuilder()
      for (j in 0 until runs.length()) {
        val run = runs.getJSONObject(j); val start = text.length
        text.append(run.getString("text")); val end = text.length
        val flags = android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE
        fun span(value: Any) { if (end > start) text.setSpan(value, start, end, flags) }
        span(TypefaceSpan(run.getString("font")))
        span(AbsoluteSizeSpan(max(1, run.getDouble("size").roundToInt())))
        span(StyleSpan((if (run.getBoolean("bold")) Typeface.BOLD else 0) or (if (run.getBoolean("italic")) Typeface.ITALIC else 0)))
        if (run.getBoolean("underline")) span(UnderlineSpan())
        val rgb = run.getJSONArray("color")
        span(ForegroundColorSpan(Color.rgb((rgb.getDouble(0)*255).toInt(), (rgb.getDouble(1)*255).toInt(), (rgb.getDouble(2)*255).toInt())))
      }
      val left = paragraph.getDouble("left").toFloat(); val right = paragraph.getDouble("right").toFloat()
      val width = floor(bounds.width() - left - right).toInt()
      if (width <= 0) throw IOException("El texto no cabe en el área elegida.")
      val indent = paragraph.getDouble("first").roundToInt()
      if (text.isNotEmpty()) text.setSpan(LeadingMarginSpan.Standard(indent, 0), 0, text.length, android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
      val alignment = when (paragraph.getString("align")) { "center" -> Layout.Alignment.ALIGN_CENTER; "right" -> Layout.Alignment.ALIGN_OPPOSITE; else -> Layout.Alignment.ALIGN_NORMAL }
      val layout = StaticLayout.Builder.obtain(text, 0, text.length, TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12f }, width)
        .setAlignment(alignment).setIncludePad(false).build()
      if (y + layout.height > bounds.bottom + 0.1f) throw IOException("El texto no cabe; amplía el área antes de guardar.")
      // StaticLayout wraps words, but a single long token can still exceed the box.
      for (line in 0 until layout.lineCount) if (layout.getLineWidth(line) > width + 0.1f) throw IOException("El texto supera el ancho del área.")
      canvas.save(); canvas.translate(bounds.left + left, y); layout.draw(canvas); canvas.restore()
      y += layout.height
    }
  }
  fun render(payload: String): String = checked {
    val plan = JSONObject(payload); val pages = plan.getJSONArray("pages")
    val areas = plan.getJSONArray("redactions"); val overlays = plan.getJSONArray("overlays")
    val export = plan.getString("mode") == "--export"
    val pdf = if (export) PdfDocument() else null
    val result = JSONArray()
    try {
      for (i in 0 until pages.length()) {
        val page = pages.getJSONObject(i)
        val bytes = Base64.decode(page.getString("imageBase64"), Base64.DEFAULT)
        val original = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: throw IOException("Página no válida.")
        val bitmap = try { original.copy(Bitmap.Config.ARGB_8888, true) ?: throw IOException("Memoria insuficiente para mostrar la página.") } finally { original.recycle() }
        try {
          val canvas = Canvas(bitmap); val width = page.getDouble("width"); val height = page.getDouble("height")
          canvas.scale((bitmap.width / width).toFloat(), (bitmap.height / height).toFloat())
          val white = Paint().apply { color = Color.WHITE }
          val qr = page.getJSONArray("qrRegions")
          for (q in 0 until qr.length()) canvas.drawRect(box(qr.getJSONArray(q)), white)
          val pageAreas = (0 until areas.length())
            .map { areas.getJSONObject(it) }
            .filter { it.getInt("page") == i }
            .sortedWith(compareBy({ box(it.getJSONArray("rect")).top }, { box(it.getJSONArray("rect")).left }))
          // Erase every area first. Painting a white rectangle after its
          // neighbour's alias was written would cut that alias in half.
          for (area in pageAreas) {
            val erased = box(area.getJSONArray("rect")).apply { inset(-1.5f, -1.5f) }
            canvas.drawRect(erased, white)
            verifyErased(bitmap, erased, (bitmap.width / width).toFloat(), (bitmap.height / height).toFloat())
          }
          var lastAliasRight = 0f
          var lastAliasTop = -1f
          for (area in pageAreas) {
            if (!fits(area)) {
              if (export) throw IOException("Un alias no cabe; acórtalo antes de exportar.")
              continue
            }
            val r = box(area.getJSONArray("rect"))
            val p = paint(area)
            val sameLine = lastAliasTop >= 0f && abs(r.top - lastAliasTop) < r.height() * 0.6f
            // Keep a visible gap between adjacent aliases so two values never
            // read as a single token ("PERSON_02ID_01").
            val left = if (sameLine && lastAliasRight + 1.5f > r.left) lastAliasRight + 1.5f else r.left
            if (left + p.measureText(area.getString("alias")) > r.right + 0.01f)
              throw IOException("Los alias contiguos no caben separados; acórtalos antes de exportar.")
            canvas.drawText(area.getString("alias"), left, r.top + (r.height() - p.descent() - p.ascent()) / 2, p)
            lastAliasRight = left + p.measureText(area.getString("alias"))
            lastAliasTop = r.top
          }
          for (o in 0 until overlays.length()) {
            val overlay = overlays.getJSONObject(o); if (overlay.getInt("page") != i) continue
            eraseParagraph(bitmap, canvas, overlay, (bitmap.width / width).toFloat(), (bitmap.height / height).toFloat())
            rich(canvas, overlay)
          }
          // Manual covers are the last pixel operation, after text edits and aliases.
          // They also work when OCR found no text in the selected rectangle.
          val manual = page.optJSONArray("manualRegions") ?: JSONArray()
          for (m in 0 until manual.length()) {
            val bounds = box(manual.getJSONArray(m)).apply { inset(-1.5f, -1.5f) }
            canvas.drawRect(bounds, white)
            verifyErased(bitmap, bounds, (bitmap.width / width).toFloat(), (bitmap.height / height).toFloat())
          }
          if (pdf != null) {
            val output = pdf.startPage(PdfDocument.PageInfo.Builder(max(1, width.roundToInt()), max(1, height.roundToInt()), i + 1).create())
            output.canvas.drawBitmap(bitmap, null, RectF(0f, 0f, width.toFloat(), height.toFloat()), Paint(Paint.FILTER_BITMAP_FLAG))
            pdf.finishPage(output)
          } else result.put(JSONObject().put("width", width).put("height", height).put("imageBase64", encoded(bitmap)).put("glyphs", JSONArray()).put("source", "ocr").put("qrRegions", JSONArray()).put("manualRegions", manual))
        } finally { bitmap.recycle() }
      }
      if (pdf != null) ByteArrayOutputStream().use { pdf.writeTo(it); Base64.encodeToString(it.toByteArray(), Base64.NO_WRAP) } else result.toString()
    } finally { pdf?.close() }
  }
}
