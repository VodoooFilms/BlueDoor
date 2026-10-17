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
fn repeated_fixture(times: usize, gap_samples: usize) -> Option<Vec<f32>> {
    let base = spanish_fixture()?;
    let mut out = Vec::with_capacity(base.len() * times);
    for _ in 0..times {
        out.extend_from_slice(&base);
        out.resize(out.len() + gap_samples, 0.0);
    }
    Some(out)
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
    let Some(audio) = repeated_fixture(3, 16_000 * 2) else {
        return;
    };
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
    let (Some(session), Some(audio)) = (open_session("ggml-large-v3-turbo.bin"), spanish_fixture())
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
    let Ok(fixture) = std::env::var("LOCAL_LEGAL_DIAR_TEST_AUDIO") else {
        return;
    };
    let Ok(diar_model) = std::env::var("LOCAL_LEGAL_NEMOTRON_MODEL") else {
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
    let result = run_transcription(audio, &cfg, &session, Some(Path::new(&diar_model))).unwrap();
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
    let Ok(fixture) = std::env::var("LOCAL_LEGAL_DIAR_TEST_AUDIO") else {
        return;
    };
    let Ok(model_path) = std::env::var("LOCAL_LEGAL_DIAR_TEST_WHISPER") else {
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
    let diar_model = std::env::var("LOCAL_LEGAL_NEMOTRON_MODEL").unwrap();
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
        Path::new("/missing/local-legal-internal-decoder"),
    )
    .unwrap_err();
    assert!(result.contains("Reinstala Local Legal"));
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
