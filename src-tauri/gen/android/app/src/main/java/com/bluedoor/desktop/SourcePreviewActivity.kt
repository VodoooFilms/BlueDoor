package com.bluedoor.desktop

import android.app.Activity
import android.os.Bundle
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.media.ExifInterface
import android.graphics.pdf.PdfRenderer
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.max
import kotlin.math.min

/** In-app source viewer. No original is sent to an external application. */
class SourcePreviewActivity : Activity() {
  private var renderer: PdfRenderer? = null
  private var bitmap: Bitmap? = null
  private var page = 0
  private lateinit var image: SourceImageView
  private lateinit var status: TextView
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    page = savedInstanceState?.getInt("page") ?: 0
    val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setBackgroundColor(Color.WHITE) }
    fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
    root.setPadding(dp(16), dp(16), dp(16), dp(16))
    val controls = LinearLayout(this)
    image = SourceImageView(this)
    status = TextView(this).apply { setTextColor(Color.BLACK) }
    fun action(symbol: String, label: String, run: () -> Unit) {
      controls.addView(Button(this).apply {
        text = symbol; textSize = 24f; contentDescription = label
        if (android.os.Build.VERSION.SDK_INT >= 26) tooltipText = label
        setOnClickListener { run() }
      }, LinearLayout.LayoutParams(0, dp(48), 1f).apply { marginEnd = dp(8) })
    }
    action("×", "Cerrar fuente") { finish() }
    action("‹", "Página anterior") { if (page > 0) { page--; render() } }
    action("›", "Página siguiente") { if (page + 1 < (renderer?.pageCount ?: 1)) { page++; render() } }
    status.setPadding(0, dp(12), 0, dp(16))
    root.addView(controls); root.addView(status)
    root.addView(image, LinearLayout.LayoutParams(-1, 0, 1f))
    setContentView(root)
    try {
      val uri = BlueDoorAndroid.sourceUri ?: throw IllegalStateException("Fuente no disponible; importa otra vez.")
      if (contentResolver.getType(uri) == "application/pdf") {
        val descriptor = contentResolver.openFileDescriptor(uri, "r") ?: throw IllegalStateException("No se pudo abrir la fuente.")
        renderer = PdfRenderer(descriptor)
        render()
      } else {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, bounds) }
        var sample = 1
        while (max(bounds.outWidth, bounds.outHeight) / sample > 2000) sample *= 2
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        bitmap = contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, options) }
        if (bitmap == null) throw IllegalStateException("La vista fuente admite PDF e imágenes. Para DOCX/TXT revisa el texto extraído.")
        val orientation = runCatching { contentResolver.openInputStream(uri).use { input -> ExifInterface(input!!).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) } }.getOrDefault(ExifInterface.ORIENTATION_NORMAL)
        bitmap = BlueDoorAndroid.orientBitmap(bitmap!!, orientation)
        image.setImageBitmap(bitmap)
        status.text = "Fuente original · vuelve para revisar el texto extraído"
      }
    } catch (error: Exception) { status.text = error.message ?: "No se pudo mostrar la fuente." }
  }
  private fun render() {
    try {
      val pdf = renderer ?: return
      image.setImageDrawable(null); bitmap?.recycle()
      pdf.openPage(page).use { source ->
        val scale = min(3.0, 2000.0 / max(source.width, source.height))
        bitmap = Bitmap.createBitmap(max(1, (source.width * scale).toInt()), max(1, (source.height * scale).toInt()), Bitmap.Config.ARGB_8888)
        bitmap!!.eraseColor(Color.WHITE)
        source.render(bitmap!!, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
      }
      image.setImageBitmap(bitmap); status.text = "Fuente original · página ${page + 1} de ${pdf.pageCount}"
    } catch (error: Exception) { status.text = "No se pudo mostrar la página." }
  }
  override fun onSaveInstanceState(outState: Bundle) { outState.putInt("page", page); super.onSaveInstanceState(outState) }
  override fun onDestroy() { image.setImageDrawable(null); bitmap?.recycle(); renderer?.close(); super.onDestroy() }
}
