//! Android build: the transcription engine (transcribe.cpp/FFmpeg) is not
//! compiled into the mobile application. The commands stay registered with the
//! same names so the shared frontend bundle still runs, but they refuse work
//! instead of pretending to transcribe.
use crate::types::{AppState, TranscriptionResult, WhisperConfig};
use std::path::Path;
use tauri::State;

const UNAVAILABLE: &str = "La transcripción de audio no forma parte de BlueDoor para Android. Usa la protección de documentos.";

#[tauri::command]
pub(crate) async fn choose_audio_file(
    _app_handle: tauri::AppHandle,
    _state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    Err(UNAVAILABLE.into())
}

#[tauri::command]
pub(crate) async fn transcribe_file(
    _file_path: String,
    _app_handle: tauri::AppHandle,
    _config: WhisperConfig,
    _state: State<'_, AppState>,
) -> Result<TranscriptionResult, String> {
    Err(UNAVAILABLE.into())
}

#[tauri::command]
pub(crate) fn prepare_transcribe_blob(
    _config: WhisperConfig,
    _state: State<'_, AppState>,
) -> String {
    String::new()
}

#[tauri::command]
pub(crate) async fn transcribe_blob(
    _request: tauri::ipc::Request<'_>,
    _app_handle: tauri::AppHandle,
    _state: State<'_, AppState>,
) -> Result<TranscriptionResult, String> {
    Err(UNAVAILABLE.into())
}

pub(crate) fn cleanup_leftovers(_models_dir: &Path) {}
