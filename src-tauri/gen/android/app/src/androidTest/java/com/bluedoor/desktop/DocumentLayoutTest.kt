package com.bluedoor.desktop

import android.graphics.*
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.util.Base64
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.File

@RunWith(AndroidJUnit4::class)
class DocumentLayoutTest {
  private fun fixture(): JSONObject {
    val bitmap = Bitmap.createBitmap(400, 600, Bitmap.Config.ARGB_8888)
    bitmap.eraseColor(Color.WHITE)
    val canvas = Canvas(bitmap)
    canvas.drawRect(20f, 20f, 60f, 60f, Paint().apply { color = Color.BLUE })
    canvas.drawText("Ana Perez", 100f, 150f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.BLACK; textSize = 20f })
    val image = ByteArrayOutputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it); Base64.encodeToString(it.toByteArray(), Base64.NO_WRAP) }
    bitmap.recycle()
    val page = JSONObject().put("width", 400).put("height", 600).put("imageBase64", image).put("qrRegions", JSONArray()).put("glyphs", JSONArray()).put("source", "ocr")
    val area = JSONObject().put("page", 0).put("entityIndex", 0).put("rect", JSONArray(listOf(95, 125, 120, 35))).put("alias", "P01").put("fontSize", 16).put("fontName", "sans-serif").put("color", JSONArray(listOf(0,0,0)))
    return JSONObject().put("mode", "--preview").put("pages", JSONArray().put(page).put(JSONObject(page.toString())))
      .put("redactions", JSONArray().put(area)).put("overlays", JSONArray())
  }
  private fun bitmap(page: JSONObject): Bitmap {
    val bytes = Base64.decode(page.getString("imageBase64"), Base64.DEFAULT)
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
  }
  @Test fun protectedPreviewPreservesPagesAndExportFlattensOnlyChangedPixels() {
    val plan = fixture(); val preview = JSONArray(DocumentLayout.render(plan.toString()))
    assertEquals(2, preview.length())
    val original = bitmap(plan.getJSONArray("pages").getJSONObject(0))
    val protected = bitmap(preview.getJSONObject(0)); val unchanged = bitmap(preview.getJSONObject(1))
    assertEquals(400, protected.width); assertEquals(600, protected.height)
    assertEquals(Color.BLUE, protected.getPixel(30, 30))
    assertTrue(original.sameAs(unchanged)); assertFalse(original.sameAs(protected))
    plan.put("mode", "--export")
    val bytes = Base64.decode(DocumentLayout.render(plan.toString()), Base64.DEFAULT)
    assertEquals("%PDF-", String(bytes.take(5).toByteArray()))
    val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "layout-test.pdf")
    try {
      file.writeBytes(bytes)
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { fd -> PdfRenderer(fd).use { pdf ->
        assertEquals(2, pdf.pageCount)
        pdf.openPage(0).use { page -> assertEquals(400, page.width); assertEquals(600, page.height) }
      } }
    } finally { file.delete(); original.recycle(); protected.recycle(); unchanged.recycle() }
  }
  @Test fun opaqueErasureCoversOcrEdgePixelsOnTintedPaper() {
    val plan = fixture()
    val page = plan.getJSONArray("pages").getJSONObject(0)
    val original = bitmap(page)
    val source = try { original.copy(Bitmap.Config.ARGB_8888, true) } finally { original.recycle() }
    source.eraseColor(Color.rgb(245, 244, 240))
    // Simulate an OCR box that misses a dark edge by one point.
    Canvas(source).drawRect(94f, 130f, 110f, 150f, Paint().apply { color = Color.BLACK })
    page.put("imageBase64", ByteArrayOutputStream().use {
      source.compress(Bitmap.CompressFormat.PNG, 100, it)
      Base64.encodeToString(it.toByteArray(), Base64.NO_WRAP)
    })
    plan.getJSONArray("redactions").getJSONObject(0).put("alias", "")
    val output = bitmap(JSONArray(DocumentLayout.render(plan.toString())).getJSONObject(0))
    try {
      for (y in 126 until 159) for (x in 94 until 215)
        assertEquals("Original pixel at $x,$y", Color.WHITE, output.getPixel(x, y))
      assertEquals(Color.rgb(245, 244, 240), output.getPixel(90, 130))
    } finally { source.recycle(); output.recycle() }
  }
  @Test fun adjacentAliasesBlockInsteadOfOverflowingTheirErasedArea() {
    val plan = fixture(); val areas = plan.getJSONArray("redactions")
    val first = areas.getJSONObject(0)
    first.put("alias", "P01")
    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 16f; typeface = Typeface.create("sans-serif", Typeface.NORMAL) }
    val width = paint.measureText("P01")
    first.put("rect", JSONArray(listOf(95f, 125f, width, 35f)))
    areas.put(JSONObject(first.toString()).put("rect", JSONArray(listOf(95f + width, 125f, width, 35f))))
    plan.put("mode", "--export")
    assertTrue(DocumentLayout.render(plan.toString()).startsWith("ERROR:"))
  }
  @Test fun oversizedAliasesBlockExportAndAreReportedBeforeReview() {
    val plan = fixture()
    plan.getJSONArray("redactions").getJSONObject(0).put("alias", "PERSONA_DEMASIADO_LARGA")
    assertEquals(0, JSONArray(DocumentLayout.checkAliases(plan.getJSONArray("redactions").toString())).getInt(0))
    plan.put("mode", "--export")
    assertTrue(DocumentLayout.render(plan.toString()).startsWith("ERROR:"))
  }
  @Test fun richEditsStayOnPageAndRejectOverflowAndGraphics() {
    val plan = fixture()
    val run = JSONObject().put("text", "Texto corregido").put("font", "sans-serif").put("size", 16)
      .put("color", JSONArray(listOf(0, 0, 0))).put("bold", true).put("italic", false).put("underline", false)
    val paragraph = JSONObject().put("runs", JSONArray().put(run)).put("left", 0).put("right", 0).put("first", 0).put("align", "left")
    val rectangle = JSONArray(listOf(95, 125, 150, 40))
    val overlay = JSONObject().put("page", 0).put("eraseRect", rectangle).put("eraseRegions", JSONArray().put(rectangle))
      .put("rect", rectangle).put("paragraphs", JSONArray().put(paragraph))
    plan.put("redactions", JSONArray()).put("overlays", JSONArray().put(overlay))
    val preview = JSONArray(DocumentLayout.render(plan.toString()))
    val source = bitmap(plan.getJSONArray("pages").getJSONObject(0)); val edited = bitmap(preview.getJSONObject(0))
    val untouched = bitmap(preview.getJSONObject(1))
    try { assertFalse(source.sameAs(edited)); assertTrue(source.sameAs(untouched)); assertEquals(Color.BLUE, edited.getPixel(30,30)) }
    finally { source.recycle(); edited.recycle(); untouched.recycle() }
    run.put("text", "Texto demasiado largo ".repeat(100))
    assertTrue(DocumentLayout.render(plan.toString()).startsWith("ERROR:"))
    run.put("text", "Corto")
    overlay.put("rect", JSONArray(listOf(10, 10, 300, 200)))
    assertTrue(DocumentLayout.render(plan.toString()).startsWith("ERROR:"))
  }
  @Test fun manualCoverWithoutOcrErasesPreviewAndExportPixels() {
    val plan = fixture()
    // The blue square has no OCR text: manual coverage must still remove it.
    plan.getJSONArray("pages").getJSONObject(0).put("manualRegions", JSONArray().put(JSONArray(listOf(18, 18, 45, 45))))
    val preview = JSONArray(DocumentLayout.render(plan.toString()))
    val bitmap = bitmap(preview.getJSONObject(0))
    try { assertEquals(Color.WHITE, bitmap.getPixel(30, 30)) } finally { bitmap.recycle() }
    plan.put("mode", "--export")
    val bytes = Base64.decode(DocumentLayout.render(plan.toString()), Base64.DEFAULT)
    val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "manual-test.pdf")
    val output = Bitmap.createBitmap(400,600,Bitmap.Config.ARGB_8888)
    try {
      file.writeBytes(bytes)
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { fd -> PdfRenderer(fd).use { pdf ->
        pdf.openPage(0).use { page -> page.render(output,null,null,PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY) }
      } }
      assertEquals(Color.WHITE, output.getPixel(30,30))
    } finally { output.recycle(); file.delete() }
  }
  @Test fun multilingualPiiRunsOnAndroidAndCoversWrappedSurname() {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val path = InstrumentationRegistry.getArguments().getString("glinerMultiPath")
    org.junit.Assume.assumeNotNull(path)
    val payload = JSONObject(instrumentation.context.assets.open("gliner-multi-input.json").bufferedReader().use { it.readText() }).put("path",path)
    try {
      val result = JSONObject(GlinerRuntime.run(payload.toString()))
      File(instrumentation.targetContext.cacheDir,"qa-multi-output.json").writeText(result.toString())
      val shape = result.getJSONArray("shape")
      assertEquals(1,shape.getInt(0)); assertEquals(payload.getInt("words"),shape.getInt(1))
      assertEquals(12,shape.getInt(2)); assertEquals(8,shape.getInt(3))
      // Synthetic source: Nombre : Carlos Andrés Mendoza \n VALDEZ.
      // Words 2..5 are one PERSON even across the physical newline.
      val score = result.getJSONArray("logits").getDouble((2*12+3)*8)
      println("ORT VERSION: " + ai.onnxruntime.OrtEnvironment.getVersion())
      assertTrue("Wrapped full-name confidence must exceed 0.5; logit=$score",score>0.0)
    } finally { GlinerRuntime.unload() }
  }
  @Test fun uploaderIsNotRegisteredInInstalledManifest() {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val info = context.packageManager.getServiceInfo(
      android.content.ComponentName(context, "com.google.android.datatransport.runtime.backends.TransportBackendDiscovery"),
      android.content.pm.PackageManager.GET_META_DATA)
    assertFalse(info.metaData?.containsKey("backend:com.google.android.datatransport.cct.CctBackendFactory") == true)
  }
  @Test fun localOcrReturnsUtf8OffsetsAndGeometry() {
    val plan = fixture(); val source = bitmap(plan.getJSONArray("pages").getJSONObject(0))
    val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "layout-ocr.png")
    try {
      file.outputStream().use { source.compress(Bitmap.CompressFormat.PNG, 100, it) }
      val layout = JSONObject(BlueDoorAndroid.extractLayout(file.absolutePath))
      assertTrue(layout.getString("text").contains("Ana"))
      val page = layout.getJSONArray("pages").getJSONObject(0)
      assertEquals(841.89, page.getDouble("height"), 0.01); assertEquals(400.0 / 600.0, page.getDouble("width") / page.getDouble("height"), 0.001)
      val text = layout.getString("text").toByteArray(Charsets.UTF_8); val glyphs = page.getJSONArray("glyphs")
      assertTrue(glyphs.length() > 0)
      for (i in 0 until glyphs.length()) {
        val glyph = glyphs.getJSONObject(i)
        assertTrue(glyph.getInt("end") <= text.size)
        assertTrue(String(text.copyOfRange(glyph.getInt("start"), glyph.getInt("end")), Charsets.UTF_8).isNotBlank())
      }
    } finally { file.delete(); source.recycle() }
  }
}
