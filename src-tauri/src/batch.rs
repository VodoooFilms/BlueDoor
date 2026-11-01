//! Explicit folder grants and process-local document queue. No private batch
//! content is persisted or accepted back from the renderer.
pub(crate) mod aliases;
use crate::privacy::{PrivacyState, Session};
use serde::Serialize;
#[cfg(not(target_os = "android"))]
use std::path::PathBuf;
use std::{path::Path, sync::Mutex};
use tauri::Manager;
#[cfg(not(target_os = "android"))]
use tauri_plugin_dialog::DialogExt;

const MAX_FILES: usize = 20;
const MAX_TOTAL: u64 = 50 * 1024 * 1024;
const MAX_FILE: u64 = 10 * 1024 * 1024;
const MAX_MEMORY: usize = 128 * 1024 * 1024;
#[derive(Default)]
pub(crate) struct BatchState(Mutex<Batch>);
#[derive(Default, Clone)]
struct Batch {
    id: String,
    items: Vec<Item>,
    active: Option<String>,
    processing: bool,
    started: bool,
    previous: Option<Session>,
    #[allow(dead_code)]
    previous_source: String,
    identities: Vec<aliases::Identity>,
    next_alias: std::collections::HashMap<crate::privacy::Kind, usize>,
}
#[derive(Clone)]
struct Item {
    id: String,
    name: String,
    source: String,
    status: String,
    error: String,
    session: Option<Session>,
    exported_revision: Option<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchView {
    id: String,
    items: Vec<ItemView>,
    active: Option<String>,
    processing: bool,
    started: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemView {
    id: String,
    file_name: String,
    status: String,
    error: String,
    session_id: Option<String>,
}
fn locked() -> String {
    "Lote no disponible.".into()
}
fn sync_current(app: &tauri::AppHandle, batch: &mut Batch) -> Result<(), String> {
    let current = crate::privacy::get_privacy_session(app.clone())?;
    remember_current(batch, current);
    Ok(())
}
fn remember_current(batch: &mut Batch, current: Option<Session>) {
    if let Some(item) = batch
        .items
        .iter_mut()
        .find(|i| Some(&i.id) == batch.active.as_ref())
    {
        if let Some(current) =
            current.filter(|s| item.session.as_ref().is_some_and(|old| old.id == s.id))
        {
            item.session = Some(current);
        }
    }
}
fn view(batch: &Batch) -> BatchView {
    BatchView {
        id: batch.id.clone(),
        active: batch.active.clone(),
        processing: batch.processing,
        started: batch.started,
        items: batch
            .items
            .iter()
            .map(|i| ItemView {
                id: i.id.clone(),
                file_name: i.name.clone(),
                status: if let Some(s) = &i.session {
                    if s.reviewed && i.exported_revision == Some(s.revision) {
                        "exported"
                    } else if s.reviewed {
                        "approved"
                    } else if s.analyzed {
                        "review"
                    } else {
                        &i.status
                    }
                } else {
                    &i.status
                }
                .into(),
                error: i.error.clone(),
                session_id: i.session.as_ref().map(|s| s.id.clone()),
            })
            .collect(),
    }
}
fn supported(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "txt" | "md" | "docx" | "pdf" | "png" | "jpg" | "jpeg" | "tif" | "tiff" | "heic"
            )
        })
}
fn session_bytes(session: &Session) -> usize {
    let layout = session.layout.as_ref().map_or(0, |l| {
        l.pages
            .iter()
            .map(|p| {
                p.image_base64.len()
                    + p.glyphs
                        .iter()
                        .map(|g| std::mem::size_of_val(g) + g.font_name.len())
                        .sum::<usize>()
            })
            .sum::<usize>()
    });
    layout + session.original.len() + session.sanitized.len()
}
fn validate_candidates(candidates: &[(String, String, u64)]) -> Result<(), String> {
    if candidates.is_empty() {
        return Err("No hay documentos compatibles en esta carpeta.".into());
    }
    if candidates.len() > MAX_FILES
        || candidates
            .iter()
            .fold(0u64, |total, i| total.saturating_add(i.2))
            > MAX_TOTAL
    {
        return Err(
            "El lote admite hasta 20 documentos y 50 MB en total. Elige una carpeta más pequeña."
                .into(),
        );
    }
    Ok(())
}
#[cfg(not(target_os = "android"))]
fn choose_folder(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|p| {
            p.into_path()
                .map_err(|_| "Selecciona una carpeta local.".into())
        })
        .transpose()
}
#[tauri::command]
pub(crate) async fn choose_private_batch(
    app: tauri::AppHandle,
) -> Result<Option<BatchView>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Do not replace an existing batch implicitly, including a prepared one.
        if !app
            .state::<BatchState>()
            .0
            .lock()
            .map_err(|_| locked())?
            .id
            .is_empty()
        {
            return Err("Cierra el lote actual antes de elegir otro.".into());
        }
        #[cfg(not(target_os = "android"))]
        let candidates = {
            let Some(folder) = choose_folder(&app)? else {
                return Ok(None);
            };
            let mut files = Vec::new();
            for entry in std::fs::read_dir(folder).map_err(|_| "No se pudo leer la carpeta.")? {
                let entry = entry.map_err(|_| "No se pudo leer la carpeta.")?;
                // Never recurse or follow symlinks outside the selected folder.
                if !entry
                    .file_type()
                    .map_err(|_| "No se pudo leer la carpeta.")?
                    .is_file()
                {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if !supported(&name) {
                    continue;
                }
                let size = entry
                    .metadata()
                    .map_err(|_| "No se pudo leer el documento.")?
                    .len();
                files.push((
                    name,
                    entry
                        .path()
                        .canonicalize()
                        .map_err(|_| "No se pudo abrir el documento.")?
                        .to_string_lossy()
                        .into_owned(),
                    size,
                ));
                if files.len() > MAX_FILES {
                    break;
                }
            }
            files
        };
        #[cfg(target_os = "android")]
        let candidates = {
            let Some(tree) = crate::android::batch_choose_tree(false)? else {
                return Ok(None);
            };
            let value = crate::android::batch_list_tree(&tree)?;
            let files: Vec<serde_json::Value> =
                serde_json::from_str(&value).map_err(|_| "No se pudo leer la carpeta.")?;
            files
                .into_iter()
                .filter_map(|f| {
                    let name = f["name"].as_str()?.to_owned();
                    supported(&name).then(|| {
                        (
                            name,
                            f["uri"].as_str().unwrap_or_default().to_owned(),
                            f["size"].as_u64().unwrap_or(MAX_FILE + 1),
                        )
                    })
                })
                .collect::<Vec<_>>()
        };
        validate_candidates(&candidates)?;
        let previous = crate::privacy::get_privacy_session(app.clone())?;
        #[cfg(target_os = "android")]
        let previous_source = crate::android::batch_source()?;
        #[cfg(not(target_os = "android"))]
        let previous_source = String::new();
        let batch_state = app.state::<BatchState>();
        let mut state = batch_state.0.lock().map_err(|_| locked())?;
        if !state.id.is_empty() {
            return Err("Cierra el lote actual antes de elegir otro.".into());
        }
        let mut candidates = candidates;
        candidates.sort_by_key(|c| c.0.to_lowercase());
        *state = Batch {
            id: uuid::Uuid::new_v4().to_string(),
            previous,
            previous_source,
            items: candidates
                .into_iter()
                .map(|(name, source, size)| Item {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    source,
                    status: if size == 0 || size > MAX_FILE {
                        "error"
                    } else {
                        "pending"
                    }
                    .into(),
                    error: if size == 0 || size > MAX_FILE {
                        "Selecciona un documento de hasta 10 MB."
                    } else {
                        ""
                    }
                    .into(),
                    session: None,
                    exported_revision: None,
                })
                .collect(),
            ..Default::default()
        };
        Ok(Some(view(&state)))
    })
    .await
    .map_err(|_| "La selección del lote se interrumpió.")?
}
#[tauri::command]
pub(crate) fn get_private_batch(app: tauri::AppHandle) -> Result<Option<BatchView>, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    if batch.id.is_empty() {
        return Ok(None);
    }
    if !batch.processing {
        sync_current(&app, &mut batch)?;
    }
    Ok(Some(view(&batch)))
}
#[tauri::command]
pub(crate) fn start_private_batch(
    app: tauri::AppHandle,
    id: String,
    selected: Vec<String>,
) -> Result<BatchView, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    if batch.id != id || batch.started || batch.processing {
        return Err(locked());
    }
    if selected.is_empty()
        || selected.iter().any(|id| {
            !batch
                .items
                .iter()
                .any(|i| i.id == *id && i.status == "pending")
        })
    {
        return Err("Selecciona al menos un documento compatible.".into());
    }
    batch.items.retain(|i| selected.contains(&i.id));
    batch.started = true;
    Ok(view(&batch))
}
struct ProcessingLease(tauri::AppHandle);
impl Drop for ProcessingLease {
    fn drop(&mut self) {
        if let Ok(mut batch) = self.0.state::<BatchState>().0.lock() {
            batch.processing = false;
        }
    }
}
#[tauri::command]
pub(crate) async fn process_private_batch_item(
    app: tauri::AppHandle,
    batch_id: String,
    item_id: String,
    model_id: String,
) -> Result<BatchView, String> {
    let source = {
        let state = app.state::<BatchState>();
        let mut batch = state.0.lock().map_err(|_| locked())?;
        if batch.id != batch_id || !batch.started || batch.processing {
            return Err(locked());
        }
        sync_current(&app, &mut batch)?;
        let item = batch
            .items
            .iter_mut()
            .find(|i| i.id == item_id && i.status == "pending")
            .ok_or_else(locked)?;
        item.status = "processing".into();
        let source = item.source.clone();
        batch.processing = true;
        batch.active = None;
        source
    };
    let lease = ProcessingLease(app.clone());
    let result: Result<Session, String> = async {
        #[cfg(target_os = "android")]
        let path = {
            let source = source.clone();
            tauri::async_runtime::spawn_blocking(move || crate::android::materialize_uri(&source))
                .await
                .map_err(|_| "La importación local se interrumpió.")??
        };
        #[cfg(not(target_os = "android"))]
        let path = {
            let path = PathBuf::from(&source);
            if !std::fs::symlink_metadata(&path).map_err(|_|"No se pudo abrir el documento.")?.file_type().is_file()
                || path.canonicalize().map_err(|_|"No se pudo abrir el documento.")? != path {
                return Err("El archivo cambió desde la selección de la carpeta.".into());
            }
            path
        };
        let authorized = app
            .state::<crate::types::AppState>()
            .authorize_document_path(&path)?;
        let imported = crate::documents::import_private_document(
            app.clone(),
            Some(authorized.to_string_lossy().into_owned()),
        )
        .await;
        #[cfg(target_os = "android")]
        {
            let _ = std::fs::remove_file(&path);
        }
        let mut session = imported?.ok_or("No se pudo abrir el documento.")?;
        #[cfg(target_os = "android")]
        {
            crate::android::remember_source(&source)?;
        }
        // Use the display name granted by the picker, never a cache UUID.
        {
            let state = app.state::<BatchState>();
            let batch = state.0.lock().map_err(|_| locked())?;
            let retained = batch.items.iter().filter_map(|i| i.session.as_ref()).map(session_bytes).sum::<usize>()
                + batch.previous.as_ref().map_or(0, session_bytes);
            if retained.saturating_add(session_bytes(&session)) > MAX_MEMORY {
                return Err("El lote alcanzó su límite de memoria. Revisa y exporta los documentos listos antes de iniciar otro lote.".into());
            }
            session.file_name = batch
                .items
                .iter()
                .find(|i| i.id == item_id)
                .ok_or_else(locked)?
                .name
                .clone();
        }
        *app.state::<PrivacyState>().0.lock().map_err(|_| locked())? = Some(session.clone());
        crate::privacy::analyze_private_document(
            session.id,
            session.revision,
            model_id,
            app.clone(),
        )
        .await
    }
    .await;
    {
        let state = app.state::<BatchState>();
        let mut batch = state.0.lock().map_err(|_| locked())?;
        let item = batch
            .items
            .iter_mut()
            .find(|i| i.id == item_id)
            .ok_or_else(locked)?;
        match result {
            Ok(session) => {
                item.session = Some(session);
                item.status = "review".into();
            }
            Err(error) => {
                item.status = "error".into();
                item.error = error;
            }
        }
        let mut staged = batch.clone();
        if let Some(index) = staged.items.iter().position(|i| i.id == item_id) {
            if let Some(session) = staged.items[index].session.clone() {
                match aliases::reconcile(&mut staged, &session) {
                    Ok(mapped) => {
                        staged.items[index].session = Some(mapped.clone());
                        *app.state::<PrivacyState>().0.lock().map_err(|_| locked())? = Some(mapped);
                        *batch = staged;
                    }
                    Err(error) => {
                        batch.items[index].session = None;
                        batch.items[index].status = "error".into();
                        batch.items[index].error = error;
                    }
                }
            }
        }
        batch.active = Some(item_id);
    }
    drop(lease);
    get_private_batch(app)?.ok_or_else(locked)
}
#[tauri::command]
pub(crate) fn select_private_batch_item(
    app: tauri::AppHandle,
    id: String,
) -> Result<Session, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    if batch.processing {
        return Err("Espera a que termine la detección del lote.".into());
    }
    sync_current(&app, &mut batch)?;
    let item = batch.items.iter().find(|i| i.id == id).ok_or_else(locked)?;
    let session = item
        .session
        .clone()
        .ok_or("Este documento todavía no está listo para revisar.")?;
    #[cfg(target_os = "android")]
    crate::android::remember_source(&item.source)?;
    *app.state::<PrivacyState>().0.lock().map_err(|_| locked())? = Some(session.clone());
    batch.active = Some(id);
    Ok(session)
}
#[tauri::command]
pub(crate) fn cancel_private_batch_pending(app: tauri::AppHandle) -> Result<BatchView, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    for item in &mut batch.items {
        if item.status == "pending" {
            item.status = "cancelled".into();
        }
    }
    Ok(view(&batch))
}
#[tauri::command]
pub(crate) fn close_private_batch(app: tauri::AppHandle) -> Result<Option<Session>, String> {
    let state = app.state::<BatchState>();
    let mut batch = state.0.lock().map_err(|_| locked())?;
    if batch.processing {
        return Err("Espera a que termine la detección del lote.".into());
    }
    // Match the single-document close boundary: clear model-side private
    // extraction caches as well as the queue's session snapshots.
    {
        let llm = app.state::<crate::llm::LlmState>();
        #[allow(unused_mut)]
        let mut cache = llm
            .0
            .try_lock()
            .map_err(|_| "Espera a que termine la detección del lote.")?;
        if let Some((_, engine)) = cache.as_ref() {
            engine.clear_extractions();
        }
        #[cfg(target_os = "android")]
        {
            *cache = None;
        }
    }
    let previous = batch.previous.clone();
    #[cfg(target_os = "android")]
    crate::android::remember_source(&batch.previous_source)?;
    *app.state::<PrivacyState>().0.lock().map_err(|_| locked())? = previous.clone();
    *batch = Batch::default();
    if previous.is_none() {
        crate::session_status::clear(&app)?;
    }
    Ok(previous)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportReport {
    exported: usize,
    failed: usize,
}
#[tauri::command]
pub(crate) async fn export_private_batch(
    app: tauri::AppHandle,
    lang: String,
) -> Result<Option<ExportReport>, String> {
    // Folder selection is native and per export. The renderer supplies no path.
    #[cfg(not(target_os = "android"))]
    let folder = {
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || choose_folder(&handle))
            .await
            .map_err(|_| "No se pudo abrir la carpeta.")??
    };
    #[cfg(target_os = "android")]
    let folder = tauri::async_runtime::spawn_blocking(|| crate::android::batch_choose_tree(true))
        .await
        .map_err(|_| "No se pudo abrir la carpeta.")??;
    let Some(folder) = folder else {
        return Ok(None);
    };
    let (sessions, active, source) = {
        let state = app.state::<BatchState>();
        let mut batch = state.0.lock().map_err(|_| locked())?;
        if batch.processing || !batch.started {
            return Err(locked());
        }
        sync_current(&app, &mut batch)?;
        let sessions = batch
            .items
            .iter()
            .filter_map(|i| {
                i.session
                    .as_ref()
                    .filter(|s| s.reviewed)
                    .map(|s| (i.id.clone(), s.clone()))
            })
            .collect::<Vec<_>>();
        if sessions.is_empty() {
            return Err("No hay documentos aprobados para exportar.".into());
        }
        let active = crate::privacy::get_privacy_session(app.clone())?;
        #[cfg(target_os = "android")]
        let source = crate::android::batch_source()?;
        #[cfg(not(target_os = "android"))]
        let source = String::new();
        batch.processing = true;
        (sessions, active, source)
    };
    let lease = ProcessingLease(app.clone());
    let mut report = ExportReport {
        exported: 0,
        failed: 0,
    };
    for (index, (item_id, session)) in sessions.into_iter().enumerate() {
        let result: Result<(), String> = async {
            *app.state::<PrivacyState>().0.lock().map_err(|_| locked())? = Some(session.clone());
            let name = format!(
                "Protected-{:02}-{}.pdf",
                index + 1,
                &uuid::Uuid::new_v4().to_string()[..8]
            );
            #[cfg(not(target_os = "android"))]
            let dest = folder.join(&name);
            #[cfg(target_os = "android")]
            let dest = app
                .path()
                .app_cache_dir()
                .map_err(|_| locked())?
                .join(&name);
            // Never overwrite an existing document, including an original.
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(|_| "No se pudo crear el PDF sin sobrescribir archivos.")?;
            let result: Result<(), String> = async {
                let authorized = app
                    .state::<crate::types::AppState>()
                    .authorize_pdf_path(&dest)?;
                let output = authorized.to_string_lossy().into_owned();
                if session.layout_available {
                    crate::layout::export_private_layout(
                        app.clone(),
                        session.id.clone(),
                        session.revision,
                        output,
                    )
                    .await?;
                } else {
                    crate::pdf::generate_pdf(
                        if lang == "en" {
                            "Protected document"
                        } else {
                            "Documento protegido"
                        }
                        .into(),
                        vec![],
                        lang.clone(),
                        "local".into(),
                        0.0,
                        output,
                        app.state::<crate::types::AppState>(),
                        Some(session.sanitized.clone()),
                        None,
                        None,
                        "system".into(),
                        12.0,
                        "left".into(),
                        Some(crate::pdf::PrivateExport {
                            id: session.id.clone(),
                            revision: session.revision,
                        }),
                        app.clone(),
                    )
                    .await?;
                }
                #[cfg(target_os = "android")]
                {
                    crate::android::batch_write_tree(&folder, &dest, &name)?;
                }
                Ok(())
            }
            .await;
            #[cfg(target_os = "android")]
            {
                let _ = std::fs::remove_file(&dest);
            }
            #[cfg(not(target_os = "android"))]
            if result.is_err() {
                let _ = std::fs::remove_file(&dest);
            }
            result?;
            let state = app.state::<BatchState>();
            let mut batch = state.0.lock().map_err(|_| locked())?;
            if let Some(item) = batch.items.iter_mut().find(|i| i.id == item_id) {
                item.exported_revision = Some(session.revision);
            }
            Ok(())
        }
        .await;
        if result.is_ok() {
            report.exported += 1;
        } else {
            report.failed += 1;
        }
    }
    *app.state::<PrivacyState>().0.lock().map_err(|_| locked())? = active;
    #[cfg(target_os = "android")]
    crate::android::remember_source(&source)?;
    #[cfg(not(target_os = "android"))]
    let _ = source;
    drop(lease);
    Ok(Some(report))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_limits_are_explicit_and_do_not_truncate() {
        assert!(validate_candidates(&[]).is_err());
        assert!(validate_candidates(&vec![("a.pdf".into(), "/a.pdf".into(), 1); 21]).is_err());
        assert!(validate_candidates(&[("a.pdf".into(), "/a.pdf".into(), MAX_TOTAL + 1)]).is_err());
        assert!(validate_candidates(&[("a.pdf".into(), "/a.pdf".into(), 1)]).is_ok());
    }
    #[test]
    fn states_never_infer_approval_from_detection() {
        let mut s = Session::new("a.txt".into(), "Original".into()).unwrap();
        s.analyzed = true;
        let mut batch = Batch::default();
        batch.items.push(Item {
            id: "a".into(),
            name: "a.txt".into(),
            source: "/a.txt".into(),
            status: "review".into(),
            error: String::new(),
            session: Some(s),
            exported_revision: None,
        });
        assert_eq!(view(&batch).items[0].status, "review");
        batch.items[0].session.as_mut().unwrap().reviewed = true;
        assert_eq!(view(&batch).items[0].status, "approved");
        batch.items[0].exported_revision = Some(0);
        assert_eq!(view(&batch).items[0].status, "exported");
        batch.items[0].session.as_mut().unwrap().reviewed = false;
        assert_eq!(view(&batch).items[0].status, "review");
    }
    #[test]
    fn edits_and_review_stay_attached_to_the_matching_native_session() {
        let one = Session::new("one.txt".into(), "One".into()).unwrap();
        let two = Session::new("two.txt".into(), "Two".into()).unwrap();
        let mut batch = Batch {
            active: Some("1".into()),
            ..Default::default()
        };
        for (id, session) in [("1", one.clone()), ("2", two.clone())] {
            batch.items.push(Item {
                id: id.into(),
                name: session.file_name.clone(),
                source: String::new(),
                status: "review".into(),
                error: String::new(),
                session: Some(session),
                exported_revision: None,
            });
        }
        let mut edited = one;
        edited.sanitized = "Corrected one".into();
        edited.reviewed = true;
        edited.revision = 5;
        remember_current(&mut batch, Some(edited));
        assert_eq!(
            batch.items[0].session.as_ref().unwrap().sanitized,
            "Corrected one"
        );
        assert_eq!(batch.items[1].session.as_ref().unwrap().original, "Two");
        assert!(!batch.items[1].session.as_ref().unwrap().reviewed);
        remember_current(&mut batch, Some(two)); // stale/unrelated active session cannot overwrite one
        assert_eq!(batch.items[0].session.as_ref().unwrap().revision, 5);
        assert!(batch.items[0].session.as_ref().unwrap().reviewed);
    }
    #[test]
    fn supported_files_exclude_executables_and_unknown_extensions() {
        assert!(supported("REPORT.PDF"));
        assert!(supported("notes.docx"));
        assert!(!supported("payload.exe"));
        assert!(!supported("file.pdf.exe"));
    }
}
