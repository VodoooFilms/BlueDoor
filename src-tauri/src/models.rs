use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::paths::models_dir;

/// (id, nombre, peso, fichero). La tabla es la ÚNICA fuente de catálogo Whisper.
pub(crate) const GLINER_ID: &str = "gliner-pii-small-int8";
pub(crate) const GLINER_FILE: &str = "gliner-pii-small-int8.onnx";
pub(crate) const GLINER_TOKENIZER_ID: &str = "gliner-pii-tokenizer";
pub(crate) const GLINER_TOKENIZER_FILE: &str = "gliner-pii-tokenizer.json";
pub(crate) const GLINER_MULTI_ID: &str = "gliner-multi-pii-fp16";
pub(crate) const GLINER_MULTI_FILE: &str = "gliner-multi-pii-fp16.onnx";
pub(crate) const GLINER_MULTI_TOKENIZER_ID: &str = "gliner-multi-pii-tokenizer";
pub(crate) const GLINER_MULTI_TOKENIZER_FILE: &str = "gliner-multi-pii-tokenizer.json";
pub(crate) fn gliner_files(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match id {
        GLINER_ID => Some((GLINER_FILE, GLINER_TOKENIZER_FILE, GLINER_TOKENIZER_ID)),
        GLINER_MULTI_ID => Some((
            GLINER_MULTI_FILE,
            GLINER_MULTI_TOKENIZER_FILE,
            GLINER_MULTI_TOKENIZER_ID,
        )),
        _ => None,
    }
}
pub(crate) const QWEN_ID: &str = "qwen3-fast";
pub(crate) const QWEN_FILE: &str = "ggml-qwen3-1.7b-instruct-q4.gguf";
pub(crate) const QWEN_Q8_ID: &str = "qwen3-q8";
pub(crate) const QWEN_Q8_FILE: &str = "ggml-qwen3-1.7b-instruct-q8.gguf";
pub(crate) const QWEN_PII_ID: &str = "belegal-pii-v1";
pub(crate) const QWEN_PII_FILE: &str = "belegal-pii-v1-q4.gguf";
pub(crate) fn qwen_file(id: &str) -> Result<&'static str, String> {
    match id {
        QWEN_ID => Ok(QWEN_FILE),
        QWEN_Q8_ID => Ok(QWEN_Q8_FILE),
        QWEN_PII_ID if model_assets().contains_key(QWEN_PII_ID) => Ok(QWEN_PII_FILE),
        _ => Err("Peso Qwen desconocido".into()),
    }
}

const WHISPER_MODELS: &[(&str, &str, &str, &str)] = &[
    ("tiny", "Tiny", "78 MB", "ggml-tiny.bin"),
    ("base", "Base", "148 MB", "ggml-base.bin"),
    ("small", "Small", "488 MB", "ggml-small.bin"),
    ("medium", "Medium", "1.5 GB", "ggml-medium.bin"),
    (
        "large-v3-turbo",
        "Large v3 Turbo",
        "1.62 GB",
        "ggml-large-v3-turbo.bin",
    ),
    ("large-v3", "Large v3", "3.10 GB", "ggml-large-v3.bin"),
    (
        "large-v3-q5",
        "Large v3 Q5",
        "1.08 GB",
        "ggml-large-v3-q5_0.bin",
    ),
    (
        "latam-turbo-q5",
        "LatAm Turbo Q5",
        "574 MB",
        "ggml-whisper-large-v3-turbo-latam-q5_0.bin",
    ),
];

fn whisper_entry(
    size: &str,
) -> Result<&'static (&'static str, &'static str, &'static str, &'static str), String> {
    WHISPER_MODELS
        .iter()
        .find(|(id, _, _, _)| *id == size)
        .ok_or_else(|| format!("Modelo desconocido: {}", size))
}

/// Modelos Parakeet (FastConformer TDT) servidos por transcribe.cpp.
/// Mismo formato de tupla que `WHISPER_MODELS` para reutilizar el downloader.
pub(crate) const PARAKEET_MODELS: &[(&str, &str, &str, &str)] = &[(
    "parakeet-tdt-0.6b-v3",
    "Parakeet TDT v3 (rápido, 25 idiomas)",
    "740 MB",
    "parakeet-tdt-0.6b-v3-Q8_0.gguf",
)];

pub(crate) fn parakeet_file(id: &str) -> Result<&'static str, String> {
    PARAKEET_MODELS
        .iter()
        .find(|(entry_id, _, _, _)| *entry_id == id)
        .map(|(_, _, _, file)| *file)
        .ok_or_else(|| format!("Modelo Parakeet desconocido: {}", id))
}

pub(crate) const NEMOTRON_ID: &str = "nemotron3-diarization";
pub(crate) const NEMOTRON_FILE: &str = "Nemotron-3-Diarization.q8_0.gguf";

/// Fichero del Whisper "large-v3-turbo" (recomendado por defecto en `setup()`).
pub(crate) fn whisper_default_file() -> &'static str {
    WHISPER_MODELS
        .iter()
        .find(|(id, _, _, _)| *id == "large-v3-turbo")
        .map(|(_, _, _, file)| *file)
        .unwrap_or(WHISPER_MODELS[0].3)
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub size: String,
    pub path: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelsList {
    pub whisper: Vec<ModelEntry>,
    pub parakeet: Vec<ModelEntry>,
    pub diarization: Vec<ModelEntry>,
    pub privacy: Vec<ModelEntry>,
}

#[tauri::command]
pub(crate) fn list_models(app_handle: tauri::AppHandle) -> ModelsList {
    let mut whisper = Vec::new();
    for (id, name, size, file) in WHISPER_MODELS {
        let path = models_dir(&app_handle)
            .join(file)
            .to_string_lossy()
            .to_string();
        whisper.push(ModelEntry {
            id: id.to_string(),
            name: name.to_string(),
            size: size.to_string(),
            installed: model_appears_installed(std::path::Path::new(&path)),
            path,
        });
    }
    let parakeet = PARAKEET_MODELS
        .iter()
        .map(|(id, name, size, file)| {
            let path = models_dir(&app_handle).join(file);
            ModelEntry {
                id: id.to_string(),
                name: name.to_string(),
                size: size.to_string(),
                installed: model_appears_installed(&path),
                path: path.to_string_lossy().to_string(),
            }
        })
        .collect();
    let path = models_dir(&app_handle).join(NEMOTRON_FILE);
    let diarization = vec![ModelEntry {
        id: NEMOTRON_ID.to_string(),
        name: "NVIDIA Nemotron 3 Diarization".to_string(),
        size: "102 MB".to_string(),
        installed: model_appears_installed(&path),
        path: path.to_string_lossy().to_string(),
    }];
    let mut privacy: Vec<ModelEntry> = [
        (QWEN_ID, QWEN_FILE, "Q4_K_M", "1.36 GB"),
        (QWEN_Q8_ID, QWEN_Q8_FILE, "Q8_0", "2.22 GB"),
    ]
    .into_iter()
    .map(|(id, file, weight, size)| {
        let path = models_dir(&app_handle).join(file);
        ModelEntry {
            id: id.into(),
            name: format!("Qwen3-1.7B {weight}"),
            size: size.into(),
            installed: model_appears_installed(&path),
            path: path.to_string_lossy().into(),
        }
    })
    .collect();
    // A trained model is offered only by builds containing its pinned hash.
    // It has no download URL: training artifacts stay on this machine.
    if let Some(asset) = model_assets().get(QWEN_PII_ID) {
        let path = models_dir(&app_handle).join(QWEN_PII_FILE);
        privacy.push(ModelEntry {
            id: QWEN_PII_ID.into(),
            name: "BlueDoor PII v1 · postentrenado".into(),
            size: format!("{:.2} GB", asset.size as f64 / 1_000_000_000.0),
            installed: model_appears_installed(&path),
            path: path.to_string_lossy().into(),
        });
    }
    #[cfg(target_os = "android")]
    {
        // Android catalog excludes audio weights and desktop-only Q8 models.
        whisper.clear();
        privacy.retain(|entry| entry.id == QWEN_ID);
        let path = models_dir(&app_handle).join(GLINER_FILE);
        privacy.push(ModelEntry {
            id: GLINER_ID.into(),
            name: "GLiNER PII Small v1.0 · INT8 (experimental)".into(),
            size: "82.7 MB + tokenizador 3.6 MB".into(),
            installed: model_appears_installed(&path)
                && model_appears_installed(&models_dir(&app_handle).join(GLINER_TOKENIZER_FILE)),
            path: path.to_string_lossy().into(),
        });
        let path = models_dir(&app_handle).join(GLINER_MULTI_FILE);
        privacy.push(ModelEntry {
            id: GLINER_MULTI_ID.into(),
            name: "GLiNER Multi PII · español · FP16".into(),
            size: "579,7 MB + tokenizador 16,3 MB".into(),
            installed: model_appears_installed(&path)
                && model_appears_installed(
                    &models_dir(&app_handle).join(GLINER_MULTI_TOKENIZER_FILE),
                ),
            path: path.to_string_lossy().into(),
        });
    }
    ModelsList {
        whisper,
        parakeet: if cfg!(target_os = "android") {
            vec![]
        } else {
            parakeet
        },
        diarization: if cfg!(target_os = "android") {
            vec![]
        } else {
            diarization
        },
        privacy,
    }
}

#[tauri::command]
pub(crate) async fn download_diarization_model(
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let destination = models_dir(&app_handle).join(NEMOTRON_FILE);
    let ah = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        download_file(&ah, &destination, NEMOTRON_ID)?;
        Ok::<String, String>(destination.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Tarea interrumpida: {e}"))?
}

#[tauri::command]
pub(crate) fn delete_diarization_model(app_handle: tauri::AppHandle) -> Result<(), String> {
    let path = models_dir(&app_handle).join(NEMOTRON_FILE);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("No se pudo eliminar Nemotron 3: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn delete_model(size: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    let (_, _, _, file_name) = *whisper_entry(&size)?;
    let path = models_dir(&app_handle).join(file_name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("No se pudo eliminar el modelo: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn delete_parakeet_model(
    id: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let path = models_dir(&app_handle).join(parakeet_file(&id)?);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("No se pudo eliminar el modelo: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn download_model(
    size: String,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let (_, _, _, file_name) = *whisper_entry(&size)?;
    let destination = models_dir(&app_handle).join(file_name);
    let label = size.clone();
    let ah = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        download_file(&ah, &destination, &label)?;
        Ok::<String, String>(destination.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Tarea interrumpida: {}", e))?
    .map_err(|e| format!("No se pudo descargar el modelo: {}", e))
}

#[tauri::command]
pub(crate) fn get_model_path(size: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    let (_, _, _, file_name) = *whisper_entry(&size)?;
    Ok(models_dir(&app_handle)
        .join(file_name)
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub(crate) async fn download_parakeet_model(
    id: String,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let destination = models_dir(&app_handle).join(parakeet_file(&id)?);
    let label = id.clone();
    let ah = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        download_file(&ah, &destination, &label)?;
        Ok::<String, String>(destination.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Tarea interrumpida: {}", e))?
    .map_err(|e| format!("No se pudo descargar el modelo: {}", e))
}

/// Como el resto de comandos de catálogo, solo responde por ficheros de la
/// lista en la carpeta de modelos. El basename limita el catálogo y la
/// comparación canónica impide que `..` o enlaces salgan del directorio.
#[tauri::command]
pub(crate) fn check_model_exists(path: String, app_handle: tauri::AppHandle) -> bool {
    let requested = std::path::Path::new(&path);
    if asset_for_path(requested).is_none() {
        return false;
    }
    let Ok(exact) = requested.canonicalize() else {
        return false;
    };
    let Ok(models) = models_dir(&app_handle).canonicalize() else {
        return false;
    };
    exact.parent() == Some(models.as_path()) && model_appears_installed(&exact)
}

#[derive(Deserialize)]
struct ModelAsset {
    url: String,
    size: u64,
    sha256: String,
}

fn model_assets() -> &'static std::collections::HashMap<String, ModelAsset> {
    static ASSETS: OnceLock<std::collections::HashMap<String, ModelAsset>> = OnceLock::new();
    ASSETS.get_or_init(|| {
        serde_json::from_str(include_str!("../model-manifest.json"))
            .expect("checked-in model manifest")
    })
}

fn asset_for_path(path: &std::path::Path) -> Option<&'static ModelAsset> {
    let file = path.file_name()?.to_str()?;
    let id = WHISPER_MODELS
        .iter()
        .find(|m| m.3 == file)
        .map(|m| m.0)
        .or_else(|| (file == NEMOTRON_FILE).then_some(NEMOTRON_ID))
        .or_else(|| (file == GLINER_FILE).then_some(GLINER_ID))
        .or_else(|| (file == GLINER_TOKENIZER_FILE).then_some(GLINER_TOKENIZER_ID))
        .or_else(|| (file == GLINER_MULTI_FILE).then_some(GLINER_MULTI_ID))
        .or_else(|| (file == GLINER_MULTI_TOKENIZER_FILE).then_some(GLINER_MULTI_TOKENIZER_ID))
        .or_else(|| (file == QWEN_FILE).then_some(QWEN_ID))
        .or_else(|| (file == QWEN_Q8_FILE).then_some(QWEN_Q8_ID))
        .or_else(|| (file == QWEN_PII_FILE).then_some(QWEN_PII_ID))
        .or_else(|| PARAKEET_MODELS.iter().find(|m| m.3 == file).map(|m| m.0))?;
    model_assets().get(id)
}

pub(crate) fn model_appears_installed(path: &std::path::Path) -> bool {
    asset_for_path(path).is_some_and(|a| {
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.len() == a.size)
    })
}

fn verify_reader(mut reader: impl std::io::Read, asset: &ModelAsset) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("No se pudo comprobar el modelo: {}", e))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hash.update(&buf[..n]);
    }
    if total != asset.size || format!("{:x}", hash.finalize()) != asset.sha256 {
        return Err("El modelo local está incompleto o dañado. Abre Modelos, elimínalo y vuelve a descargarlo desde la aplicación.".into());
    }
    Ok(())
}

pub(crate) fn verify_model_file(path: &std::path::Path) -> Result<(), String> {
    let asset = asset_for_path(path)
        .ok_or("Modelo no reconocido. Selecciona un modelo desde la aplicación.")?;
    let file = File::open(path)
        .map_err(|_| "Falta el modelo local. Descárgalo desde Modelos.".to_string())?;
    verify_reader(file, asset)
}

static DOWNLOAD_LOCK: Mutex<()> = Mutex::new(());
static CANCEL_DOWNLOAD: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub(crate) fn cancel_model_download() {
    CANCEL_DOWNLOAD.store(true, Ordering::Relaxed);
}

fn check_cancelled() -> Result<(), String> {
    if CANCEL_DOWNLOAD.load(Ordering::Relaxed) {
        Err("Descarga cancelada. Puedes volver a intentarlo.".into())
    } else {
        Ok(())
    }
}

fn download_file(
    app_handle: &tauri::AppHandle,
    destination: &std::path::Path,
    label: &str,
) -> Result<(), String> {
    let _guard = DOWNLOAD_LOCK
        .try_lock()
        .map_err(|_| "Ya hay una descarga en curso.".to_string())?;
    CANCEL_DOWNLOAD.store(false, Ordering::Relaxed);
    if verify_model_file(destination).is_ok() {
        return Ok(());
    }
    let asset = model_assets().get(label).ok_or("Modelo no reconocido")?;
    let parent = destination
        .parent()
        .ok_or("No se pudo localizar la carpeta de modelos")?;
    let available = fs2::available_space(parent)
        .map_err(|e| format!("No se pudo comprobar el almacenamiento: {}", e))?;
    if available < asset.size + 64 * 1024 * 1024 {
        return Err(format!(
            "No hay espacio suficiente. Libera al menos {:.2} GB y vuelve a intentarlo.",
            (asset.size + 64 * 1024 * 1024) as f64 / 1e9
        ));
    }
    let agent = ureq::AgentBuilder::new()
        .https_only(true)
        .redirects(10)
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(15))
        .build();
    let mut last_error = String::new();
    for attempt in 1..=3 {
        check_cancelled()?;
        let _ = app_handle.emit("model-download-progress", (label, 0));
        match download_attempt(&agent, destination, label, app_handle, asset) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = format!("{} (intento {}/3)", e, attempt),
        }
        check_cancelled()?;
    }
    Err(last_error)
}

fn download_attempt(
    agent: &ureq::Agent,
    destination: &std::path::Path,
    label: &str,
    app_handle: &tauri::AppHandle,
    asset: &ModelAsset,
) -> Result<(), String> {
    let resp = agent.get(&asset.url).call().map_err(|e| {
        format!(
            "No se pudo descargar el modelo. Comprueba tu conexión: {}",
            e
        )
    })?;
    let mut temp = tempfile::Builder::new()
        .prefix("bluedoor-model-")
        .suffix(".part")
        .tempfile_in(
            destination
                .parent()
                .ok_or("Carpeta de modelos no disponible")?,
        )
        .map_err(|e| {
            format!(
                "No se pudo preparar la descarga. Comprueba el espacio disponible: {}",
                e
            )
        })?;
    let mut reader = resp.into_reader();
    let mut hash = Sha256::new();
    let mut received = 0u64;
    let mut buf = [0u8; 128 * 1024];
    loop {
        check_cancelled()?;
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Descarga interrumpida: {}", e))?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > asset.size {
            return Err("La descarga no coincide con el modelo esperado.".into());
        }
        temp.write_all(&buf[..n]).map_err(|e| {
            format!(
                "No se pudo guardar el modelo. Comprueba el espacio disponible: {}",
                e
            )
        })?;
        hash.update(&buf[..n]);
        let pct = (received.saturating_mul(100) / asset.size).min(99) as u32;
        let _ = app_handle.emit("model-download-progress", (label, pct));
    }
    check_cancelled()?;
    if received != asset.size || format!("{:x}", hash.finalize()) != asset.sha256 {
        return Err("La descarga está incompleta o dañada; se volverá a intentar.".into());
    }
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("No se pudo guardar el modelo: {}", e))?;
    temp.persist(destination)
        .map_err(|e| format!("No se pudo finalizar la descarga: {}", e))?;
    let _ = app_handle.emit("model-download-progress", (label, 100));
    Ok(())
}

#[cfg(test)]
mod portability_tests {
    use super::*;
    /// Toda entrada del catálogo debe tener su asset en el manifest, con su
    /// hash. Si faltara, la descarga fallaría en tiempo de ejecución.
    #[test]
    fn every_catalog_entry_has_a_manifest_asset() {
        for (_, _, _, file) in WHISPER_MODELS {
            assert!(
                asset_for_path(std::path::Path::new(file)).is_some(),
                "Whisper sin manifest: {}",
                file
            );
        }
        for (_, _, _, file) in PARAKEET_MODELS.iter() {
            assert!(
                asset_for_path(std::path::Path::new(file)).is_some(),
                "Parakeet sin manifest: {}",
                file
            );
        }
        assert!(asset_for_path(std::path::Path::new(NEMOTRON_FILE)).is_some());
        assert!(asset_for_path(std::path::Path::new(QWEN_FILE)).is_some());
        assert!(asset_for_path(std::path::Path::new(QWEN_Q8_FILE)).is_some());
        // Y a la inversa: las claves del manifest son ids, y ninguno debe
        // quedar huerfano (Nemotron usa id propio, no el nombre del fichero).
        for id in model_assets().keys() {
            let known = WHISPER_MODELS.iter().any(|m| m.0 == id)
                || PARAKEET_MODELS.iter().any(|m| m.0 == id)
                || id == NEMOTRON_ID
                || id == QWEN_ID
                || id == QWEN_Q8_ID
                || id == QWEN_PII_ID
                || id == GLINER_ID
                || id == GLINER_TOKENIZER_ID
                || id == GLINER_MULTI_ID
                || id == GLINER_MULTI_TOKENIZER_ID;
            assert!(
                known,
                "asset en el manifest que no esta en el catalogo: {}",
                id
            );
        }
    }

    #[test]
    fn catalog_has_pinned_integrity_metadata() {
        for (_, _, _, file) in WHISPER_MODELS {
            let a = asset_for_path(std::path::Path::new(file)).unwrap();
            assert!(a.url.starts_with("https://huggingface.co/"));
            assert!(!a.url.contains("/main/"));
            assert_eq!(a.sha256.len(), 64);
            assert!(a.size > 1_000_000);
        }
        // Parakeet se sirve desde un repo propio, pero también pineado por commit.
        for (_, _, _, file) in PARAKEET_MODELS.iter() {
            let a = asset_for_path(std::path::Path::new(file)).unwrap();
            assert!(a.url.starts_with("https://huggingface.co/"));
            assert!(!a.url.contains("/main/"), "URL sin pinear: {}", a.url);
            assert_eq!(a.sha256.len(), 64);
            assert!(a.size > 1_000_000);
        }
        let parakeet = asset_for_path(std::path::Path::new(
            parakeet_file("parakeet-tdt-0.6b-v3").unwrap(),
        ))
        .unwrap();
        assert_eq!(
            parakeet.sha256,
            "5859f77944efcd8eafa23a6350731960b2b55b2203df51f319665c807d802cc7"
        );
        assert_eq!(parakeet.size, 739_508_576);
        let qwen = asset_for_path(std::path::Path::new(QWEN_FILE)).unwrap();
        assert_eq!(qwen.size, 1_359_279_776);
        assert_eq!(
            qwen.sha256,
            "3d0b790534fe4b79525fc3692950408dca41171676ed7e21db57af5c65ef6ab6"
        );
        assert!(qwen.url.ends_with(&qwen.sha256));
        if let Some(trained) = asset_for_path(std::path::Path::new(QWEN_PII_FILE)) {
            assert!(
                trained.url.is_empty(),
                "Local training has no remote distribution endpoint"
            );
            assert_eq!(trained.sha256.len(), 64);
            assert!(trained.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(trained.size > 1_000_000);
            assert_eq!(qwen_file(QWEN_PII_ID).unwrap(), QWEN_PII_FILE);
        }
        let nemotron = asset_for_path(std::path::Path::new(NEMOTRON_FILE)).unwrap();
        assert_eq!(
            nemotron.sha256,
            "08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1"
        );
    }
    #[test]
    fn verifies_content_not_just_length() {
        let a = ModelAsset {
            url: String::new(),
            size: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        };
        assert!(verify_reader(&b"abc"[..], &a).is_ok());
        assert!(verify_reader(&b"abd"[..], &a).is_err());
        assert!(verify_reader(&b"ab"[..], &a).is_err());
    }
}

#[tauri::command]
pub(crate) async fn download_privacy_model(
    id: String,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    if let Some((model_file, tokenizer_file, tokenizer_id)) = gliner_files(&id) {
        return tauri::async_runtime::spawn_blocking(move || {
            let model = models_dir(&app_handle).join(model_file);
            download_file(&app_handle, &model, &id)?;
            check_cancelled()?;
            download_file(
                &app_handle,
                &models_dir(&app_handle).join(tokenizer_file),
                tokenizer_id,
            )?;
            Ok(model.to_string_lossy().into_owned())
        })
        .await
        .map_err(|_| "La descarga se interrumpió.")?;
    }
    if id == QWEN_PII_ID {
        return Err(
            "Este modelo se entrenó en este equipo. Restaura su copia local para reinstalarlo."
                .into(),
        );
    }
    let destination = models_dir(&app_handle).join(qwen_file(&id)?);
    tauri::async_runtime::spawn_blocking(move || {
        download_file(&app_handle, &destination, &id)?;
        Ok(destination.to_string_lossy().to_string())
    })
    .await
    .map_err(|_| "Descarga local interrumpida")?
}

#[tauri::command]
pub(crate) fn delete_privacy_model(id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let state = app_handle.state::<crate::llm::LlmState>();
    let mut cache = state
        .0
        .try_lock()
        .map_err(|_| "Termina el análisis local antes de borrar Qwen3")?;
    *cache = None;
    #[cfg(target_os = "android")]
    if let Some((model_file, tokenizer_file, _)) = gliner_files(&id) {
        crate::android::unload_gliner()?;
        for file in [model_file, tokenizer_file] {
            let path = models_dir(&app_handle).join(file);
            if path.exists() {
                std::fs::remove_file(path).map_err(|_| "No se pudo eliminar GLiNER.")?;
            }
        }
        return Ok(());
    }
    let path = models_dir(&app_handle).join(qwen_file(&id)?);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("No se pudo eliminar Qwen3: {e}"))?;
    }
    Ok(())
}
