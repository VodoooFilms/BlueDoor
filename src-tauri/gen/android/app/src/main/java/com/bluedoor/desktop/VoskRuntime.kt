package com.bluedoor.desktop

import android.util.Base64
import org.json.JSONObject
import org.vosk.Model
import org.vosk.Recognizer

/** Local language-specific decoder. Keep the small model warm between recordings. */
object VoskRuntime {
    private var model: Model? = null
    private var modelPath = ""
    @Synchronized fun unload() { model?.close(); model = null; modelPath = "" }
    @Synchronized fun run(payload: String): String {
        return try {
            val input = JSONObject(payload)
            val path = input.getString("modelPath")
            if (model == null || modelPath != path) {
                model?.close()
                model = null
                model = Model(path)
                modelPath = path
            }
            val pcm = Base64.decode(input.getString("pcm"), Base64.NO_WRAP)
            val parts = mutableListOf<String>()
            Recognizer(model, 16000.0f).use { recognizer ->
                var offset = 0
                while (offset < pcm.size) {
                    val count = minOf(8192, pcm.size - offset)
                    val chunk = pcm.copyOfRange(offset, offset + count)
                    if (recognizer.acceptWaveForm(chunk, count)) {
                        JSONObject(recognizer.result).optString("text").takeIf { it.isNotBlank() }?.let(parts::add)
                    }
                    offset += count
                }
                JSONObject(recognizer.finalResult).optString("text").takeIf { it.isNotBlank() }?.let(parts::add)
            }
            JSONObject().put("text", parts.joinToString(" ")).toString()
        } catch (error: Exception) {
            JSONObject().put("error", error.message ?: "Vosk could not transcribe the recording").toString()
        }
    }
}
