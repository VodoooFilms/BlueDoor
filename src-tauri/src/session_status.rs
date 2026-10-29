//! Durable session-presence marker. Its sole payload is one byte, never text,
//! filenames, entities or aliases. It exists only for mobile process recovery.
#[cfg(any(target_os = "android", test))]
fn marker_path(directory: &std::path::Path) -> std::path::PathBuf {
    directory.join(".private-session-active")
}
#[cfg(any(target_os = "android", test))]
fn write_marker(directory: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(directory).map_err(|_| "No se pudo guardar el estado de la sesión.")?;
    let mut file = std::fs::File::create(marker_path(directory))
        .map_err(|_| "No se pudo guardar el estado de la sesión.")?;
    file.write_all(b"1")
        .and_then(|_| file.sync_all())
        .map_err(|_| "No se pudo guardar el estado de la sesión.".into())
}
#[cfg(any(target_os = "android", test))]
fn take_marker(directory: &std::path::Path) -> Result<bool, String> {
    match std::fs::remove_file(marker_path(directory)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("No se pudo leer el estado de la sesión anterior.".into()),
    }
}
pub(crate) fn mark(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        write_marker(
            &app.path()
                .app_data_dir()
                .map_err(|_| "Datos privados no disponibles.")?,
        )?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
    Ok(())
}
pub(crate) fn clear(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        take_marker(
            &app.path()
                .app_data_dir()
                .map_err(|_| "Datos privados no disponibles.")?,
        )?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
    Ok(())
}
#[tauri::command]
pub(crate) fn interrupted_private_session(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let state = app.state::<crate::privacy::PrivacyState>();
        let session = state.0.lock().map_err(|_| "Sesión privada no disponible")?;
        if session.is_some() {
            return Ok(false);
        }
        take_marker(
            &app.path()
                .app_data_dir()
                .map_err(|_| "Datos privados no disponibles.")?,
        )
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(false)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marker_has_no_document_content_and_is_consumed_once() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!take_marker(directory.path()).unwrap());
        write_marker(directory.path()).unwrap();
        assert_eq!(std::fs::read(marker_path(directory.path())).unwrap(), b"1");
        assert!(take_marker(directory.path()).unwrap());
        assert!(!take_marker(directory.path()).unwrap());
    }
}
