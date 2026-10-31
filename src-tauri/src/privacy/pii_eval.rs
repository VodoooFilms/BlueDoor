use super::*;
use std::collections::BTreeMap;

/// Offline regression gate for the actual production deterministic detector.
/// This does NOT claim to measure Qwen or GLiNER inference.
#[test]
#[ignore = "explicit corpus gate: pnpm eval:pii"]
fn adversarial_pii_eval() {
    let rows: serde_json::Value = serde_json::from_str(include_str!("../../../tests/pii_eval/corpus.json")).unwrap();
    let mut counts: BTreeMap<String, [usize; 3]> = BTreeMap::new(); // gold, covered, false positives
    for row in rows.as_array().unwrap() {
        let text = row["text"].as_str().unwrap();
        let hits = deterministic_detections(text);
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
        // Each unique predicted span is a TP only if fully within an annotated
        // private value. Partial protection still counts as a recall miss.
        let mut unique = HashSet::new();
        for hit in &hits {
            for (start, _) in text.match_indices(&hit.text) {
                if !unique.insert((start, start + hit.text.len())) { continue; }
                let owner = gold.iter().find(|g| {
                    let value = g["value"].as_str().unwrap();
                    let offset = text.find(value).unwrap();
                    offset <= start && start + hit.text.len() <= offset + value.len()
                });
                if owner.is_none() {
                    counts.entry("unassigned".into()).or_default()[2] += 1;
                }
            }
        }
    }
    let gold: usize = counts.values().map(|c| c[0]).sum();
    let covered: usize = counts.values().map(|c| c[1]).sum();
    let fp: usize = counts.values().map(|c| c[2]).sum();
    let report = serde_json::json!({"layer":"production deterministic rules only", "categories":counts,
        "gold":gold,"covered":covered,"falsePositiveSpans":fp,"recall":covered as f64/gold as f64});
    println!("PII_EVAL {}", serde_json::to_string_pretty(&report).unwrap());
    if let Ok(path) = std::env::var("PII_EVAL_REPORT") { std::fs::write(path, report.to_string()).unwrap(); }
    assert!(covered * 100 >= gold * 99, "PII recall below 99%: {covered}/{gold}");
}
