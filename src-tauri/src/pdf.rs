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

fn validate_pdf_destination(path: &Path) -> Result<PathBuf, String> {
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
    if let Some(private) = private.as_ref() {
        crate::privacy::gate_private_export(
            &app,
            &private.id,
            private.revision,
            text.as_deref().unwrap_or_default(),
            html.as_deref(),
            &title,
        )?;
    }
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

// --- Parser del HTML de un párrafo en runs con estilo (color/tamaño/negrita/itálica) ---

#[derive(Debug, Clone)]
struct HtmlRun {
    text: String,
    font_size: Option<u8>,
    color: Option<(u8, u8, u8)>,
    font_family: Option<String>,
    bold: bool,
    italic: bool,
    underline: bool,
}

#[derive(Debug)]
struct HtmlBlock {
    padding_left: f32,
    align: Option<genpdf::Alignment>,
    runs: Vec<HtmlRun>,
}

#[derive(Clone, Default)]
struct Cursor {
    padding_left: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<(u8, u8, u8)>,
    font_size: Option<u8>,
    font_family: Option<String>,
}

#[derive(Default)]
struct StylePatch {
    padding_left: Option<f32>,
    color: Option<(u8, u8, u8)>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    font_size: Option<u8>,
    font_family: Option<String>,
    align: Option<genpdf::Alignment>,
}

/// Valores 1-7 de `<font size>` → px (mismas tallas que el frontend `sizeToLevel`).
const FONT_LEVELS: [u8; 7] = [10, 13, 16, 18, 24, 32, 48];

fn font_level_to_px(l: u8) -> Option<u8> {
    (1..=7).contains(&l).then(|| FONT_LEVELS[(l - 1) as usize])
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == ';') {
                let ent: String = chars[i + 1..i + 1 + rel].iter().collect();
                let rep = match ent.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => ent.strip_prefix("#39").map(|_| '\''),
                };
                if let Some(c) = rep {
                    out.push(c);
                    i += rel + 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn parse_color(v: &str) -> Option<(u8, u8, u8)> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() == 6 {
            let rd = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let gd = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let bd = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((rd, gd, bd));
        }
        if hex.len() == 3 {
            let d = |c: char| {
                let s = format!("{}{}", c, c);
                u8::from_str_radix(&s, 16).ok()
            };
            let (r, g, b) = (
                d(hex.as_bytes()[0] as char),
                d(hex.as_bytes()[1] as char),
                d(hex.as_bytes()[2] as char),
            );
            return match (r, g, b) {
                (Some(r), Some(g), Some(b)) => Some((r, g, b)),
                _ => None,
            };
        }
        return None;
    }
    if let Some(rest) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let nums: Vec<&str> = rest.split(',').map(|x| x.trim()).collect();
        let p = |s: &str| s.parse::<f32>().ok().map(|f| f.round() as u8);
        if nums.len() >= 3 {
            return match (p(nums[0]), p(nums[1]), p(nums[2])) {
                (Some(r), Some(g), Some(b)) => Some((r, g, b)),
                _ => None,
            };
        }
    }
    None
}

fn parse_font_size(v: &str) -> Option<u8> {
    let v = v.trim().to_ascii_lowercase();
    // execCommand("fontSize") en WKWebView emite keywords CSS
    // (x-small..xxx-large), no px. El editor previsualiza con el tamaño que
    // renderiza el keyword, por lo que el PDF debe usar esos mismos px.
    let kw = match v.as_str() {
        "xx-small" => Some(9),
        "x-small" => Some(10),
        "small" => Some(13),
        "medium" => Some(16),
        "large" => Some(18),
        "x-large" => Some(24),
        "xx-large" => Some(32),
        "xxx-large" => Some(48),
        _ => None,
    };
    if let Some(px) = kw {
        return Some(px);
    }
    if let Some(px) = v.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(|f| f.round() as u8);
    }
    if let Some(pt) = v.strip_suffix("pt") {
        return pt.trim().parse::<f32>().ok().map(|f| f.round() as u8);
    }
    v.parse().ok()
}

fn parse_style_attr(style: &str) -> StylePatch {
    let mut p = StylePatch::default();
    for pair in style.split(';') {
        let Some((k, v)) = pair.split_once(':') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().to_ascii_lowercase();
        match k.as_str() {
            "padding-left" => {
                // Editor ruler and conversation layout store CSS pixels.
                p.padding_left = v
                    .strip_suffix("px")
                    .unwrap_or(&v)
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(|n| n.clamp(0.0, 480.0));
            }
            "color" => p.color = parse_color(&v),
            "font-weight" => {
                p.bold = Some(
                    v == "bold"
                        || v == "bolder"
                        || matches!(v.as_str(), "600" | "700" | "800" | "900"),
                )
            }
            "font-style" => p.italic = Some(v == "italic" || v == "oblique"),
            "text-decoration" => p.underline = Some(v.contains("underline")),
            "font-size" => p.font_size = parse_font_size(&v),
            "font-family" => {
                let first = v.split(',').next().unwrap_or("");
                let fam = first.trim().trim_matches(|c| c == '"' || c == '\'');
                if !fam.is_empty() {
                    p.font_family = Some(fam.to_string());
                }
            }
            "text-align" => {
                p.align = Some(match v.as_str() {
                    "center" => genpdf::Alignment::Center,
                    "right" => genpdf::Alignment::Right,
                    _ => genpdf::Alignment::Left,
                });
            }
            _ => {}
        }
    }
    p
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let idx = tag.find(name)?;
    let after = tag[idx + name.len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let quote = after.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = after[1..].find(quote)?;
        Some(after[1..1 + end].to_string())
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(after.len());
        Some(after[..end].to_string())
    }
}

fn flush_text(buf: &mut String, runs: &mut Vec<HtmlRun>, cur: &Cursor) {
    let s = decode_entities(buf);
    buf.clear();
    if s.is_empty() {
        return;
    }
    if s.trim().is_empty() {
        // Whitespace between two styled runs is content: dropping it glues
        // "<b>PERSON_01</b> presenta" into "PERSON_01presenta" on the page.
        // Attach it to the run before it; a block's leading whitespace is
        // trimmed later by `flush_block`.
        if let Some(last) = runs.last_mut() {
            last.text.push_str(&s);
        }
        return;
    }
    if let Some(last) = runs.last_mut() {
        if cur.bold == last.bold
            && cur.italic == last.italic
            && cur.underline == last.underline
            && cur.color == last.color
            && cur.font_size == last.font_size
            && cur.font_family == last.font_family
        {
            last.text.push_str(&s);
            return;
        }
    }
    runs.push(HtmlRun {
        text: s,
        font_size: cur.font_size,
        color: cur.color,
        font_family: cur.font_family.clone(),
        bold: cur.bold,
        italic: cur.italic,
        underline: cur.underline,
    });
}

fn flush_block(
    runs: &mut Vec<HtmlRun>,
    blocks: &mut Vec<HtmlBlock>,
    align: Option<genpdf::Alignment>,
    padding_left: f32,
) {
    if runs.is_empty() {
        return;
    }
    if let Some(first) = runs.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = runs.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    runs.retain(|r| !r.text.is_empty());
    if runs.is_empty() {
        return;
    }
    blocks.push(HtmlBlock {
        padding_left,
        align,
        runs: std::mem::take(runs),
    });
}

/// Visible text of the rich body exactly as the PDF would draw it. The export
/// gate compares this against the reviewed text, so rich formatting may be
/// applied but no extra content may ride along inside the markup.
pub(crate) fn html_visible_text(html: &str) -> String {
    parse_html_to_blocks(html)
        .iter()
        .map(|block| {
            // Runs are adjacent inline fragments drawn one after another, so
            // they concatenate without a separator. Inserting one here would
            // split "EMAIL_01." into "EMAIL_01 ." and refuse a PDF whose text
            // is in fact identical to the reviewed document.
            block
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        // A block boundary is a line break, which reads as one space.
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convierte el HTML generado por el editor en bloques de runs con estilo.
/// Tolera el subconjunto que produce contenteditable/execCommand
/// (div/b/font/span/i/u/br) e ignora el resto; las entidades se decodifican.
fn parse_html_to_blocks(html: &str) -> Vec<HtmlBlock> {
    let mut blocks: Vec<HtmlBlock> = Vec::new();
    let mut runs: Vec<HtmlRun> = Vec::new();
    let mut align: Option<genpdf::Alignment> = None;
    let mut stack: Vec<Cursor> = vec![Cursor::default()];
    let mut buf = String::new();
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '<' {
            buf.push(chars[i]);
            i += 1;
            continue;
        }
        let Some(rel) = chars[i + 1..].iter().position(|&c| c == '>') else {
            buf.push(chars[i]);
            i += 1;
            continue;
        };
        let tag: String = chars[i + 1..i + 1 + rel].iter().collect();
        i += rel + 2;
        let tag = tag.trim();
        let (close, plain) = match tag.strip_prefix('/') {
            Some(t) => (true, t.trim()),
            None => (false, tag),
        };
        let plain = plain.trim_end_matches('/').trim();
        let name = plain
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let cur = |stack: &Vec<Cursor>| stack.last().cloned().unwrap_or_default();
        let pop = |stack: &mut Vec<Cursor>| {
            if stack.len() > 1 {
                stack.pop();
            }
        };

        flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));

        match name.as_str() {
            "div" | "p" => {
                if close {
                    flush_block(
                        &mut runs,
                        &mut blocks,
                        align,
                        stack.last().map_or(0.0, |c| c.padding_left),
                    );
                    align = None;
                    pop(&mut stack);
                } else if let Some(st) = attr_value(plain, "style").map(|s| parse_style_attr(&s)) {
                    if let Some(a) = st.align {
                        align = Some(a);
                    }
                    let mut c = cur(&stack);
                    c.padding_left = st.padding_left.unwrap_or(c.padding_left);
                    stack.push(c);
                } else {
                    let c = cur(&stack);
                    stack.push(c);
                }
            }
            "b" | "strong" => {
                flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
                if close {
                    pop(&mut stack);
                } else {
                    let mut c = cur(&mut stack);
                    c.bold = true;
                    stack.push(c);
                }
            }
            "i" | "em" => {
                flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
                if close {
                    pop(&mut stack);
                } else {
                    let mut c = cur(&mut stack);
                    c.italic = true;
                    stack.push(c);
                }
            }
            "u" | "ins" => {
                flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
                if close {
                    pop(&mut stack);
                } else {
                    let mut c = cur(&mut stack);
                    c.underline = true;
                    stack.push(c);
                }
            }
            "font" => {
                flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
                if close {
                    pop(&mut stack);
                } else {
                    let mut c = cur(&mut stack);
                    if let Some(sz) = attr_value(plain, "size")
                        .and_then(|s| s.parse::<u8>().ok())
                        .and_then(font_level_to_px)
                    {
                        c.font_size = Some(sz);
                    }
                    if let Some(face) = attr_value(plain, "face") {
                        c.font_family = Some(face);
                    }
                    stack.push(c);
                }
            }
            "span" => {
                flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
                if close {
                    pop(&mut stack);
                } else {
                    let mut c = cur(&mut stack);
                    if let Some(st) = attr_value(plain, "style").map(|s| parse_style_attr(&s)) {
                        if let Some(col) = st.color {
                            c.color = Some(col);
                        }
                        if let Some(b) = st.bold {
                            c.bold = b;
                        }
                        if let Some(il) = st.italic {
                            c.italic = il;
                        }
                        if let Some(ul) = st.underline {
                            c.underline = ul;
                        }
                        if let Some(px) = st.font_size {
                            c.font_size = Some(px);
                        }
                        if let Some(ff) = st.font_family {
                            c.font_family = Some(ff);
                        }
                    }
                    stack.push(c);
                }
            }
            "br" => flush_text(&mut buf, &mut runs, stack.last().expect("cursor")),
            _ => {}
        }
    }
    flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
    flush_block(
        &mut runs,
        &mut blocks,
        align,
        stack.last().map_or(0.0, |c| c.padding_left),
    );
    blocks
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

fn load_font_family(
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
            "Belegal _ Local demo",
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
            "Generado por Belegal _ Local demo — Local y privado",
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
mod tests {
    use super::*;
    use crate::types::Segment;

    #[test]
    fn pdf_generation_writes_file() {
        let dest = std::env::temp_dir().join("belegal-local-demo-test.pdf");
        let _ = std::fs::remove_file(&dest);

        let font_family = load_font_family("system").unwrap();

        let mut doc = genpdf::Document::new(font_family);
        doc.set_title("Prueba");
        doc.set_minimal_conformance();
        doc.push(
            genpdf::elements::Paragraph::new("Hola, esto es una prueba").styled_string(
                "Hola",
                genpdf::style::Style::new().with_color(genpdf::style::Color::Rgb(242, 138, 35)),
            ),
        );
        doc.render_to_file(&dest).unwrap();
        assert!(dest.exists());
        assert!(dest.metadata().unwrap().len() > 1000);
        let _ = std::fs::remove_file(&dest);
    }

    #[test]
    fn pdf_report_is_multipage() {
        let dest = std::env::temp_dir().join("belegal-local-demo-report-test.pdf");
        let _ = std::fs::remove_file(&dest);

        let segments = vec![Segment {
            start: 0.0,
            end: 1.5,
            text: "Hola, esto es una prueba.".to_string(),
            speaker: None,
        }];
        let mut long = String::new();
        for i in 0..120 {
            long.push_str(&format!(
                "Párrafo número {} de la transcripción, para comprobar el salto de página y que la cabecera y el pie se repiten correctamente en cada hoja.\n",
                i
            ));
        }

        let out = tauri::async_runtime::block_on(super::generate_pdf_at_path(
            "Reunión de prueba".to_string(),
            segments,
            "es".to_string(),
            "small".to_string(),
            92.0,
            dest.clone(),
            Some(long),
            None,
            None,
            "system".to_string(),
            16.0,
            "left".to_string(),
        ))
        .unwrap();
        let len = std::path::Path::new(&out).metadata().unwrap().len();
        eprintln!("PDF report generado en {} ({} bytes)", out, len);
        assert!(len > 5000, "PDF demasiado pequeño: {}", len);
    }

    #[test]
    fn html_parser_extracts_styled_runs() {
        let blocks = parse_html_to_blocks(
            "<div style=\"text-align: right;\">Hola <b>mundo</b> grande</div>\
             <div>Segunda <span style=\"color: #ff1a2b; font-weight: bold;\">rojo</span> y <font size=\"5\">grande</font></div>\
             <div><span style=\"font-size: x-large; font-family: times;\">TimesXL</span></div>",
        );
        assert_eq!(blocks.len(), 3, "debería haber 3 párrafos");
        assert!(matches!(blocks[0].align, Some(genpdf::Alignment::Right)));
        assert_eq!(
            blocks[0]
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Hola ", "mundo", " grande"]
        );
        assert!(blocks[0].runs[1].bold);
        assert!(!blocks[0].runs[0].bold);
        assert_eq!(blocks[1].runs[0].text, "Segunda ");
        assert_eq!(blocks[1].runs[1].color, Some((255, 26, 43)));
        assert!(blocks[1].runs[1].bold && !blocks[1].runs[1].italic);
        assert_eq!(blocks[1].runs[2].text, " y ");
        assert_eq!(blocks[1].runs[3].font_size, Some(24));
        assert_eq!(
            blocks[2].runs[0].font_size,
            Some(24),
            "keyword x-large -> 24px"
        );
        assert_eq!(
            blocks[2].runs[0].font_family.as_deref(),
            Some("times"),
            "font-family se propaga al run"
        );
    }

    #[test]
    fn conversation_indents_do_not_leak_into_speaker_headings() {
        let blocks = parse_html_to_blocks(
            "<div><strong>Antonio:</strong></div>\
             <div style=\"padding-left: 48px\">Hola <i>Sol</i>.</div>\
             <div style=\"padding-left: 48px\">Segunda línea.</div>\
             <div><strong>Sol:</strong></div>\
             <div style=\"padding-left: 48px\">Hola.</div>",
        );
        assert_eq!(blocks.len(), 5);
        assert!(blocks[0].runs[0].bold);
        assert_eq!(blocks[1].padding_left, 48.0);
        assert!(blocks[1].runs[1].italic);
        assert_eq!(blocks[2].padding_left, 48.0);
        assert_eq!(blocks[3].padding_left, 0.0);
        assert_eq!(blocks[4].padding_left, 48.0);
    }

    #[test]
    fn html_decodes_entities_and_nbsp() {
        let blocks = parse_html_to_blocks("<div>a&nbsp;b &amp; c &lt;d&gt;</div>");
        assert_eq!(blocks[0].runs[0].text, "a b & c <d>");
    }

    #[test]
    fn visible_text_ignores_formatting_but_not_content() {
        // Formatting may ride along; extra words may not.
        assert_eq!(
            html_visible_text("<div><b>PERSON_01</b> <font size=\"7\">presenta</font> el escrito</div>"),
            "PERSON_01 presenta el escrito"
        );
        assert_ne!(
            html_visible_text("<div>PERSON_01 presenta el escrito</div>"),
            html_visible_text("<div>PERSON_01 presenta el escrito y su cedula 0912345678</div>")
        );
        // Text hidden behind markup still counts: it is what the PDF draws.
        assert!(html_visible_text("<div>a<span>b</span></div>").contains("ab"));
        // Entities are decoded, so the comparison sees the drawn characters.
        assert_eq!(html_visible_text("<div>a&amp;b</div>"), "a&b");
    }

    #[test]
    fn styling_around_punctuation_does_not_change_the_visible_text() {
        // Regression: whitespace-only runs used to be dropped, which glued
        // words together on the page and made the export gate refuse a PDF
        // whose text was in fact the reviewed one.
        for html in [
            "<div>Reclama <b>EMAIL_01</b>.</div>",
            "<div>Reclama <b>EMAIL_01</b> el <i>2026-09-30</i>.</div>",
            "<div><b>PERSON_01</b> <font size=\"7\">presenta</font> el escrito</div>",
        ] {
            let plain = html
                .replace("<b>", "")
                .replace("</b>", "")
                .replace("<i>", "")
                .replace("</i>", "")
                .replace("<font size=\"7\">", "")
                .replace("</font>", "")
                .replace("<div>", "")
                .replace("</div>", "");
            assert_eq!(
                html_visible_text(html),
                plain,
                "styling changed the drawn text for {html}"
            );
        }
    }

    #[test]
    fn pdf_rich_html_is_written_with_styles() {
        let dest = std::env::temp_dir().join("belegal-local-demo-richtext-test.pdf");
        let _ = std::fs::remove_file(&dest);

        let segments = vec![Segment {
            start: 0.0,
            end: 1.5,
            text: "original".to_string(),
            speaker: None,
        }];
        let html = "<div style=\"text-align: center;\">Un <b>título</b> centrado</div>\
             <div>Texto con <font size=\"5\">tamaño</font> y <span style=\"color: rgb(242, 138, 35);\">color</span>, <span style=\"font-family: times; font-size: x-large;\">fuente</span>. Fin.</div>"
            .to_string();

        let out = tauri::async_runtime::block_on(super::generate_pdf_at_path(
            "Estilos".to_string(),
            segments,
            "es".to_string(),
            "small".to_string(),
            10.0,
            dest.clone(),
            None,
            Some(html),
            None,
            "system".to_string(),
            16.0,
            "left".to_string(),
        ))
        .unwrap();
        let len = std::path::Path::new(&out).metadata().unwrap().len();
        eprintln!("PDF rich generado en {} ({} bytes)", out, len);
        assert!(len > 2000, "PDF rich demasiado pequeño: {}", len);
    }
}
