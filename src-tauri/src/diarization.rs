//! Optional local prototype. The checkpoint is managed by Models; the native
//! NeMo-Speech.cpp runtime is configured separately until it is bundled.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::paths::models_dir;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SpeakerInterval {
    pub start: f64,
    pub end: f64,
    pub speaker: u8,
}

#[derive(Deserialize)]
struct DiarizationOutput {
    segments: Vec<SpeakerInterval>,
}

#[derive(Deserialize)]
struct LocalConfig {
    nemo_bin: PathBuf,
}

fn configured_bin() -> Result<PathBuf, String> {
    let env_bin = std::env::var_os("LOCAL_LEGAL_NEMO_BIN").map(PathBuf::from);
    // A debug .app launched by Finder does not inherit shell environment.
    // Keep this fallback limited to the separate local test app.
    #[cfg(debug_assertions)]
    let file_paths = || {
        let home = std::env::var_os("HOME")?;
        let config = PathBuf::from(home).join(
            "Library/Application Support/com.locallegal.desktop.diarizationtest/diarization.json",
        );
        let data = std::fs::read(config).ok()?;
        let parsed: LocalConfig = serde_json::from_slice(&data).ok()?;
        Some(parsed.nemo_bin)
    };
    #[cfg(not(debug_assertions))]
    let file_paths = || None;
    let bin = env_bin
        .or_else(file_paths)
        .ok_or("No está configurada la prueba local de voces.")?;
    Some(bin)
        .filter(|p| p.is_file())
        .ok_or("No está configurado LOCAL_LEGAL_NEMO_BIN para la prueba de voces.".into())
}

#[tauri::command]
pub(crate) fn diarization_available(app_handle: tauri::AppHandle) -> bool {
    configured_bin().is_ok()
        && crate::models::model_appears_installed(&managed_model_path(&app_handle))
}

pub(crate) fn managed_model_path(app_handle: &tauri::AppHandle) -> PathBuf {
    models_dir(app_handle).join(crate::models::NEMOTRON_FILE)
}

pub(crate) fn diarize(audio: &[f32], model: &Path) -> Result<Vec<SpeakerInterval>, String> {
    let bin = configured_bin()?;
    crate::models::verify_model_file(model)?;
    let temp = tempfile::Builder::new()
        .prefix("local-legal-diar-")
        .tempdir()
        .map_err(|e| format!("No se pudo preparar la prueba de voces: {e}"))?;
    let wav = temp.path().join("audio.wav");
    let result = temp.path().join("speakers.json");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec)
        .map_err(|e| format!("No se pudo preparar el WAV de voces: {e}"))?;
    for sample in audio {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * 32767.0) as i16)
            .map_err(|e| format!("No se pudo guardar el WAV de voces: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("No se pudo terminar el WAV de voces: {e}"))?;

    let device = if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    };
    let stderr_path = temp.path().join("stderr.log");
    let stderr_file = std::fs::File::create(&stderr_path)
        .map_err(|e| format!("No se pudo preparar la salida de voces: {e}"))?;
    let mut child = Command::new(bin)
        .arg("diarize")
        .arg(&wav)
        .arg("--model")
        .arg(model)
        .arg("--device")
        .arg(device)
        .arg("--format")
        .arg("json")
        .arg("--output")
        .arg(&result)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("No se pudo iniciar Nemotron 3: {e}"))?;
    let seconds = u64::try_from(audio.len())
        .unwrap_or(u64::MAX)
        .saturating_add(15_999)
        / 16_000;
    let timeout = Duration::from_secs(seconds.saturating_mul(2).saturating_add(30).min(600));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Nemotron 3 tardó demasiado y se detuvo.".into());
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("No se pudo esperar Nemotron 3: {e}"));
            }
        }
    };
    if !status.success() {
        let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
        return Err(format!("Falló Nemotron 3: {}", stderr.trim()));
    }
    let data =
        std::fs::read(result).map_err(|e| format!("No se pudo leer el resultado de voces: {e}"))?;
    let parsed: DiarizationOutput = serde_json::from_slice(&data)
        .map_err(|e| format!("El resultado de voces no es válido: {e}"))?;
    Ok(parsed
        .segments
        .into_iter()
        .filter(|s| s.start.is_finite() && s.end.is_finite() && s.end > s.start && s.speaker > 0)
        .collect())
}
