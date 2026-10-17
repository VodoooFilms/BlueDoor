use super::*;
use crate::types::Segment;

#[test]
fn pdf_generation_writes_file() {
    let dest = std::env::temp_dir().join("bluedoor-test.pdf");
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
    let dest = std::env::temp_dir().join("bluedoor-report-test.pdf");
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
        html_visible_text(
            "<div><b>PERSON_01</b> <font size=\"7\">presenta</font> el escrito</div>"
        ),
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
    let dest = std::env::temp_dir().join("bluedoor-richtext-test.pdf");
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
