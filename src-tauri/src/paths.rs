use tauri::Manager;

pub fn models_dir(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .expect("failed to get app data dir");
    let dir = data_dir.join("models");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn transcripts_file(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    app_handle
        .path()
        .app_data_dir()
        .expect("failed to get app data dir")
        .join("transcripts.json")
}

/// Directorio del Vault cifrado: `vault.key`, `index.enc` y `entries/`.
pub fn vault_dir(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    app_handle
        .path()
        .app_data_dir()
        .expect("failed to get app data dir")
        .join("vault")
}
