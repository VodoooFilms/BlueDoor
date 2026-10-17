//! Opt-in measurements against external, independently annotated documents.
//! This records every failure; it does not assert that redaction is perfect.
use super::*;
use std::path::Path;

#[test]
#[ignore = "Requires downloaded validation data and verified local Qwen weights"]
fn measure_dataset_workflow() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../artifacts/benchmark");
    let fixture_name = std::env::var("LOCAL_LEGAL_BENCHMARK_FILE")
        .unwrap_or_else(|_| "openpii-validation.jsonl".into());
    let output_name = std::env::var("LOCAL_LEGAL_BENCHMARK_OUTPUT")
        .unwrap_or_else(|_| "baseline-results.json".into());
    let fixture = std::fs::read_to_string(root.join(fixture_name)).unwrap();
    let model = match std::env::var("LOCAL_LEGAL_BENCHMARK_MODEL") {
        Ok(path) => {
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let path = std::path::PathBuf::from(path);
            let expected = std::env::var("LOCAL_LEGAL_BENCHMARK_MODEL_SHA256")
                .expect("External evaluation weights require an explicit SHA-256");
            let mut hash = Sha256::new();
            let mut file = std::fs::File::open(&path).unwrap();
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let count = file.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            assert_eq!(format!("{:x}", hash.finalize()), expected);
            path
        }
        Err(_) => {
            let path = crate::platform::test_models_dir().join(crate::models::QWEN_FILE);
            crate::models::verify_model_file(&path).unwrap();
            path
        }
    };
    let engine = crate::llm::LlmEngine::load(model.to_str().unwrap()).unwrap();
    if let Ok(destination) = std::env::var("LOCAL_LEGAL_EXPORT_TRAINING_CONTRACT") {
        std::fs::write(destination, engine.privacy_prompt_for_test("{{DOCUMENT}}")).unwrap();
        return;
    }
    let mut rows: Vec<serde_json::Value> = fixture
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    if let Ok(path) = std::env::var("LOCAL_LEGAL_PAYPAL_DOCUMENT") {
        let helper =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime/local-legal-ocr");
        let output = std::process::Command::new(helper)
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        rows.push(serde_json::json!({"uid":"paypal-local", "language":"en", "source_text":String::from_utf8(output.stdout).unwrap(), "privacy_mask":[]}));
    }
    let mut results = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let original = row["source_text"].as_str().unwrap();
        let timer = std::time::Instant::now();
        let outcome = analyze_with_progress(&engine, original, &mut |_, _, _| {});
        let result = match outcome {
            Ok(analysis) => match sanitize(original, &analysis.entities) {
                Ok(sanitized) => {
                    serde_json::json!({"input":row,"sanitized":sanitized,"entities":analysis.entities,"complete":analysis.complete(),"failedFragments":analysis.failed_fragments,"seconds":timer.elapsed().as_secs_f64()})
                }
                Err(error) => {
                    serde_json::json!({"input":row,"error":error,"seconds":timer.elapsed().as_secs_f64()})
                }
            },
            Err(error) => {
                serde_json::json!({"input":row,"error":error,"seconds":timer.elapsed().as_secs_f64()})
            }
        };
        println!(
            "Measured document {} in {:.1}s (complete={})",
            index + 1,
            timer.elapsed().as_secs_f64(),
            result["complete"]
        );
        results.push(result);
        std::fs::write(
            root.join(&output_name),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
    }
}
