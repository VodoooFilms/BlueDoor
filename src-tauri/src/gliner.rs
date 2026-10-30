//! GLiNER's pinned tokenizer and token-level ONNX adapter. Suggestions enter
//! the same grounding, deterministic alias and export gates as Qwen.
use crate::privacy::{Detection, Kind};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokenizers::Tokenizer;

const LABELS: [(&str, Kind); 8] = [
    ("person", Kind::Person),
    ("organization", Kind::Company),
    ("national identification number", Kind::Id),
    ("email address", Kind::Email),
    ("phone number", Kind::Phone),
    ("address", Kind::Address),
    ("case number", Kind::Case),
    ("bank account number", Kind::Other),
];
#[derive(Serialize)]
struct Input {
    path: String,
    ids: Vec<i64>,
    mask: Vec<i64>,
    words: usize,
    max_width: usize,
}
#[derive(Deserialize)]
struct Output {
    shape: Vec<usize>,
    logits: Vec<f32>,
}
fn prepare(
    tokenizer: &Tokenizer,
    text: &str,
    path: &Path,
) -> Result<(Input, Vec<(usize, usize)>), String> {
    prepare_with_person_label(tokenizer, text, path, LABELS[0].0)
}
fn prepare_with_person_label(
    tokenizer: &Tokenizer,
    text: &str,
    path: &Path,
    person_label: &str,
) -> Result<(Input, Vec<(usize, usize)>), String> {
    let splitter = Regex::new(r"\w+(?:[-_]\w+)*|\S").expect("fixed splitter");
    let words: Vec<_> = splitter.find_iter(text).collect();
    if words.is_empty() {
        return Err("El fragmento no contiene texto.".into());
    }
    let mut input: Vec<String> = LABELS
        .iter()
        .enumerate()
        .flat_map(|(index, (label, _))| {
            [
                "<<ENT>>".into(),
                if index == 0 {
                    person_label.into()
                } else {
                    (*label).into()
                },
            ]
        })
        .collect();
    input.push("<<SEP>>".into());
    let skip = input.len() as u32;
    input.extend(words.iter().map(|word| {
        let word = word.as_str();
        if person_label == "person name"
            && word.chars().filter(|c| c.is_alphabetic()).count() >= 2
            && word
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(char::is_uppercase)
        {
            // Case is normalized only in the auxiliary model input. The word
            // offsets and every returned span still refer to the untouched OCR
            // source, including accents and misspelled/doubled letters.
            let mut chars = word.chars();
            format!("{}{}", chars.next().unwrap(), chars.as_str().to_lowercase())
        } else {
            word.into()
        }
    }));
    let encoded = tokenizer
        .encode(input, true)
        .map_err(|_| "No se pudo tokenizar con GLiNER.")?;
    if encoded.len() > 2048 {
        return Err("El análisis local no terminó dentro del límite. Divide este bloque.".into());
    }
    let mut previous = None;
    let mask = encoded
        .get_word_ids()
        .iter()
        .map(|&word| {
            let value = match word {
                Some(index) if index >= skip && word != previous => (index - skip + 1) as i64,
                _ => 0,
            };
            previous = word;
            value
        })
        .collect();
    Ok((
        Input {
            path: path.to_string_lossy().into(),
            ids: encoded.get_ids().iter().map(|&id| id as i64).collect(),
            mask,
            words: words.len(),
            max_width: 0,
        },
        words
            .iter()
            .map(|word| (word.start(), word.end()))
            .collect(),
    ))
}
fn decode(text: &str, words: &[(usize, usize)], output: Output) -> Result<Vec<Detection>, String> {
    decode_spans(text, words, output, false)
}
fn decode_people(
    text: &str,
    words: &[(usize, usize)],
    output: Output,
) -> Result<Vec<Detection>, String> {
    decode_spans(text, words, output, true)
}
fn decode_spans(
    text: &str,
    words: &[(usize, usize)],
    output: Output,
    people_only: bool,
) -> Result<Vec<Detection>, String> {
    // Span-level Multi PII uses the standard 0.5 confidence gate; the
    // older token graph retains its calibrated 0.35 threshold.
    let threshold = if output.shape == [1, words.len(), 12, LABELS.len()] {
        0.0
    } else {
        (0.35f32 / 0.65).ln()
    };
    let mut candidates = vec![];
    if output.logits.iter().any(|s| !s.is_finite()) {
        return Err("GLiNER devolvió puntuaciones no válidas.".into());
    }
    if output.shape == [1, words.len(), 12, LABELS.len()]
        && output.logits.len() == words.len() * 12 * LABELS.len()
    {
        for start in 0..words.len() {
            for width in 0..12.min(words.len() - start) {
                for label in 0..LABELS.len() {
                    if people_only && label != 0 {
                        continue;
                    }
                    let score = output.logits[(start * 12 + width) * LABELS.len() + label];
                    let end = start + width;
                    if score > threshold
                        && !(LABELS[label].1 == Kind::Person
                            && (start..=end).any(|w| is_field_word(&text[words[w].0..words[w].1])))
                    {
                        candidates.push((score, start, end, label));
                    }
                }
            }
        }
    } else if output.shape == [1, words.len(), LABELS.len(), 3]
        && output.logits.len() == words.len() * LABELS.len() * 3
    {
        let at = |word, label, tag| output.logits[(word * LABELS.len() + label) * 3 + tag];
        for (label, _) in LABELS.iter().enumerate() {
            if people_only && label != 0 {
                continue;
            }
            for start in 0..words.len() {
                if at(start, label, 0) <= threshold {
                    continue;
                }
                let mut inside = f32::INFINITY;
                for end in start..words.len() {
                    inside = inside.min(at(end, label, 2));
                    if inside < threshold {
                        break;
                    }
                    let score = inside.min(at(start, label, 0)).min(at(end, label, 1));
                    let spans_label = LABELS[label].1 == Kind::Person
                        && (start..=end).any(|w| is_field_word(&text[words[w].0..words[w].1]));
                    if score > threshold && !spans_label {
                        candidates.push((score, start, end, label));
                    }
                }
            }
        }
    } else {
        return Err("GLiNER devolvió una salida incompatible o incompleta.".into());
    }
    // A short first-name span can score above the full name. Preserve a
    // complete, already-above-threshold name instead of discarding its surname
    // during greedy overlap removal. Never extend beyond a model-proposed span.
    let complete_people: Vec<(usize, usize)> = candidates
        .iter()
        .filter_map(|&(_, start, end, label)| {
            if LABELS[label].1 != Kind::Person || end - start + 1 > 8 {
                return None;
            }
            let span = &text[words[start].0..words[end].1];
            let is_name = span.split_whitespace().all(|word| {
                !is_field_word(word)
                    && word
                        .chars()
                        .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '’' | '.' | ','))
            });
            is_name.then_some((start, end))
        })
        .collect();
    candidates.retain(|&(_, start, end, label)| {
        LABELS[label].1 != Kind::Person
            || !complete_people
                .iter()
                .any(|&(a, b)| a <= start && b >= end && (a < start || b > end))
    });
    // Specific kinds win over generic person spans, so a person candidate that
    // merely re-covers an identifier or a bank name can never shadow it. At
    // equal kind the longer span covers more of the printed value, and the
    // score only breaks true ties.
    candidates.sort_by(|a, b| {
        kind_priority(&LABELS[a.3].1)
            .cmp(&kind_priority(&LABELS[b.3].1))
            .then_with(|| (b.2 - b.1).cmp(&(a.2 - a.1)))
            .then_with(|| b.0.total_cmp(&a.0))
    });
    let mut selected: Vec<(usize, usize)> = vec![];
    let mut detections = vec![];
    let separator = Regex::new(r"(?i)\s+(?:y|e|and)\s+").expect("name conjunction");
    for (_, start, end, label) in candidates {
        if selected.iter().any(|&(a, b)| start <= b && end >= a) {
            continue;
        }
        selected.push((start, end));
        let span = &text[words[start].0..words[end].1];
        if LABELS[label].1 == Kind::Person {
            // A model-proposed span can contain two parties separated by a
            // conjunction. Keep both source names covered without assigning
            // them the same identity or erasing the connecting legal text.
            let parts: Vec<_> = separator.split(span).collect();
            if parts.len() > 1
                && parts.iter().all(|part| {
                    part.split_whitespace().count() >= 2
                        && part.chars().all(|c| {
                            c.is_alphabetic() || c.is_whitespace() || matches!(c, '-' | '\'' | '’')
                        })
                })
            {
                detections.extend(parts.into_iter().map(|part| Detection {
                    kind: Kind::Person,
                    text: part.into(),
                }));
                continue;
            }
        }
        detections.push(Detection {
            kind: LABELS[label].1,
            text: span.into(),
        });
    }
    Ok(detections)
}

/// Printed labels and generic legal words that can never be part of a name
/// span. Shared by the completeness preference and the final rejection.
fn is_field_word(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "banco"
            | "bank"
            | "cooperativa"
            | "total"
            | "usd"
            | "cedula"
            | "cédula"
            | "proceso"
            | "email"
            | "ordenante"
            | "beneficiario"
            | "titular"
            | "comprobante"
            | "transferencia"
            | "nombre"
            | "apellido"
            | "firma"
            | "fecha"
            | "monto"
            | "cuenta"
    )
}

/// The two GLiNER passes can propose overlapping spans for the same source
/// text (a "person" that starts inside an identifier and the identifier
/// itself). Keep one span per region, preferring the specific non-person kind
/// and, at equal kind, the longer span.
pub(crate) fn resolve_overlaps(text: &str, detections: Vec<Detection>) -> Vec<Detection> {
    let mut spans: Vec<(usize, usize, Detection)> = detections
        .into_iter()
        .filter_map(|detection| {
            text.find(&detection.text)
                .map(|start| (start, start + detection.text.len(), detection))
        })
        .collect();
    spans.sort_by(|a, b| {
        kind_priority(&a.2.kind)
            .cmp(&kind_priority(&b.2.kind))
            .then_with(|| (b.1 - b.0).cmp(&(a.1 - a.0)))
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut kept: Vec<(usize, usize)> = vec![];
    let mut result = vec![];
    for (start, end, detection) in spans {
        if kept.iter().any(|&(a, b)| start < b && end > a) {
            continue;
        }
        kept.push((start, end));
        result.push(detection);
    }
    result.sort_by_key(|detection| text.find(&detection.text));
    result
}

fn kind_priority(kind: &Kind) -> u8 {
    match kind {
        Kind::Id | Kind::Other | Kind::Case | Kind::Email | Kind::Phone | Kind::Address => 0,
        Kind::Company => 1,
        Kind::Person => 2,
    }
}
#[cfg(target_os = "android")]
pub(crate) struct GlinerEngine {
    tokenizer: Tokenizer,
    path: std::path::PathBuf,
    multi: bool,
}
#[cfg(target_os = "android")]
impl GlinerEngine {
    pub(crate) fn load(app: &tauri::AppHandle, id: &str) -> Result<Self, String> {
        let directory = crate::paths::models_dir(app);
        let (model_file, tokenizer_file, _) =
            crate::models::gliner_files(id).ok_or("GLiNER desconocido")?;
        let path = directory.join(model_file);
        let tokenizer_path = directory.join(tokenizer_file);
        let _timer = crate::performance::Timer::new("model_verify");
        for file in [&path, &tokenizer_path] {
            crate::models::verify_model_file(file).map_err(|_| {
                "GLiNER no está instalado completo o está dañado. Descárgalo desde Modelos."
            })?;
        }
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|_| "No se pudo leer el tokenizador GLiNER.")?;
        Ok(Self {
            tokenizer,
            path,
            multi: id == crate::models::GLINER_MULTI_ID,
        })
    }
}
#[cfg(target_os = "android")]
impl crate::llm::EntityExtractor for GlinerEngine {
    fn extract(
        &self,
        document: &str,
        progress: &mut dyn FnMut(usize),
    ) -> Result<Vec<Detection>, String> {
        crate::llm::check_cancelled()?;
        let mut detections = vec![];
        let words: Vec<_> = Regex::new(r"\w+(?:[-_]\w+)*|\S")
            .unwrap()
            .find_iter(document)
            .map(|w| (w.start(), w.end()))
            .collect();
        let mut start = 0;
        while start < words.len() {
            let mut end = words.len().min(start + if self.multi { 120 } else { 400 });
            // Tokenize both passes before running either. No tokenizer truncation
            // is permitted; bounded overlapping windows cover the entire input.
            let prepared = loop {
                let text = &document[words[start].0..words[end - 1].1];
                let first = prepare(&self.tokenizer, text, &self.path);
                let second =
                    prepare_with_person_label(&self.tokenizer, text, &self.path, "person name");
                if let (Ok(first), Ok(second)) = (first, second) {
                    let limit = if self.multi { 384 } else { 2048 };
                    if first.0.ids.len() <= limit && second.0.ids.len() <= limit {
                        break (text, first, second);
                    }
                }
                if end - start <= 1 {
                    return Err("Un fragmento no cabe en GLiNER; no se aprobó el documento.".into());
                }
                end = start + (end - start) / 2;
            };
            for (pass, (mut input, offsets)) in [prepared.1, prepared.2].into_iter().enumerate() {
                crate::llm::check_cancelled()?;
                input.max_width = if self.multi { 12 } else { 0 };
                let output = crate::android::run_gliner(
                    &serde_json::to_string(&input).map_err(|_| "Entrada GLiNER no válida.")?,
                )?;
                crate::llm::check_cancelled()?;
                progress(input.ids.len());
                let output =
                    serde_json::from_str(&output).map_err(|_| "GLiNER no completó el análisis.")?;
                detections.extend(if pass == 0 {
                    decode(prepared.0, &offsets, output)?
                } else {
                    decode_people(prepared.0, &offsets, output)?
                });
            }
            if end == words.len() {
                break;
            }
            start = end.saturating_sub(12).max(start + 1);
        }
        Ok(resolve_overlaps(document, detections))
    }
}
#[cfg(target_os = "android")]
impl Drop for GlinerEngine {
    fn drop(&mut self) {
        let _ = crate::android::unload_gliner();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_model_proposed_name_wins_over_higher_scoring_first_name() {
        let text = "Maria Perez";
        let words = vec![(0, 5), (6, text.len())];
        let mut logits = vec![-10.; 2 * 8 * 3];
        logits[0] = 5.;
        logits[1] = 5.;
        logits[2] = 5.;
        logits[8 * 3 + 1] = 0.5;
        logits[8 * 3 + 2] = 0.5;
        let hits = decode(
            text,
            &words,
            Output {
                shape: vec![1, 2, 8, 3],
                logits,
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, text);
    }
    #[test]
    fn name_focused_pass_keeps_complete_person_when_company_scores_higher() {
        let text = "DIEGO ALFONSO HERRERA LOZANO";
        let words: Vec<_> = Regex::new(r"\w+")
            .unwrap()
            .find_iter(text)
            .map(|hit| (hit.start(), hit.end()))
            .collect();
        let mut logits = vec![-10.; words.len() * 8 * 3];
        for word in 0..words.len() {
            logits[word * 8 * 3 + 2] = 1.;
        }
        logits[0] = 1.;
        logits[(words.len() - 1) * 8 * 3 + 1] = 1.;
        // An all-caps word is more strongly classified as a company.
        for tag in 0..3 {
            logits[3 + tag] = 5.;
        }
        let hits = decode_people(
            text,
            &words,
            Output {
                shape: vec![1, words.len(), 8, 3],
                logits,
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, Kind::Person);
        assert_eq!(hits[0].text, text);
    }

    #[test]
    fn conjunction_separates_source_names_without_erasing_legal_connector() {
        let text = "Diego Herrera y Ana Vera";
        let words: Vec<_> = Regex::new(r"\w+")
            .unwrap()
            .find_iter(text)
            .map(|hit| (hit.start(), hit.end()))
            .collect();
        let mut logits = vec![-10.; words.len() * 8 * 3];
        for word in 0..words.len() {
            logits[word * 8 * 3 + 2] = 1.;
        }
        logits[0] = 1.;
        logits[(words.len() - 1) * 8 * 3 + 1] = 1.;
        let hits = decode_people(
            text,
            &words,
            Output {
                shape: vec![1, words.len(), 8, 3],
                logits,
            },
        )
        .unwrap();
        assert_eq!(
            hits.iter().map(|hit| hit.text.as_str()).collect::<Vec<_>>(),
            vec!["Diego Herrera", "Ana Vera"]
        );
    }
    #[test]
    fn decoder_keeps_unicode_source_and_rejects_incomplete_tensor() {
        let text = "María Pérez";
        let words = vec![(0, 6), (7, text.len())];
        let mut logits = vec![-10.; 2 * 8 * 3];
        logits[0] = 5.;
        logits[2] = 5.;
        logits[8 * 3 + 1] = 5.;
        logits[8 * 3 + 2] = 5.;
        let hits = decode(
            text,
            &words,
            Output {
                shape: vec![1, 2, 8, 3],
                logits,
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, text);
        assert_eq!(hits[0].kind, Kind::Person);
        assert!(decode(
            text,
            &words,
            Output {
                shape: vec![1, 2, 8, 3],
                logits: vec![]
            }
        )
        .is_err());
    }
    #[test]
    #[ignore = "Requires scripts/check-gliner-names.py output from the pinned local ONNX model"]
    fn real_name_coverage() {
        let file = std::env::var("BLUEDOOR_GLINER_NAMES").unwrap();
        let cases: serde_json::Value =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        for case in cases.as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let words: Vec<(usize, usize)> = serde_json::from_value(case["words"].clone()).unwrap();
            let mut hits = vec![];
            for (index, output) in case["outputs"].as_array().unwrap().iter().enumerate() {
                let output = serde_json::from_value(output.clone()).unwrap();
                hits.extend(
                    if index == 0 {
                        decode(text, &words, output)
                    } else {
                        decode_people(text, &words, output)
                    }
                    .unwrap(),
                );
            }
            let entities = crate::privacy::merge_detection_sources(text, hits, vec![]).unwrap();
            let protected = crate::privacy::sanitize(text, &entities).unwrap();
            for name in case["names"].as_array().unwrap() {
                let name = name.as_str().unwrap();
                assert!(
                    entities.iter().any(|entity| entity.original == name),
                    "incomplete model coverage: {name}; entities={entities:?}"
                );
                for word in name
                    .split_whitespace()
                    .filter(|word| word.chars().count() > 3)
                {
                    assert!(!protected.contains(word), "residual name word: {word}");
                }
            }
            for preserved in case["preserved"].as_array().unwrap() {
                assert!(
                    protected.contains(preserved.as_str().unwrap()),
                    "legal prose was consumed: {protected}"
                );
            }
        }
    }
    #[test]
    fn person_span_that_swallows_a_field_label_does_not_hide_the_identifier() {
        let text = "CEDULA 0912345678";
        let words = vec![(0, 6), (7, text.len())];
        let mut logits = vec![-10.; 2 * 8 * 3];
        // Person proposal covering the label and the number.
        for tag in 0..3 {
            logits[0 * 8 * 3 + tag] = 5.;
            logits[1 * 8 * 3 + tag] = 5.;
        }
        // Identifier proposal for the number itself.
        for tag in 0..3 {
            logits[1 * 8 * 3 + 2 * 3 + tag] = 4.;
        }
        let hits = decode(
            text,
            &words,
            Output {
                shape: vec![1, 2, 8, 3],
                logits,
            },
        )
        .unwrap();
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.kind, hit.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(Kind::Id, "0912345678")]
        );
    }
    #[test]
    fn overlapping_passes_keep_the_identifier_and_drop_the_person_fragment() {
        let text = "CEDULA 0912345678";
        let hits = resolve_overlaps(
            text,
            vec![
                Detection {
                    kind: Kind::Person,
                    text: "CEDULA 0912345678".into(),
                },
                Detection {
                    kind: Kind::Id,
                    text: "0912345678".into(),
                },
                Detection {
                    kind: Kind::Id,
                    text: "0912345678".into(),
                },
            ],
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, Kind::Id);
        assert_eq!(hits[0].text, "0912345678");
    }
    #[test]
    #[ignore = "Requires the pinned tokenizer and Python reference fixture"]
    fn tokenizer_matches_reference() {
        let directory = std::env::var("BLUEDOOR_GLINER_QA").unwrap();
        let directory = Path::new(&directory);
        let tokenizer = Tokenizer::from_file(directory.join("tokenizer.json")).unwrap();
        let text = "Maria Perez realizó una transferencia desde BANCO ANDINO. Cédula 0912345678. Email maria@example.com. Proceso 17230-2025-01234. Total USD 1500.";
        let (input, _) = prepare(&tokenizer, text, Path::new("model.onnx")).unwrap();
        let reference: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("test-input.json")).unwrap())
                .unwrap();
        assert_eq!(serde_json::to_value(input.ids).unwrap(), reference["ids"]);
        assert_eq!(serde_json::to_value(input.mask).unwrap(), reference["mask"]);
    }
}
