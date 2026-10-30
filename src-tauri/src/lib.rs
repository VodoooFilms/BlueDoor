#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod audio;
#[cfg(target_os = "android")]
#[path = "audio_stub.rs"]
mod audio;
mod diagnostics;
#[cfg(not(target_os = "android"))]
mod diarization;
#[cfg(target_os = "android")]
#[path = "diarization_stub.rs"]
mod diarization;
mod documents;
#[cfg(any(target_os = "android", test))]
mod gliner;
mod layout;
mod llm;
mod models;
mod paths;
mod pdf;
mod performance;
mod platform;
mod privacy;
mod session_status;
mod store;
mod types;
mod vault;

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
        #[cfg(not(target_os = "android"))]
        stt: Mutex::new(None),
        authorized_audio_paths: Mutex::new(std::collections::HashSet::new()),
        authorized_pdf_paths: Mutex::new(std::collections::HashSet::new()),
        authorized_document_paths: Mutex::new(std::collections::HashSet::new()),
        pending_blob_configs: Mutex::new(std::collections::HashMap::new()),
    };

    let builder = tauri::Builder::default();
    #[cfg(not(target_os = "android"))]
    let builder = builder
        // One process owns the private session and transcripts.json. Without
        // this, two instances race on the history file and hold two copies of
        // the private document in memory. Must be the first plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            // La autorización ocurre en el hilo nativo, antes de que el evento
            // de drop llegue al renderer: una ruta solo es importable si el
            // sistema operativo la soltó en esta ventana.
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                let state = window.app_handle().state::<AppState>();
                for path in paths {
                    let _ = state.authorize_document_path(path);
                }
            }
        })
        .manage(app_state)
        .manage(llm::LlmState::default())
        .manage(privacy::PrivacyState::default())
        .manage(vault::VaultState::default())
        .setup(|app| {
            #[cfg(target_os = "android")]
            android::set_app(app.handle().clone());
            #[cfg(target_os = "macos")]
            if let Ok(executable) = std::env::current_exe() {
                if let Some(directory) = executable.parent() {
                    // llama.cpp scans the working directory for optional backends.
                    // Finder launches bundled apps with cwd=/, which can stall
                    // enumeration of system mounts before local inference starts.
                    if executable
                        .to_string_lossy()
                        .contains(".app/Contents/MacOS/")
                    {
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
            platform::app_platform,
            performance::get_local_performance,
            platform::prepare_share_export,
            platform::share_export,
            #[cfg(target_os = "android")]
            android::preview_import_source,
            #[cfg(target_os = "android")]
            android::private_source_page,
            #[cfg(target_os = "android")]
            android::cancel_private_import,
            platform::open_chatgpt_home,
            documents::import_private_document,
            documents::import_private_text,
            layout::preview_private_layout,
            layout::save_private_layout_text,
            layout::review_private_layout,
            layout::export_private_layout,
            privacy::get_privacy_session,
            session_status::interrupted_private_session,
            privacy::get_private_extraction,
            privacy::clear_privacy_session,
            privacy::reset_private_analysis,
            privacy::analyze_private_document,
            llm::cancel_private_analysis,
            llm::unload_privacy_model,
            llm::configure_mobile_analysis,
            privacy::review_private_document,
            privacy::update_private_document,
            privacy::update_private_source,
            privacy::validate_private_export,
            privacy::suggest_private_ocr,
            privacy::apply_private_ocr,
            privacy::preview_private_outbound,
            models::download_privacy_model,
            models::delete_privacy_model,
            #[cfg(not(target_os = "android"))]
            diarization::diarization_available,
            models::get_model_path,
            models::check_model_exists,
            #[cfg(not(target_os = "android"))]
            models::download_model,
            models::cancel_model_download,
            models::list_models,
            #[cfg(not(target_os = "android"))]
            models::download_diarization_model,
            models::delete_model,
            #[cfg(not(target_os = "android"))]
            models::delete_diarization_model,
            #[cfg(not(target_os = "android"))]
            models::download_parakeet_model,
            #[cfg(not(target_os = "android"))]
            models::delete_parakeet_model,
            #[cfg(not(target_os = "android"))]
            audio::choose_audio_file,
            #[cfg(not(target_os = "android"))]
            audio::transcribe_file,
            #[cfg(not(target_os = "android"))]
            audio::prepare_transcribe_blob,
            #[cfg(not(target_os = "android"))]
            audio::transcribe_blob,
            pdf::choose_pdf_destination,
            pdf::generate_pdf,
            store::load_transcripts,
            store::save_transcripts,
            vault::vault_status,
            vault::vault_create,
            vault::vault_unlock,
            vault::vault_unlock_recovery,
            vault::vault_lock,
            vault::vault_list,
            vault::vault_import,
            vault::vault_export_entry,
            vault::vault_delete_entry,
            vault::vault_backup,
            vault::vault_restore,
            vault::vault_migrate_transcripts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BlueDoor");
}
