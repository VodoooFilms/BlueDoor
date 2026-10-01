use crate::paths::transcripts_file;
use crate::types::StoredTranscript;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

// Serializes in-process readers/writers. Multiple app instances still require
// a single-instance policy; a mutex is not an interprocess lock.
static STORE_LOCK: Mutex<()> = Mutex::new(());

fn read_transcripts(path: &Path) -> Result<Vec<StoredTranscript>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "No se pudo leer el historial; el archivo se conserva: {}",
                e
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!(
            "No se pudo abrir el historial; el archivo se conserva: {}",
            e
        )),
    }
}

/// Exclusive sibling temporary, restrictive permissions and atomic replacement.
/// Failed rendering/writing never touches the previous destination.
pub(crate) fn atomic_write(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File) -> Result<(), String>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = parent.join(format!(".belegal-local-demo-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&tmp)
            .map_err(|e| format!("No se pudo crear el archivo temporal: {}", e))?;
        write(&mut file)?;
        file.sync_all()
            .map_err(|e| format!("No se pudo sincronizar: {}", e))?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| format!("No se pudo guardar: {}", e))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(tmp);
    result
}

/// Upper bound per transcript, so the history cannot become a dumping ground.
const MAX_TRANSCRIPT_CHARS: usize = 2_000_000;

fn save_at(path: &Path, transcripts: &[StoredTranscript]) -> Result<(), String> {
    // Never replace corrupt/unreadable history, even if a caller ignored a
    // previous load failure. Recovery must be an explicit operation.
    read_transcripts(path)?;
    for transcript in transcripts {
        validate_storable(transcript)?;
    }
    let bytes = serde_json::to_vec_pretty(transcripts)
        .map_err(|e| format!("No se pudo serializar: {}", e))?;
    atomic_write(path, |file| {
        file.write_all(&bytes)
            .map_err(|e| format!("No se pudo escribir: {}", e))
    })
}

/// The history is a transcript store, not a document store. A private document
/// must never reach it: the session lives in memory and is discarded on close,
/// and persisting it would contradict what PRIVACY.md promises. The renderer
/// supplies this list, so the restriction is enforced here rather than trusted.
fn validate_storable(transcript: &StoredTranscript) -> Result<(), String> {
    const FORBIDDEN: [&str; 3] = ["Documento sanitizado", "Private Legal", "Private AI"];
    if FORBIDDEN
        .iter()
        .any(|marker| transcript.title.contains(marker))
    {
        return Err("El historial guarda transcripciones, no documentos privados.".into());
    }
    if transcript.text.len() > MAX_TRANSCRIPT_CHARS {
        return Err("La transcripción es demasiado larga para el historial.".into());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn load_transcripts(
    app_handle: tauri::AppHandle,
) -> Result<Vec<StoredTranscript>, String> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| "El historial no está disponible".to_string())?;
    read_transcripts(&transcripts_file(&app_handle))
}

#[tauri::command]
pub(crate) fn save_transcripts(
    app_handle: tauri::AppHandle,
    transcripts: Vec<StoredTranscript>,
) -> Result<(), String> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| "El historial no está disponible".to_string())?;
    save_at(&transcripts_file(&app_handle), &transcripts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_history_is_never_overwritten() {
        let path = std::env::temp_dir().join(format!("vtp-store-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"{broken").unwrap();
        assert!(read_transcripts(&path).is_err());
        assert!(save_at(&path, &[]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_history_is_empty_and_roundtrips_legacy_format() {
        let path = std::env::temp_dir().join(format!("vtp-store-{}.json", uuid::Uuid::new_v4()));
        assert!(read_transcripts(&path).unwrap().is_empty());
        let original = r#"[{"id":"1","title":"Español 日本語","text":"Original 123","segments":[{"start":0,"end":1,"text":"Original 123"}],"createdAt":"2026-09-09T00:00:00Z","duration":1,"language":"es","model":"small","audioSource":"recording","corrected":"Original 123.","correctedHtml":"<b>Original 123.</b>"}]"#;
        let records: Vec<StoredTranscript> = serde_json::from_str(original).unwrap();
        save_at(&path, &records).unwrap();
        assert_eq!(
            serde_json::to_value(read_transcripts(&path).unwrap()).unwrap(),
            serde_json::to_value(records).unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn speaker_labels_survive_history_roundtrip() {
        let path = std::env::temp_dir().join(format!("vtp-store-{}.json", uuid::Uuid::new_v4()));
        let original = r#"[{"id":"voices","title":"Reunión","text":"Hablante 1: Hola\nHablante 2: Buenas","segments":[{"start":0,"end":1,"text":"Hola","speaker":1},{"start":1,"end":2,"text":"Buenas","speaker":2}],"createdAt":"2026-09-24T00:00:00Z","duration":2,"language":"es","model":"large-v3-turbo","audioSource":"recording","speakerNames":{"1":"Ana","2":"Luis"}}]"#;
        let records: Vec<StoredTranscript> = serde_json::from_str(original).unwrap();
        save_at(&path, &records).unwrap();
        let restored = read_transcripts(&path).unwrap();
        assert_eq!(restored[0].segments[0].speaker, Some(1));
        assert_eq!(restored[0].segments[1].speaker, Some(2));
        assert_eq!(restored[0].text, records[0].text);
        let names = restored[0].speaker_names.as_ref().unwrap();
        assert_eq!(names.get("1").map(String::as_str), Some("Ana"));
        assert_eq!(names.get("2").map(String::as_str), Some("Luis"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_write_preserves_destination_and_cleans_temporary() {
        let dir = std::env::temp_dir().join(format!("vtp-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("existing.pdf");
        std::fs::write(&path, b"previous").unwrap();
        assert!(atomic_write(&path, |file| {
            file.write_all(b"partial").unwrap();
            Err("simulated rendering failure".into())
        })
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
