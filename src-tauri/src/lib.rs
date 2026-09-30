mod audio;
mod diagnostics;
mod diarization;
mod documents;
mod llm;
mod models;
mod paths;
mod pdf;
mod platform;
mod privacy;
mod store;
mod types;

use std::sync::Mutex;
use tauri::Manager;

use crate::types::{AppState, WhisperConfig};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        config: Mutex::new(WhisperConfig {
            model_path: String::new(),
            language: None,
            threads: 0, // 0 = automático (todos los cores)
            diarize: false,
        }),
        stt: Mutex::new(None),
        authorized_audio_paths: Mutex::new(std::collections::HashSet::new()),
        authorized_pdf_paths: Mutex::new(std::collections::HashSet::new()),
        pending_blob_configs: Mutex::new(std::collections::HashMap::new()),
    };

    tauri::Builder::default()
        // One process owns the private session and transcripts.json. Without
        // this, two instances race on the history file and hold two copies of
        // the private document in memory. Must be the first plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .manage(llm::LlmState::default())
        .manage(privacy::PrivacyState::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            if let Ok(executable) = std::env::current_exe() {
                if let Some(directory) = executable.parent() {
                    // llama.cpp scans the working directory for optional backends.
                    // Finder launches bundled apps with cwd=/, which can stall
                    // enumeration of system mounts before local inference starts.
                    if executable.to_string_lossy().contains(".app/Contents/MacOS/") {
                        std::env::set_current_dir(directory)?;
                    }
                }
            }
            let data_dir = app.path().app_data_dir()?;
            let models_dir = data_dir.join("models");
            // Report a writable-data error in the UI instead of aborting launch.
            let _ = std::fs::create_dir_all(&models_dir);
            audio::cleanup_leftovers(&models_dir);
            let state = app.state::<AppState>();
            let mut cfg = state.config.lock().unwrap();
            if cfg.model_path.trim().is_empty() {
                cfg.model_path = models_dir
                    .join(models::whisper_default_file())
                    .to_string_lossy()
                    .to_string();
            }
            drop(cfg);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            diagnostics::check_installation,
            diagnostics::prepare_local_setup,
            platform::open_server_home,
            documents::import_private_document,
            documents::paste_private_document,
            privacy::get_privacy_session,
            privacy::get_private_extraction,
            privacy::clear_privacy_session,
            privacy::reset_private_analysis,
            privacy::analyze_private_document,
            privacy::review_private_document,
            privacy::update_private_document,
            privacy::validate_private_export,
            privacy::suggest_private_ocr,
            privacy::apply_private_ocr,
            privacy::preview_private_outbound,
            models::download_privacy_model,
            models::delete_privacy_model,
            diarization::diarization_available,
            models::get_model_path,
            models::check_model_exists,
            models::download_model,
            models::cancel_model_download,
            models::list_models,
            models::download_diarization_model,
            models::delete_model,
            models::delete_diarization_model,
            models::download_parakeet_model,
            models::delete_parakeet_model,
            audio::choose_audio_file,
            audio::transcribe_file,
            audio::prepare_transcribe_blob,
            audio::transcribe_blob,
            pdf::choose_pdf_destination,
            pdf::generate_pdf,
            store::load_transcripts,
            store::save_transcripts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Belegal _ Local demo");
}
