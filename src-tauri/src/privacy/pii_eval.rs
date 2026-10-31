use super::*;
use std::collections::BTreeMap;

/// Offline regression gate for the actual production deterministic detector.
/// This does NOT claim to measure Qwen or GLiNER inference.
#[test]
#[ignore = "explicit corpus gate: pnpm eval:pii"]
fn adversarial_pii_eval() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/pii_eval/corpus.json")).unwrap();
    let mut counts: BTreeMap<String, [usize; 4]> = BTreeMap::new(); // gold, fully covered, true positive spans, false positive spans
    for row in rows.as_array().unwrap() {
        let text = row["text"].as_str().unwrap();
        let hits = merge_detection_sources(text, vec![], vec![])
            .unwrap()
            .into_iter()
            .map(|e| Detection {
                kind: e.kind,
                text: e.original,
            })
            .collect::<Vec<_>>();
        let gold = row["gold"].as_array().unwrap();
        for span in gold {
            let category = span["category"].as_str().unwrap();
            let value = span["value"].as_str().unwrap();
            let start = text.find(value).expect("gold must occur in source");
            let end = start + value.len();
            let mut covered = vec![false; value.len()];
            for hit in &hits {
                for (offset, _) in text.match_indices(&hit.text) {
                    for pos in start.max(offset)..end.min(offset + hit.text.len()) {
                        covered[pos - start] = true;
                    }
                }
            }
            let c = counts.entry(category.into()).or_default();
            c[0] += 1;
            c[1] += usize::from(covered.iter().all(|v| *v));
        }
        // A precision TP must exactly match a gold value and category.
        // Partial byte protection still counts as a recall miss.
        let mut unique = HashSet::new();
        for hit in &hits {
            for (start, _) in text.match_indices(&hit.text) {
                if !unique.insert((start, start + hit.text.len())) {
                    continue;
                }
                let category = if structured::rut_valid(&hit.text) {
                    "rut"
                } else if structured::iban_valid(&hit.text) {
                    "iban"
                } else if structured::luhn_valid(&hit.text) {
                    "card"
                } else {
                    match hit.kind {
                        Kind::Person => "person",
                        Kind::Address => "address",
                        Kind::Email => "email",
                        Kind::Phone => "phone",
                        Kind::Id => "cedula",
                        Kind::Other if looks_like_date_or_amount(&hit.text) => "dob",
                        _ => "other",
                    }
                };
                let exact = gold.iter().any(|g| {
                    g["value"].as_str() == Some(&hit.text)
                        && g["category"].as_str() == Some(category)
                });
                counts.entry(category.into()).or_default()[if exact { 2 } else { 3 }] += 1;
            }
        }
    }
    let gold: usize = counts.values().map(|c| c[0]).sum();
    let covered: usize = counts.values().map(|c| c[1]).sum();
    let tp: usize = counts.values().map(|c| c[2]).sum();
    let fp: usize = counts.values().map(|c| c[3]).sum();
    let categories: BTreeMap<_,_> = counts.iter().map(|(k,c)| (k, serde_json::json!({
        "gold":c[0], "covered":c[1], "tp":c[2], "fp":c[3],
        "recall": if c[0]>0 { Some(c[1] as f64/c[0] as f64) } else { None },
        "precision": if c[2]+c[3]>0 { Some(c[2] as f64/(c[2]+c[3]) as f64) } else { None }
    }))).collect();
    let report = serde_json::json!({"layer":"production deterministic rules only", "categories":categories,
        "gold":gold,"covered":covered,"falsePositiveSpans":fp,"recall":covered as f64/gold as f64,"precision":tp as f64/(tp+fp) as f64});
    println!(
        "PII_EVAL {}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    if let Ok(path) = std::env::var("PII_EVAL_REPORT") {
        std::fs::write(path, report.to_string()).unwrap();
    }
    assert!(
        covered * 100 >= gold * 99,
        "PII recall below 99%: {covered}/{gold}"
    );
}
