//! Android build: speaker diarization depends on the desktop transcription
//! runtime and is not offered on mobile.
#[tauri::command]
pub(crate) fn diarization_available(_app_handle: tauri::AppHandle) -> bool {
    false
}
