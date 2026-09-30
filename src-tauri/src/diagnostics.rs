use tauri::Manager;

/// Application-owned components are repaired by reinstalling the signed app;
/// downloadable models are checked separately and repaired by the model manager.
#[tauri::command]
pub(crate) fn check_installation(app_handle: tauri::AppHandle) -> Result<(), String> {
    let decoder = crate::platform::decoder_path(&app_handle)?;
    if !decoder.is_file() {
        return Err(
            "La instalación está incompleta. Reinstala Belegal _ Local demo; tus documentos se conservan."
                .into(),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if decoder
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(
                "Un componente de audio no puede iniciarse. Reinstala Belegal _ Local demo.".into(),
            );
        }
    }
    let directory = app_handle
        .path()
        .app_data_dir()
        .map_err(|_| "No se pudo abrir el almacenamiento local")?;
    std::fs::create_dir_all(&directory).map_err(|_| {
        "No se pudo abrir el almacenamiento local. Comprueba los permisos y el espacio disponible."
    })?;
    let _probe = tempfile::Builder::new()
        .prefix(".belegal-local-demo-write-check-")
        .tempfile_in(directory)
        .map_err(|_| {
            "No se pueden guardar documentos. Comprueba los permisos y el espacio disponible."
        })?;
    Ok(())
}
