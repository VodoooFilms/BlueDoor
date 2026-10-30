package com.bluedoor.desktop

import ai.onnxruntime.*
import org.json.JSONArray
import org.json.JSONObject
import java.nio.LongBuffer

/** CPU-only ONNX session. Paths and token IDs come from the verified Rust adapter. */
object GlinerRuntime {
  private var session: OrtSession? = null
  private var modelPath: String? = null
  @Synchronized fun unload() { session?.close(); session = null; modelPath = null }
  @Synchronized fun run(payload: String): String {
    val data = JSONObject(payload)
    val path = data.getString("path")
    val env = OrtEnvironment.getEnvironment(OrtLoggingLevel.ORT_LOGGING_LEVEL_ERROR)
    if (session == null || modelPath != path) {
      unload()
      OrtSession.SessionOptions().use { options ->
        options.setIntraOpNumThreads(4); options.setInterOpNumThreads(1)
        options.setSessionLogLevel(OrtLoggingLevel.ORT_LOGGING_LEVEL_ERROR)
        session = env.createSession(path, options)
      }
      modelPath = path
    }
    val ids = data.getJSONArray("ids")
    val words = data.getInt("words")
    require(ids.length() in 1..2048 && words in 1..2048)
    val mask = data.getJSONArray("mask")
    require(mask.length() == ids.length())
    val shape = longArrayOf(1, ids.length().toLong())
    val inputs = linkedMapOf<String, OnnxTensor>()
    try {
      inputs["input_ids"] = OnnxTensor.createTensor(env, LongBuffer.wrap(LongArray(ids.length()) { ids.getLong(it) }), shape)
      inputs["attention_mask"] = OnnxTensor.createTensor(env, LongBuffer.wrap(LongArray(ids.length()) { 1L }), shape)
      inputs["words_mask"] = OnnxTensor.createTensor(env, LongBuffer.wrap(LongArray(mask.length()) { mask.getLong(it) }), shape)
      inputs["text_lengths"] = OnnxTensor.createTensor(env, LongBuffer.wrap(longArrayOf(words.toLong())), longArrayOf(1, 1))
      session!!.run(inputs).use { result ->
        val tensor = result.get("logits").get() as OnnxTensor
        val expected = longArrayOf(1, words.toLong(), 8, 3)
        require(tensor.info.shape.contentEquals(expected))
        val values = tensor.floatBuffer
        val logits = JSONArray()
        while (values.hasRemaining()) logits.put(values.get().toDouble())
        return JSONObject().put("shape", JSONArray(expected.toList())).put("logits", logits).toString()
      }
    } finally { inputs.values.forEach { it.close() } }
  }
}
