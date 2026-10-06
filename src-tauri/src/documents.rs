//! Bounded, local-only text extraction and on-device OCR. No network.
use crate::privacy::{PrivacyState, Session, MAX_DOCUMENT_BYTES};
use std::io::Read;
use std::path::Path;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_XML_BYTES: u64 = 4 * 1024 * 1024;

fn docx_text(bytes: &[u8]) -> Result<String, String> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "El DOCX no es válido.")?;
    let mut entry = archive
        .by_name("word/document.xml")
        .map_err(|_| "El DOCX no contiene texto principal.")?;
    if entry.size() > MAX_XML_BYTES {
        return Err("El DOCX supera el tamaño de extracción permitido.".into());
    }
    let mut xml = String::new();
    entry
        .by_ref()
        .take(MAX_XML_BYTES + 1)
        .read_to_string(&mut xml)
        .map_err(|_| "No se pudo leer el DOCX.")?;
    if xml.len() as u64 > MAX_XML_BYTES {
        return Err("El DOCX supera el tamaño de extracción permitido.".into());
    }
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut output = String::new();
    let mut in_text = false;
    loop {
        use quick_xml::events::Event;
        match reader
            .read_event()
            .map_err(|_| "El texto del DOCX está dañado.")?
        {
            Event::Start(e) if e.local_name().as_ref() == b"t" => in_text = true,
            Event::End(e) if e.local_name().as_ref() == b"t" => in_text = false,
            Event::End(e) if e.local_name().as_ref() == b"p" => output.push('\n'),
            Event::Empty(e) if e.local_name().as_ref() == b"tab" => output.push('\t'),
            Event::Empty(e) if matches!(e.local_name().as_ref(), b"br" | b"cr") => {
                output.push('\n')
            }
            Event::Text(e) if in_text => {
                output.push_str(&e.unescape().map_err(|_| "Texto XML no válido.")?)
            }
            Event::DocType(_) => return Err("No se admiten declaraciones externas en DOCX.".into()),
            Event::Eof => break,
            _ => (),
        }
        if output.len() > MAX_DOCUMENT_BYTES {
            return Err("El documento supera 64 KB de texto; no se truncará.".into());
        }
    }
    Ok(output)
}
fn ocr_text(path: &Path, executable: Option<&Path>) -> Result<String, String> {
    let executable = executable.ok_or(if cfg!(target_os = "macos") {
        "Falta el componente OCR local. Reinstala la aplicación."
    } else {
        "OCR local disponible en macOS para este demo. Usa un PDF con texto o TXT/DOCX en esta plataforma."
    })?;
    if !executable.is_file() {
        return Err("Falta el componente OCR local. Reinstala la aplicación.".into());
    }
    let output = std::process::Command::new(executable)
        .arg(path)
        .output()
        .map_err(|_| "No se pudo iniciar el OCR local.")?;
    if !output.status.success() {
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        return Err(diagnostic.trim().chars().take(500).collect());
    }
    let text = String::from_utf8(output.stdout).map_err(|_| "OCR devolvió texto no válido.")?;
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err("El texto OCR supera 64 KB; divide el documento.".into());
    }
    Ok(text)
}

#[cfg(target_os = "macos")]
fn ocr_executable(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let bundled = app.path().resource_dir().ok()?.join("runtime/belegal-ocr");
    if bundled.is_file() {
        return Some(bundled);
    }
    #[cfg(debug_assertions)]
    {
        let development =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/belegal-ocr");
        if development.is_file() {
            return Some(development);
        }
    }
    Some(bundled)
}

fn extract(path: &Path, ocr_executable: Option<&Path>) -> Result<String, String> {
    let meta = path
        .metadata()
        .map_err(|_| "No se pudo abrir el documento.")?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err("Selecciona un documento de hasta 10 MB.".into());
    }
    let mut bytes = vec![];
    std::fs::File::open(path)
        .map_err(|_| "No se pudo abrir el documento.")?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "No se pudo leer el documento.")?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("El documento supera 10 MB.".into());
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = match extension.as_str() {
        "txt" | "md" => String::from_utf8(bytes)
            .map_err(|_| "Guarda el texto como UTF-8 e impórtalo de nuevo.")?,
        "docx" => docx_text(&bytes)?,
        "pdf" if ocr_executable.is_some() => ocr_text(path, ocr_executable)?,
        "pdf" => {
            let text = pdf_extract::extract_text_from_mem(&bytes)
                .map_err(|_| "No se pudo extraer texto del PDF.")?;
            if text.trim().is_empty() {
                return Err("El PDF no contiene texto extraíble. OCR local disponible en macOS para este demo.".into());
            }
            text
        }
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "heic" => ocr_text(path, ocr_executable)?,
        _ => return Err("Formatos admitidos: TXT, MD, DOCX, PDF, PNG, JPG, TIFF y HEIC.".into()),
    };
    Ok(text.trim_start_matches('\u{feff}').replace("\r\n", "\n"))
}
#[tauri::command]
pub(crate) async fn import_private_document(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Result<Option<Session>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = if let Some(path) = path {
            std::path::PathBuf::from(path)
        } else {
            let selected = app
                .dialog()
                .file()
                .add_filter(
                    "Documentos locales",
                    &[
                        "txt", "md", "docx", "pdf", "png", "jpg", "jpeg", "tif", "tiff", "heic",
                    ],
                )
                .blocking_pick_file();
            let Some(selected) = selected else {
                return Ok(None);
            };
            selected
                .into_path()
                .map_err(|_| "Selecciona un documento local.")?
        };
        #[cfg(target_os = "macos")]
        let ocr = ocr_executable(&app);
        #[cfg(not(target_os = "macos"))]
        let ocr: Option<std::path::PathBuf> = None;
        #[cfg(target_os = "macos")]
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        #[cfg(target_os = "macos")]
        let layout = if matches!(
            extension.as_str(),
            "pdf" | "png" | "jpg" | "jpeg" | "tif" | "tiff" | "heic"
        ) {
            let metadata = path
                .metadata()
                .map_err(|_| "No se pudo abrir el documento.")?;
            if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
                return Err("Selecciona un documento de hasta 10 MB.".into());
            }
            Some(std::sync::Arc::new(crate::layout::extract(
                &path,
                &crate::layout::executable(&app)?,
            )?))
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        let layout: Option<std::sync::Arc<crate::layout::DocumentLayout>> = None;
        let text = if let Some(layout) = &layout {
            layout.text.clone()
        } else {
            extract(&path, ocr.as_deref())?
        };
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Documento local")
            .to_string();
        let mut session = Session::new(name, text)?;
        session.layout_available = layout.is_some();
        session.layout = layout;
        *app.state::<PrivacyState>()
            .0
            .lock()
            .map_err(|_| "Sesión privada no disponible")? = Some(session.clone());
        Ok(Some(session))
    })
    .await
    .map_err(|_| "La importación local se interrumpió.")?
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn extracts_docx_runs_paragraphs_and_xml_entities() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
        writer
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Ana &amp; </w:t></w:r><w:r><w:t>Pedro</w:t></w:r></w:p><w:p><w:r><w:t>0912345678</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert_eq!(docx_text(&bytes).unwrap(), "Ana & Pedro\n0912345678\n");
    }
    #[test]
    fn utf8_import_is_exact_and_invalid_or_large_files_fail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.txt");
        std::fs::write(&path, "María Pérez\r\nID 0912345678").unwrap();
        assert_eq!(extract(&path, None).unwrap(), "María Pérez\nID 0912345678");
        std::fs::write(&path, [0xff]).unwrap();
        assert!(extract(&path, None).is_err());
        std::fs::write(&path, vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert!(extract(&path, None).is_err());
        assert!(docx_text(b"not a zip").is_err());
    }
    #[test]
    fn text_pdf_is_extracted_locally_without_a_converter() {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
            "<< /Length 67 >>\nstream\nBT /F1 12 Tf 50 700 Td (Ana Perez ana@example.com 0912345678) Tj ET\nendstream",
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = vec![0];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, object));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in &offsets[1..] {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
        ));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.pdf");
        std::fs::write(&path, pdf).unwrap();
        let text = extract(&path, None).unwrap();
        assert!(text.contains("Ana Perez"));
        assert!(text.contains("ana@example.com"));
        assert!(text.contains("0912345678"));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn local_ocr_reads_image_and_scanned_pdf() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let helper = root.join("resources/runtime/belegal-ocr");
        for name in ["private-scan.png", "private-scan.pdf"] {
            let source = root.join("../tests/fixtures").join(name);
            let result = extract(&source, Some(&helper)).unwrap();
            assert!(result.contains("Maria Perez"), "OCR missed name in {name}");
            assert!(result.contains("0912345678"), "OCR missed ID in {name}");
            assert!(result.contains("USD 1500"), "OCR missed amount in {name}");
        }
        let scanned = root.join("../tests/fixtures/private-scan.pdf");
        assert!(extract(&scanned, None).is_err(), "scanned PDF needs OCR");
    }
}
