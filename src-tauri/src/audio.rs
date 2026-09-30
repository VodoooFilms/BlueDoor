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
        .get("x-belegal-local-demo-config")
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
            if name.starts_with("belegal-local-demo-rec-")
                || name.starts_with("belegal-local-demo-dec-")
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn convert_to_wav_and_load(path: &Path, decoder: &Path) -> Result<Vec<f32>, String> {
    if !decoder.is_file() {
        return Err("Falta un componente de audio de la aplicación. Reinstala Belegal _ Local demo; tus documentos se conservan.".into());
    }
    // Private, exclusive temporary directory; RAII also cleans failures.
    let temp = tempfile::Builder::new()
        .prefix("belegal-local-demo-dec-")
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
        "No se pudo iniciar un componente de audio. Reinstala Belegal _ Local demo.".to_string()
    })?;
    if !output.status.success() {
        return Err("No se pudo leer el audio. El archivo puede estar dañado, no contener audio o usar un formato no compatible. Comprueba también el espacio disponible.".into());
    }
    load_wav_as_f32(&wav)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WhisperConfig;

    fn stt_word(text: &str, t0_ms: i64, t1_ms: i64) -> SttWord {
        SttWord {
            t0_ms,
            t1_ms,
            seg_index: 0,
            first_token: 0,
            n_tokens: 0,
            text: text.to_string(),
        }
    }

    #[test]
    fn parakeet_words_are_grouped_into_sentence_segments() {
        let words = vec![
            stt_word("Hola", 0, 500),
            stt_word("mundo.", 500, 900),
            stt_word("Segunda", 1200, 1700),
            stt_word("frase", 1700, 2100),
            stt_word("aquí.", 2100, 2500),
            stt_word("Sin", 3000, 3300),
            stt_word("punto", 3300, 3700),
            stt_word("final", 3700, 4100),
        ];
        let segments = segment_words(&words);
        assert_eq!(segments.len(), 3, "{:?}", segments);
        assert_eq!(segments[0].text, "Hola mundo.");
        assert_eq!(segments[0].start, 0.0);
        assert_eq!(segments[0].end, 0.9);
        assert_eq!(segments[1].text, "Segunda frase aquí.");
        assert_eq!(segments[1].start, 1.2);
        assert_eq!(segments[1].end, 2.5);
        // Sin puntuación final, se cierra al agotar el buffer.
        assert_eq!(segments[2].text, "Sin punto final");
        assert_eq!(segments[2].end, 4.1);
    }

    #[test]
    fn parakeet_segments_respect_the_duration_cap() {
        // 40 palabras de 1 s sin puntuación: debe partir por el tope de 30 s.
        let words: Vec<SttWord> = (0..40)
            .map(|i| stt_word("palabra", i * 1000, (i + 1) * 1000))
            .collect();
        let segments = segment_words(&words);
        assert!(
            segments.len() > 1,
            "un solo segmento de 40 s: {:?}",
            segments
        );
        assert!(segments.iter().all(|s| s.end - s.start <= 30.5));
    }

    #[test]
    fn parakeet_blank_words_do_not_create_empty_segments() {
        let words = vec![stt_word("  ", 0, 100), stt_word("Hola.", 100, 600)];
        let segments = segment_words(&words);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hola.");
    }

    /// Carga un modelo real del directorio de modelos y devuelve su sesión.
    /// Devuelve `None` (skip) si el modelo no está descargado.
    fn open_session(file: &str) -> Option<Mutex<SttSession>> {
        let path = crate::platform::test_models_dir().join(file);
        if !path.exists() {
            eprintln!("SKIP: {} no está descargado", file);
            return None;
        }
        let model = transcribe_cpp::Model::load(&path).unwrap();
        Some(Mutex::new(model.session().unwrap()))
    }

    fn spanish_fixture() -> Option<Vec<f32>> {
        let p = crate::platform::test_audio_fixture();
        if !p.exists() {
            eprintln!("SKIP: fixture no encontrado");
            return None;
        }
        Some(load_wav_as_f32(&p).unwrap())
    }

    /// Repite el fixture varias veces (con una pausa) para tener audio con
    /// varias frases sin depender de TTS ni de ficheros de la maquina.
    fn repeated_fixture(times: usize, gap_samples: usize) -> Vec<f32> {
        let base = spanish_fixture().expect("fixture");
        let mut out = Vec::with_capacity(base.len() * times);
        for _ in 0..times {
            out.extend_from_slice(&base);
            out.resize(out.len() + gap_samples, 0.0);
        }
        out
    }

    fn config_for(file: &str, language: Option<&str>) -> WhisperConfig {
        WhisperConfig {
            model_path: crate::platform::test_models_dir()
                .join(file)
                .to_string_lossy()
                .to_string(),
            language: language.map(|c| c.to_string()),
            threads: 0,
            diarize: false,
        }
    }

    #[test]
    fn parakeet_transcribes_spanish_with_segments() {
        let (Some(session), Some(audio)) = (
            open_session("parakeet-tdt-0.6b-v3-Q8_0.gguf"),
            spanish_fixture(),
        ) else {
            return;
        };
        let cfg = config_for("parakeet-tdt-0.6b-v3-Q8_0.gguf", Some("es"));
        let result = run_transcription(audio, &cfg, &session, None).unwrap();
        eprintln!("PARAKEET TEXT={:?}", result.text);
        assert!(!result.text.is_empty(), "parakeet devolvió texto vacío");
        assert!(
            result.text.contains("Hola"),
            "texto inesperado: {}",
            result.text
        );
        // El modelo entrega un único segmento para toda la grabación, así que
        // los segmentos de aquí salen de la re-segmentación por frase.
        assert!(
            !result.segments.is_empty(),
            "parakeet no devolvió segmentos"
        );
        // Invariante: los segmentos reconstruyen el texto completo y cubren
        // toda la duración.
        let joined = result
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined, result.text.trim());
        assert!(
            (result.segments.last().unwrap().end - result.duration).abs() < 1.0,
            "el ultimo segmento no llega al final: {:?}",
            result.segments
        );
        assert!(
            result.segments.iter().all(|s| s.end >= s.start),
            "segmentos desordenados: {:?}",
            result.segments
        );
        assert!(result.duration > 0.0);
    }

    /// Regresión: Parakeet devuelve UN solo segmento para toda la grabación, así
    /// que si no re-seccionamos por frase el editor y el PDF reciben un bloque
    /// gigante sin timestamps utilizables.
    #[test]
    fn parakeet_splits_multiple_sentences_into_segments() {
        let Some(session) = open_session("parakeet-tdt-0.6b-v3-Q8_0.gguf") else {
            return;
        };
        let audio = repeated_fixture(3, 16_000 * 2);
        let cfg = config_for("parakeet-tdt-0.6b-v3-Q8_0.gguf", Some("es"));
        let result = run_transcription(audio, &cfg, &session, None).unwrap();
        assert!(
            result.segments.len() >= 3,
            "esperaba al menos 3 frases, obtuve {}: {:?}",
            result.segments.len(),
            result.segments
        );
        assert!(result
            .segments
            .windows(2)
            .all(|w| w[1].start >= w[0].start && w[1].start >= w[0].end - 0.05));
    }

    #[test]
    fn whisper_still_transcribes_spanish_through_the_single_engine() {
        let (Some(session), Some(audio)) =
            (open_session("ggml-large-v3-turbo.bin"), spanish_fixture())
        else {
            return;
        };
        let cfg = config_for("ggml-large-v3-turbo.bin", Some("es"));
        let result = run_transcription(audio, &cfg, &session, None).unwrap();
        eprintln!(
            "WHISPER TEXT={:?} SEGMENTS={}",
            result.text,
            result.segments.len()
        );
        assert!(!result.text.is_empty(), "whisper devolvió texto vacío");
        assert_eq!(result.language, "es", "esperaba español");
        assert!(result.text.contains("Hola"), "texto: {}", result.text);
        assert!(!result.segments.is_empty(), "whisper sin segmentos");
    }

    /// Un mismo proceso carga y transcribe con los dos modelos, alternando.
    /// Es la regresión que fallaba con whisper-rs y transcribe.cpp a la vez
    /// (dos copias de ggml con layouts incompatibles -> GGML_ASSERT).
    #[test]
    fn both_model_families_run_in_one_process() {
        let (Some(parakeet), Some(whisper), Some(audio)) = (
            open_session("parakeet-tdt-0.6b-v3-Q8_0.gguf"),
            open_session("ggml-large-v3-turbo.bin"),
            spanish_fixture(),
        ) else {
            return;
        };
        let pcfg = config_for("parakeet-tdt-0.6b-v3-Q8_0.gguf", Some("es"));
        let wcfg = config_for("ggml-large-v3-turbo.bin", Some("es"));

        let first = run_transcription(audio.clone(), &pcfg, &parakeet, None).unwrap();
        assert!(first.text.contains("Hola"), "parakeet: {}", first.text);

        let second = run_transcription(audio.clone(), &wcfg, &whisper, None).unwrap();
        assert!(second.text.contains("Hola"), "whisper: {}", second.text);

        // Y de vuelta a Parakeet, para descartar estado compartido por orden.
        let third = run_transcription(audio, &pcfg, &parakeet, None).unwrap();
        assert!(third.text.contains("Hola"), "parakeet 2ª: {}", third.text);
    }

    #[test]
    fn speaker_prefix_is_emitted_only_when_speaker_changes() {
        let turns = vec![
            Segment {
                start: 0.0,
                end: 1.0,
                text: "uno".to_string(),
                speaker: Some(1),
            },
            Segment {
                start: 1.0,
                end: 2.0,
                text: "dos".to_string(),
                speaker: Some(1),
            },
            Segment {
                start: 2.0,
                end: 3.0,
                text: "tres".to_string(),
                speaker: Some(2),
            },
            Segment {
                start: 3.0,
                end: 4.0,
                text: "cuatro".to_string(),
                speaker: Some(1),
            },
        ];
        assert_eq!(
            format_speaker_turns(&turns),
            "Hablante 1: uno\ndos\nHablante 2: tres\nHablante 1: cuatro"
        );
    }

    /// Regresión: Parakeet + Nemotron deben funcionar juntos. Nemotron decide
    /// los turnos por tiempo y cada intervalo se transcribe por separado, asi
    /// que el modelo de ASR no necesita saber de hablantes. Este test es la
    /// prueba viva de que NO hay que bloquear la diarizacion con Parakeet.
    #[test]
    fn parakeet_works_with_nemotron_diarization() {
        let Ok(fixture) = std::env::var("BELEGAL_LOCAL_DEMO_DIAR_TEST_AUDIO") else {
            return;
        };
        let Ok(diar_model) = std::env::var("BELEGAL_LOCAL_DEMO_NEMOTRON_MODEL") else {
            return;
        };
        let Some(session) = open_session("parakeet-tdt-0.6b-v3-Q8_0.gguf") else {
            return;
        };
        let audio = load_wav_as_f32(Path::new(&fixture)).unwrap();
        let cfg = WhisperConfig {
            diarize: true,
            ..config_for("parakeet-tdt-0.6b-v3-Q8_0.gguf", Some("es"))
        };
        let result =
            run_transcription(audio, &cfg, &session, Some(Path::new(&diar_model))).unwrap();
        let speakers: std::collections::BTreeSet<u8> =
            result.segments.iter().filter_map(|s| s.speaker).collect();
        assert!(
            speakers.len() >= 2,
            "Parakeet deberia dar varios hablantes con Nemotron, dio {:?}: {}",
            speakers,
            result.text
        );
        assert!(result.text.contains("Hablante 1:"));
        assert!(result.text.contains("Hablante 2:"));
        assert!(result
            .segments
            .iter()
            .all(|s| s.start < s.end && s.end <= result.duration));
    }

    #[test]
    fn local_two_speaker_pipeline() {
        let Ok(fixture) = std::env::var("BELEGAL_LOCAL_DEMO_DIAR_TEST_AUDIO") else {
            return;
        };
        let Ok(model_path) = std::env::var("BELEGAL_LOCAL_DEMO_DIAR_TEST_WHISPER") else {
            return;
        };
        let audio = load_wav_as_f32(Path::new(&fixture)).unwrap();
        let cfg = WhisperConfig {
            model_path: model_path.clone(),
            language: None,
            threads: 0,
            diarize: true,
        };
        let model = transcribe_cpp::Model::load(&model_path).unwrap();
        let session = model.session().unwrap();
        let diar_model = std::env::var("BELEGAL_LOCAL_DEMO_NEMOTRON_MODEL").unwrap();
        let result = run_transcription(
            audio,
            &cfg,
            &Mutex::new(session),
            Some(Path::new(&diar_model)),
        )
        .unwrap();
        let speakers: std::collections::HashSet<u8> =
            result.segments.iter().filter_map(|s| s.speaker).collect();
        assert!(speakers.len() >= 2, "result: {}", result.text);
        assert!(result.text.contains("Hablante 1:"));
        assert!(result.text.contains("Hablante 2:"));
        // Parakeet agrupa por frases, así que los turnos pueden colapsarse en
        // pocos segmentos; lo que debe cumplirse es que cada segmento con
        // hablante sea interior al clip y los intervalos sean válidos.
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.start < s.end && s.end <= result.duration),
            "segmentos con intervalos inválidos en: {}",
            result.text
        );
    }

    #[test]
    fn pcm_recording_decodes_without_any_executable() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(
                &mut bytes,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 16000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for value in [0i16, 16384, -16384, 32767] {
                writer.write_sample(value).unwrap();
            }
            writer.finalize().unwrap();
        }
        bytes.set_position(0);
        let audio = load_wav_reader(bytes).unwrap();
        assert_eq!(audio.len(), 4);
        assert_eq!(audio[1], 0.5);
        assert_eq!(audio[2], -0.5);
        assert!(load_wav_reader(std::io::Cursor::new(b"not audio")).is_err());
    }

    #[test]
    fn missing_internal_decoder_requests_reinstall_not_system_tools() {
        let result = convert_to_wav_and_load(
            Path::new("input.mp3"),
            Path::new("/missing/belegal-local-demo-internal-decoder"),
        )
        .unwrap_err();
        assert!(result.contains("Reinstala Belegal _ Local demo"));
        assert!(!result.contains("brew"));
        assert!(!result.contains("Instala FFmpeg"));
    }

    #[test]
    fn transcribe_spanish_works() {
        let model_path = crate::platform::test_models_dir()
            .join("ggml-small.bin")
            .to_string_lossy()
            .to_string();
        let model = std::path::Path::new(&model_path);
        if !model.exists() {
            eprintln!("SKIP: model not found");
            return;
        }
        let audio = match crate::platform::test_audio_fixture() {
            p if p.exists() => load_wav_as_f32(&p).unwrap(),
            _ => {
                let p = crate::platform::test_audio_fixture();
                eprintln!("SKIP: fixture {} not found", p.display());
                return;
            }
        };

        let cfg = WhisperConfig {
            model_path: model.to_string_lossy().to_string(),
            // Validate Spanish transcription deterministically. Auto-detection
            // can classify a short clip as another language.
            language: Some("es".to_string()),
            threads: 0,
            diarize: false,
        };
        let model = transcribe_cpp::Model::load(&cfg.model_path).unwrap();
        let session = model.session().unwrap();
        let result = run_transcription(audio, &cfg, &Mutex::new(session), None).unwrap();
        eprintln!("LANG={} TEXT={:?}", result.language, result.text);
        assert!(!result.text.is_empty(), "transcription was empty");
        assert_eq!(
            result.language, "es",
            "expected Spanish, got {}",
            result.language
        );
        assert!(
            result.text.contains("Hola"),
            "unexpected text: {}",
            result.text
        );
    }
}
