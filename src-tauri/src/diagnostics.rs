use tauri::Manager;

/// Trigger OS-owned folder consent before downloads. Probe only access: do not
/// import documents or leave files behind. Microphone consent lives in WebView.
#[tauri::command]
pub(crate) async fn prepare_local_setup(app_handle: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        check_installation(app_handle.clone())?;
        let models = crate::paths::models_dir(&app_handle);
        probe_writable_directory(&models)?;
        #[cfg(target_os = "macos")]
        {
            // These are the standard import/export locations. A save/open panel
            // still authorizes the specific file selected by the user later.
            for directory in [
                app_handle.path().document_dir(),
                app_handle.path().desktop_dir(),
                app_handle.path().download_dir(),
            ] {
                let directory = directory.map_err(|e| e.to_string())?;
                if directory.try_exists().map_err(|_| {
                    format!(
                        "No se pudo acceder a {}. Revisa los permisos de Archivos y carpetas.",
                        directory.display()
                    )
                })? {
                    probe_writable_directory(&directory)?;
                }
            }
            let ocr = app_handle
                .path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("runtime/bluedoor-ocr");
            if !ocr.is_file() {
                return Err("Falta el componente de análisis OCR. Reinstala BlueDoor.".into());
            }
        }
        crate::platform::load_owned_backends()?;
        crate::pdf::load_font_family("system")?;
        Ok(())
    })
    .await
    .map_err(|e| format!("No se pudo preparar la aplicación: {e}"))?
}

fn probe_writable_directory(directory: &std::path::Path) -> Result<(), String> {
    let denied = || {
        format!(
        "No se pudo leer o escribir en {}. Permite el acceso en Ajustes del Sistema → Privacidad y seguridad → Archivos y carpetas y vuelve a intentarlo.",
        directory.display()
    )
    };
    let mut entries = std::fs::read_dir(directory).map_err(|_| denied())?;
    if let Some(entry) = entries.next() {
        entry.map_err(|_| denied())?;
    }
    let _probe = tempfile::Builder::new()
        .prefix(".bluedoor-setup-")
        .tempfile_in(directory)
        .map_err(|_| denied())?;
    Ok(())
}

#[cfg(test)]
mod setup_tests {
    #[test]
    fn access_probe_leaves_no_files_and_preserves_documents() {
        let directory = tempfile::tempdir().unwrap();
        let document = directory.path().join("document.txt");
        std::fs::write(&document, "private text").unwrap();
        super::probe_writable_directory(directory.path()).unwrap();
        assert_eq!(std::fs::read_to_string(document).unwrap(), "private text");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn access_probe_rejects_a_missing_directory() {
        let directory = tempfile::tempdir().unwrap();
        assert!(super::probe_writable_directory(&directory.path().join("missing")).is_err());
    }
}

/// Application-owned components are repaired by reinstalling the signed app;
/// downloadable models are checked separately and repaired by the model manager.
#[tauri::command]
pub(crate) fn check_installation(app_handle: tauri::AppHandle) -> Result<(), String> {
    let decoder = crate::platform::decoder_path(&app_handle)?;
    if !decoder.is_file() {
        return Err(
            "La instalación está incompleta. Reinstala BlueDoor; tus documentos se conservan."
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
            return Err("Un componente de audio no puede iniciarse. Reinstala BlueDoor.".into());
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
        .prefix(".bluedoor-write-check-")
        .tempfile_in(directory)
        .map_err(|_| {
            "No se pueden guardar documentos. Comprueba los permisos y el espacio disponible."
        })?;
    Ok(())
}
