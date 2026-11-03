//! Vosk's model archive is verified before extraction into private app storage.
use crate::types::{Segment, TranscriptionResult, WhisperConfig};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{io::Read, path::Path, sync::Mutex};
static RUNNING: Mutex<()> = Mutex::new(());
pub(crate) fn transcribe(
    audio: Vec<f32>,
    config: &WhisperConfig,
) -> Result<TranscriptionResult, String> {
    let _guard = RUNNING
        .try_lock()
        .map_err(|_| "Ya hay una transcripción en curso.")?;
    let path = Path::new(&config.model_path);
    crate::models::verify_model_file(path)?;
    let parent = path.parent().ok_or("Ruta del modelo no válida")?;
    let (root, cache, language) = match path.file_name().and_then(|s| s.to_str()) {
        Some(crate::models::VOSK_FILE) => (
            "vosk-model-small-es-0.42",
            "vosk-small-es-0.42-verified-v1",
            "es",
        ),
        Some(crate::models::VOSK_EN_FILE) => (
            "vosk-model-small-en-us-0.15",
            "vosk-small-en-0.15-verified-v1",
            "en",
        ),
        _ => return Err("Modelo Vosk no reconocido".into()),
    };
    let directory = parent.join(cache);
    if !directory.join(".complete").exists() {
        let stage = parent.join(format!("vosk-extract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let extraction = (|| -> Result<(), String> {
            let mut archive =
                zip::ZipArchive::new(std::fs::File::open(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if archive.len() > 1024 {
                return Err("Archivo de modelo no válido".into());
            }
            let mut total = 0u64;
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
                let relative = entry.enclosed_name().ok_or("Ruta de modelo no válida")?;
                if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                    return Err("Enlace no admitido en el modelo".into());
                }
                total = total
                    .checked_add(entry.size())
                    .ok_or("Modelo demasiado grande")?;
                if total > 512 * 1024 * 1024 {
                    return Err("Modelo demasiado grande".into());
                }
                let output = stage.join(relative);
                if entry.is_dir() {
                    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
                } else {
                    std::fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
                    let mut file = std::fs::File::create(output).map_err(|e| e.to_string())?;
                    std::io::copy(&mut entry.by_ref().take(512 * 1024 * 1024), &mut file)
                        .map_err(|e| e.to_string())?;
                }
            }
            std::fs::write(stage.join(".complete"), root).map_err(|e| e.to_string())?;
            if directory.exists() {
                std::fs::remove_dir_all(&directory).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&stage, &directory).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if extraction.is_err() {
            let _ = std::fs::remove_dir_all(stage);
        }
        extraction?;
    }
    let pcm: Vec<u8> = audio
        .iter()
        .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let payload = serde_json::json!({"modelPath":directory.join(root),"pcm":STANDARD.encode(pcm)})
        .to_string();
    let response = crate::android::transcribe_vosk(&payload)?;
    let result: serde_json::Value =
        serde_json::from_str(&response).map_err(|_| "Vosk devolvió un resultado no válido")?;
    if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        return Err(format!("Vosk: {error}"));
    }
    let text = result
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_owned();
    if text.is_empty() {
        return Err("No se reconoció voz. Vuelve a grabar más cerca del micrófono.".into());
    }
    let duration = audio.len() as f64 / 16000.0;
    Ok(TranscriptionResult {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.clone(),
        segments: vec![Segment {
            start: 0.0,
            end: duration,
            text,
            speaker: None,
        }],
        language: language.into(),
        duration,
    })
}
