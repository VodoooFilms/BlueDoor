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
    let splitter = Regex::new(r"\w+(?:[-_]\w+)*|\S").expect("fixed splitter");
    let words: Vec<_> = splitter.find_iter(text).collect();
    if words.is_empty() {
        return Err("El fragmento no contiene texto.".into());
    }
    let mut input: Vec<String> = LABELS
        .iter()
        .flat_map(|(label, _)| ["<<ENT>>".into(), (*label).into()])
        .collect();
    input.push("<<SEP>>".into());
    let skip = input.len() as u32;
    input.extend(words.iter().map(|word| word.as_str().into()));
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
        },
        words
            .iter()
            .map(|word| (word.start(), word.end()))
            .collect(),
    ))
}
fn decode(text: &str, words: &[(usize, usize)], output: Output) -> Result<Vec<Detection>, String> {
    // This pinned graph exports [batch, words, labels, start/end/inside].
    if output.shape != [1, words.len(), LABELS.len(), 3]
        || output.logits.len() != words.len() * LABELS.len() * 3
        || output.logits.iter().any(|score| !score.is_finite())
    {
        return Err("GLiNER devolvió una salida incompatible o incompleta.".into());
    }
    let threshold = (0.35f32 / 0.65).ln();
    let at = |word, label, tag| output.logits[(word * LABELS.len() + label) * 3 + tag];
    let mut candidates = vec![];
    for (label, _) in LABELS.iter().enumerate() {
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
                if score > threshold {
                    candidates.push((score, start, end, label));
                }
            }
        }
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
                let lowered = word.to_lowercase();
                ![
                    "banco",
                    "bank",
                    "cooperativa",
                    "total",
                    "usd",
                    "cedula",
                    "cédula",
                    "proceso",
                    "email",
                    "ordenante",
                    "beneficiario",
                    "titular",
                    "comprobante",
                    "transferencia",
                ]
                .contains(&lowered.as_str())
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
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut selected: Vec<(usize, usize)> = vec![];
    let mut detections = vec![];
    for (_, start, end, label) in candidates {
        if selected.iter().any(|&(a, b)| start <= b && end >= a) {
            continue;
        }
        selected.push((start, end));
        detections.push(Detection {
            kind: LABELS[label].1,
            text: text[words[start].0..words[end].1].into(),
        });
    }
    Ok(detections)
}
#[cfg(target_os = "android")]
pub(crate) struct GlinerEngine {
    tokenizer: Tokenizer,
    path: std::path::PathBuf,
}
#[cfg(target_os = "android")]
impl GlinerEngine {
    pub(crate) fn load(app: &tauri::AppHandle) -> Result<Self, String> {
        let directory = crate::paths::models_dir(app);
        let path = directory.join(crate::models::GLINER_FILE);
        let tokenizer_path = directory.join(crate::models::GLINER_TOKENIZER_FILE);
        let _timer = crate::performance::Timer::new("model_verify");
        for file in [&path, &tokenizer_path] {
            crate::models::verify_model_file(file).map_err(|_| {
                "GLiNER no está instalado completo o está dañado. Descárgalo desde Modelos."
            })?;
        }
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|_| "No se pudo leer el tokenizador GLiNER.")?;
        Ok(Self { tokenizer, path })
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
        let (input, words) = prepare(&self.tokenizer, document, &self.path)?;
        let output = crate::android::run_gliner(
            &serde_json::to_string(&input).map_err(|_| "Entrada GLiNER no válida.")?,
        )?;
        crate::llm::check_cancelled()?;
        progress(input.ids.len());
        decode(
            document,
            &words,
            serde_json::from_str(&output).map_err(|_| "GLiNER no completó el análisis.")?,
        )
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
