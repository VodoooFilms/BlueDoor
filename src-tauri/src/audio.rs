use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use transcribe_cpp::{
    RunOptions as SttRunOptions, Session as SttSession, TimestampKind, Word as SttWord,
};

use crate::types::{AppState, Segment, TranscriptionResult, WhisperConfig};

const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "ogg", "oga", "flac", "webm", "mp4", "m4v", "mov", "mkv", "aif",
    "aiff", "wma", "3gp",
];

fn run_transcription(
    audio_data: Vec<f32>,
    config: &WhisperConfig,
    session: &Mutex<SttSession>,
    diarization_model: Option<&Path>,
) -> Result<TranscriptionResult, String> {
    if config.model_path.trim().is_empty() {
        return Err("No hay un modelo de transcripción configurado.".to_string());
    }
    if config.diarize {
        return run_speaker_transcription(
            audio_data,
            config,
            session,
            diarization_model.ok_or("Falta el modelo Nemotron 3.")?,
        );
    }
    run_plain_transcription(audio_data, config, session)
}

/// Transcribe con el modelo activo. Un único motor (transcribe.cpp) atiende
/// tanto los Whisper `ggml-*.bin` como los Parakeet `.gguf`.
fn run_plain_transcription(
    audio_data: Vec<f32>,
    config: &WhisperConfig,
    session: &Mutex<SttSession>,
) -> Result<TranscriptionResult, String> {
    let mut session = session
        .lock()
        .map_err(|_| "La sesión de transcripción no está disponible.".to_string())?;
    let options = build_run_options(config, best_timestamps(&session.model()));
    let transcript = session
        .run(&audio_data, &options)
        .map_err(|e| format!("Falló la transcripción: {}", e))?;

    // Tres formas de obtener segmentos, de mejor a peor:
    //  1. El modelo ya los da (Whisper con `Segment`).
    //  2. Re-seccionamos por frase desde los timestamps por palabra, que es lo
    //     que necesita Parakeet (devuelve un único segmento para todo).
    //  3. Sin alineación: un único segmento con la duración completa, para que
    //     la vista con timestamps y el PDF nunca se queden sin contenido.
    let mut segments: Vec<Segment> = transcript
        .segments
        .iter()
        .map(|seg| Segment {
            start: seg.t0_ms as f64 / 1000.0,
            end: seg.t1_ms as f64 / 1000.0,
            text: seg.text.trim().to_string(),
            speaker: None,
        })
        .filter(|seg| !seg.text.is_empty())
        .collect();
    // Si pedimos timestamps por palabra y el modelo los da, mandan ellos: es la
    // única forma de cortar el único bloque que devuelve Parakeet. Solo se
    // llega aquí cuando el modelo admite `Word` (Whisper no, y por eso nunca
    // falla pidiendo más de lo que soporta).
    if options.timestamps == TimestampKind::Word {
        let by_words = segment_words(&transcript.words);
        if !by_words.is_empty() {
            segments = by_words;
        }
    }
    if segments.is_empty() && !transcript.text.trim().is_empty() {
        let total = audio_data.len() as f64 / 16000.0;
        segments.push(Segment {
            start: 0.0,
            end: total,
            text: transcript.text.trim().to_string(),
            speaker: None,
        });
    }

    let text = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let duration = segments.iter().map(|s| s.end).fold(0.0_f64, f64::max);

    let language = transcript
        .language
        .clone()
        .filter(|code| !code.is_empty())
        .or_else(|| config.language.clone())
        .unwrap_or_else(|| "auto".to_string());

    Ok(TranscriptionResult {
        id: uuid::Uuid::new_v4().to_string(),
        text: if text.is_empty() {
            transcript.text.trim().to_string()
        } else {
            text
        },
        segments,
        language,
        duration,
    })
}

/// Cada familia admite una granularidad máxima: Whisper solo llega a
/// `Segment`, Parakeet a `Word`/`Token`. Pedir más de lo soportado aborta la
/// transcripción, así que se pide lo más rico que el modelo permita.
fn best_timestamps(model: &transcribe_cpp::Model) -> TimestampKind {
    match model.capabilities().max_timestamp_kind {
        TimestampKind::None | TimestampKind::Auto => TimestampKind::Segment,
        TimestampKind::Segment => TimestampKind::Segment,
        TimestampKind::Word | TimestampKind::Token => TimestampKind::Word,
    }
}

fn build_run_options(config: &WhisperConfig, timestamps: TimestampKind) -> SttRunOptions {
    let mut options = SttRunOptions {
        timestamps,
        ..Default::default()
    };
    match config.language.as_deref() {
        Some("auto") | None => options.language = None,
        Some(code) => options.language = Some(code.to_string()),
    }
    options
}

/// Tope de duración/longitud por segmento de Parakeet. El modelo entrega
/// timestamps por palabra pero un único segmento para toda la grabación, así
/// que re-seccionamos por frase y, si no hay puntuación, por estos topes.
const PARAKEET_SEGMENT_MAX_SECS: f64 = 30.0;
const PARAKEET_SEGMENT_MAX_CHARS: usize = 320;

fn ends_sentence(text: &str) -> bool {
    matches!(text.trim_end().chars().next_back(), Some('.' | '?' | '!'))
}

/// Convierte los timestamps por palabra de Parakeet en segmentos con
/// timestamps, cerrando cada uno en final de frase (o al alcanzar el tope).
pub(crate) fn segment_words(words: &[SttWord]) -> Vec<Segment> {
    fn flush(buffer: &mut Vec<&SttWord>, segments: &mut Vec<Segment>) {
        if buffer.is_empty() {
            return;
        }
        let text = buffer
            .iter()
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            segments.push(Segment {
                start: buffer[0].t0_ms as f64 / 1000.0,
                end: buffer[buffer.len() - 1].t1_ms as f64 / 1000.0,
                text,
                speaker: None,
            });
        }
        buffer.clear();
    }

    let mut segments = Vec::new();
    let mut buffer: Vec<&SttWord> = Vec::new();
    for word in words {
        let trimmed = word.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        buffer.push(word);
        let span = (word.t1_ms - buffer[0].t0_ms) as f64 / 1000.0;
        let chars: usize = buffer.iter().map(|w| w.text.trim().chars().count()).sum();
        if ends_sentence(trimmed)
            || span >= PARAKEET_SEGMENT_MAX_SECS
            || chars >= PARAKEET_SEGMENT_MAX_CHARS
        {
            flush(&mut buffer, &mut segments);
        }
    }
    flush(&mut buffer, &mut segments);
    segments
}

fn format_speaker_turns(turns: &[Segment]) -> String {
    let mut previous_speaker: Option<u8> = None;
    turns
        .iter()
        .map(|turn| {
            let prefix = match (turn.speaker, previous_speaker) {
                (Some(speaker), Some(previous)) if speaker == previous => String::new(),
                (Some(speaker), _) => {
                    previous_speaker = Some(speaker);
                    format!("Hablante {speaker}: ")
                }
                (None, _) => {
                    previous_speaker = None;
                    String::new()
                }
            };
            format!("{prefix}{}", turn.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_speaker_transcription(
    audio_data: Vec<f32>,
    config: &WhisperConfig,
    session: &Mutex<SttSession>,
    diarization_model: &Path,
) -> Result<TranscriptionResult, String> {
    let intervals = crate::diarization::diarize(&audio_data, diarization_model)?;
    if intervals.is_empty() {
        return Err("Nemotron 3 no detectó voces en este audio.".into());
    }

    let plain_config = WhisperConfig {
        diarize: false,
        ..config.clone()
    };
    let duration = audio_data.len() as f64 / 16000.0;
    let mut turns = Vec::new();
    let mut languages = Vec::new();
    for interval in intervals {
        let first = (interval.start.max(0.0) * 16000.0).floor() as usize;
        let last = (interval.end.min(duration) * 16000.0).ceil() as usize;
        if first >= last || last > audio_data.len() {
            continue;
        }
        let result =
            run_plain_transcription(audio_data[first..last].to_vec(), &plain_config, session)?;
        let cleaned = result.text.trim();
        if cleaned.is_empty() {
            continue;
        }
        if result.language != "auto" && !languages.contains(&result.language) {
            languages.push(result.language);
        }
        turns.push(Segment {
            start: first as f64 / 16000.0,
            end: last as f64 / 16000.0,
            text: cleaned.to_string(),
            speaker: Some(interval.speaker),
        });
    }
    if turns.is_empty() {
        return Err("El modelo no encontró texto en los turnos detectados.".into());
    }
    let text = format_speaker_turns(&turns);
    Ok(TranscriptionResult {
        id: uuid::Uuid::new_v4().to_string(),
        text,
        segments: turns,
        language: languages.join(","),
        duration,
    })
}

#[tauri::command]
pub(crate) async fn choose_audio_file(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app_handle
            .dialog()
            .file()
            .add_filter("Audio / Video", AUDIO_EXTENSIONS)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("No se pudo abrir el selector de audio: {}", e))?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| "El archivo seleccionado no es una ruta local válida.".to_string())?;
    let path = validate_input_path(&path)?;
    let path = state.authorize_audio_path(&path)?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
pub(crate) async fn transcribe_file(
    file_path: String,
    app_handle: tauri::AppHandle,
    config: WhisperConfig,
    state: State<'_, AppState>,
) -> Result<TranscriptionResult, String> {
    let file_path = state.consume_audio_path(Path::new(&file_path))?;
    let resolved = state.resolve_config(&config, app_handle.clone());
    let use_diarization = resolved.diarize;
    let diarization_model = crate::diarization::managed_model_path(&app_handle);
    let decoder = crate::platform::decoder_path(&app_handle)?;
    tauri::async_runtime::spawn_blocking(move || {
        let audio_data = load_audio_as_f32(&file_path, Some(&decoder))?;
        let state = app_handle.state::<AppState>();
        let session = state.stt_session(&resolved.model_path, resolved.threads as i32)?;
        run_transcription(
            audio_data,
            &resolved,
            &session,
            use_diarization.then_some(diarization_model.as_path()),
        )
    })
    .await
    .map_err(|e| format!("Tarea interrumpida: {}", e))?
}

#[tauri::command]
pub(crate) fn prepare_transcribe_blob(config: WhisperConfig, state: State<'_, AppState>) -> String {
    state.store_blob_config(config)
}

#[tauri::command]
pub(crate) async fn transcribe_blob(
    request: tauri::ipc::Request<'_>,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<TranscriptionResult, String> {
    let token = request
        .headers()
        .get("x-bluedoor-config")
        .and_then(|value| value.to_str().ok())
        .ok_or("Falta la configuración de la transcripción.")?;
    let data = match request.body() {
        tauri::ipc::InvokeBody::Raw(data) => data.clone(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("El audio debe enviarse como datos binarios.".into())
        }
    };
    let config = state.consume_blob_config(token)?;
    let resolved = state.resolve_config(&config, app_handle.clone());
    let use_diarization = resolved.diarize;
    let diarization_model = crate::diarization::managed_model_path(&app_handle);
    tauri::async_runtime::spawn_blocking(move || {
        let audio = load_wav_reader(std::io::Cursor::new(data))?;
        let state = app_handle.state::<AppState>();
        let session = state.stt_session(&resolved.model_path, resolved.threads as i32)?;
        run_transcription(
            audio,
            &resolved,
            &session,
            use_diarization.then_some(diarization_model.as_path()),
        )
    })
    .await
    .map_err(|e| format!("Tarea interrumpida: {}", e))?
}

fn validate_input_path(path: &Path) -> Result<PathBuf, String> {
    let input = path
        .canonicalize()
        .map_err(|_| "No se pudo abrir el archivo seleccionado.".to_string())?;
    if !input.is_file() {
        return Err("Selecciona un archivo de audio o vídeo.".into());
    }
    let extension = input
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or("El archivo seleccionado no tiene una extensión válida.")?;
    if !AUDIO_EXTENSIONS.iter().any(|allowed| *allowed == extension) {
        return Err("Selecciona un archivo de audio o vídeo compatible.".into());
    }
    Ok(input)
}

fn load_audio_as_f32(path: &Path, decoder: Option<&Path>) -> Result<Vec<f32>, String> {
    let input = validate_input_path(path)?;
    if input
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        if let Ok(audio) = load_wav_as_f32(&input) {
            return Ok(audio);
        }
    }
    convert_to_wav_and_load(
        &input,
        decoder.ok_or("El decodificador de audio no está disponible.")?,
    )
}

fn load_wav_as_f32(path: &Path) -> Result<Vec<f32>, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("No se pudo abrir el audio: {}", e))?;
    load_wav_reader(std::io::BufReader::new(file))
}

fn load_wav_reader<R: std::io::Read + std::io::Seek>(input: R) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::new(input).map_err(|e| format!("No se pudo leer el WAV: {}", e))?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err("El formato de audio no es válido.".into());
    }
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .map(|s| {
                s.map(|v| {
                    if v.is_finite() {
                        v.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("No se pudo leer el audio: {}", e))?,
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("No se pudieron leer las muestras de audio: {}", e))?,
        (hound::SampleFormat::Int, bits @ (8 | 24 | 32)) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / (1u64 << (bits - 1)) as f32))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("No se pudieron leer las muestras de audio: {}", e))?,
        _ => {
            return Err(format!(
                "Formato de audio no soportado ({} bits por muestra)",
                spec.bits_per_sample
            ))
        }
    };

    if samples.is_empty() {
        return Err("El archivo no contiene audio.".into());
    }
    // Convert to mono 16kHz if needed
    let mono = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect()
    } else {
        samples
    };

    // Resample to 16kHz if needed
    if spec.sample_rate != 16000 {
        let ratio = 16000.0 / spec.sample_rate as f32;
        let new_len = (mono.len() as f32 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src_idx = i as f32 / ratio;
            let idx = src_idx as usize;
            let frac = src_idx - idx as f32;
            let sample = if idx + 1 < mono.len() {
                mono[idx] * (1.0 - frac) + mono[idx + 1] * frac
            } else {
                mono[idx]
            };
            resampled.push(sample);
        }
        Ok(resampled)
    } else {
        Ok(mono)
    }
}

/// Elimina residuos temporales/privados de ejecuciones anteriores:
/// descargas parciales (.part) y audios temporales de transcripción.
pub(crate) fn cleanup_leftovers(models_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("part") {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("bluedoor-rec-") || name.starts_with("bluedoor-dec-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn convert_to_wav_and_load(path: &Path, decoder: &Path) -> Result<Vec<f32>, String> {
    if !decoder.is_file() {
        return Err("Falta un componente de audio de la aplicación. Reinstala BlueDoor; tus documentos se conservan.".into());
    }
    // Private, exclusive temporary directory; RAII also cleans failures.
    let temp = tempfile::Builder::new()
        .prefix("bluedoor-dec-")
        .tempdir()
        .map_err(|e| {
            format!(
                "No se pudo preparar el audio. Comprueba el espacio disponible: {}",
                e
            )
        })?;
    let wav = temp.path().join("audio.wav");
    let mut command = std::process::Command::new(decoder);
    command
        .args([
            "-nostdin",
            "-y",
            "-loglevel",
            "error",
            "-protocol_whitelist",
            "file",
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-dn",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "wav",
            "-acodec",
            "pcm_s16le",
        ])
        .arg(&wav)
        .env_remove("DYLD_LIBRARY_PATH")
        .env_remove("DYLD_INSERT_LIBRARIES");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Do not flash a console window while importing a file from the GUI.
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().map_err(|_| {
        "No se pudo iniciar un componente de audio. Reinstala BlueDoor.".to_string()
    })?;
    if !output.status.success() {
        return Err("No se pudo leer el audio. El archivo puede estar dañado, no contener audio o usar un formato no compatible. Comprueba también el espacio disponible.".into());
    }
    load_wav_as_f32(&wav)
}

#[cfg(test)]
mod tests;
