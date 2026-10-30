mod editor;
// Immutable page snapshots with UTF-8 source offsets. Pixel replacements are
// computed from the same entity map as text sanitization, never UI rectangles.
use crate::privacy::{Entity, PrivacyState, Session};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "android"))]
use std::process::{Command, Stdio};
use tauri::Manager;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Glyph {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub rect: [f64; 4],
    pub font_size: f64,
    pub font_name: String,
    pub color: [f64; 3],
    pub confidence: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page {
    pub width: f64,
    pub height: f64,
    pub image_base64: String,
    pub glyphs: Vec<Glyph>,
    pub source: String,
    #[serde(default)]
    pub qr_regions: Vec<[f64; 4]>,
    #[serde(default)]
    pub manual_regions: Vec<[f64; 4]>,
}
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DocumentLayout {
    #[serde(default)]
    pub edits: Vec<editor::TextEdit>,
    pub text: String,
    pub pages: Vec<Page>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Redaction {
    pub page: usize,
    pub entity_index: usize,
    pub rect: [f64; 4],
    pub alias: String,
    pub font_size: f64,
    pub font_name: String,
    pub color: [f64; 3],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutPreview {
    pub id: String,
    pub revision: u64,
    pub pages: Vec<Page>,
    pub redactions: Vec<Redaction>,
    pub estimated: bool,
    pub blocks: Vec<editor::TextBlock>,
    pub unlabeled_aliases: Vec<String>,
}
#[derive(Serialize)]
struct RenderPlan<'a> {
    pages: &'a [Page],
    redactions: &'a [Redaction],
    overlays: &'a [editor::RichOverlay],
}

pub(crate) fn executable(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        return Ok(PathBuf::from("android-layout"));
    }
    #[cfg(not(target_os = "android"))]
    {
        let bundled = app
            .path()
            .resource_dir()
            .map_err(|_| "Componente de formato no disponible.")?
            .join("runtime/bluedoor-layout");
        #[cfg(debug_assertions)]
        if !bundled.is_file() {
            let development =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/bluedoor-layout");
            if development.is_file() {
                return Ok(development);
            }
        }
        if !bundled.is_file() {
            return Err("Conservar el formato está disponible en macOS. Reinstala la app si falta el componente.".into());
        }
        Ok(bundled)
    }
}
#[cfg(not(target_os = "android"))]
pub(crate) fn extract(path: &Path, helper: &Path) -> Result<DocumentLayout, String> {
    // El helper lee la ruta como argumento posicional tras `--layout`: se
    // exige absoluta para que `-algo` nunca se lea como opción.
    crate::types::AppState::absolute_process_path(path)?;
    let output = Command::new(helper)
        .arg("--layout")
        .arg(path)
        .output()
        .map_err(|_| "No se pudo obtener el formato del documento.")?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap_or("No se pudo obtener el formato.")
            .chars()
            .take(500)
            .collect());
    }
    let document: DocumentLayout = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Coordenadas del documento no válidas.")?;
    validate_layout(&document)?;
    Ok(document)
}
#[cfg(target_os = "android")]
pub(crate) fn extract_android(path: &Path) -> Result<DocumentLayout, String> {
    let json =
        crate::android::layout_call("extractLayout", path.to_str().ok_or("Ruta no válida")?)?;
    let document: DocumentLayout =
        serde_json::from_str(&json).map_err(|_| "Formato Android no válido")?;
    validate_layout(&document)?;
    Ok(document)
}
#[cfg(target_os = "macos")]
pub(crate) fn named_entities(
    text: &str,
    helper: &Path,
) -> Result<Vec<crate::privacy::Detection>, String> {
    let mut child = Command::new(helper)
        .arg("--entities")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "No se pudo iniciar el detector local de nombres.")?;
    child
        .stdin
        .take()
        .ok_or("Detector local no disponible")?
        .write_all(text.as_bytes())
        .map_err(|_| "No se pudo leer el texto local.")?;
    let output = child
        .wait_with_output()
        .map_err(|_| "El detector local no terminó.")?;
    if !output.status.success() {
        return Err("No se pudo ejecutar el detector local de nombres.".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "Detecciones locales no válidas.".into())
}
#[cfg(any(target_os = "macos", target_os = "android"))]
pub(crate) fn fit_generated_aliases(
    document: &DocumentLayout,
    entities: &mut [Entity],
    helper: &Path,
) -> Result<(), String> {
    let areas = redactions(document, entities)?;
    let failed = alias_overflows(&areas, helper)?;
    for index in failed {
        let entity = entities.get(index).ok_or("Zona de alias no válida")?;
        let old = entity.alias.clone();
        let prefix = match entity.kind {
            crate::privacy::Kind::Person => "P",
            crate::privacy::Kind::Company => "C",
            crate::privacy::Kind::Id => "I",
            crate::privacy::Kind::Email => "E",
            crate::privacy::Kind::Phone => "T",
            crate::privacy::Kind::Address => "A",
            crate::privacy::Kind::Case => "X",
            crate::privacy::Kind::Other => "O",
        };
        if let Some((_, number)) = old.rsplit_once('_') {
            let compact = format!("{prefix}{number}");
            for member in entities.iter_mut().filter(|e| e.alias == old) {
                member.alias = compact.clone();
            }
        }
    }
    Ok(())
}
fn alias_overflows(areas: &[Redaction], helper: &Path) -> Result<Vec<usize>, String> {
    #[cfg(target_os = "android")]
    {
        let _ = helper;
        let result = crate::android::layout_call(
            "checkLayoutAliases",
            &serde_json::to_string(areas).map_err(|_| "Alias no válidos")?,
        )?;
        return serde_json::from_str(&result).map_err(|_| "Comprobación de alias no válida".into());
    }
    #[cfg(not(target_os = "android"))]
    {
        let mut child = Command::new(helper)
            .arg("--check-aliases")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "No se pudo comprobar el tamaño de los alias.")?;
        let bytes = serde_json::to_vec(areas).map_err(|_| "Zonas de alias no válidas")?;
        child
            .stdin
            .take()
            .ok_or("Comprobación de alias no disponible")?
            .write_all(&bytes)
            .map_err(|_| "No se pudo comprobar el tamaño de los alias.")?;
        let output = child
            .wait_with_output()
            .map_err(|_| "La comprobación de alias no terminó.")?;
        if !output.status.success() {
            return Err("No se pudo comprobar el tamaño de los alias.".into());
        }
        serde_json::from_slice(&output.stdout).map_err(|_| "Comprobación de alias no válida".into())
    }
}

fn validate_layout(document: &DocumentLayout) -> Result<(), String> {
    if document.text.is_empty()
        || document.text.len() > crate::privacy::MAX_DOCUMENT_BYTES
        || document.pages.is_empty()
        || document.pages.len() > 30
    {
        return Err("Documento fuera de los límites de formato.".into());
    }
    for page in &document.pages {
        if !page.width.is_finite()
            || !page.height.is_finite()
            || page.width <= 0.0
            || page.height <= 0.0
            || page.width > 20_000.0
            || page.height > 20_000.0
            || page.image_base64.is_empty()
        {
            return Err("Tamaño de página no válido.".into());
        }
        if page.qr_regions.len() > 1000 || page.qr_regions.iter().any(|r| !valid_rect(r, page)) {
            return Err("Coordenadas QR no válidas.".into());
        }
        for glyph in &page.glyphs {
            if glyph.start >= glyph.end
                || glyph.end > document.text.len()
                || !document.text.is_char_boundary(glyph.start)
                || !document.text.is_char_boundary(glyph.end)
                || !valid_rect(&glyph.rect, page)
                || !glyph.font_size.is_finite()
                || glyph.font_size <= 0.0
                || !glyph
                    .color
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            {
                return Err("Coordenadas del texto no válidas.".into());
            }
        }
    }
    Ok(())
}
fn valid_rect(r: &[f64; 4], page: &Page) -> bool {
    r.iter().all(|v| v.is_finite())
        && r[0] >= 0.0
        && r[1] >= 0.0
        && r[2] > 0.0
        && r[3] > 0.0
        && r[0] + r[2] <= page.width + 0.01
        && r[1] + r[3] <= page.height + 0.01
}

pub(crate) fn redactions(
    document: &DocumentLayout,
    entities: &[Entity],
) -> Result<Vec<Redaction>, String> {
    // Validation and longest-first precedence must match sanitize().
    crate::privacy::sanitize(&document.text, entities)?;
    let mut matches = vec![];
    for (index, entity) in entities.iter().enumerate() {
        for found in crate::privacy::matching_pattern(&entity.original)?.find_iter(&document.text) {
            if crate::privacy::boundary(&document.text, found.start(), found.end()) {
                matches.push((found.start(), found.end(), index));
            }
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut cursor = 0;
    let mut result = Vec::new();
    for (start, end, index) in matches {
        if start < cursor {
            continue;
        }
        cursor = end;
        let mut covered = vec![false; end - start];
        let mut occurrence: Vec<Redaction> = vec![];
        for (number, page) in document.pages.iter().enumerate() {
            let mut last_line = None;
            for glyph in page
                .glyphs
                .iter()
                .filter(|g| g.start < end && g.end > start)
            {
                for byte in glyph.start.max(start)..glyph.end.min(end) {
                    covered[byte - start] = true;
                }
                let r = glyph.rect;
                if let Some(previous) = occurrence.last_mut().filter(|previous| {
                    previous.page == number
                        && last_line == Some(glyph.line)
                        && ((previous.rect[1] + previous.rect[3] / 2.0) - (r[1] + r[3] / 2.0)).abs()
                            < r[3] * 0.6
                }) {
                    let x = previous.rect[0].min(r[0]);
                    let y = previous.rect[1].min(r[1]);
                    let right = (previous.rect[0] + previous.rect[2]).max(r[0] + r[2]);
                    let bottom = (previous.rect[1] + previous.rect[3]).max(r[1] + r[3]);
                    previous.rect = [x, y, right - x, bottom - y];
                } else {
                    occurrence.push(Redaction {
                        page: number,
                        entity_index: index,
                        rect: r,
                        alias: String::new(),
                        font_size: glyph.font_size,
                        font_name: glyph.font_name.clone(),
                        color: glyph.color,
                    });
                }
                last_line = Some(glyph.line);
            }
        }
        for (offset, character) in document.text[start..end].char_indices() {
            if !character.is_whitespace()
                && !(offset..offset + character.len_utf8()).all(|i| covered[i])
            {
                return Err("Un dato privado no tiene coordenadas completas. Revisa el OCR o usa la exportación de texto; no se exportará una cobertura parcial.".into());
            }
        }
        // A wrapped name can begin in a tiny cell/line fragment. Put the label
        // in its widest covered segment; every other segment is still erased.
        let label_index = occurrence
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.rect[2].total_cmp(&b.rect[2]))
            .map(|(index, _)| index)
            .ok_or("No se pudo localizar un dato privado en la página.")?;
        let first = occurrence
            .get_mut(label_index)
            .ok_or("No se pudo localizar un dato privado en la página.")?;
        first.alias = entities[index].alias.clone();
        if document.text[start..end].ends_with('.') {
            first.alias.push('.');
        }
        for mut area in occurrence {
            let page = &document.pages[area.page];
            // Cover antialiasing edges, clipped to the physical page.
            let right = (area.rect[0] + area.rect[2] + 0.7).min(page.width);
            let bottom = (area.rect[1] + area.rect[3] + 0.7).min(page.height);
            area.rect[0] = (area.rect[0] - 0.7).max(0.0);
            area.rect[1] = (area.rect[1] - 0.7).max(0.0);
            area.rect[2] = right - area.rect[0];
            area.rect[3] = bottom - area.rect[1];
            result.push(area);
        }
    }
    if result.len() > 4000 {
        return Err("Demasiadas zonas privadas en el documento.".into());
    }
    Ok(result)
}
fn snapshot(app: &tauri::AppHandle, id: &str, revision: u64) -> Result<Session, String> {
    app.state::<PrivacyState>()
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?
        .as_ref()
        .filter(|s| s.id == id && s.revision == revision)
        .cloned()
        .ok_or("La sesión cambió. Recarga la vista del documento.".into())
}
fn plan(session: &Session) -> Result<(DocumentLayout, Vec<Redaction>), String> {
    let document = session
        .layout
        .as_deref()
        .ok_or("Este documento no tiene formato conservable. Importa un PDF o una imagen.")?;
    if session.original != document.text {
        return Err(
            "La extracción cambió: vuelve a importar el documento para conservar su formato."
                .into(),
        );
    }
    if session.edited
        && document.edits.is_empty()
        && document.pages.iter().all(|p| p.manual_regions.is_empty())
    {
        return Err("Esta vista utiliza el mapa de alias. Aplica de nuevo el mapa para conservar el formato original.".into());
    }
    let mut active = document.clone();
    if !session.edited {
        active.edits.clear();
    }
    if !session.analyzed {
        for page in &mut active.pages {
            page.qr_regions.clear();
        }
    }
    Ok((
        active,
        if session.analyzed {
            redactions(document, &session.entities)?
        } else {
            vec![]
        },
    ))
}
#[cfg(test)]
fn render(
    helper: &Path,
    document: &DocumentLayout,
    redactions: &[Redaction],
    mode: &str,
) -> Result<Vec<u8>, String> {
    render_overlays(helper, document, redactions, &[], mode)
}
fn render_overlays(
    helper: &Path,
    document: &DocumentLayout,
    redactions: &[Redaction],
    overlays: &[editor::RichOverlay],
    mode: &str,
) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "android")]
    {
        use base64::Engine;
        let _ = helper;
        let payload = serde_json::json!({"mode": mode, "pages": document.pages, "redactions": redactions, "overlays": overlays}).to_string();
        let result = crate::android::layout_call("renderLayout", &payload)?;
        return if mode == "--export" {
            base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|_| "PDF Android no válido".into())
        } else {
            Ok(result.into_bytes())
        };
    }
    #[cfg(not(target_os = "android"))]
    {
        let mut child = Command::new(helper)
            .arg(mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "No se pudo preparar el documento protegido.")?;
        let payload = serde_json::to_vec(&RenderPlan {
            pages: &document.pages,
            redactions,
            overlays,
        })
        .map_err(|_| "No se pudo preparar el documento.")?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or("No se pudo preparar el documento.")?;
        // Concurrent write avoids pipe deadlock when a large preview is returned.
        let writer = std::thread::spawn(move || stdin.write_all(&payload));
        let output = child
            .wait_with_output()
            .map_err(|_| "La exportación se interrumpió.")?;
        writer
            .join()
            .map_err(|_| "La exportación se interrumpió.")?
            .map_err(|_| "No se pudo preparar el documento.")?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr)
                .lines()
                .last()
                .unwrap_or("No se pudo conservar el formato.")
                .chars()
                .take(500)
                .collect());
        }
        Ok(output.stdout)
    }
}

#[tauri::command]
pub(crate) async fn preview_private_layout(
    app: tauri::AppHandle,
    id: String,
    revision: u64,
) -> Result<LayoutPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = snapshot(&app, &id, revision)?;
        let (document, areas) = plan(&session)?;
        let (blocks, overlays, _) = editor::compiled(&document, &session.entities)?;
        let pages = if areas.is_empty()
            && overlays.is_empty()
            && document
                .pages
                .iter()
                .all(|p| p.qr_regions.is_empty() && p.manual_regions.is_empty())
        {
            document
                .pages
                .iter()
                .map(|p| Page {
                    glyphs: vec![],
                    ..p.clone()
                })
                .collect()
        } else {
            serde_json::from_slice(&render_overlays(
                &executable(&app)?,
                &document,
                &areas,
                &overlays,
                "--preview",
            )?)
            .map_err(|_| "Vista previa no válida.")?
        };
        snapshot(&app, &id, revision)?;
        Ok(LayoutPreview {
            id,
            revision,
            pages,
            redactions: areas,
            unlabeled_aliases: alias_overflows(
                &redactions(&document, &session.entities)?,
                &executable(&app)?,
            )?
            .into_iter()
            .filter_map(|index| session.entities.get(index).map(|e| e.alias.clone()))
            .collect(),
            blocks,
            estimated: document.pages.iter().any(|p| p.source == "ocr"),
        })
    })
    .await
    .map_err(|_| "La vista previa se interrumpió.")?
}
#[tauri::command]
pub(crate) async fn export_private_layout(
    app: tauri::AppHandle,
    id: String,
    revision: u64,
    dest_path: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = snapshot(&app, &id, revision)?; let (document, areas) = plan(&session)?;
        let private = crate::pdf::PrivateExport { id: id.clone(), revision };
        crate::privacy::gate_private_export(&app, Some(&private), &session.sanitized, None, "Protected", &[])?;
        let (_,overlays,pure_map) = editor::compiled(&document, &session.entities)?;
        if pure_map.split_whitespace().collect::<Vec<_>>() != session.sanitized.split_whitespace().collect::<Vec<_>>() { return Err("La versión revisada contiene cambios de texto. Aplica el mapa de nuevo para exportar el formato original.".into()); }
        let path = crate::pdf::validate_pdf_destination(Path::new(&dest_path))?;
        let path = app.state::<crate::types::AppState>().consume_pdf_path(&path)?;
        let bytes = render_overlays(&executable(&app)?, &document, &areas, &overlays, "--export")?;
        if !bytes.starts_with(b"%PDF-") { return Err("El componente no produjo un PDF válido.".into()); }
        let state = app.state::<PrivacyState>(); let guard = state.0.lock().map_err(|_| "Sesión privada no disponible")?;
        if !guard.as_ref().is_some_and(|s| s.id == id && s.revision == revision && s.reviewed) { return Err("La revisión cambió durante la exportación. Vuelve a revisarla.".into()); }
        crate::store::atomic_write(&path, |file| file.write_all(&bytes).map_err(|_| "No se pudo guardar el PDF.".into()))?;
        Ok(path.to_string_lossy().into_owned())
    }).await.map_err(|_| "La exportación se interrumpió.")?
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::privacy::Kind;
    fn helper() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/bluedoor-layout")
    }
    #[test]
    fn qr_codes_are_erased_in_preview_and_export_without_text_redactions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/layout-qr.pdf");
        let original = std::fs::read(&root).unwrap();
        let document = extract(&root, &helper()).unwrap();
        assert_eq!(document.pages.len(), 2);
        assert!(document.pages.iter().all(|p| p.qr_regions.len() == 1));
        let mut session = Session::new("fixture.pdf".into(), document.text.clone()).unwrap();
        session.layout = Some(document.clone().into());
        assert!(plan(&session)
            .unwrap()
            .0
            .pages
            .iter()
            .all(|p| p.qr_regions.is_empty()));
        session.analyzed = true;
        assert!(plan(&session)
            .unwrap()
            .0
            .pages
            .iter()
            .all(|p| p.qr_regions.len() == 1));
        let temp = tempfile::tempdir().unwrap();
        let bytes = render(&helper(), &document, &[], "--export").unwrap();
        let output = temp.path().join("protected.pdf");
        std::fs::write(&output, &bytes).unwrap();
        let exported = extract(&output, &helper()).unwrap();
        assert!(exported.pages.iter().all(|p| p.qr_regions.is_empty()));
        let preview: Vec<Page> =
            serde_json::from_slice(&render(&helper(), &document, &[], "--preview").unwrap())
                .unwrap();
        for (index, page) in preview.iter().enumerate() {
            assert_eq!(page.qr_regions, document.pages[index].qr_regions);
            // Re-encode the actual preview pixels to a PDF, with no second mask,
            // so scanning that file verifies the preview itself was erased.
            let preview_document = DocumentLayout {
                edits: vec![],
                text: document.text.clone(),
                pages: vec![Page {
                    qr_regions: vec![],
                    ..page.clone()
                }],
            };
            let file = temp.path().join(format!("preview-{index}.pdf"));
            std::fs::write(
                &file,
                render(&helper(), &preview_document, &[], "--export").unwrap(),
            )
            .unwrap();
            assert!(extract(&file, &helper()).unwrap().pages[0]
                .qr_regions
                .is_empty());
        }
        assert_eq!(std::fs::read(root).unwrap(), original);
    }
    #[test]
    #[ignore = "Requires BLUEDOOR_QA_LAYOUT and BLUEDOOR_QA_RESULT from local inference QA"]
    fn local_pdf_analysis_renders_all_pages() {
        let source = std::env::var("BLUEDOOR_QA_LAYOUT").unwrap();
        let result = std::env::var("BLUEDOOR_QA_RESULT").unwrap();
        let document: DocumentLayout =
            serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
        let qwen: Vec<Entity> = serde_json::from_slice(&std::fs::read(result).unwrap()).unwrap();
        let blocks = editor::blocks(&document, &qwen).unwrap();
        if let Ok(index) = std::env::var("BLUEDOOR_QA_EDIT_BLOCK") {
            let block = if index == "auto" {
                blocks
                    .iter()
                    .find(|b| b.html.contains("2016"))
                    .expect("QA edit target must exist")
            } else {
                &blocks[index.parse::<usize>().unwrap()]
            };
            assert!(
                block.html.contains("2016"),
                "QA edit target must contain the replacement"
            );
            let mut edited = document.clone();
            edited.edits.push(editor::TextEdit {
                block_id: block.id,
                html: block.html.replace("2016", "2016 di"),
                font: block.font.clone(),
                size: block.size,
                align: block.align.clone(),
                bounds: Some(block.bounds),
            });
            let (_, overlays, sanitized) = editor::compiled(&edited, &qwen).unwrap();
            let pages = render_overlays(
                &helper(),
                &edited,
                &redactions(&edited, &qwen).unwrap(),
                &overlays,
                "--preview",
            )
            .unwrap();
            assert!(sanitized.contains("2016 di"));
            eprintln!(
                "LOCAL EDIT QA small correction rendered=true pages={}",
                serde_json::from_slice::<Vec<Page>>(&pages).unwrap().len()
            );
        }
        let paragraphs = blocks
            .iter()
            .filter(|b| b.min_height > b.size * 2.2)
            .count();
        eprintln!(
            "LOCAL EDIT QA blocks={} multiline_paragraphs={paragraphs}",
            blocks.len()
        );
        let names = named_entities(&document.text, &helper()).unwrap();
        let mut map = crate::privacy::merge_detection_sources(
            &document.text,
            qwen.iter()
                .map(|e| crate::privacy::Detection {
                    kind: e.kind,
                    text: e.original.clone(),
                })
                .collect(),
            names,
        )
        .unwrap();
        fit_generated_aliases(&document, &mut map, &helper()).unwrap();
        let sanitized = crate::privacy::sanitize(&document.text, &map).unwrap();
        // External QA expectations stay outside source control, like the input.
        if let Ok(path) = std::env::var("BLUEDOOR_QA_EXPECTED_NAMES") {
            let names: Vec<String> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert!(!names.is_empty());
            for name in &names {
                let pattern = crate::privacy::matching_pattern(name).unwrap();
                assert!(
                    pattern.is_match(&document.text),
                    "expectation must occur in the source"
                );
                assert!(
                    !pattern.is_match(&sanitized),
                    "expected person must be protected"
                );
            }
            eprintln!(
                "LOCAL PDF QA expected_people={} all_covered=true",
                names.len()
            );
        }
        let areas = redactions(&document, &map).unwrap();
        let rendered = render(&helper(), &document, &areas, "--preview").unwrap();
        let pages: Vec<Page> = serde_json::from_slice(&rendered).unwrap();
        assert_eq!(pages.len(), document.pages.len());
        eprintln!(
            "LOCAL PDF RENDER pages={} entities={} redaction_areas={}",
            pages.len(),
            map.len(),
            areas.len()
        );
    }
    fn entity(kind: Kind, original: &str, alias: &str) -> Entity {
        Entity {
            kind,
            original: original.into(),
            alias: alias.into(),
            source: "review".into(),
        }
    }
    #[test]
    fn unprintable_alias_labels_still_erase_every_private_pixel() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/layout-document.pdf");
        let document = extract(&root, &helper()).unwrap();
        let map = vec![entity(
            Kind::Person,
            "María Pérez",
            &format!("PERSON_{}", "A".repeat(80)),
        )];
        let areas = redactions(&document, &map).unwrap();
        assert_eq!(alias_overflows(&areas, &helper()).unwrap(), vec![0]);
        let protected: Vec<Page> =
            serde_json::from_slice(&render(&helper(), &document, &areas, "--preview").unwrap())
                .unwrap();
        let mut blank = areas.clone();
        for area in &mut blank {
            area.alias.clear();
        }
        let erased: Vec<Page> =
            serde_json::from_slice(&render(&helper(), &document, &blank, "--preview").unwrap())
                .unwrap();
        for (a, b) in protected.iter().zip(erased.iter()) {
            assert_eq!(
                a.image_base64, b.image_base64,
                "oversized labels produce the fully erased pixels, never the original data"
            );
        }
        assert!(render(&helper(), &document, &areas, "--export")
            .unwrap()
            .starts_with(b"%PDF-"));
    }
    #[test]
    fn paragraph_edits_reject_graphics_inside_the_original_erase_region() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let path = root.join("tests/fixtures/layout-paragraph-graphics.pdf");
        let original = std::fs::read(&path).unwrap();
        let document = extract(&path, &helper()).unwrap();
        let blocks = editor::blocks(&document, &[]).unwrap();
        for page in 0..3 {
            let block = blocks
                .iter()
                .find(|b| b.page == page && b.html.contains("Primera"))
                .unwrap();
            assert!(block.html.contains("Tercera"));
            let mut edited = document.clone();
            edited.edits.push(editor::TextEdit {
                block_id: block.id,
                html: "<p>Párrafo corregido.</p>".into(),
                font: block.font.clone(),
                size: block.size,
                align: block.align.clone(),
                bounds: Some(block.rect),
            });
            let (_, overlays, _) = editor::compiled(&edited, &[]).unwrap();
            assert!(!overlays[0].erase_regions.is_empty());
            for mode in ["--preview", "--export"] {
                let error = render_overlays(&helper(), &edited, &[], &overlays, mode).unwrap_err();
                assert!(
                    error.contains("El párrafo contiene"),
                    "page={page}, mode={mode}: {error}"
                );
            }
        }
        assert_eq!(
            std::fs::read(path).unwrap(),
            original,
            "source PDF stays intact"
        );
    }
    #[test]
    fn justified_pdf_spacing_can_be_edited_and_saved_as_a_flowing_paragraph() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut document = extract(
            &root.join("tests/fixtures/layout-paragraphs.pdf"),
            &helper(),
        )
        .unwrap();
        // Model a PDF's physical justification without changing its original
        // pixels: extra whitespace is not represented by any physical glyph.
        let mut positions = std::collections::HashMap::new();
        let mut source = String::new();
        for (offset, character) in document.text.char_indices() {
            positions.insert(offset, source.len());
            if character == ' ' {
                source.push_str("\t\r \u{a0}");
            } else {
                source.push(character);
            }
        }
        positions.insert(document.text.len(), source.len());
        for page in &mut document.pages {
            for glyph in &mut page.glyphs {
                glyph.start = positions[&glyph.start];
                glyph.end = positions[&glyph.end];
            }
        }
        document.text = source;
        let map = vec![entity(Kind::Person, "María Pérez", "PERSON_01")];
        let block = editor::blocks(&document, &map).unwrap().remove(0);
        assert!(!block.html.contains(['\t', '\r', '\u{a0}']));
        document.edits.push(editor::TextEdit {
            block_id: block.id,
            html: block.html.replace("continúa", "continúa bien"),
            font: block.font,
            size: block.size,
            align: block.align,
            bounds: Some(block.bounds),
        });
        let (_, overlays, text) = editor::compiled(&document, &map).unwrap();
        assert!(text.contains("continúa bien"));
        assert!(!text.contains("María Pérez"));
        assert!(render_overlays(
            &helper(),
            &document,
            &redactions(&document, &map).unwrap(),
            &overlays,
            "--export"
        )
        .unwrap()
        .starts_with(b"%PDF-"));
    }
    #[test]
    fn paragraph_editing_reflows_lines_without_merging_tables_or_moving_other_content() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut document = extract(
            &root.join("tests/fixtures/layout-paragraphs.pdf"),
            &helper(),
        )
        .unwrap();
        let map = vec![entity(Kind::Person, "María Pérez", "PERSON_01")];
        let blocks = editor::blocks(&document, &map).unwrap();
        let paragraph = blocks
            .iter()
            .find(|b| b.html.contains("Este párrafo"))
            .unwrap();
        assert!(paragraph.html.contains("que continúa"));
        assert!(paragraph.html.contains("sin duplicar líneas"));
        assert!(paragraph.html.contains("PERSON_01"));
        assert!(!paragraph.html.contains("María Pérez"));
        assert!(!paragraph.html.contains("otro párrafo"));
        assert!(paragraph.min_height > 30.);
        for title in [
            "Cuenta origen",
            "Cuenta destino",
            "Primer elemento",
            "Segundo elemento",
        ] {
            let block = blocks.iter().find(|b| b.html.contains(title)).unwrap();
            assert!(
                block.min_height < 20.,
                "table cells and list items remain independent"
            );
        }
        let bounds = [
            paragraph.bounds[0],
            paragraph.bounds[1],
            380.,
            paragraph.bounds[3],
        ];
        document.edits.push(editor::TextEdit { block_id: paragraph.id, bounds:Some(bounds),
            html:format!("<div style=\"padding-left: 8px; padding-right: 12px; text-indent: 4px\">{}</div><div>Un párrafo nuevo dentro del mismo espacio disponible.</div>", "El párrafo corregido contiene PERSON_01 y palabras adicionales. ".repeat(5)),
            font:"Helvetica".into(),size:12.,align:"left".into() });
        let (after_blocks, overlays, text) = editor::compiled(&document, &map).unwrap();
        assert_eq!(after_blocks[paragraph.id].bounds, bounds);
        assert_eq!(overlays[0].paragraphs.len(), 2);
        assert!(text.contains("Un párrafo nuevo"));
        assert!(!text.contains("que continúa"));
        assert!(!text.contains("María Pérez"));
        let areas = redactions(&document, &map).unwrap();
        let preview =
            render_overlays(&helper(), &document, &areas, &overlays, "--preview").unwrap();
        let pages: Vec<Page> = serde_json::from_slice(&preview).unwrap();
        let before: Vec<Page> =
            serde_json::from_slice(&render(&helper(), &document, &areas, "--preview").unwrap())
                .unwrap();
        assert_eq!(pages[1].image_base64, before[1].image_base64);
        let directory = root.join("artifacts/layout-workflow");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("paragraph-edited.preview.json"), preview).unwrap();
        std::fs::write(
            directory.join("paragraph-before.preview.json"),
            serde_json::to_vec(&before).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("paragraph-blocks.json"),
            serde_json::to_vec(&blocks).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("paragraph-plan.json"),
            serde_json::to_vec(&RenderPlan {
                pages: &document.pages,
                redactions: &areas,
                overlays: &overlays,
            })
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("paragraph-edited.pdf"),
            render_overlays(&helper(), &document, &areas, &overlays, "--export").unwrap(),
        )
        .unwrap();
        document.edits[0].bounds = Some([bounds[0], bounds[1], 500., bounds[3]]);
        let (_, image_overlap, _) = editor::compiled(&document, &map).unwrap();
        assert!(
            render_overlays(&helper(), &document, &areas, &image_overlap, "--export")
                .unwrap_err()
                .contains("contenido existente"),
            "graphics without glyph coordinates must also stop growth"
        );
        document.edits[0].bounds = Some([bounds[0], bounds[1], bounds[2], paragraph.min_height]);
        let (_, tight, _) = editor::compiled(&document, &map).unwrap();
        assert!(
            render_overlays(&helper(), &document, &areas, &tight, "--export")
                .unwrap_err()
                .contains("no cabe")
        );
        document.edits[0].bounds =
            Some([bounds[0], bounds[1], bounds[2], paragraph.limits[3] + 30.]);
        assert!(
            editor::compiled(&document, &map).is_err(),
            "growth cannot invade the next paragraph"
        );
    }
    #[test]
    fn wider_text_area_is_rendered_and_exported_without_changing_other_pages() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut document =
            extract(&root.join("tests/fixtures/layout-document.pdf"), &helper()).unwrap();
        let title = editor::blocks(&document, &[]).unwrap().remove(0);
        document.edits.push(editor::TextEdit {
            block_id: title.id, html: "<div>Informe de prueba con una corrección más extensa que necesita espacio adicional en la página</div>".into(),
            font: "Helvetica".into(), size: 20.0, align: "left".into(), bounds: None,
        });
        let (_, overlays, _) = editor::compiled(&document, &[]).unwrap();
        assert!(
            render_overlays(&helper(), &document, &[], &overlays, "--export")
                .unwrap_err()
                .contains("no cabe")
        );
        let expanded = [title.bounds[0], title.bounds[1], 500.0, title.bounds[3]];
        document.edits[0].bounds = Some(expanded);
        let (blocks, overlays, _) = editor::compiled(&document, &[]).unwrap();
        assert_eq!(blocks[0].bounds, expanded);
        assert_eq!(overlays[0].erase_rect, title.rect);
        assert_eq!(overlays[0].rect, expanded);
        let preview: Vec<Page> = serde_json::from_slice(
            &render_overlays(&helper(), &document, &[], &overlays, "--preview").unwrap(),
        )
        .unwrap();
        let original: Vec<Page> = serde_json::from_slice(
            &render_overlays(&helper(), &document, &[], &[], "--preview").unwrap(),
        )
        .unwrap();
        assert_eq!(preview.len(), document.pages.len());
        assert_eq!(
            preview[1].image_base64, original[1].image_base64,
            "untouched page pixels remain identical"
        );
        let pdf = render_overlays(&helper(), &document, &[], &overlays, "--export").unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let output = root.join("artifacts/layout-workflow");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            output.join("resized-text.preview.json"),
            serde_json::to_vec(&preview).unwrap(),
        )
        .unwrap();
        std::fs::write(output.join("resized-text.pdf"), pdf).unwrap();
        document.edits[0].bounds = Some([
            title.bounds[0],
            title.bounds[1],
            document.pages[0].width,
            title.bounds[3],
        ]);
        assert!(
            editor::compiled(&document, &[]).is_err(),
            "cannot move text beyond the page"
        );
    }
    #[test]
    fn document_layout_workflow_preserves_pages_and_exports_modified_pixels() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let directory = root.join("artifacts/layout-workflow");
        std::fs::create_dir_all(&directory).unwrap();
        let documents = [
            (
                "layout-document.pdf",
                vec![
                    entity(Kind::Person, "María Pérez", "PERSON_01"),
                    entity(Kind::Email, "maria@example.com", "EMAIL_01"),
                    entity(Kind::Id, "0912345678", "ID_01"),
                ],
            ),
            (
                "private-scan.pdf",
                vec![
                    entity(Kind::Person, "Maria Perez", "PERSON_01"),
                    entity(Kind::Id, "0912345678", "ID_01"),
                ],
            ),
        ];
        for (name, map) in documents {
            let layout = extract(&root.join("tests/fixtures").join(name), &helper()).unwrap();
            let areas = redactions(&layout, &map).unwrap();
            assert!(!areas.is_empty());
            if name == "layout-document.pdf" {
                assert_eq!(layout.pages.len(), 2);
                assert!((layout.pages[0].width - 595.28).abs() < 0.01);
                assert!((layout.pages[1].width - 792.0).abs() < 0.01);
                assert_eq!(
                    areas.iter().filter(|a| a.entity_index == 0).count(),
                    3,
                    "all repeated names must be covered"
                );
                let name_area = areas.iter().find(|a| a.entity_index == 0).unwrap();
                assert!(
                    (name_area.rect[0] - 48.0).abs() < 1.0,
                    "independent reportlab x coordinate"
                );
                assert!(
                    (name_area.rect[1] - 109.0).abs() < 10.0,
                    "independent reportlab baseline"
                );
            } else {
                assert!(layout.pages.iter().all(|p| p.source == "ocr"));
            }
            let preview = render(&helper(), &layout, &areas, "--preview").unwrap();
            let pdf = render(&helper(), &layout, &areas, "--export").unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
            let pages: Vec<Page> = serde_json::from_slice(&preview).unwrap();
            assert_eq!(pages.len(), layout.pages.len());
            std::fs::write(directory.join(format!("{name}.preview.json")), preview).unwrap();
            std::fs::write(
                directory.join(format!("{name}.plan.json")),
                serde_json::to_vec(&RenderPlan {
                    pages: &layout.pages,
                    redactions: &areas,
                    overlays: &[],
                })
                .unwrap(),
            )
            .unwrap();
            std::fs::write(directory.join(format!("{name}.protected.pdf")), pdf).unwrap();
        }
    }
    #[test]
    fn rich_text_correction_preserves_pages_and_rebases_aliases() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut document =
            extract(&root.join("tests/fixtures/layout-document.pdf"), &helper()).unwrap();
        let map = vec![
            entity(Kind::Person, "María Pérez", "PERSON_01"),
            entity(Kind::Email, "maria@example.com", "EMAIL_01"),
            entity(Kind::Id, "0912345678", "ID_01"),
        ];
        let (blocks, _, text) = editor::compiled(&document, &map).unwrap();
        assert_eq!(
            text,
            crate::privacy::sanitize(&document.text, &map).unwrap()
        );
        let title = blocks
            .iter()
            .find(|b| b.html.contains("Informe de prueba"))
            .unwrap();
        document.edits.push(editor::TextEdit{bounds:None,block_id:title.id,html:"<div style=\"padding-left: 5px; padding-right: 3px; text-indent: 2px\"><span style=\"font-family: Georgia; font-size: 14px; color: #204080\"><b>Informe corregido</b></span></div>".into(),font:"georgia".into(),size:14.,align:"left".into()});
        let (_, overlays, text) = editor::compiled(&document, &map).unwrap();
        assert!(text.starts_with("Informe corregido\n"));
        assert!(!text.contains("Informe de prueba"));
        assert_eq!(overlays[0].paragraphs[0].left, 5.);
        assert_eq!(overlays[0].paragraphs[0].right, 3.);
        assert_eq!(overlays[0].paragraphs[0].first, 2.);
        assert!(overlays[0].paragraphs[0].runs[0].bold);
        assert_eq!(overlays[0].paragraphs[0].runs[0].font, "georgia");
        let areas = redactions(&document, &map).unwrap();
        let preview =
            render_overlays(&helper(), &document, &areas, &overlays, "--preview").unwrap();
        let pdf = render_overlays(&helper(), &document, &areas, &overlays, "--export").unwrap();
        let output = root.join("artifacts/layout-workflow");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("rich-edited.pdf"), pdf).unwrap();
        std::fs::write(output.join("rich-edited.preview.json"), preview).unwrap();
        std::fs::write(
            output.join("rich-edited.plan.json"),
            serde_json::to_vec(&RenderPlan {
                pages: &document.pages,
                redactions: &areas,
                overlays: &overlays,
            })
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            output.join("text-blocks.json"),
            serde_json::to_vec(&blocks).unwrap(),
        )
        .unwrap();
        // Saved formatting must survive a later global alias rename.
        let name = blocks
            .iter()
            .find(|b| b.html.contains("PERSON_01"))
            .unwrap();
        document.edits.push(editor::TextEdit {
            block_id: name.id,
            bounds: None,
            html: "<div><b>PER</b><span>SON_01</span></div>".into(),
            font: "Helvetica".into(),
            size: 10.,
            align: "left".into(),
        });
        let mut renamed = map.clone();
        renamed[0].alias = "CLIENTE_01".into();
        editor::rebase_aliases(&mut document, &map, &renamed).unwrap();
        let (_, _, text) = editor::compiled(&document, &renamed).unwrap();
        assert!(text.contains("CLIENTE_01"));
        assert!(!text.contains("PERSON_01"));
        assert!(text.starts_with("Informe corregido"));
        document.edits[0].html = format!("<div>{}</div>", "Demasiado texto ".repeat(500));
        let (_, overlays, _) = editor::compiled(&document, &renamed).unwrap();
        assert!(render_overlays(
            &helper(),
            &document,
            &redactions(&document, &renamed).unwrap(),
            &overlays,
            "--export"
        )
        .unwrap_err()
        .contains("no cabe"));
    }
    #[test]
    fn incomplete_coordinates_and_stale_or_edited_layouts_are_rejected() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/layout-document.pdf");
        let document = extract(&root, &helper()).unwrap();
        let map = vec![entity(Kind::Person, "María Pérez", "PERSON_01")];
        let mut damaged = document.clone();
        damaged.pages[0].glyphs.remove(18);
        // Remove the whole first occurrence's coordinate coverage, independently
        // of font/renderer behavior. Partial protection must not be exportable.
        let start = damaged.text.find("María Pérez").unwrap();
        damaged.pages[0]
            .glyphs
            .retain(|g| g.end <= start || g.start >= start + "María Pérez".len());
        assert!(redactions(&damaged, &map).is_err());
        let mut session = Session::new("document.pdf".into(), document.text.clone()).unwrap();
        session.layout = Some(std::sync::Arc::new(document));
        session.layout_available = true;
        assert!(plan(&session).is_ok());
        session.original.push('x');
        assert!(plan(&session).is_err());
        session.original = session.layout.as_ref().unwrap().text.clone();
        session.edited = true;
        assert!(plan(&session).is_err());
    }
}

#[tauri::command]
pub(crate) async fn save_private_layout_text(
    app: tauri::AppHandle,
    id: String,
    revision: u64,
    edit: editor::TextEdit,
) -> Result<Session, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = snapshot(&app, &id, revision)?;
        if !session.analyzed {
            return Err("Protege el documento antes de editarlo.".into());
        }
        let (document, areas) = plan(&session)?;
        let mut document = document.clone();
        document.edits.retain(|e| e.block_id != edit.block_id);
        document.edits.push(edit);
        let (_, overlays, text) = editor::compiled(&document, &session.entities)?;
        // Render first: overflow or invalid formatting must never commit a
        // correction that cannot be shown or exported in full.
        render_overlays(
            &executable(&app)?,
            &document,
            &areas,
            &overlays,
            "--preview",
        )?;
        let state = app.state::<PrivacyState>();
        let mut guard = state.0.lock().map_err(|_| "Sesión privada no disponible")?;
        let current = guard
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision)
            .ok_or("La edición está desactualizada.")?;
        current.layout = Some(std::sync::Arc::new(document));
        current.sanitized = text;
        current.edited = true;
        current.reviewed = false;
        current.warnings = crate::privacy::leak_warnings(&current.sanitized, &current.entities);
        current.revision += 1;
        Ok(current.clone())
    })
    .await
    .map_err(|_| "La corrección se interrumpió.")?
}

#[tauri::command]
pub(crate) async fn review_private_layout(
    app: tauri::AppHandle,
    id: String,
    revision: u64,
    entities: Vec<Entity>,
    confirm: bool,
) -> Result<Session, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let before = snapshot(&app, &id, revision)?;
        let (source, _) = plan(&before)?;
        let mut document = source.clone();
        editor::rebase_aliases(&mut document, &before.entities, &entities)?;
        let areas = redactions(&document, &entities)?;
        let (_, overlays, text) = editor::compiled(&document, &entities)?;
        render_overlays(
            &executable(&app)?,
            &document,
            &areas,
            &overlays,
            "--preview",
        )?;
        let next = crate::privacy::review_private_document(
            id.clone(),
            revision,
            entities,
            confirm,
            app.clone(),
        )?;
        if document.edits.is_empty() && document.pages.iter().all(|p| p.manual_regions.is_empty()) {
            return Ok(next);
        }
        let state = app.state::<PrivacyState>();
        let mut guard = state.0.lock().map_err(|_| "Sesión privada no disponible")?;
        let current = guard
            .as_mut()
            .filter(|s| s.id == id && s.revision == next.revision)
            .ok_or("La revisión cambió.")?;
        current.layout = Some(std::sync::Arc::new(document));
        current.sanitized = text;
        current.edited = true;
        current.warnings = crate::privacy::leak_warnings(&current.sanitized, &current.entities);
        current.reviewed = confirm && current.analysis_complete && current.warnings.is_empty();
        // Invalidate any preview that started between map review and the
        // atomic installation of the preserved rich corrections.
        current.revision += 1;
        Ok(current.clone())
    })
    .await
    .map_err(|_| "La revisión se interrumpió.")?
}

fn add_manual_region(
    document: &mut DocumentLayout,
    page: usize,
    rect: [f64; 4],
) -> Result<(), String> {
    let p = document.pages.get_mut(page).ok_or("Página no válida.")?;
    let [x, y, w, h] = rect;
    if rect.iter().any(|v| !v.is_finite())
        || x < 0.0
        || y < 0.0
        || w < 1.0
        || h < 1.0
        || x + w > p.width
        || y + h > p.height
        || p.manual_regions.len() >= 200
    {
        return Err(
            "La cobertura debe quedar dentro de la página y tener un tamaño válido.".into(),
        );
    }
    let mut bounds = rect;
    // Cover entire intersecting OCR words, never leave a clipped word visible.
    for g in &p.glyphs {
        if intersects(rect, g.rect) {
            let right = (bounds[0] + bounds[2]).max(g.rect[0] + g.rect[2]);
            let bottom = (bounds[1] + bounds[3]).max(g.rect[1] + g.rect[3]);
            bounds[0] = bounds[0].min(g.rect[0]);
            bounds[1] = bounds[1].min(g.rect[1]);
            bounds[2] = right - bounds[0];
            bounds[3] = bottom - bounds[1];
        }
    }
    p.manual_regions.push(bounds);
    Ok(())
}
fn intersects(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[0] + b[2] && a[0] + a[2] > b[0] && a[1] < b[1] + b[3] && a[1] + a[3] > b[1]
}
#[tauri::command]
pub(crate) async fn save_private_manual_cover(
    app: tauri::AppHandle,
    id: String,
    revision: u64,
    page: usize,
    rect: Option<[f64; 4]>,
) -> Result<Session, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !cfg!(target_os = "android") {
            return Err("La cobertura manual está disponible en Android.".into());
        }
        let session = snapshot(&app, &id, revision)?;
        if !session.analyzed {
            return Err("Analiza el documento antes de cubrirlo.".into());
        }
        let (source, areas) = plan(&session)?;
        let mut document = source.clone();
        if let Some(rect) = rect {
            add_manual_region(&mut document, page, rect)?;
        } else {
            document
                .pages
                .get_mut(page)
                .ok_or("Página no válida.")?
                .manual_regions
                .pop()
                .ok_or("No hay cobertura que deshacer en esta página.")?;
        }
        let (_, overlays, text) = editor::compiled(&document, &session.entities)?;
        render_overlays(
            &executable(&app)?,
            &document,
            &areas,
            &overlays,
            "--preview",
        )?;
        let state = app.state::<PrivacyState>();
        let mut guard = state.0.lock().map_err(|_| "Sesión privada no disponible")?;
        let current = guard
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision)
            .ok_or("La cobertura está desactualizada.")?;
        let edited = !document.edits.is_empty()
            || document.pages.iter().any(|p| !p.manual_regions.is_empty());
        current.layout = Some(std::sync::Arc::new(document));
        current.sanitized = text;
        current.reviewed = false;
        current.edited = edited;
        current.warnings = crate::privacy::leak_warnings(&current.sanitized, &current.entities);
        current.revision += 1;
        Ok(current.clone())
    })
    .await
    .map_err(|_| "La cobertura se interrumpió.")?
}

#[cfg(test)]
mod manual_cover_tests {
    use super::*;
    fn document() -> DocumentLayout {
        serde_json::from_value(serde_json::json!({"text":"Ana Perez\nSEGUNDO", "pages":[{"width":200,"height":200,"imageBase64":"", "source":"ocr", "glyphs":[
            {"start":0,"end":9,"line":0,"rect":[10,10,70,12],"fontSize":10,"fontName":"sans-serif","color":[0,0,0],"confidence":1},
            {"start":10,"end":17,"line":1,"rect":[10,30,60,12],"fontSize":10,"fontName":"sans-serif","color":[0,0,0],"confidence":1}
        ]}]})).unwrap()
    }
    #[test]
    fn covers_omitted_ocr_words_in_text_and_preserves_layout_plan() {
        let mut document = document();
        add_manual_region(&mut document, 0, [20., 32., 20., 4.]).unwrap();
        assert_eq!(document.pages[0].manual_regions[0], [10., 30., 60., 12.]);
        let (_, _, text) = editor::compiled(&document, &[]).unwrap();
        assert!(text.contains("Ana Perez"));
        assert!(!text.contains("SEGUNDO"));
        let mut session = Session::new("scan.pdf".into(), document.text.clone()).unwrap();
        session.layout = Some(std::sync::Arc::new(document));
        session.edited = true;
        session.analyzed = true;
        assert!(
            plan(&session).is_ok(),
            "manual-only edits must remain previewable/exportable"
        );
    }
    #[test]
    fn invalid_rectangles_never_commit_and_non_ocr_areas_work() {
        let mut document = document();
        for (page, rect) in [
            (1, [1., 1., 10., 10.]),
            (0, [-1., 1., 10., 10.]),
            (0, [1., 1., 0., 10.]),
            (0, [190., 1., 20., 10.]),
            (0, [1., f64::NAN, 10., 10.]),
        ] {
            assert!(add_manual_region(&mut document, page, rect).is_err());
            assert!(document.pages[0].manual_regions.is_empty());
        }
        add_manual_region(&mut document, 0, [100., 100., 20., 20.]).unwrap();
        assert_eq!(document.pages[0].manual_regions.len(), 1);
        assert_eq!(editor::compiled(&document, &[]).unwrap().2, document.text);
    }
}
