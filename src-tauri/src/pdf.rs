mod html;
pub(crate) use html::html_visible_text;
use html::parse_html_to_blocks;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::types::{AppState, Segment};

/// Identity of the reviewed private session a PDF export claims to carry. The
/// writer resolves it against Rust-owned state; the renderer cannot mint one.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrivateExport {
    pub id: String,
    pub revision: u64,
}

pub(crate) fn validate_pdf_destination(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .ok_or("Selecciona una ruta de destino válida.")?;
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if extension.as_deref() != Some("pdf") {
        return Err("El archivo de destino debe terminar en .pdf.".into());
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|_| "La carpeta de destino no existe.".to_string())?;
    if !parent.is_dir() {
        return Err("La carpeta de destino no es válida.".into());
    }
    Ok(parent.join(file_name))
}

#[tauri::command]
pub(crate) async fn choose_pdf_destination(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    default_name: String,
) -> Result<Option<String>, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app_handle
            .dialog()
            .file()
            .add_filter("PDF", &["pdf"])
            .set_file_name(default_name)
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("No se pudo abrir el selector de destino: {}", e))?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| "El destino no es una ruta local válida.".to_string())?;
    let path = validate_pdf_destination(&path)?;
    let path = state.authorize_pdf_path(&path)?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_pdf(
    title: String,
    segments: Vec<Segment>,
    language: String,
    model: String,
    duration: f64,
    dest_path: String,
    state: State<'_, AppState>,
    text: Option<String>,
    html: Option<String>,
    speaker_names: Option<HashMap<String, String>>,
    font: String,
    size: f64,
    align: String,
    private: Option<PrivateExport>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let destination = validate_pdf_destination(Path::new(&dest_path))?;
    let destination = state.consume_pdf_path(&destination)?;
    // A private document is gated here, in the writer, not in the renderer: the
    // payload must match the reviewed session and the map must be non-empty.
    // This runs on every export, not only when the renderer asks for it, so a
    // payload that simply omits the private session cannot slip through.
    crate::privacy::gate_private_export(
        &app,
        private.as_ref(),
        text.as_deref().unwrap_or_default(),
        html.as_deref(),
        &title,
        &[("el idioma", &language), ("el modelo", &model)],
    )?;
    generate_pdf_at_path(
        title,
        segments,
        language,
        model,
        duration,
        destination,
        text,
        html,
        speaker_names,
        font,
        size,
        align,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn generate_pdf_at_path(
    title: String,
    segments: Vec<Segment>,
    language: String,
    model: String,
    duration: f64,
    destination: PathBuf,
    text: Option<String>,
    html: Option<String>,
    speaker_names: Option<HashMap<String, String>>,
    font: String,
    size: f64,
    align: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use genpdf::Element as _; // métodos styled()/styled_string() sobre elementos

        let timestamp = chrono::Local::now().format("%d/%m/%Y · %H:%M");
        let duration_str = format!("{} min {} s", duration as u64 / 60, duration as u64 % 60);

        let font_family = load_font_family(&font)?;

        let mut doc = genpdf::Document::new(font_family);
        doc.set_title(&title);
        doc.set_paper_size(genpdf::PaperSize::A4);
        doc.set_line_spacing(1.4);
        doc.set_font_size(10);
        doc.set_minimal_conformance();
        doc.set_page_decorator(ReportDecorator::new(title.clone()));

        // Formato de documento elegido por el usuario (cuerpo del texto).
        let body_alignment = match align.as_str() {
            "center" => genpdf::Alignment::Center,
            "right" => genpdf::Alignment::Right,
            _ => genpdf::Alignment::Left,
        };
        let body_style = genpdf::style::Style::new().with_font_size(size.clamp(8.0, 72.0) as u8);

        // ==== Bloque de título ====
        doc.push(
            genpdf::elements::Paragraph::new(title.clone())
                .styled(genpdf::style::Style::new().bold().with_font_size(20)),
        );
        doc.push(genpdf::elements::Break::new(0.5));

        // ==== Metadatos: la información del documento ====
        let meta_style = genpdf::style::Style::new()
            .with_font_size(10)
            .with_color(genpdf::style::Color::Greyscale(90));
        doc.push(
            genpdf::elements::Paragraph::new(format!(
                "Duración: {}   ·   Idioma: {}   ·   Modelo: {}",
                duration_str,
                language.to_uppercase(),
                model
            ))
            .styled(meta_style),
        );
        doc.push(
            genpdf::elements::Paragraph::new(format!("Generado el {}", timestamp)).styled(
                genpdf::style::Style::new()
                    .with_font_size(8)
                    .with_color(genpdf::style::Color::Greyscale(140)),
            ),
        );
        doc.push(genpdf::elements::Break::new(1));

        // ==== Cuerpo del documento ====
        // 1) HTML ricotexto del editor (color/tamaño/negrita/itálica/alineación por párrafo).
        let blocks = html
            .as_deref()
            .filter(|h| !h.trim().is_empty())
            .map(parse_html_to_blocks)
            .unwrap_or_default();

        if !blocks.is_empty() {
            // genpdf 0.2 no dibuja subrayado; se respetan negrita, itálica, color,
            // tamaño y familia (por run) tal como las emite el editor.
            // Cacheamos las familias extra que haya que registrar en el documento.
            let base_family = resolve_family(&font).to_string();
            let mut families: std::collections::HashMap<
                String,
                genpdf::fonts::FontFamily<genpdf::fonts::Font>,
            > = std::collections::HashMap::new();
            for block in blocks {
                let mut para = genpdf::elements::Paragraph::new("");
                for run in block.runs {
                    let mut st = genpdf::style::Style::new();
                    if run.bold {
                        st = st.bold();
                    }
                    if run.italic {
                        st = st.italic();
                    }
                    if let Some(px) = run.font_size {
                        st = st.with_font_size(px.clamp(8, 72));
                    }
                    if let Some((r, g, b)) = run.color {
                        st = st.with_color(genpdf::style::Color::Rgb(r, g, b));
                    }
                    if let Some(ff) = run.font_family {
                        let id = resolve_family(&ff).to_string();
                        if id != base_family {
                            let fam = match families.get(&id) {
                                Some(f) => *f,
                                None => {
                                    let loaded = load_font_family(&id)?;
                                    let fam = doc.add_font_family(loaded);
                                    families.insert(id.clone(), fam);
                                    fam
                                }
                            };
                            st = st.with_font_family(fam);
                        }
                    }
                    para.push_styled(run.text.as_str(), st);
                }
                doc.push(
                    para.aligned(block.align.unwrap_or(body_alignment))
                        .styled(body_style)
                        .padded(genpdf::Margins::trbl(
                            0.0,
                            0.0,
                            0.0,
                            block.padding_left * 25.4 / 96.0,
                        )),
                );
                doc.push(genpdf::elements::Break::new(0.55));
            }
        } else {
            // 2) Sin HTML: texto plano (la corrección), y si no, los segments con timestamp.
            match text {
                Some(corrected) if !corrected.trim().is_empty() => {
                    for para in corrected.split('\n') {
                        let para = para.trim();
                        if para.is_empty() {
                            continue;
                        }
                        doc.push(
                            genpdf::elements::Paragraph::new(para)
                                .aligned(body_alignment)
                                .styled(body_style),
                        );
                        doc.push(genpdf::elements::Break::new(0.55));
                    }
                }
                _ => {
                    let time_style = genpdf::style::Style::new()
                        .bold()
                        .with_color(genpdf::style::Color::Rgb(217, 119, 6));
                    let mut previous_speaker: Option<u8> = None;
                    for seg in &segments {
                        let mins = seg.start as u64 / 60;
                        let secs = seg.start as u64 % 60;
                        let time = format!("[{:02}:{:02}]", mins, secs);
                        let mut para = genpdf::elements::Paragraph::new("");
                        para.push_styled(format!("{}  ", time), time_style);
                        if let Some(speaker) = seg.speaker {
                            if previous_speaker != Some(speaker) {
                                let label = speaker_names
                                    .as_ref()
                                    .and_then(|names| {
                                        names
                                            .get(&speaker.to_string())
                                            .map(|name| name.trim())
                                            .filter(|name| !name.is_empty())
                                    })
                                    .map(|name| format!("{name}: "))
                                    .unwrap_or_else(|| format!("Hablante {speaker}: "));
                                para.push_styled(label, time_style);
                            }
                        }
                        previous_speaker = seg.speaker;
                        para.push_styled(seg.text.as_str(), genpdf::style::Style::new());
                        doc.push(para.aligned(body_alignment).styled(body_style));
                        doc.push(genpdf::elements::Break::new(0.4));
                    }
                }
            }
        }

        crate::store::atomic_write(&destination, |file| {
            doc.render(file)
                .map_err(|e| format!("No se pudo generar el PDF: {}", e))
        })?;

        Ok(destination.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Tarea interrumpida: {}", e))?
}

/// Resuelve un id de fuente del frontend ("system") o un nombre/familia CSS
/// ("Times New Roman", "times", "Helvetica Neue", ...) al id de familia
/// soportado por `load_font_family`. Desconocidas -> "system" (Arial).
fn resolve_family(font: &str) -> &'static str {
    let l = font.to_ascii_lowercase();
    if l.contains("times") {
        "times"
    } else if l.contains("georgia") {
        "georgia"
    } else if l.contains("courier") {
        "courier"
    } else if l.contains("verdana") {
        "verdana"
    } else {
        "system"
    }
}

/// Windows ships the same families as macOS but uses legacy short filenames
/// (`arialbd.ttf`, `timesi.ttf`, …), whereas macOS names files by family and
/// weight. Keep the editor-facing family names unchanged and resolve only at
/// PDF rendering time.
fn font_file_names(base: &str, weight: usize, suffix: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let stem = match (base, weight) {
            ("Arial", 0) => "arial",
            ("Arial", 1) => "arialbd",
            ("Arial", 2) => "ariali",
            ("Arial", 3) => "arialbi",
            ("Times New Roman", 0) => "times",
            ("Times New Roman", 1) => "timesbd",
            ("Times New Roman", 2) => "timesi",
            ("Times New Roman", 3) => "timesbi",
            ("Courier New", 0) => "cour",
            ("Courier New", 1) => "courbd",
            ("Courier New", 2) => "couri",
            ("Courier New", 3) => "courbi",
            ("Georgia", 0) => "georgia",
            ("Georgia", 1) => "georgiab",
            ("Georgia", 2) => "georgiai",
            ("Georgia", 3) => "georgiaz",
            ("Verdana", 0) => "verdana",
            ("Verdana", 1) => "verdanab",
            ("Verdana", 2) => "verdanai",
            ("Verdana", 3) => "verdanaz",
            _ => "arial",
        };
        vec![format!("{}.ttf", stem)]
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = weight;
        vec![format!("{}{}.ttf", base, suffix)]
    }
}

pub(crate) fn load_font_family(
    font: &str,
) -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>, String> {
    // Familia del PDF según la opción del frontend. "system"/Helvetica se resuelven
    // a Arial (los .ttc de Helvetica no los soporta genpdf).
    let base = match resolve_family(font) {
        "times" => "Times New Roman",
        "georgia" => "Georgia",
        "courier" => "Courier New",
        "verdana" => "Verdana",
        _ => "Arial",
    };
    let kinds = [
        ("regular", ""),
        ("bold", " Bold"),
        ("italic", " Italic"),
        ("bold_italic", " Bold Italic"),
    ];
    // Cargamos cada peso de la familia elegida; si falta algún archivo, caemos a Arial.
    let mut weights: [Option<genpdf::fonts::FontData>; 4] = [None, None, None, None];
    for (i, (_, kind)) in kinds.iter().enumerate() {
        for dir in crate::platform::font_directories() {
            for fam in [base, "Arial"] {
                for name in font_file_names(fam, i, kind) {
                    let file = dir.join(name);
                    if file.exists() {
                        weights[i] = Some(
                            genpdf::fonts::FontData::load(&file, None)
                                .map_err(|e| format!("No se pudo cargar la fuente: {}", e))?,
                        );
                        break;
                    }
                }
                if weights[i].is_some() {
                    break;
                }
            }
            if weights[i].is_some() {
                break;
            }
        }
    }
    let [regular, bold, italic, bold_italic] = weights;
    Ok(genpdf::fonts::FontFamily {
        regular: regular.ok_or("Falta la fuente regular")?,
        bold: bold.ok_or("Falta la fuente bold")?,
        italic: italic.ok_or("Falta la fuente italic")?,
        bold_italic: bold_italic.ok_or("Falta la fuente bold italic")?,
    })
}

/// Decorador de página: dibuja cabecera superior (marca + titulo), una regla,
/// y pie de pagina (texto + numero de pagina) en cada hoja del PDF.
struct ReportDecorator {
    title: String,
    page: usize,
}

impl ReportDecorator {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            page: 0,
        }
    }
}

impl genpdf::PageDecorator for ReportDecorator {
    fn decorate_page<'a>(
        &mut self,
        context: &genpdf::Context,
        mut area: genpdf::render::Area<'a>,
        _style: genpdf::style::Style,
    ) -> Result<genpdf::render::Area<'a>, genpdf::error::Error> {
        self.page += 1;
        let page: f64 = area.size().height.into();
        let width: f64 = area.size().width.into();
        let left = 20.0;
        let right = 20.0;

        // --- Cabecera superior ---
        let _ = area.print_str(
            &context.font_cache,
            genpdf::Position::new(left, 4.5),
            genpdf::style::Style::new()
                .bold()
                .with_font_size(9)
                .with_color(genpdf::style::Color::Greyscale(70)),
            "BlueDoor",
        );
        // Titulo alineado a la derecha (recortado para que siempre quepa).
        let header_style = genpdf::style::Style::new()
            .with_font_size(8)
            .with_color(genpdf::style::Color::Greyscale(120));
        let mut header_title = self.title.clone();
        loop {
            let w: f64 = header_style
                .str_width(&context.font_cache, &header_title)
                .into();
            if w <= width - left - right - 60.0 || header_title.chars().count() <= 12 {
                break;
            }
            let cut = header_title
                .char_indices()
                .nth_back(0)
                .map(|(i, _)| i)
                .unwrap_or(0);
            header_title.truncate(cut);
        }
        let w: f64 = header_style
            .str_width(&context.font_cache, &header_title)
            .into();
        let _ = area.print_str(
            &context.font_cache,
            genpdf::Position::new(width - right - w, 4.5),
            header_style,
            header_title,
        );
        // Regla bajo la cabecera.
        area.draw_line(
            vec![
                genpdf::Position::new(left, 12.5),
                genpdf::Position::new(width - right, 12.5),
            ],
            genpdf::style::Style::new().with_color(genpdf::style::Color::Greyscale(200)),
        );

        // --- Pie de pagina ---
        let footer_style = genpdf::style::Style::new()
            .with_font_size(8)
            .with_color(genpdf::style::Color::Greyscale(150));
        let _ = area.print_str(
            &context.font_cache,
            genpdf::Position::new(left, page - 7.0),
            footer_style,
            "Generado por BlueDoor — Local y privado",
        );
        let page_label = format!("Página {}", self.page);
        let pw: f64 = footer_style
            .str_width(&context.font_cache, &page_label)
            .into();
        let _ = area.print_str(
            &context.font_cache,
            genpdf::Position::new(width - right - pw, page - 7.0),
            footer_style,
            page_label,
        );
        area.draw_line(
            vec![
                genpdf::Position::new(left, page - 12.0),
                genpdf::Position::new(width - right, page - 12.0),
            ],
            genpdf::style::Style::new().with_color(genpdf::style::Color::Greyscale(220)),
        );

        // Area de contenido: dentro de los margenes (deja sitio a cabecera/pie).
        area.add_margins(genpdf::Margins::trbl(19.0, 20.0, 20.0, 20.0));
        Ok(area)
    }
}

#[cfg(test)]
mod tests;

// The layout editor shares the rich-text parser used by the existing TextEdit
// export. Only typed runs are passed to CoreText, never executable HTML.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutRun {
    pub text: String,
    pub font: String,
    pub size: f64,
    pub color: [f64; 3],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutParagraph {
    pub runs: Vec<LayoutRun>,
    pub left: f64,
    pub right: f64,
    pub first: f64,
    pub align: String,
}
pub(crate) fn layout_paragraphs(
    html: &str,
    font: &str,
    size: f64,
    align: &str,
) -> Vec<LayoutParagraph> {
    parse_html_to_blocks(html)
        .into_iter()
        .map(|b| LayoutParagraph {
            left: b.padding_left as f64,
            right: b.padding_right as f64,
            first: b.text_indent as f64,
            align: b
                .align
                .map(|a| match a {
                    genpdf::Alignment::Center => "center",
                    genpdf::Alignment::Right => "right",
                    _ => "left",
                })
                .unwrap_or(align)
                .into(),
            runs: b
                .runs
                .into_iter()
                .map(|r| LayoutRun {
                    text: r.text,
                    font: r.font_family.unwrap_or_else(|| font.into()),
                    size: r.font_size.map(f64::from).unwrap_or(size),
                    color: r
                        .color
                        .map(|(r, g, b)| [r as f64 / 255., g as f64 / 255., b as f64 / 255.])
                        .unwrap_or([0., 0., 0.]),
                    bold: r.bold,
                    italic: r.italic,
                    underline: r.underline,
                })
                .collect(),
        })
        .collect()
}
