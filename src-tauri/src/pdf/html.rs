// --- Parser del HTML de un párrafo en runs con estilo (color/tamaño/negrita/itálica) ---

#[derive(Debug, Clone)]
pub(super) struct HtmlRun {
    pub(super) text: String,
    pub(super) font_size: Option<u8>,
    pub(super) color: Option<(u8, u8, u8)>,
    pub(super) font_family: Option<String>,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
}

#[derive(Debug)]
pub(super) struct HtmlBlock {
    pub(super) padding_left: f32,
    pub(super) padding_right: f32,
    pub(super) text_indent: f32,
    pub(super) align: Option<genpdf::Alignment>,
    pub(super) runs: Vec<HtmlRun>,
}

#[derive(Clone, Default)]
struct Cursor {
    padding_left: f32,
    padding_right: f32,
    text_indent: f32,
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
    padding_right: Option<f32>,
    text_indent: Option<f32>,
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
            "padding-right" | "text-indent" => {
                let n = v
                    .strip_suffix("px")
                    .unwrap_or(&v)
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(|n| n.clamp(-480., 480.));
                if k == "padding-right" {
                    p.padding_right = n.map(|n| n.max(0.));
                } else {
                    p.text_indent = n;
                }
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
    padding_right: f32,
    text_indent: f32,
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
        padding_right,
        text_indent,
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
pub(super) fn parse_html_to_blocks(html: &str) -> Vec<HtmlBlock> {
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
                        stack.last().map_or(0.0, |c| c.padding_right),
                        stack.last().map_or(0.0, |c| c.text_indent),
                    );
                    align = None;
                    pop(&mut stack);
                } else if let Some(st) = attr_value(plain, "style").map(|s| parse_style_attr(&s)) {
                    if let Some(a) = st.align {
                        align = Some(a);
                    }
                    let mut c = cur(&stack);
                    c.padding_left = st.padding_left.unwrap_or(c.padding_left);
                    c.padding_right = st.padding_right.unwrap_or(c.padding_right);
                    c.text_indent = st.text_indent.unwrap_or(c.text_indent);
                    c.font_size = st.font_size.or(c.font_size);
                    c.font_family = st.font_family.or(c.font_family);
                    c.color = st.color.or(c.color);
                    c.bold = st.bold.unwrap_or(c.bold);
                    c.italic = st.italic.unwrap_or(c.italic);
                    c.underline = st.underline.unwrap_or(c.underline);
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
            "br" => flush_block(
                &mut runs,
                &mut blocks,
                align,
                stack.last().map_or(0., |c| c.padding_left),
                stack.last().map_or(0., |c| c.padding_right),
                stack.last().map_or(0., |c| c.text_indent),
            ),
            _ => {}
        }
    }
    flush_text(&mut buf, &mut runs, stack.last().expect("cursor"));
    flush_block(
        &mut runs,
        &mut blocks,
        align,
        stack.last().map_or(0.0, |c| c.padding_left),
        stack.last().map_or(0.0, |c| c.padding_right),
        stack.last().map_or(0.0, |c| c.text_indent),
    );
    blocks
}
