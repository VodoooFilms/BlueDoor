use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use transcribe_cpp::{
    Model as SttModelHandle, Session as SttSession, SessionOptions as SttSessionOptions,
};

/// Número de hilos recomendado para esta máquina (usa todos los cores
/// disponibles, con tope para no saturar el sistema).
pub fn auto_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4)
        .clamp(2, 16)
}

/// Tope duro de hilos por sesión. `threads` llega del renderer: un valor
/// absurdo satura la máquina o hace fallar la creación de la sesión.
pub const MAX_THREADS: i32 = 64;

/// Número de hilos efectivo: `0` significa automático y cualquier valor
/// explícito se acota al rango soportado por el motor.
pub fn resolve_threads(threads: i32) -> i32 {
    if threads > 0 {
        threads.clamp(1, MAX_THREADS)
    } else {
        auto_threads()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<u8>,
}

/// Formato de documento (vista previa base + PDF): fuente, tamaño, alineación.
/// Los campos vienen del frontend. Todos con `default` para tolerar formatos
/// guardados por versiones anteriores (sin estos campos).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocFormat {
    pub font: String,
    pub size: f64,
    pub align: String,
}

/// Transcripción persistida en disco (solo local, en App Support).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTranscript {
    pub id: String,
    pub title: String,
    pub text: String,
    pub segments: Vec<Segment>,
    pub created_at: String,
    pub duration: f64,
    pub language: String,
    pub model: String,
    pub audio_source: String,
    #[serde(default)]
    pub file_name: Option<String>,
    /// Texto editado y guardado. Conservamos este nombre para leer historiales
    /// anteriores; los segmentos originales no se sustituyen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected: Option<String>,
    /// Contenido ricotexto del editor (HTML). El PDF usa `corrected` (plano).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected_html: Option<String>,
    /// Formato de documento del texto corregido (si se ha guardado).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<DocFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_names: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub id: String,
    pub text: String,
    pub segments: Vec<Segment>,
    pub language: String,
    pub duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperConfig {
    pub model_path: String,
    pub language: Option<String>,
    pub threads: usize,
    #[serde(default)]
    pub diarize: bool,
}

pub struct AppState {
    pub config: Mutex<WhisperConfig>,
    /// Caché de la sesión de transcripción (path -> sesión). Un único motor
    /// (transcribe.cpp) sirve tanto los Whisper `ggml-*.bin` como los Parakeet
    /// `.gguf`, así que solo hay una entrada. `Session::run` pide `&mut self`,
    /// de ahí el `Mutex` interior.
    pub stt: Mutex<Option<SttCacheEntry>>,
    pub authorized_audio_paths: Mutex<HashSet<PathBuf>>,
    pub authorized_pdf_paths: Mutex<HashSet<PathBuf>>,
    pub authorized_document_paths: Mutex<HashSet<PathBuf>>,
    pub(crate) pending_blob_configs: Mutex<HashMap<String, WhisperConfig>>,
}

fn canonical_existing_path(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|_| "No se pudo abrir el archivo seleccionado.".to_string())
}

fn canonical_destination_path(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .ok_or("Selecciona una ruta de destino válida.")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|_| "La carpeta de destino no existe.".to_string())?;
    if !parent.is_dir() {
        return Err("La carpeta de destino no es válida.".into());
    }
    Ok(parent.join(file_name))
}

/// Sesión de transcripción cacheada. Los hilos forman parte de la clave
/// porque viven en la sesión, no en cada ejecución.
pub struct SttCacheEntry {
    pub model_path: String,
    pub n_threads: i32,
    pub session: Arc<Mutex<SttSession>>,
}

impl SttCacheEntry {
    fn matches(&self, model_path: &str, n_threads: i32) -> bool {
        self.model_path == model_path && self.n_threads == n_threads
    }

    fn session(&self) -> Arc<Mutex<SttSession>> {
        self.session.clone()
    }
}

impl AppState {
    pub fn authorize_audio_path(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = canonical_existing_path(path)?;
        self.authorized_audio_paths
            .lock()
            .unwrap()
            .insert(canonical.clone());
        Ok(canonical)
    }

    pub fn consume_audio_path(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = canonical_existing_path(path)?;
        if self
            .authorized_audio_paths
            .lock()
            .unwrap()
            .remove(&canonical)
        {
            Ok(canonical)
        } else {
            Err("Selecciona el audio mediante el diálogo de BlueDoor.".into())
        }
    }

    pub fn authorize_pdf_path(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = canonical_destination_path(path)?;
        self.authorized_pdf_paths
            .lock()
            .unwrap()
            .insert(canonical.clone());
        Ok(canonical)
    }

    pub fn consume_pdf_path(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = canonical_destination_path(path)?;
        if self.authorized_pdf_paths.lock().unwrap().remove(&canonical) {
            Ok(canonical)
        } else {
            Err("Selecciona el destino mediante el diálogo de BlueDoor.".into())
        }
    }

    /// Los documentos importados llegan por drop nativo o por diálogo, nunca
    /// por una ruta que el renderer repita de otro sitio. El drop se autoriza
    /// en `DragDropEvent::Drop`, en el hilo nativo, antes de que el evento
    /// llegue al renderer; el import consume la entrada única.
    pub fn authorize_document_path(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = canonical_existing_path(path)?;
        self.authorized_document_paths
            .lock()
            .unwrap()
            .insert(canonical.clone());
        Ok(canonical)
    }

    pub fn consume_document_path(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = canonical_existing_path(path)?;
        if self
            .authorized_document_paths
            .lock()
            .unwrap()
            .remove(&canonical)
        {
            Ok(canonical)
        } else {
            Err(
                "Arrastra el documento a la ventana de BlueDoor o selecciónalo en el diálogo."
                    .into(),
            )
        }
    }

    pub fn store_blob_config(&self, config: WhisperConfig) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        self.pending_blob_configs
            .lock()
            .unwrap()
            .insert(token.clone(), config);
        token
    }

    pub fn consume_blob_config(&self, token: &str) -> Result<WhisperConfig, String> {
        self.pending_blob_configs
            .lock()
            .unwrap()
            .remove(token)
            .ok_or_else(|| "La configuración de audio no está disponible.".into())
    }

    /// Los helpers nativos reciben la ruta del usuario como argumento
    /// posicional; una ruta relativa que empiece por `-` se leería como opción
    /// del asistente. Diálogos y drop siempre entregan rutas absolutas, así
    /// que se exige el invariante antes de cada spawn.
    pub(crate) fn absolute_process_path(path: &Path) -> Result<(), String> {
        if path.is_absolute() {
            Ok(())
        } else {
            Err("Ruta local no válida.".into())
        }
    }

    pub fn resolve_model_path(&self, model_path: &str) -> String {
        if !model_path.trim().is_empty() {
            return model_path.to_string();
        }
        self.config.lock().unwrap().model_path.clone()
    }

    /// Completa la config que llega del frontend: ruta definitiva y diarización
    /// solo si el modelo la admite y el runtime externo está disponible.
    pub fn resolve_config(
        &self,
        config: &WhisperConfig,
        app_handle: tauri::AppHandle,
    ) -> WhisperConfig {
        let model_path = self.resolve_model_path(&config.model_path);
        // Nemotron decide los turnos por tiempo y cada intervalo se transcribe
        // por separado, asi que da igual que modelo de ASR sea: Parakeet
        // tambien admite diarizacion.
        let diarize = config.diarize && crate::diarization::diarization_available(app_handle);
        WhisperConfig {
            model_path,
            language: config.language.clone(),
            threads: config.threads,
            diarize,
        }
    }

    /// Devuelve (o carga y cachea) la sesión de transcripción del modelo
    /// indicado. Verifica el fichero contra el manifest antes de cargarlo.
    pub fn stt_session(
        &self,
        model_path: &str,
        threads: i32,
    ) -> Result<Arc<Mutex<SttSession>>, String> {
        let path = model_path.trim().to_string();
        if path.is_empty() || !Path::new(&path).exists() {
            return Err("No hay un modelo de transcripción descargado.".to_string());
        }
        // threads == 0 => automático, con el mismo tope que usaba whisper-rs.
        let n_threads = resolve_threads(threads);
        {
            let guard = self.stt.lock().unwrap();
            if let Some(entry) = guard.as_ref() {
                if entry.matches(&path, n_threads) {
                    return Ok(entry.session());
                }
            }
        }
        crate::models::verify_model_file(Path::new(&path))?;
        let model = SttModelHandle::load(&path)
            .map_err(|e| format!("No se pudo cargar el modelo: {}", e))?;
        let session = model
            .session_with(&SttSessionOptions {
                n_threads,
                ..Default::default()
            })
            .map_err(|e| format!("No se pudo iniciar el motor de transcripción: {}", e))?;
        let session = Arc::new(Mutex::new(session));
        let mut guard = self.stt.lock().unwrap();
        *guard = Some(SttCacheEntry {
            model_path: path,
            n_threads,
            session: session.clone(),
        });
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        AppState {
            config: Mutex::new(WhisperConfig {
                model_path: String::new(),
                language: None,
                threads: 0,
                diarize: false,
            }),
            stt: Mutex::new(None),
            authorized_audio_paths: Mutex::new(HashSet::new()),
            authorized_pdf_paths: Mutex::new(HashSet::new()),
            authorized_document_paths: Mutex::new(HashSet::new()),
            pending_blob_configs: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn legacy_config_without_diarize_still_deserializes() {
        let config: WhisperConfig =
            serde_json::from_str(r#"{"modelPath":"ggml-small.bin","language":"es","threads":4}"#)
                .unwrap();
        assert!(!config.diarize);
        assert_eq!(config.model_path, "ggml-small.bin");
    }

    #[test]
    fn explicit_thread_count_is_clamped_to_supported_range() {
        assert_eq!(resolve_threads(1), 1);
        assert_eq!(resolve_threads(8), 8);
        assert_eq!(resolve_threads(MAX_THREADS), MAX_THREADS);
        assert_eq!(resolve_threads(MAX_THREADS + 1), MAX_THREADS);
        assert_eq!(resolve_threads(i32::MAX), MAX_THREADS);
        // 0 (y negativos) significan automático, nunca "sin hilos".
        let auto = auto_threads();
        assert_eq!(resolve_threads(0), auto);
        assert_eq!(resolve_threads(-4), auto);
        assert!((2..=16).contains(&auto));
    }

    #[test]
    fn blob_config_is_single_use() {
        let state = test_state();
        let token = state.store_blob_config(WhisperConfig {
            model_path: "model.bin".into(),
            language: Some("es".into()),
            threads: 4,
            diarize: true,
        });
        let config = state.consume_blob_config(&token).unwrap();
        assert_eq!(config.model_path, "model.bin");
        assert_eq!(config.threads, 4);
        assert!(state.consume_blob_config(&token).is_err());
    }

    #[test]
    fn selected_paths_are_single_use() {
        let state = test_state();
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("recording.wav");
        let pdf = dir.path().join("transcript.pdf");
        std::fs::write(&audio, b"wav").unwrap();

        let audio = state.authorize_audio_path(&audio).unwrap();
        assert_eq!(state.consume_audio_path(&audio).unwrap(), audio);
        assert!(state.consume_audio_path(&audio).is_err());

        let pdf = state.authorize_pdf_path(&pdf).unwrap();
        assert_eq!(state.consume_pdf_path(&pdf).unwrap(), pdf);
        assert!(state.consume_pdf_path(&pdf).is_err());
    }

    #[test]
    fn dropped_document_paths_are_single_use() {
        let state = test_state();
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("escritura.pdf");
        std::fs::write(&document, b"pdf").unwrap();

        let canonical = state.authorize_document_path(&document).unwrap();
        assert_eq!(state.consume_document_path(&document).unwrap(), canonical);
        // Una entrada queda agotada: repetirla o adivinar otra ruta falla.
        assert!(state.consume_document_path(&document).is_err());
        let guessed = dir.path().join("no-soltado.pdf");
        std::fs::write(&guessed, b"pdf").unwrap();
        assert!(state.consume_document_path(&guessed).is_err());
    }

    #[test]
    fn helpers_only_receive_absolute_paths() {
        assert!(AppState::absolute_process_path(Path::new("-etc/expediente.pdf")).is_err());
        assert!(AppState::absolute_process_path(Path::new("escritura.pdf")).is_err());
        let absolute = std::env::temp_dir().join("escritura.pdf");
        assert!(AppState::absolute_process_path(&absolute).is_ok());
    }
}
