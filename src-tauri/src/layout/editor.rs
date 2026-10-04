//! Rich corrections reuse the existing editor's HTML parser; geometry and
//! original pixels remain native-owned. Only changed blocks are repainted.
use super::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TextEdit {
    pub block_id: usize,
    pub html: String,
    pub font: String,
    pub size: f64,
    pub align: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextBlock {
    pub id: usize,
    pub page: usize,
    pub rect: [f64; 4],
    pub bounds: [f64; 4],
    pub html: String,
    pub font: String,
    pub size: f64,
    pub align: String,
    #[serde(skip)]
    pub start: usize,
    #[serde(skip)]
    pub end: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RichOverlay {
    pub page: usize,
    pub erase_rect: [f64; 4],
    pub rect: [f64; 4],
    pub paragraphs: Vec<crate::pdf::LayoutParagraph>,
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn matches(
    document: &DocumentLayout,
    entities: &[Entity],
) -> Result<Vec<(usize, usize, String)>, String> {
    let mut matches = vec![];
    for entity in entities {
        for found in crate::privacy::matching_pattern(&entity.original)?.find_iter(&document.text) {
            if crate::privacy::boundary(&document.text, found.start(), found.end()) {
                let mut alias = entity.alias.clone();
                if found.as_str().ends_with('.') {
                    alias.push('.');
                }
                matches.push((found.start(), found.end(), alias));
            }
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut end = 0;
    matches.retain(|m| {
        if m.0 < end {
            false
        } else {
            end = m.1;
            true
        }
    });
    Ok(matches)
}
fn fragment(text: &str, start: usize, end: usize, matches: &[(usize, usize, String)]) -> String {
    let mut result = String::new();
    let mut cursor = start;
    for (a, b, alias) in matches.iter().filter(|(a, b, _)| *a < end && *b > start) {
        if *a > cursor {
            result.push_str(&text[cursor..*a]);
        }
        if *a >= start {
            result.push_str(alias);
        }
        cursor = (*b).min(end);
    }
    result.push_str(&text[cursor..end]);
    result
}
pub(crate) fn blocks(
    document: &DocumentLayout,
    entities: &[Entity],
) -> Result<Vec<TextBlock>, String> {
    let matches = matches(document, entities)?;
    let mut result: Vec<TextBlock> = vec![];
    for (page_index, page) in document.pages.iter().enumerate() {
        let mut groups: Vec<Vec<&Glyph>> = vec![];
        for glyph in &page.glyphs {
            let same = groups.last().and_then(|g| g.last()).is_some_and(|last| {
                last.line == glyph.line
                    && glyph.start >= last.end
                    && (glyph.rect[1] - last.rect[1]).abs() < glyph.rect[3] * 0.7
                    && glyph.rect[0] - (last.rect[0] + last.rect[2]) < glyph.font_size * 2.5
            });
            if same {
                groups.last_mut().unwrap().push(glyph)
            } else {
                groups.push(vec![glyph]);
            }
        }
        for group in groups {
            let first = group[0];
            let last = group.last().unwrap();
            let x = group
                .iter()
                .map(|g| g.rect[0])
                .fold(f64::INFINITY, f64::min);
            let y = group
                .iter()
                .map(|g| g.rect[1])
                .fold(f64::INFINITY, f64::min);
            let right = group
                .iter()
                .map(|g| g.rect[0] + g.rect[2])
                .fold(0., f64::max);
            let bottom = group
                .iter()
                .map(|g| g.rect[1] + g.rect[3])
                .fold(0., f64::max);
            let rect = [
                (x - 0.7).max(0.),
                (y - 0.7).max(0.),
                (right + 0.7).min(page.width) - (x - 0.7).max(0.),
                (bottom + 0.7).min(page.height) - (y - 0.7).max(0.),
            ];
            // Space may grow only to the next physical line or page edge.
            let next_y = page
                .glyphs
                .iter()
                .filter(|g| {
                    g.rect[1] > bottom + 1. && g.rect[0] < right + 2. && g.rect[0] + g.rect[2] > x
                })
                .map(|g| g.rect[1] - 1.)
                .fold(page.height - 20., f64::min);
            let bounds = [
                rect[0],
                rect[1],
                rect[2],
                (next_y - rect[1]).max(rect[3]).min(page.height - rect[1]),
            ];
            let mut html = String::from("<div>");
            let mut begin = 0;
            while begin < group.len() {
                let style = group[begin];
                let mut end = begin + 1;
                while end < group.len()
                    && group[end].font_name == style.font_name
                    && group[end].font_size == style.font_size
                    && group[end].color == style.color
                {
                    end += 1;
                }
                let stop = if end < group.len() {
                    group[end].start
                } else {
                    last.end
                };
                let text = fragment(&document.text, style.start, stop, &matches);
                let color = style.color.map(|v| (v * 255.).round() as u8);
                let name = style.font_name.to_ascii_lowercase();
                html.push_str(&format!("<span style=\"font-family: {}; font-size: {}px; color: rgb({}, {}, {}); font-weight: {}; font-style: {}\">{}</span>",escape(&style.font_name),style.font_size,color[0],color[1],color[2],if name.contains("bold") {"bold"} else {"normal"},if name.contains("italic")||name.contains("oblique") {"italic"} else {"normal"},escape(&text)));
                begin = end;
            }
            html.push_str("</div>");
            let id = result.len();
            result.push(TextBlock {
                id,
                page: page_index,
                rect,
                bounds,
                html,
                font: first.font_name.clone(),
                size: first.font_size,
                align: "left".into(),
                start: first.start,
                end: last.end,
            });
        }
    }
    Ok(result)
}
pub(crate) fn compiled(
    document: &DocumentLayout,
    entities: &[Entity],
) -> Result<(Vec<TextBlock>, Vec<RichOverlay>, String), String> {
    let mut blocks = blocks(document, entities)?;
    let mut overlays = vec![];
    let matches = matches(document, entities)?;
    let mut replacements = std::collections::HashMap::new();
    let mut used = std::collections::HashSet::new();
    for edit in &document.edits {
        if !used.insert(edit.block_id)
            || edit.html.len() > 128_000
            || !edit.size.is_finite()
            || !(4.0..=96.0).contains(&edit.size)
            || edit.font.len() > 120
            || !["left", "center", "right"].contains(&edit.align.as_str())
        {
            return Err("Corrección de texto no válida.".into());
        }
        let block = blocks
            .get_mut(edit.block_id)
            .ok_or("La zona de texto ya no existe.")?;
        let paragraphs =
            crate::pdf::layout_paragraphs(&edit.html, &edit.font, edit.size, &edit.align);
        let text = paragraphs
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        replacements.insert(block.id, text);
        block.html = edit.html.clone();
        block.font = edit.font.clone();
        block.size = edit.size;
        block.align = edit.align.clone();
        overlays.push(RichOverlay {
            page: block.page,
            erase_rect: block.rect,
            rect: block.bounds,
            paragraphs,
        });
    }
    let mut ordered = blocks.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|b| b.start);
    let mut text = String::new();
    let mut cursor = 0;
    for block in ordered {
        if block.start < cursor {
            return Err("La corrección cruza zonas de texto. Revisa el mapa primero.".into());
        }
        text.push_str(&fragment(&document.text, cursor, block.start, &matches));
        text.push_str(
            &replacements
                .remove(&block.id)
                .unwrap_or_else(|| fragment(&document.text, block.start, block.end, &matches)),
        );
        cursor = block.end;
    }
    text.push_str(&fragment(
        &document.text,
        cursor,
        document.text.len(),
        &matches,
    ));
    if text.trim().is_empty() || text.len() > crate::privacy::MAX_DOCUMENT_BYTES {
        return Err("El documento corregido está vacío o supera 64 KB.".into());
    }
    Ok((blocks, overlays, text))
}

pub(crate) fn rebase_aliases(
    document: &mut DocumentLayout,
    old: &[Entity],
    new: &[Entity],
) -> Result<(), String> {
    let mut changes = std::collections::HashMap::new();
    for previous in old {
        if let Some(next) = new
            .iter()
            .find(|e| e.kind == previous.kind && e.original == previous.original)
        {
            if let Some(known) = changes.insert(previous.alias.clone(), next.alias.clone()) {
                if known != next.alias {
                    return Err(
                        "Un alias compartido debe mantener el mismo reemplazo en el texto editado."
                            .into(),
                    );
                }
            }
        }
    }
    if changes.iter().all(|(a, b)| a == b) {
        return Ok(());
    }
    for edit in &mut document.edits {
        let paragraphs =
            crate::pdf::layout_paragraphs(&edit.html, &edit.font, edit.size, &edit.align);
        let mut result = String::new();
        for paragraph in paragraphs {
            let text = paragraph
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>();
            let mut matches = vec![];
            for (old, new) in &changes {
                if old.is_empty() {
                    continue;
                }
                for (start, _) in text.match_indices(old) {
                    if crate::privacy::boundary(&text, start, start + old.len()) {
                        matches.push((start, start + old.len(), new.clone()));
                    }
                }
            }
            matches.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
            let mut end = 0;
            matches.retain(|m| {
                if m.0 < end {
                    false
                } else {
                    end = m.1;
                    true
                }
            });
            result.push_str(&format!("<div style=\"padding-left: {}px; padding-right: {}px; text-indent: {}px; text-align: {}\">",paragraph.left,paragraph.right,paragraph.first,paragraph.align));
            let mut start = 0;
            for run in paragraph.runs {
                let replacement = fragment(&text, start, start + run.text.len(), &matches);
                start += run.text.len();
                result.push_str(&format!("<span style=\"font-family: {}; font-size: {}px; color: rgb({}, {}, {}); font-weight: {}; font-style: {}; text-decoration: {}\">{}</span>",escape(&run.font),run.size,(run.color[0]*255.).round(),(run.color[1]*255.).round(),(run.color[2]*255.).round(),if run.bold {"bold"} else {"normal"},if run.italic {"italic"} else {"normal"},if run.underline {"underline"} else {"none"},escape(&replacement)));
            }
            result.push_str("</div>");
        }
        edit.html = result;
    }
    Ok(())
}
