use super::*;
use std::path::Path;

/// llama.cpp initialises one Metal device per context and refuses a second
/// concurrent one, so the real-inference tests must not run in parallel.
/// The app itself never has this problem: `analyze_private_document`
/// serialises work behind a `try_lock` on the engine cache.
static REAL_INFERENCE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Explicit local QA: the input stays outside the repository. Measure actual
/// inference and retries, rather than inferring capacity from page count.
#[test]
#[ignore = "Requires BLUEDOOR_QA_LAYOUT and verified local Qwen weights"]
fn real_qwen_user_pdf_capacity() {
    let path = std::env::var("BLUEDOOR_QA_LAYOUT").expect("set local layout JSON path");
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let text = value["text"].as_str().unwrap();
    let (_guard, engine) = locked_qwen_engine();
    struct Measured<'a> {
        engine: &'a crate::llm::LlmEngine,
        calls: std::cell::Cell<usize>,
        errors: std::cell::Cell<usize>,
    }
    impl EntityExtractor for Measured<'_> {
        fn extract(
            &self,
            text: &str,
            progress: &mut dyn FnMut(usize),
        ) -> Result<Vec<Detection>, String> {
            self.calls.set(self.calls.get() + 1);
            let start = std::time::Instant::now();
            let prompt = self.engine.privacy_prompt_for_test(text);
            let prompt_tokens = self.engine.count_tokens_for_test(&prompt).unwrap();
            let mut output_tokens = 0;
            let outcome = self.engine.extract(text, &mut |n| {
                output_tokens = n;
                progress(n);
            });
            if outcome.is_err() {
                self.errors.set(self.errors.get() + 1);
            }
            eprintln!("CAPACITY call={} chars={} prompt_tokens={} output_tokens={} seconds={:.2} result={}",
                self.calls.get(), text.chars().count(), prompt_tokens, output_tokens, start.elapsed().as_secs_f64(),
                match &outcome { Ok(v) => format!("{} suggestions", v.len()), Err(e) => e.clone() });
            outcome
        }
        fn extract_exact(
            &self,
            text: &str,
            progress: &mut dyn FnMut(usize),
        ) -> Result<Vec<Detection>, String> {
            self.engine.extract_exact(text, progress)
        }
    }
    let measured = Measured {
        engine: &engine,
        calls: std::cell::Cell::new(0),
        errors: std::cell::Cell::new(0),
    };
    let start = std::time::Instant::now();
    let analysis = analyze_with_progress(&measured, text, &mut |_, _, _| {}).unwrap();
    eprintln!("CAPACITY pages={} source_chars={} calls={} errors={} fragments={} failed={} entities={} seconds={:.2}",
        value["pages"].as_array().unwrap().len(), text.chars().count(), measured.calls.get(), measured.errors.get(),
        analysis.fragments, analysis.failed_fragments, analysis.entities.len(), start.elapsed().as_secs_f64());
    assert!(analysis.complete(), "all source fragments must be analyzed");
    if let Ok(path) = std::env::var("BLUEDOOR_QA_RESULT") {
        let bytes = serde_json::to_vec(&analysis.entities).unwrap();
        crate::store::atomic_write(Path::new(&path), |file| {
            use std::io::Write;
            file.write_all(&bytes).map_err(|e| e.to_string())
        })
        .unwrap();
    }
    let calls = engine.generations_for_test();
    for pass in 1..=2 {
        let started = std::time::Instant::now();
        let repeated = analyze_with_progress(&engine, text, &mut |_, _, _| {}).unwrap();
        assert!(repeated.complete());
        // Failed large tasks may be tried again; their successful children must
        // be reused. No new inference is permitted for completed source blocks.
        let extra = engine.generations_for_test() - calls;
        eprintln!(
            "CAPACITY repeat={} seconds={:.2} additional_generations={extra}",
            pass,
            started.elapsed().as_secs_f64()
        );
        assert_eq!(extra, 0, "a completed document must not run Qwen again");
    }
}

/// Loads a verified Qwen engine while holding the serialisation lock. The
/// returned guard must stay alive for as long as the engine is used.
fn locked_qwen_engine() -> (std::sync::MutexGuard<'static, ()>, crate::llm::LlmEngine) {
    let guard = REAL_INFERENCE.lock().unwrap_or_else(|e| e.into_inner());
    let path = crate::platform::test_models_dir().join(crate::models::QWEN_FILE);
    crate::models::verify_model_file(&path).expect("install verified Qwen model first");
    let engine = crate::llm::LlmEngine::load(path.to_str().unwrap()).unwrap();
    (guard, engine)
}
fn entity(kind: Kind, original: &str, alias: &str) -> Entity {
    Entity {
        kind,
        original: original.into(),
        alias: alias.into(),
        source: "test".into(),
    }
}
#[test]
fn ocr_receipt_name_fragments_and_masked_accounts_are_redacted() {
    let original = "José Antonio Martínez Valdez comparece.\nXIMENA BARRAGÁN CON SEDE EN LA PARROQUIA\nnombre\nJOSE ANTONIO MI\nE\nJOSE ANTONIO MARTIN\nJOSE ANTONIO MARTA\n4410XXXXX319\n2204XXX521\nSeptiembre 2025-2026\nBANCO PICHINCHA\n$500.00";
    let entities = merge_detections(
        original,
        vec![Detection {
            kind: Kind::Person,
            text: "José Antonio Martínez Valdez".into(),
        }],
    )
    .unwrap();
    let out = sanitize(original, &entities).unwrap();
    for value in ["XIMENA BARRAGÁN", "JOSE ANTONIO", "4410", "2204"] {
        assert!(!out.contains(value), "{out}");
    }
    let aliases: HashSet<_> = entities
        .iter()
        .filter(|e| {
            e.original.starts_with("JOSE ANTONIO") || e.original.starts_with("José Antonio")
        })
        .map(|e| &e.alias)
        .collect();
    assert_eq!(aliases.len(), 1);
    assert!(out.contains("2025-2026"));
    assert!(out.contains("$500.00"));
    assert!(!out.contains("BANCO PICHINCHA"));
    assert!(out.contains("COMPANY_01"));
    assert!(leak_warnings(&out, &entities).is_empty());
    assert!(!leak_warnings("4410XXXXX319", &[]).is_empty());
}
#[test]
fn ocr_variant_detection_does_not_consume_prose_or_infer_one_token_names() {
    let original = "José Antonio Martínez comparece.\nJOSE ANTONIO PRESENTA EL ESCRITO PARA SU DEFENSA\nJOSE OTRO APELLIDO\nJOSE ANTONIO SOLICITA ASISTENCIA\nJosé Antonio solicita asistencia.\n2025–2026\n2025—2026";
    let entities = merge_detections(
        original,
        vec![Detection {
            kind: Kind::Person,
            text: "José Antonio Martínez".into(),
        }],
    )
    .unwrap();
    let out = sanitize(original, &entities).unwrap();
    assert!(out.contains("JOSE ANTONIO PRESENTA EL ESCRITO PARA SU DEFENSA"));
    assert!(out.contains("JOSE OTRO APELLIDO"));
    assert!(out.contains("JOSE ANTONIO SOLICITA ASISTENCIA"));
    assert!(out.contains("José Antonio solicita asistencia."));
    assert!(out.contains("2025–2026\n2025—2026"));
}
#[test]
fn stable_aliases_preserve_facts_dates_amounts_and_unicode() {
    let original="María Pérez demanda a Banco ABC. MARÍA PÉREZ reclama USD 1.500,00 el 2026-09-30. Correo: maria@example.com; cédula: 0912345678.";
    let entities = merge_detections(
        original,
        vec![
            Detection {
                kind: Kind::Person,
                text: "María Pérez".into(),
            },
            Detection {
                kind: Kind::Company,
                text: "Banco ABC".into(),
            },
        ],
    )
    .unwrap();
    let out = sanitize(original, &entities).unwrap();
    assert!(out.contains("PERSON_01 demanda a COMPANY_01. PERSON_01"));
    assert!(out.contains("USD 1.500,00 el 2026-09-30"));
    assert!(!out.contains("0912345678"));
    assert!(!out.contains("maria@example.com"));
    assert!(leak_warnings(&out, &entities).is_empty());
}
#[test]
fn patterns_cover_ids_email_phone_address_case_and_account() {
    let text="Ana. email: ana@example.com. Tel: +593 99 123 4567; ID 0912345678; RUC 1790012345001. Proceso 17230-2025-01234. Dirección: Av. Amazonas 123, Quito\nCuenta bancaria: EC1234567890";
    let entities = merge_detections(text, vec![]).unwrap();
    for kind in [
        Kind::Email,
        Kind::Phone,
        Kind::Id,
        Kind::Case,
        Kind::Address,
        Kind::Other,
    ] {
        assert!(entities.iter().any(|e| e.kind == kind), "{kind:?}");
    }
    let out = sanitize(text, &entities).unwrap();
    assert!(leak_warnings(&out, &entities).is_empty(), "{out}");
}
#[test]
fn compact_judicial_numbers_are_redacted_and_residuals_block_review() {
    for text in [
        "JUICIO DE ALIMENTOS No. 17203201900137",
        "Juicio No.\n17203201900137",
        "Proceso: 17203-2019-00137",
        "Causa número 17203201900137",
        "Referencia 17203201900137",
    ] {
        let entities = merge_detections(text, vec![]).unwrap();
        assert!(entities.iter().any(|e| e.kind == Kind::Case), "{text}");
        let output = sanitize(text, &entities).unwrap();
        assert!(output.contains("CASE_01"), "{output}");
        assert!(!output.contains("17203"), "{output}");
        assert!(leak_warnings(&output, &entities).is_empty());
        assert!(!leak_warnings(text, &[]).is_empty());
    }
    let amount = "Total USD 17203201900137";
    assert!(deterministic_detections(amount).is_empty());
}
#[test]
fn clearing_analysis_keeps_original_and_invalidates_export() {
    let mut session =
        Session::new("original.pdf".into(), "Documento original privado".into()).unwrap();
    session.sanitized = "Documento protegido".into();
    session.analyzed = true;
    session.reviewed = true;
    session.analysis_complete = true;
    session.edited = true;
    session.warnings.push("advertencia".into());
    session.advisories.push("aviso".into());
    let revision = session.revision;
    reset_analysis(&mut session);
    assert_eq!(session.original, "Documento original privado");
    assert_eq!(session.extracted_original, session.original);
    assert_eq!(session.file_name, "original.pdf");
    assert_eq!(session.revision, revision + 1);
    assert!(!session.analyzed && !session.reviewed && !session.edited);
    assert!(session.sanitized.is_empty() && session.entities.is_empty());
    assert!(session.warnings.is_empty() && session.advisories.is_empty());
    assert!(
        check_export_payload(&session, "Documento protegido", None, "Documento protegido").is_err()
    );
}

#[test]
fn automatic_ocr_cleanup_preserves_boundaries_and_initial_extraction() {
    let mut session =
        Session::new("scan.PDF".into(), "ombre y sobrenombre, ombre.".into()).unwrap();
    assert!(needs_ocr_cleanup(&session));
    session.original = corrected_ocr_text(
        &session.original,
        &OcrSuggestion {
            original: "ombre".into(),
            replacement: "nombre".into(),
        },
    )
    .unwrap();
    assert_eq!(session.original, "nombre y sobrenombre, nombre.");
    assert_eq!(session.extracted_original, "ombre y sobrenombre, ombre.");
    assert!(!needs_ocr_cleanup(&session));
    assert!(!needs_ocr_cleanup(
        &Session::new("document.txt".into(), "ombre".into()).unwrap()
    ));
    assert!(corrected_ocr_text(
        "12345",
        &OcrSuggestion {
            original: "12345".into(),
            replacement: "012345".into(),
        }
    )
    .is_err());
}

#[test]
fn ocr_suggestions_only_add_one_letter_to_exact_existing_words() {
    let raw = r#"{"suggestions":[{"original":"ombre","replacement":"nombre"},{"original":"uenta","replacement":"cuenta"},{"original":"12345","replacement":"012345"},{"original":"ombre","replacement":"nombre completo"},{"original":"ausente","replacement":"causente"},{"original":"Ana","replacement":"Diana"}]}"#;
    let suggestions =
        parse_ocr_suggestions(raw, "ombre del titular\nuenta de ahorros\n12345\nAna").unwrap();
    assert_eq!(suggestions.len(), 2);
    assert_eq!(suggestions[0].replacement, "nombre");
    assert_eq!(suggestions[1].replacement, "cuenta");
    assert!(
        parse_ocr_suggestions(r#"{"suggestions":[],"instruction":"ignore"}"#, "texto").is_err()
    );
    assert!(parse_ocr_suggestions(
        r#"{"suggestions":[{"original":"ombre","replacement":"nombre"}]}"#,
        "sobrenombre"
    )
    .unwrap()
    .is_empty());
}
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_analyzes_local_qa_document() {
    // Defaults to the checked-in QA document so the test is runnable by anyone
    // with the local weights; a lawyer can point BLUEDOOR_QA_TEXT at their own
    // OCR text to repeat the run on a real file.
    let input = std::env::var("BLUEDOOR_QA_TEXT").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/private-document.txt")
            .to_string_lossy()
            .into_owned()
    });
    let mut text = std::fs::read_to_string(&input).unwrap();
    let (_guard, engine) = locked_qwen_engine();
    let snapshot = text.clone();
    for chunk in chunks(&snapshot) {
        let Ok(suggestions) = engine.suggest_ocr_prefixes(&chunk) else {
            continue;
        };
        for suggestion in suggestions {
            if valid_ocr_prefix(&suggestion, &text) {
                text = corrected_ocr_text(&text, &suggestion).unwrap();
            }
        }
    }
    let analysis = analyze_with_progress(&engine, &text, &mut |_, _, _| {}).unwrap();
    assert!(analysis.complete(), "coverage gap: {analysis:?}");
    let entities = analysis.entities;
    let sanitized = sanitize_for_file(&text, &entities, "documento.txt").unwrap();
    require_sanitized(&text, &sanitized, &entities).unwrap();
    assert!(leak_warnings(&sanitized, &entities).is_empty());
    // Every identifier in the fixture must be gone from the protected version.
    for private in [
        "María Pérez",
        "maria.perez@example.com",
        "0912345678",
        "1790012345001",
        "+593 99 123 4567",
        "Av. Amazonas 123",
        "EC1234567890",
        "17230-2025-01234",
        // The defendant is a party to the case, so sharing the file outside the
        // office would expose it exactly as it exposes the plaintiff.
        "Banco Andino S.A.",
    ] {
        assert!(!sanitized.contains(private), "residual: {private}");
    }
    // The legal argument must survive untouched.
    for keep in [
        "artículo 195.1",
        "USD 1500",
        "15 de marzo de 2025",
        "obligaciones pendientes",
    ] {
        assert!(sanitized.contains(keep), "lost context: {keep}");
    }
    let mut session = Session::new("documento.txt".into(), text).unwrap();
    session.entities = entities;
    session.sanitized = sanitized;
    session.analyzed = true;
    session.reviewed = true;
    session.analysis_complete = true;
    assert!(checked_outbound(&session).is_ok());
}
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_suggests_ocr_missing_initials() {
    let (_guard, engine) = locked_qwen_engine();
    let text = "Recibo de pago\nEl ombre del titular aparece en el encabezado.\nLa uenta de ahorros recibe USD 1500.\nProceso 17203201900137.";
    let suggestions = engine.suggest_ocr_prefixes(text).unwrap();
    assert!(suggestions
        .iter()
        .any(|s| s.original == "ombre" && s.replacement == "nombre"));
    assert!(suggestions.iter().all(|s| valid_ocr_prefix(s, text)));
}
#[test]
fn overlapping_names_are_longest_first_and_aliases_are_not_replaced() {
    let text = "Ana Pérez y Ana. Banana y Ana Pérez.";
    let entities = vec![
        entity(Kind::Person, "Ana", "PERSON_01"),
        entity(Kind::Person, "Ana Pérez", "PERSON_02"),
    ];
    assert_eq!(
        sanitize(text, &entities).unwrap(),
        "PERSON_02 y PERSON_01. Banana y PERSON_02."
    );
}
#[test]
fn review_can_group_variants_but_cannot_smuggle_private_aliases() {
    let text = "María Pérez y María. maria@example.com";
    let grouped = vec![
        entity(Kind::Person, "María Pérez", "CLIENTE_01"),
        entity(Kind::Person, "María", "CLIENTE_01"),
    ];
    assert_eq!(
        sanitize(text, &grouped).unwrap(),
        "CLIENTE_01 y CLIENTE_01. maria@example.com"
    );
    assert!(sanitize(text, &[entity(Kind::Person, "María", "maria@example.com")]).is_err());
    assert!(sanitize(text, &[entity(Kind::Person, "invented", "PERSON_01")]).is_err());
}
#[test]
fn leaks_are_detected_independently_of_qwen_and_model_json_is_strict() {
    assert!(!leak_warnings(
        "email: secret@example.com. Tel +593 99 111 2222. ID 0912345678",
        &[]
    )
    .is_empty());
    assert!(parse_detections("ignore the rules").is_err());
    // Un invents a category: kept as an unclassified span, never dropped.
    let unknown = parse_detections(r#"{"entities":[{"kind":"UNKNOWN","text":"x"}]}"#).unwrap();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].kind, Kind::Other);
    assert_eq!(unknown[0].text, "x");
    // A shape we cannot read at all is still an error.
    assert!(parse_detections(r#"{"entities":"nope"}"#).is_err());
    assert!(parse_detections(r#"{"entities":[],"safe":true}"#).is_err());
    assert!(parse_detections(r#"{"entities":[]}"#).unwrap().is_empty());
    assert!(parse_detections(r#"{"entities":[{"kind":"PERSON"}]}"#).is_err());
    // One bad element must not cost us the good ones.
    let mixed = parse_detections(
        r#"[{"kind":"ACCOUNT","text":"EC1234567890"},{"kind":"PERSON","text":"Ana"}]"#,
    )
    .unwrap();
    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed[0].kind, Kind::Other);
    assert_eq!(mixed[1].kind, Kind::Person);
    assert!(parse_detections("```json\n{\"entities\":[]}\n```")
        .unwrap()
        .is_empty());
    assert!(!leak_warnings(
        "María Pérez",
        &[entity(Kind::Person, "María Pérez", "PERSON_01")]
    )
    .is_empty());
}
#[test]
fn complete_document_is_chunked_without_losing_unicode_or_tail() {
    let text = "á".repeat(6000) + " FINAL_PRIVATE";
    let parts = chunks(&text);
    // Every fragment plus the tail, which must never be dropped.
    assert!(
        parts.last().unwrap().ends_with("FINAL_PRIVATE"),
        "el último fragmento perdió la cola"
    );
    assert_eq!(
        parts[0].chars().count(),
        crate::privacy::chunk_chars_for_test(),
        "un fragmento lleno debe medir CHUNK_CHARS"
    );
    // 6001 characters span more than one fragment at any size that fits the
    // answer budget, and the tail still lands in its own final fragment.
    let size = crate::privacy::chunk_chars_for_test();
    assert!(
        parts.len() >= 2 && 6001 > size,
        "6001 caracteres deberían necesitar más de un fragmento de {size}"
    );

    // A document several fragments long still ends in its true last fragment.
    let long = "á".repeat(CHUNK_CHARS * 3) + " FINAL_PRIVATE";
    let parts = chunks(&long);
    assert!(parts.last().unwrap().ends_with("FINAL_PRIVATE"));

    // The overlap exists so an entity cut by a fragment boundary is still seen
    // whole by one of them: here the name starts 20 characters before the cut.
    let straddled = "á".repeat(CHUNK_CHARS - 20) + "MARÍA PÉREZ" + &"á".repeat(1000);
    let parts = chunks(&straddled);
    assert!(
        parts.iter().any(|p| p.contains("MARÍA PÉREZ")),
        "an entity split across a boundary must survive in one fragment"
    );
}
#[test]
fn banks_are_companies_and_receipt_types_remain_visible() {
    let text = "BANCO PICHINCHA\nAHORROS SOCIC\n4410XXXXX319";
    let entities = merge_detections(
        text,
        vec![
            Detection {
                kind: Kind::Address,
                text: "BANCO PICHINCHA".into(),
            },
            Detection {
                kind: Kind::Address,
                text: "AHORROS SOCIC".into(),
            },
        ],
    )
    .unwrap();
    assert!(entities
        .iter()
        .any(|e| e.kind == Kind::Company && e.original == "BANCO PICHINCHA"));
    assert!(!entities.iter().any(|e| e.original == "AHORROS SOCIC"));
    let out = sanitize(text, &entities).unwrap();
    assert!(out.contains("COMPANY_01\nAHORROS SOCIC\nOTHER_01"));
}
#[test]
fn ocr_noise_is_annotated_without_altering_amounts_aliases_or_source() {
    let text = format!("{}\nHONоSEOк\nроозаниткално\nTpо:\nCuonta:\n$500.00\n2025-2026\n672 =0\nслолдтою 22\nMaría Pérez", "Texto del documento legible. ".repeat(20));
    let entities = vec![entity(Kind::Person, "María Pérez", "PERSON_01")];
    let out = sanitize_for_file(&text, &entities, "scan.pdf").unwrap();
    assert!(out.contains("[Fragmento OCR ilegible]"));
    assert!(!out.contains("HONоSEOк"));
    assert!(out.contains("Tipo:\nCuenta:\n$500.00\n2025-2026\n672 =0\nслолдтою 22\nPERSON_01"));
    assert_eq!(
        sanitize_for_file(&text, &entities, "text.txt").unwrap(),
        sanitize(&text, &entities).unwrap()
    );
    let russian = "Это документ на русском языке.";
    assert_eq!(
        sanitize_for_file(russian, &[], "scan.pdf").unwrap(),
        russian
    );
}
#[test]
fn cleaned_ocr_output_keeps_the_export_gate_exact() {
    let mut session = reviewed_session();
    session.file_name = "scan.pdf".into();
    session.original.push_str(&format!(
        "\n{}\nHONоSEOк\n$500.00",
        "Documento legible. ".repeat(20)
    ));
    session.sanitized =
        sanitize_for_file(&session.original, &session.entities, &session.file_name).unwrap();
    assert!(session.original.contains("HONоSEOк"));
    assert!(session.sanitized.contains("[Fragmento OCR ilegible]"));
    assert!(checked_outbound(&session).is_ok());
    assert!(
        check_export_payload(&session, &session.sanitized, None, "Documento protegido").is_ok()
    );
    assert!(check_export_payload(
        &session,
        &(session.sanitized.clone() + " nuevo texto"),
        None,
        "Documento protegido"
    )
    .is_err());
}
#[test]
fn receipt_labels_are_not_addresses_but_real_account_spans_are_retained() {
    let source = "uenta Orige\nCuonta:\nTpо:\n4410XXXXX319\nAHORROS SOCIC\n2204XXX521";
    for proposed in [
        "CUENTA ORIGEN",
        "CUENTA: TPо: 4410XXXXX319",
        "Cuenta Origen: TPo: 4410XXXXX319, Ahorros Socic: 2204XXX521",
    ] {
        let model = vec![Detection {
            kind: Kind::Address,
            text: proposed.into(),
        }];
        let entities = merge_detections(source, model).unwrap();
        let out = sanitize(source, &entities).unwrap();
        assert!(!out.contains("4410"));
        assert!(!out.contains("2204"));
        assert!(leak_warnings(&out, &entities).is_empty());
    }
    for invented in [
        "Cuenta Origen de Pedro",
        "Cuenta Origen 999XXXXX000",
        "Cuenta Origen: Calle Inventada 22",
    ] {
        assert!(ground_model_detections(
            source,
            vec![Detection {
                kind: Kind::Address,
                text: invented.into()
            }]
        )
        .is_err());
    }
}
#[test]
fn normalized_model_names_use_exact_unique_source_spans() {
    let text = "José Antonio Martínez\nJOSE ANTONIO MI";
    let detections = ground_model_detections(
        text,
        vec![Detection {
            kind: Kind::Person,
            text: "Jose Antonio Martinez".into(),
        }],
    )
    .unwrap();
    assert_eq!(detections[0].text, "José Antonio Martínez");
    let entities = merge_detections(text, detections).unwrap();
    let out = sanitize(text, &entities).unwrap();
    assert!(!out.contains("Antonio"));
    assert!(!out.contains("JOSE"));
    for (source, proposed) in [
        ("Ana Pérez / Ana Péréz", "Ana Perez"),
        ("JOSE ANTONIO MI", "José Antonio Martínez"),
        ("Ana", "Pedro"),
    ] {
        assert!(ground_model_detections(
            source,
            vec![Detection {
                kind: Kind::Person,
                text: proposed.into()
            }]
        )
        .is_err());
    }
}
#[test]
fn unmatched_model_entity_retries_exact_extraction_without_dropping_it() {
    use std::cell::Cell;
    struct RetryExtractor {
        retried: Cell<bool>,
        succeeds: bool,
    }
    impl EntityExtractor for RetryExtractor {
        fn extract(&self, _: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            Ok(vec![Detection {
                kind: Kind::Person,
                text: "Pedro Inventado".into(),
            }])
        }
        fn extract_exact(
            &self,
            _: &str,
            _: &mut dyn FnMut(usize),
        ) -> Result<Vec<Detection>, String> {
            self.retried.set(true);
            Ok(vec![Detection {
                kind: Kind::Person,
                text: if self.succeeds {
                    "Ana Pérez"
                } else {
                    "Pedro Inventado"
                }
                .into(),
            }])
        }
    }
    for succeeds in [true, false] {
        let extractor = RetryExtractor {
            retried: Cell::new(false),
            succeeds,
        };
        let result =
            analyze_with_progress(&extractor, "Ana Pérez comparece.", &mut |_, _, _| {}).unwrap();
        assert!(extractor.retried.get());
        // A fragment the model refuses to ground is reported as a coverage gap,
        // never as a clean pass: nothing about that text was inspected.
        assert_eq!(result.complete(), succeeds);
        assert_eq!(result.fragments, 1);
        assert_eq!(result.failed_fragments, usize::from(!succeeds));
        if succeeds {
            assert_eq!(
                sanitize("Ana Pérez comparece.", &result.entities).unwrap(),
                "PERSON_01 comparece."
            );
            assert!(result.coverage_warning().is_none());
        } else {
            // Grounding rejected the invented entity, so the map is empty.
            assert!(result.entities.is_empty());
            assert!(result
                .coverage_warning()
                .is_some_and(|w| w.contains("no pudo leer 1 de 1")));
        }
    }
}

/// Regression: a long real document is cut into many fragments, and one
/// fragment Qwen cannot finish used to discard every entity found in the rest
/// of the file. The lawyer then saw no map at all and concluded the model had
/// not run.
#[test]
fn one_unreadable_fragment_does_not_discard_the_whole_document() {
    struct FlakyOnThird {
        calls: std::cell::Cell<usize>,
    }
    impl EntityExtractor for FlakyOnThird {
        fn extract(&self, chunk: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            self.calls.set(self.calls.get() + 1);
            // Fragments 1 and 3 read fine; fragment 2 refuses on both passes.
            if chunk.contains("SEGUNDO") {
                return Err("El análisis local no terminó dentro del límite.".into());
            }
            let name = if chunk.contains("PRIMERO") {
                "María Pérez"
            } else {
                "Luis Rivera"
            };
            Ok(vec![Detection {
                kind: Kind::Person,
                text: name.into(),
            }])
        }
        fn extract_exact(
            &self,
            chunk: &str,
            _: &mut dyn FnMut(usize),
        ) -> Result<Vec<Detection>, String> {
            if chunk.contains("SEGUNDO") {
                return Err("El análisis local no terminó dentro del límite.".into());
            }
            Err("sinSugerencias".into())
        }
    }
    // Three blocks, each longer than one fragment, so each marker lands in its
    // own fragment and only the middle one is unreadable. The filler is sized
    // from the real chunking so the test keeps meaning if it ever changes.
    // ~7 000 chars per block: one block per fragment, three blocks, and the
    // whole document stays under the 64 KB session ceiling.
    let filler = "dolor sit amet consectetur ".repeat(CHUNK_CHARS / 8);
    let text = format!(
        "PRIMERO María Pérez comparece. {filler}\nSEGUNDO Pedro Inventado demanda. {filler}\nTERCERO Luis Rivera firma. {filler}\n"
    );
    let analysis = analyze_with_progress(
        &FlakyOnThird {
            calls: std::cell::Cell::new(0),
        },
        &text,
        &mut |_, _, _| {},
    )
    .unwrap();
    assert!(analysis.fragments >= 3, "the document must be split");
    assert!(analysis.failed_fragments >= 1);
    assert!(!analysis.complete());
    // The readable fragments still produced their entities.
    let originals: Vec<&str> = analysis
        .entities
        .iter()
        .map(|e| e.original.as_str())
        .collect();
    assert!(
        originals.iter().any(|o| o.contains("María Pérez")),
        "{originals:?}"
    );
    // And the gap keeps approval closed.
    let mut session = Session::new("largo.pdf".into(), text.clone()).unwrap();
    session.entities = analysis.entities.clone();
    session.sanitized = sanitize_for_file(&text, &analysis.entities, "largo.pdf").unwrap();
    session.analyzed = true;
    // `analysis_complete` stays false on purpose: that is the state a real
    // partial analysis leaves behind, and it must keep the preview closed.
    session.analysis_complete = analysis.complete();
    session.reviewed = true;
    assert!(
        checked_outbound(&session).is_err(),
        "coverage gap must block preview"
    );
}
#[test]
fn no_silent_truncation_and_no_hallucinated_entity() {
    assert!(Session::new("x".into(), " ".into()).is_err());
    assert!(Session::new("x".into(), "a".repeat(MAX_DOCUMENT_BYTES + 1)).is_err());
    assert!(merge_detections(
        "Ana",
        vec![Detection {
            kind: Kind::Person,
            text: "Pedro".into()
        }]
    )
    .is_err());
}
#[test]
fn preview_requires_analysis_review_and_revalidates_exact_payload() {
    let mut session =
        Session::new("private-name.txt".into(), "Ana usa ana@example.com".into()).unwrap();
    session.entities = merge_detections(
        &session.original,
        vec![Detection {
            kind: Kind::Person,
            text: "Ana".into(),
        }],
    )
    .unwrap();
    session.sanitized = sanitize(&session.original, &session.entities).unwrap();
    assert!(checked_outbound(&session).is_err());
    session.analyzed = true;
    assert!(checked_outbound(&session).is_err());
    session.reviewed = true;
    session.analysis_complete = true;
    let payload = serde_json::to_value(checked_outbound(&session).unwrap()).unwrap();
    assert_eq!(payload.as_object().unwrap().len(), 2);
    assert!(!payload.to_string().contains("private-name"));
    assert!(!payload.to_string().contains("ana@example.com"));
    session.sanitized.push_str(" leaked@example.com");
    assert!(checked_outbound(&session).is_err());
    session.entities.clear();
    session.sanitized = session.original.clone();
    assert!(checked_outbound(&session).is_err());
}
#[test]
fn edited_document_requires_review_and_checks_exact_text() {
    let mut session = Session::new("source.txt".into(), "Ana usa ana@example.com".into()).unwrap();
    session.entities = merge_detections(
        &session.original,
        vec![Detection {
            kind: Kind::Person,
            text: "Ana".into(),
        }],
    )
    .unwrap();
    session.sanitized = sanitize(&session.original, &session.entities).unwrap();
    session.analyzed = true;
    session.edited = true;
    session.sanitized = "PERSON_01 presenta el escrito.\n\nSegundo párrafo.".into();
    assert!(checked_outbound(&session).is_err());
    session.reviewed = true;
    session.analysis_complete = true;
    assert_eq!(
        checked_outbound(&session).unwrap().document,
        session.sanitized
    );
    session.sanitized.push_str(" ana@example.com");
    assert!(checked_outbound(&session).is_err());
}
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_handles_ocr_receipt_fragments() {
    let (_guard, engine) = locked_qwen_engine();
    let text = "OFICIO – DOCUMENTO DE REFERENCIA\nDe: José Antonio Martínez Valdez\nAsunto: Pagos Septiembre 2025-2026\nXIMENA BARRAGÁN CON SEDE EN LA PARROQUIA\nJosé Antonio Martínez Valdez comparece.\nANEXOS\nnombre\nJOSE ANTONIO MI\nE\n$500.00\nJOSE ANTONIO MARTIN\nTRANCO PICHNCHE\nJOSE ANTONIO MARTA\nCuonta:\n4410XXXXX319\n2204XXX521";
    let entities = analyze_with_progress(&engine, text, &mut |_, _, _| {})
        .unwrap()
        .entities;
    let out = sanitize(text, &entities).unwrap();
    for private in [
        "José Antonio",
        "JOSE ANTONIO",
        "XIMENA BARRAGÁN",
        "4410",
        "2204",
    ] {
        assert!(
            !out.contains(private),
            "OCR sample still contains private information"
        );
    }
    assert!(out.contains("2025-2026"));
    assert!(out.contains("$500.00"));
    assert!(leak_warnings(&out, &entities).is_empty());
}
#[test]
#[ignore = "Requires the verified Qwen3 model; run explicitly for local inference QA"]
fn real_qwen_extracts_private_document_locally() {
    let (_guard, engine) = locked_qwen_engine();
    let text="María Pérez demanda a Banco Andino S.A. Su cédula es 0912345678, correo maria.perez@example.com y teléfono +593 99 123 4567. Dirección: Av. Amazonas 123, Quito. Proceso 17230-2025-01234. Reclama USD 1500 el 30 de septiembre de 2026.";
    let model = engine
        .extract(text, &mut |_| {})
        .expect("Qwen must return structured, completed output");
    assert!(
        model
            .iter()
            .any(|e| e.kind == Kind::Person && e.text.contains("María")),
        "Qwen must detect a person"
    );
    assert!(
        model
            .iter()
            .any(|e| e.kind == Kind::Company && e.text.contains("Banco")),
        "Qwen must detect an organization"
    );
    let entities = merge_detections(text, model).unwrap();
    let sanitized = sanitize(text, &entities).unwrap();
    assert!(!sanitized.contains("María Pérez"));
    assert!(!sanitized.contains("Banco Andino"));
    assert!(sanitized.contains("COMPANY_01. Su cédula"));
    assert!(!sanitized.contains("maria.perez@example.com"));
    assert!(sanitized.contains("USD 1500"));
    assert!(leak_warnings(&sanitized, &entities).is_empty());
}
#[test]
fn long_money_values_and_numeric_dates_are_not_redacted_as_identifiers() {
    let original = "USD 1500000000, $ 12345678, 123456789 euros. Fecha 30 09 2026 y 2026-09-30.";
    let entities = merge_detections(original, vec![]).unwrap();
    assert_eq!(sanitize(original, &entities).unwrap(), original);
    assert!(leak_warnings(original, &[]).is_empty());
}

#[test]
fn a_category_the_model_invents_does_not_leave_the_span_in_the_text() {
    // Regression: Qwen3 1.7B labels a bank account `ACCOUNT`, which is not
    // a documented kind. The whole response used to be rejected, and
    // dropping just that entry would leave the account number in the
    // sanitized text. It has to be sanitized, not discarded.
    let original = "Cuenta bancaria: EC1234567890 a nombre de Ana Ruiz.";
    let raw = r#"[{"kind":"ACCOUNT","text":"EC1234567890"},{"kind":"PERSON","text":"Ana Ruiz"}]"#;
    let entities = merge_detections(original, parse_detections(raw).unwrap()).unwrap();
    let out = sanitize(original, &entities).unwrap();
    assert!(
        !out.contains("EC1234567890"),
        "la cuenta se filtró al sanitizar: {out}"
    );
    assert!(out.contains("OTHER_01"), "{out}");
    // And the deterministic layer is still the backstop if the map is wrong.
    assert!(leak_warnings(&out, &entities).is_empty());
}

#[test]
fn an_empty_analysis_result_is_reported_not_rejected() {
    // Regression: the map invariant used to run during analysis, so a
    // document with nothing to detect failed outright and the lawyer never
    // saw the empty map. Analysis must always return a result; refusing is
    // a review/export concern.
    let original = "Doña Carmen dispone de la casa de la calle Bolívar.";
    let entities: Vec<Entity> = vec![];
    let sanitized = sanitize(original, &entities).unwrap();
    // No require_sanitized on this path: both facts are simply reported.
    assert!(entities.is_empty());
    assert_eq!(sanitized, original);
    assert!(leak_warnings(&sanitized, &entities).is_empty());
    assert!(privacy_advisories(&entities)
        .iter()
        .any(|a| a.contains("ninguna persona")));
    // And the same session is still refused downstream, where it counts.
    let mut session = Session::new("testamento.txt".into(), original.into()).unwrap();
    session.entities = entities;
    session.sanitized = sanitized;
    session.analyzed = true;
    assert!(checked_outbound(&session).is_err());
    assert!(check_export_payload(&session, &session.sanitized, None, "Doc").is_err());
}

#[test]
fn an_empty_map_can_never_approve_the_original_document() {
    // A will with names and places but no email/phone/ID: the deterministic
    // layer finds nothing, so only the map invariant can stop it.
    let original = "Doña Carmen dispone de la casa de la calle Bolívar para su sobrina.";
    assert!(deterministic_detections(original).is_empty());
    assert!(leak_warnings(original, &[]).is_empty());
    // sanitize() with no entities is the original itself.
    assert_eq!(sanitize(original, &[]).unwrap(), original);
    assert!(require_sanitized(original, original, &[]).is_err());
    assert!(
        require_sanitized(original, original, &[entity(Kind::Other, "x", "OTHER_01")]).is_err()
    );

    let mut session = Session::new("testamento.txt".into(), original.into()).unwrap();
    session.entities = vec![];
    session.sanitized = sanitize(original, &[]).unwrap();
    session.analyzed = true;
    session.reviewed = true;
    session.analysis_complete = true;
    let err = checked_outbound(&session)
        .err()
        .expect("mapa vacío debe fallar");
    assert!(err.contains("mapa no tiene entidades"), "{err}");
}

#[test]
fn a_map_that_changes_nothing_is_refused() {
    let original = "Ana llama a Pedro.";
    let entities = vec![entity(Kind::Person, "Ana", "PERSON_01")];
    // Sanitizing does change the text, so this map is acceptable.
    assert!(
        require_sanitized(original, &sanitize(original, &entities).unwrap(), &entities).is_ok()
    );
    // An entity that survives in the output would make them identical.
    let unchanged = "PERSON_01 llama a Pedro.";
    assert!(require_sanitized(unchanged, unchanged, &entities).is_err());
}

#[test]
fn advisories_call_out_the_names_the_rules_cannot_check() {
    assert!(privacy_advisories(&[])
        .iter()
        .any(|a| a.contains("ninguna persona")));
    assert!(
        privacy_advisories(&[entity(Kind::Person, "Ana", "PERSON_01")]).is_empty(),
        "con una persona en el mapa el aviso sobra"
    );
    // Organisations alone still leave every person name unchecked.
    assert!(
        privacy_advisories(&[entity(Kind::Company, "Banco", "COMPANY_01")])
            .iter()
            .any(|a| a.contains("ninguna persona"))
    );
}

#[test]
fn normalized_text_ignores_only_whitespace() {
    assert_eq!(normalized_text("a  b\nc"), "a b c");
    assert_ne!(
        normalized_text("PERSON_01 llama"),
        normalized_text("Ana llama")
    );
}

/// A reviewed session as the export gate would find it.
fn reviewed_session() -> Session {
    let original = "María Pérez reclama a Banco Andino. Correo: maria@example.com.";
    let entities = vec![
        entity(Kind::Person, "María Pérez", "PERSON_01"),
        entity(Kind::Email, "maria@example.com", "EMAIL_01"),
    ];
    let mut sanitized = original.to_string();
    for e in &entities {
        sanitized = sanitized.replace(&e.original, &e.alias);
    }
    let mut session = Session::new("demanda.pdf".into(), original.into()).unwrap();
    session.entities = entities;
    session.sanitized = sanitized;
    session.analyzed = true;
    session.reviewed = true;
    session.analysis_complete = true;
    session
}

#[test]
fn the_export_gate_approves_only_the_reviewed_document() {
    let session = reviewed_session();
    assert!(
        check_export_payload(&session, &session.sanitized, None, "Documento protegido").is_ok()
    );
}

#[test]
fn the_export_gate_refuses_an_unreviewed_session() {
    let mut session = reviewed_session();
    session.reviewed = false;
    let err = check_export_payload(&session, &session.sanitized, None, "Documento").unwrap_err();
    assert!(err.contains("revisión confirmada"), "got: {err}");
}

#[test]
fn the_export_gate_refuses_text_the_lawyer_never_saw() {
    // Unsaved editor changes: valid text, but not the reviewed document.
    let session = reviewed_session();
    let tampered = format!("{} DATO EXTRA", session.sanitized);
    let err = check_export_payload(&session, &tampered, None, "Documento").unwrap_err();
    assert!(err.contains("sin revisar"), "got: {err}");
}

#[test]
fn the_export_gate_refuses_an_empty_map_even_when_reviewed() {
    // The reviewed flag alone must not be enough: an empty map means the
    // sanitized text is the original, so this is an unreviewed document
    // that happens to be flagged as approved.
    let mut session = reviewed_session();
    session.entities.clear();
    session.sanitized = session.original.clone();
    let err = check_export_payload(&session, &session.sanitized, None, "Documento").unwrap_err();
    assert!(
        err.contains("entidades") || err.contains("idéntico"),
        "got: {err}"
    );
}

#[test]
fn the_export_gate_refuses_html_that_renders_more_than_the_reviewed_text() {
    // Rich formatting is fine, but any extra visible text — a heading, a
    // repeated line, a hidden node — is content the lawyer never reviewed.
    let session = reviewed_session();
    for html in [
        format!("<p>{}</p><h1>Demanda</h1>", session.sanitized),
        format!(
            "<p>{}</p><p style=\"display:none\">MARÍA PÉREZ</p>",
            session.sanitized
        ),
        format!("<p>{}</p><p>{}</p>", session.sanitized, session.sanitized),
    ] {
        let err = check_export_payload(&session, &session.sanitized, Some(&html), "Documento")
            .unwrap_err();
        assert!(err.contains("no coincide"), "got: {err} for {html}");
    }
}

#[test]
fn the_export_gate_accepts_html_that_only_reformats_the_reviewed_text() {
    let session = reviewed_session();
    // Mismo texto, repartido en varios runs con estilo: es lo que produce
    // el editor cuando el abogado aplica negrita o cursiva.
    let html = "<p><b>PERSON_01</b> reclama a Banco Andino. Correo: <i>EMAIL_01</i>.</p>";
    assert_eq!(
        normalized_text(&crate::pdf::html_visible_text(html)),
        normalized_text(&session.sanitized)
    );
    assert!(check_export_payload(&session, &session.sanitized, Some(html), "Documento").is_ok());
}

#[test]
fn the_export_gate_refuses_residual_leaks() {
    // A map that missed an address still gets refused by the rules.
    let mut session = reviewed_session();
    session
        .entities
        .push(entity(Kind::Person, "Banco Andino", "COMPANY_01"));
    session.sanitized = "María Pérez reclama a PERSON_01.".into();
    let err = check_export_payload(&session, &session.sanitized, None, "Documento").unwrap_err();
    assert!(err.contains("PDF bloqueado"), "got: {err}");
}

#[test]
fn the_export_gate_refuses_a_title_carrying_private_data() {
    // The title lands in the page header and the PDF metadata, so it is
    // exported content too. The renderer supplies it, so it is not trusted.
    let session = reviewed_session();
    let clean = check_export_payload(&session, &session.sanitized, None, "Documento protegido");
    assert!(clean.is_ok());
    for title in [
        "Demanda de María Pérez",
        "Reclamo maria@example.com",
        "Cédula 0912345678",
    ] {
        let err = check_export_payload(&session, &session.sanitized, None, title).unwrap_err();
        assert!(err.contains("título"), "got: {err} for {title}");
    }
}

#[test]
fn free_custom_aliases_survive_map_application_and_session_round_trip() {
    let original = "Ana Pérez. Correo ana@example.com.";
    for alias in [
        "EMAIL_xxxx",
        "Cliente principal",
        "correo privado",
        "Señora A",
        "X",
        "[correo oculto]",
    ] {
        let map = vec![
            entity(Kind::Person, "Ana Pérez", "Persona A"),
            entity(Kind::Email, "ana@example.com", alias),
        ];
        let sanitized = sanitize(original, &map).unwrap();
        assert_eq!(sanitized, format!("Persona A. Correo {alias}."));
        assert!(leak_warnings(&sanitized, &map).is_empty());
        let mut session = Session::new("fixture.txt".into(), original.into()).unwrap();
        session.entities = map;
        session.sanitized = sanitized;
        session.analyzed = true;
        let response = serde_json::to_value(&session).unwrap();
        assert_eq!(response["entities"][1]["alias"], alias);
        let restored: Vec<Entity> = serde_json::from_value(response["entities"].clone()).unwrap();
        assert_eq!(
            sanitize(&session.original, &restored).unwrap(),
            session.sanitized
        );
    }
    for alias in ["", "   ", "correo\nprivado", "ana@example.com", "Ana Pérez"] {
        assert!(sanitize(original, &[entity(Kind::Email, "ana@example.com", alias)]).is_err());
    }
}

#[test]
fn short_custom_aliases_do_not_hide_residual_identifiers() {
    let map = vec![entity(Kind::Person, "Ana Pérez", "X")];
    assert!(leak_warnings("X comparece.", &map).is_empty());
    assert!(!leak_warnings("X y X@example.com", &map).is_empty());
    let map = vec![entity(Kind::Person, "Ana Pérez", "0")];
    assert!(!leak_warnings("0. Correo 0000000@example.com", &map).is_empty());
}

/// Sizes fragments against the real tokenizer. A document fragment must leave
/// room for the JSON answer inside the 8192-token context; this pins the
/// worst-case Spanish fragment so raising the chunk size cannot silently
/// overflow the context and fail every analysis.
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_fragment_size_fits_the_context() {
    let (_guard, engine) = locked_qwen_engine();
    let filler = "comparece la señora demandada contra el banco demandado. ".repeat(120);
    let mut worst = 0usize;
    for chars in [2800usize, 6000, 9000, 12000] {
        let doc: String = filler.chars().take(chars).collect();
        let tokens = engine.prompt_tokens(&doc).expect("tokeniza");
        let per_char = tokens as f64 / chars as f64;
        eprintln!("chars={chars} tokens={tokens} ratio={per_char:.3}");
        assert!(
            crate::llm::output_budget(tokens).is_ok(),
            "{chars} caracteres ({tokens} tokens) no caben en el contexto"
        );
        worst = worst.max(tokens);
    }
    eprintln!("peor prompt medido: {worst} tokens");
}

/// The end-to-end check the demo needs: a multi-page document analysed by the
/// real model must finish, report every fragment, and leave a usable map.
/// This is the case that used to look like "Qwen is not acting".
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_protects_a_long_document_end_to_end() {
    use std::time::Instant;
    let (_guard, engine) = locked_qwen_engine();
    // ~14 KB of plausible case text: well past a single fragment.
    let unit = "Comparece la señora María Pérez contra Banco Andino S.A., quien reclama \
USD 1500 por obligaciones pendientes. La cédula 0912345678 consta en el expediente \
17230-2025-01234 y el domicilio es Av. Amazonas 123, Quito. ";
    let document = unit.repeat(60);
    assert!(
        document.len() > 10_000,
        "the fixture must span several fragments"
    );

    let fragments = chunks(&document).len();
    let mut seen = vec![];
    let started = Instant::now();
    let mut token_updates = 0usize;
    let analysis = analyze_with_progress(&engine, &document, &mut |done, total, tokens| {
        // Progress is "finished + current", so it only grows: a fragment split
        // and requeued must never move the label backwards.
        assert!(
            done >= 1 && done <= total,
            "progreso inconsistente: {done}/{total}"
        );
        // A fragment repeats on every token tick; only count fragment changes.
        if seen.last() != Some(&done) {
            seen.push(done);
        }
        if tokens > 0 {
            token_updates += 1;
        }
    })
    .unwrap();
    let elapsed = started.elapsed();

    // Every fragment is reported in order, and at least one running token count
    // arrives: this is the progress the UI renders, so a silent gap here is a
    // frozen screen for the lawyer waiting on the real model.
    // The label advances strictly and every pass is counted. A dense fragment
    // may be split and re-read, so the model can be asked for more passes than
    // the document has fragments, never fewer.
    assert_eq!(seen, (1..=seen.len()).collect::<Vec<_>>());
    assert!(
        seen.len() >= fragments,
        "se leyeron {} de {fragments} fragmentos",
        seen.len()
    );
    assert_eq!(analysis.fragments, seen.len());
    assert!(
        token_updates > 0,
        "la lectura nunca reportó tokens: la etiqueta quedaría congelada"
    );
    assert!(
        fragments >= 3,
        "expected several fragments, got {fragments}"
    );
    eprintln!(
        "documento={} chars, fragmentos={}, entidades={}, incompletos={}, tiempo={elapsed:?}",
        document.len(),
        fragments,
        analysis.entities.len(),
        analysis.failed_fragments
    );

    assert!(analysis.complete(), "coverage gap: {analysis:?}");
    let sanitized = sanitize_for_file(&document, &analysis.entities, "caso.pdf").unwrap();
    require_sanitized(&document, &sanitized, &analysis.entities).unwrap();
    assert!(leak_warnings(&sanitized, &analysis.entities).is_empty());
    for private in [
        "María Pérez",
        "0912345678",
        "17230-2025-01234",
        "Av. Amazonas 123",
    ] {
        assert!(!sanitized.contains(private), "residual: {private}");
    }
    // The bank is itself an identifier, so it must be replaced by an alias.
    assert!(
        !sanitized.contains("Banco Andino"),
        "the bank is a private entity"
    );
    assert!(
        analysis
            .entities
            .iter()
            .any(|e| e.original.contains("Banco Andino")),
        "the bank must be in the map"
    );
    // What is not an identifier has to survive untouched.
    for keep in [
        "USD 1500",
        "obligaciones pendientes",
        "Comparece",
        "reclama",
    ] {
        assert!(sanitized.contains(keep), "lost context: {keep}");
    }
    // A repeat-heavy document must not produce an unusable alias list.
    assert!(analysis.entities.len() <= 400, "too many entities");
}

/// Response size, not page count or context capacity, bounds extraction tasks.
#[test]
fn long_documents_use_tasks_small_enough_for_the_response_budget() {
    let text = "palabra ".repeat(8_000); // Supported 64 KB text budget.
    assert!(chunks(&text).iter().all(|v| v.chars().count() <= 2000));
    // Overlap must still let an entity survive a fragment boundary: no gap
    // between neighbouring fragments.
    let size = crate::privacy::chunk_chars_for_test();
    let joined = chunks(&"á".repeat(size * 3 + 100)).len();
    assert_eq!(joined, 4, "un documento de 3 fragmentos más cola da 4");
}

/// Measures the real generation loop with the backends the app actually ships
/// (Metal + Apple Silicon CPU from Contents/Frameworks). The unit tests default
/// to the CPU backend, which hides a Metal stall: the app was seen sitting at
/// "fragment 1 of 2" for minutes with 97% of the time inside Metal
/// `synchronize()`.
#[test]
#[ignore = "Requires verified local Qwen weights and the bundled Metal backend"]
fn real_qwen_metal_generation_speed_is_usable() {
    use std::time::Instant;
    let (_guard, engine) = locked_qwen_engine();
    let document = "Comparece María Pérez contra Banco Andino. Reclama USD 1500. ".repeat(200);
    let prompt = engine.privacy_prompt_for_test(&document);
    let started = Instant::now();
    let output = engine.generate(&prompt, &mut |_| {}).expect("genera");
    let elapsed = started.elapsed();
    let tokens = engine.count_tokens_for_test(&output).unwrap_or(0);
    let per_second = tokens as f64 / elapsed.as_secs_f64();
    eprintln!(
        "METAL: prompt {} chars -> {} tokens en {:?} = {per_second:.1} tok/s",
        document.len(),
        tokens,
        elapsed
    );
    assert!(tokens > 0, "el modelo no generó nada");
    // Anything below this is unusable for a lawyer waiting on a document.
    assert!(
        per_second >= 5.0,
        "Metal genera {per_second:.1} tok/s: la app se quedaría colgada"
    );
}

/// Reproduces the lawyer's report on the installed app: a document that splits
/// into two fragments used to sit on "fragment 1 of 2" with no visible change.
/// Prints a timestamp per progress tick so the silence is measurable.
#[test]
#[ignore = "Requires verified local Qwen weights and the bundled Metal backend"]
fn real_qwen_two_fragment_document_never_looks_frozen() {
    use std::time::{Duration, Instant};
    let (_guard, engine) = locked_qwen_engine();
    let document = "Comparece Ana Torres, identificado con cédula 48213755, contra Banco Andino, en el proceso EXP-2026-4471, reclamando USD 2400. Notificado en correo ana@example.com y teléfono +34 612345678. ".repeat(70);
    let started = Instant::now();
    let mut last = Instant::now();
    let mut quiet_for = Duration::ZERO;
    let mut ticks = 0usize;
    let analysis = analyze_with_progress(&engine, &document, &mut |done, total, tokens| {
        if tokens == 0 {
            eprintln!(
                "[{:>6.1}s] fragmento {done}/{total}",
                started.elapsed().as_secs_f64()
            );
        }
        let gap = last.elapsed();
        if gap > quiet_for {
            quiet_for = gap;
        }
        last = Instant::now();
        ticks += 1;
    })
    .expect("analiza");
    eprintln!(
        "2FRAG: {} chars, {} fragmentos, {} entidades, {} huecos, {} ticks, peor silencio {:?}, total {:?}",
        document.len(),
        analysis.fragments,
        analysis.entities.len(),
        analysis.failed_fragments,
        ticks,
        quiet_for,
        started.elapsed()
    );
    assert!(
        quiet_for < Duration::from_secs(20),
        "el análisis estuvo {quiet_for:?} sin reportar: se ve congelado"
    );
}

/// Regression for the failure the installed app actually showed: on
/// entity-dense legal text the model spent the whole 2048-token answer budget
/// without closing its JSON, so the fragment was lost and the label restarted
/// from zero. Running the real analysis path, the document must be fully read
/// with no coverage gap.
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_dense_document_has_no_coverage_gap() {
    let (_guard, engine) = locked_qwen_engine();
    let full = std::fs::read_to_string("/tmp/legal-2frag.txt")
        .expect("coloca /tmp/legal-2frag.txt: el documento denso que rompía la app");
    let analysis = analyze_with_progress(&engine, &full, &mut |_, _, _| {}).unwrap();
    eprintln!(
        "DENSO: {} chars, {} pasadas, {} entidades, {} huecos",
        full.len(),
        analysis.fragments,
        analysis.entities.len(),
        analysis.failed_fragments
    );
    assert_eq!(
        analysis.failed_fragments,
        0,
        "quedaron huecos: {}",
        analysis.coverage_warning().unwrap_or_default()
    );
    assert!(analysis.complete());
    assert!(
        !analysis.entities.is_empty(),
        "el documento denso debe producir entidades"
    );
    // Every person in the fixture must be found, not just the first few.
    assert!(
        analysis.entities.len() >= 40,
        "solo encontró {} entidades de un documento con decenas",
        analysis.entities.len()
    );
}

/// An overflowing fragment must be split and re-read, never dropped: the real
/// model sometimes answers 1674-token prompts and fails on 1684-token ones.
#[test]
fn overflowing_fragment_is_split_instead_of_lost() {
    struct OverflowsOnce {
        calls: std::cell::Cell<usize>,
        seen: std::cell::RefCell<Vec<usize>>,
    }
    impl EntityExtractor for OverflowsOnce {
        fn extract(
            &self,
            chunk: &str,
            progress: &mut dyn FnMut(usize),
        ) -> Result<Vec<Detection>, String> {
            let n = self.calls.get();
            self.calls.set(n + 1);
            self.seen.borrow_mut().push(chunk.chars().count());
            progress(10);
            // Only the first attempt fails; its halves fit.
            if n == 0 {
                return Err("El análisis local no terminó dentro del límite.".into());
            }
            Ok(vec![Detection {
                kind: Kind::Person,
                text: "Ana Pérez".into(),
            }])
        }
    }
    let extractor = OverflowsOnce {
        calls: std::cell::Cell::new(0),
        seen: std::cell::RefCell::new(vec![]),
    };
    let size = crate::privacy::chunk_chars_for_test();
    let text = "Ana Pérez ".repeat(size * 2);
    let analysis = analyze_with_progress(&extractor, &text, &mut |_, _, _| {}).unwrap();
    let seen = extractor.seen.borrow().clone();
    // The full fragment is attempted, and after the overflow the text is re-read
    // in halves smaller than the fragment but above the split floor.
    assert!(
        seen.contains(&size),
        "algún intento debe usar el fragmento completo: {seen:?}"
    );
    let halves: Vec<usize> = seen.iter().copied().filter(|n| *n < size).collect();
    assert!(
        !halves.is_empty(),
        "tras el desbordamiento debe leer mitades: {seen:?}"
    );
    assert!(
        halves.iter().all(|n| *n > 0 && *n < size),
        "cada división conserva texto y reduce la tarea: {halves:?}"
    );
    assert_eq!(analysis.failed_fragments, 0, "nada debe perderse");
    assert!(
        !analysis.entities.is_empty(),
        "las entidades de las mitades se conservan"
    );
    assert!(analysis.complete());
    assert!(
        analysis.fragments >= 3,
        "un fragmento partido cuenta como varios"
    );
}

/// A fragment too short to split any further is reported as a gap instead of
/// looping forever.
#[test]
fn unsplittable_fragment_becomes_a_coverage_gap() {
    struct AlwaysOverflows;
    impl EntityExtractor for AlwaysOverflows {
        fn extract(&self, _: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            Err("El análisis local no terminó dentro del límite.".into())
        }
    }
    let min = crate::privacy::min_split_chars_for_test();
    let text = "Ana Pérez ".repeat(min);
    let analysis = analyze_with_progress(&AlwaysOverflows, &text, &mut |_, _, _| {}).unwrap();
    assert!(analysis.failed_fragments > 0, "debe reportar el hueco");
    assert!(!analysis.complete());
    assert!(analysis.coverage_warning().is_some());
}

/// Splitting a fragment must not widen what the model is allowed to say.
/// Entities are anchored against the whole document, so the model still cannot
/// invent a name that is nowhere in the text.
#[test]
fn split_fragments_still_reject_entities_absent_from_the_document() {
    struct Suggests {
        proposed: &'static str,
    }
    impl EntityExtractor for Suggests {
        fn extract(&self, _: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            Ok(vec![Detection {
                kind: Kind::Person,
                text: self.proposed.into(),
            }])
        }
    }
    let document = "Comparece Ana Pérez contra Banco Andino. ".repeat(400);

    // A real name, read from a fragment of that document, is accepted.
    let real = analyze_with_progress(
        &Suggests {
            proposed: "Ana Pérez",
        },
        &document,
        &mut |_, _, _| {},
    )
    .unwrap();
    assert!(
        real.entities.iter().any(|e| e.original.contains("Ana")),
        "una entidad presente debe aceptarse"
    );

    // An invented name is refused even though the rest of the run succeeded.
    let invented = analyze_with_progress(
        &Suggests {
            proposed: "Rodrigo Inventado",
        },
        &document,
        &mut |_, _, _| {},
    )
    .unwrap();
    assert!(
        invented.entities.is_empty(),
        "una entidad ausente del documento no puede pasar: {:?}",
        invented.entities
    );
}

/// A model that fails on everything must still terminate: every split halves the
/// fragment, so the pass below the split floor is recorded as a gap and the run
/// ends instead of looping forever.
#[test]
fn always_failing_model_terminates_instead_of_looping() {
    let size = crate::privacy::chunk_chars_for_test();
    let min = crate::privacy::min_split_chars_for_test();
    // Long enough to span several fragments: the worst realistic case.
    let text = "Ana Pérez ".repeat(size * 4);
    let fragments = chunks(&text).len();
    struct Records {
        sizes: std::cell::RefCell<Vec<usize>>,
    }
    impl EntityExtractor for Records {
        fn extract(&self, chunk: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            self.sizes.borrow_mut().push(chunk.chars().count());
            Err("El análisis local no terminó dentro del límite.".into())
        }
    }
    let rec = Records {
        sizes: std::cell::RefCell::new(vec![]),
    };
    let analysis = analyze_with_progress(&rec, &text, &mut |_, _, _| {}).unwrap();
    let sizes = rec.sizes.borrow().clone();
    // Overflow must split immediately, without paying for the same task twice.
    let passes = sizes.len();
    let mut maximum_calls = 0;
    let mut pending_sizes: Vec<_> = chunks(&text).iter().map(|v| v.chars().count()).collect();
    while let Some(n) = pending_sizes.pop() {
        maximum_calls += 1;
        if n > min {
            pending_sizes.extend([n / 2 + CHUNK_OVERLAP / 2, n - n / 2 + CHUNK_OVERLAP / 2]);
        }
    }
    assert_eq!(passes, maximum_calls, "one inference per overflowing task");
    let mut tally: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for n in &sizes {
        *tally.entry(*n).or_default() += 1;
    }
    let mut v: Vec<_> = tally.into_iter().collect();
    v.sort_unstable_by_key(|(n, _)| std::cmp::Reverse(*n));
    eprintln!("PASSES {passes} tamaños {:?}", &v[..6.min(v.len())]);

    eprintln!("PASSES {passes}");
    // Each fragment is halved until it fits under the floor, so the work is a
    // bounded binary tree, not an unbounded retry loop.
    // Context overlap keeps names whole and each child still shrinks. The
    // exact ceiling below includes that overlap and excludes duplicate retries.
    let ceiling = maximum_calls;
    // The point of this test is termination, not an exact count. Halving a
    // fragment is a bounded binary tree, so the passes stay within a small
    // multiple of the fragment count no matter how long the document is.
    eprintln!("PASSES {passes} techo {ceiling} fragmentos {fragments}");
    assert!(
        passes <= ceiling * 4,
        "{passes} pasadas muy por encima del techo {ceiling} para {fragments} fragmentos"
    );
    assert!(
        passes > fragments,
        "debe dividir al menos una vez, hizo {passes}"
    );
    assert_eq!(analysis.entities.len(), 0, "sin entidades aceptadas");
    assert!(!analysis.complete());
}

#[test]
fn long_tables_without_entities_do_not_trigger_retries() {
    struct TableOnly(std::cell::Cell<usize>);
    impl EntityExtractor for TableOnly {
        fn extract(&self, _: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            self.0.set(self.0.get() + 1);
            parse_detections(r#"{"entities":[]}"#)
        }
        fn extract_exact(
            &self,
            _: &str,
            _: &mut dyn FnMut(usize),
        ) -> Result<Vec<Detection>, String> {
            panic!("a valid empty table result must never be retried")
        }
    }
    let text = "Actividad\tFecha\tMonto\nPreparación\tJulio 2026\tUSD 500\n".repeat(120);
    let extractor = TableOnly(std::cell::Cell::new(0));
    let analysis = analyze_with_progress(&extractor, &text, &mut |_, _, _| {}).unwrap();
    assert!(analysis.complete());
    assert_eq!(extractor.0.get(), chunks(&text).len());
}

#[test]
fn adaptive_splits_preserve_names_crossing_the_cut() {
    struct FindName;
    impl EntityExtractor for FindName {
        fn extract(&self, text: &str, _: &mut dyn FnMut(usize)) -> Result<Vec<Detection>, String> {
            if text.chars().count() > 1500 {
                return Err("no terminó dentro del límite".into());
            }
            Ok(if text.contains("María Pérez") {
                vec![Detection {
                    kind: Kind::Person,
                    text: "María Pérez".into(),
                }]
            } else {
                vec![]
            })
        }
    }
    // A unique name straddles the midpoint of a full extraction block.
    let text = format!("{}María Pérez{}", "x ".repeat(498), " y".repeat(496));
    let analysis = analyze_with_progress(&FindName, &text, &mut |_, _, _| {}).unwrap();
    assert!(analysis.complete());
    assert!(analysis
        .entities
        .iter()
        .any(|e| e.original == "María Pérez"));
}

#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_certificate_of_domain_transfer() {
    let (_guard, engine) = locked_qwen_engine();
    let doc = std::fs::read_to_string("/tmp/cert-dominio.txt").expect("certificado");
    let analysis = analyze_with_progress(&engine, &doc, &mut |_, _, _| {}).unwrap();
    eprintln!(
        "CERT: {} entidades, {} huecos",
        analysis.entities.len(),
        analysis.failed_fragments
    );
    for e in &analysis.entities {
        eprintln!("  {:?} {} -> {}", e.kind, e.original, e.alias);
    }
}

#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_qa_fixture_entities() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/private-document.txt")
        .to_string_lossy()
        .into_owned();
    let text = std::fs::read_to_string(&input).unwrap();
    let (_guard, engine) = locked_qwen_engine();
    let analysis = analyze_with_progress(&engine, &text, &mut |_, _, _| {}).unwrap();
    eprintln!("QA entidades={}", analysis.entities.len());
    for e in &analysis.entities {
        eprintln!("  {:?} {}", e.kind, e.original);
    }
}

#[test]
fn law_citations_and_bare_dates_are_not_entities() {
    for keep in [
        "Artículo 195.1 del Código del Trabajo",
        "art. 458 del Código Civil",
        "numeral 3 del Art. 35",
        "inciso a) del Código de Procedimiento",
        "14 de enero de 2026",
        "USD 4.826,00",
        "Total a pagar: 1.500,00",
    ] {
        assert!(
            crate::privacy::is_non_identifying_for_test(keep),
            "una cita legal no identifica a nadie y debe preservarse: {keep}"
        );
    }
    for redact in [
        "María Pérez",
        "Banco Andino",
        "1712345678",
        "maria@example.com",
        "Av. Amazonas 123",
        "Contrato de arrendamiento signed by Ana",
    ] {
        assert!(
            !crate::privacy::is_non_identifying_for_test(redact),
            "esto sí identifica y debe redactarse: {redact}"
        );
    }
}

/// Qwen drops a leading letter on degraded OCR, as it did with
/// "Contra Constructora XYZ". One such span must not cost the whole fragment,
/// but an answer made mostly of unusable spans must still be refused.
#[test]
fn a_single_misspelled_span_does_not_condemn_the_fragment() {
    let document = "ACTA. Comparece Contra Constructora XYZ y Juan Perez.";
    let usable = vec![
        Detection {
            kind: Kind::Company,
            text: "Contra Constructora XYZ".into(),
        },
        Detection {
            kind: Kind::Person,
            text: "Juan Perez".into(),
        },
    ];
    let with_one_bad = vec![
        Detection {
            kind: Kind::Company,
            text: "Contra Constructora XYZ".into(),
        },
        Detection {
            kind: Kind::Company,
            text: "ontra Constructora XYZ".into(),
        },
        Detection {
            kind: Kind::Person,
            text: "Juan Perez".into(),
        },
    ];
    let mostly_bad = vec![
        Detection {
            kind: Kind::Company,
            text: "ontra Constructora XYZ".into(),
        },
        Detection {
            kind: Kind::Company,
            text: "Zeta Industrial".into(),
        },
        Detection {
            kind: Kind::Person,
            text: "Fulano Nadie".into(),
        },
    ];
    let clean = ground_model_detections(document, usable).unwrap();
    assert_eq!(clean.len(), 2, "las entidades correctas se conservan");
    let tolerated = ground_model_detections(document, with_one_bad).unwrap();
    assert_eq!(
        tolerated.len(),
        2,
        "un span defectuoso no debe tumbar el fragmento: {tolerated:?}"
    );
    assert!(
        ground_model_detections(document, mostly_bad).is_err(),
        "una respuesta casi toda inventada debe rechazarse"
    );
}

#[test]
#[ignore = "diag"]
fn diag_speed_error() {
    let (_guard, engine) = locked_qwen_engine();
    let document = "Comparece María Pérez contra Banco Andino. Reclama USD 1500. ".repeat(200);
    let prompt = engine.privacy_prompt_for_test(&document);
    eprintln!("DIAG prompt chars={}", prompt.len());
    eprintln!(
        "DIAG prompt tokens={:?}",
        engine.count_tokens_for_test(&prompt)
    );
    let started = std::time::Instant::now();
    match engine.generate(&prompt, &mut |_| {}) {
        Ok(out) => eprintln!(
            "DIAG ok {} tokens en {:?}",
            engine.count_tokens_for_test(&out).unwrap_or(0),
            started.elapsed()
        ),
        Err(e) => eprintln!("DIAG err: {e}"),
    }
}

/// A private document must not be exportable by leaving the gate out of the
/// payload. The WebView holds `Session::original`, so "not asking" has to fail.
#[test]
fn a_loaded_private_document_cannot_be_exported_by_omitting_the_gate() {
    let session = reviewed_session();
    let exported = serde_json::to_value(&session).unwrap();
    assert!(
        exported["original"]
            .as_str()
            .unwrap()
            .contains("María Pérez"),
        "el renderer recibe el texto sin redactar, por eso la omisión importa"
    );
    let matching = crate::pdf::PrivateExport {
        id: session.id.clone(),
        revision: session.revision,
    };
    let labels: [(&str, &str); 0] = [];
    // Presenting the reviewed session is the only way to be let through.
    assert!(
        check_export_authorization(
            Some(&session),
            Some(&matching),
            &session.sanitized,
            None,
            "Documento protegido",
            &labels,
        )
        .is_ok(),
        "la sesión revisada debe poder exportarse"
    );
    // The bypass: same writer, same text, gate simply left out of the payload.
    let err = check_export_authorization(
        Some(&session),
        None,
        &session.original,
        None,
        "Documento protegido",
        &labels,
    )
    .unwrap_err();
    assert!(
        err.contains("documento privado cargado"),
        "omitir la puerta debe explicar el motivo: {err}"
    );
    // The unredacted text is refused even when the session is presented.
    assert!(
        check_export_authorization(
            Some(&session),
            Some(&matching),
            &session.original,
            None,
            "Documento",
            &labels,
        )
        .is_err(),
        "el texto sin redactar no puede salir"
    );
    // A payload naming another session cannot present this one.
    let other = crate::pdf::PrivateExport {
        id: uuid::Uuid::new_v4().to_string(),
        revision: session.revision,
    };
    assert!(
        check_export_authorization(
            Some(&session),
            Some(&other),
            &session.sanitized,
            None,
            "Documento protegido",
            &labels,
        )
        .is_err(),
        "otra sesión no vale"
    );
    // With nothing private loaded, a transcript export is untouched.
    assert!(
        check_export_authorization(None, None, "reunión de equipo", None, "Acta", &labels).is_ok(),
        "sin documento privado cargado la exportación normal sigue funcionando"
    );
}

/// The language and model line is printed in the PDF body, so it must clear the
/// same gate as the text. These are the real values the recorder sends.
#[test]
fn the_language_and_model_line_cannot_carry_private_data() {
    let session = reviewed_session();
    // What the recorder really sends must not trip the gate.
    for (name, value) in [
        ("el idioma", "es"),
        ("el modelo", "whisper-large-v3"),
        ("el modelo", "parakeet"),
        ("el idioma", "EN"),
    ] {
        assert!(
            check_export_labels(&session, &[(name, value)]).is_ok(),
            "un valor real de la app debe pasar: {name}={value}"
        );
    }
    // An identifier or a name smuggled into the model field must not.
    for (name, value) in [
        ("el modelo", "Cédula maria@example.com"),
        ("el idioma", "María Pérez"),
        ("el modelo", "whisglargev3-María Pérez"),
    ] {
        assert!(
            check_export_labels(&session, &[(name, value)]).is_err(),
            "esto se imprimiría en el PDF y debe bloquearse: {name}={value}"
        );
    }
    // An empty label is missing metadata, not a leak.
    assert!(check_export_labels(&session, &[("el modelo", "")]).is_ok());
}

/// The layout export path (`export_private_layout`) calls the gate with fixed
/// strings: no HTML payload, the literal title "Protected" and no labels. That
/// shape must keep failing closed exactly like the transcript writer does: an
/// unreviewed or analysis-incomplete session can never be exported through it,
/// regardless of how clean those hardcoded values look.
#[test]
fn the_layout_gate_shape_fails_closed_for_unconfirmed_sessions() {
    let labels: [(&str, &str); 0] = [];
    // A reviewed session passes with the exact arguments the layout exporter sends.
    let session = reviewed_session();
    let matching = crate::pdf::PrivateExport {
        id: session.id.clone(),
        revision: session.revision,
    };
    assert!(
        check_export_authorization(
            Some(&session),
            Some(&matching),
            &session.sanitized,
            None,
            "Protected",
            &labels,
        )
        .is_ok(),
        "la sesión revisada debe poder exportarse por el flujo de layout"
    );
    // Unreviewed: the flag alone is what the layout gate checks first.
    let mut unreviewed = reviewed_session();
    unreviewed.reviewed = false;
    let err = check_export_authorization(
        Some(&unreviewed),
        Some(&crate::pdf::PrivateExport {
            id: unreviewed.id.clone(),
            revision: unreviewed.revision,
        }),
        &unreviewed.sanitized,
        None,
        "Protected",
        &labels,
    )
    .unwrap_err();
    assert!(err.contains("revisión confirmada"), "got: {err}");
    // Analysis cleared after review: stale sanitized text must not export.
    let mut incomplete = reviewed_session();
    incomplete.analysis_complete = false;
    assert!(check_export_authorization(
        Some(&incomplete),
        Some(&crate::pdf::PrivateExport {
            id: incomplete.id.clone(),
            revision: incomplete.revision,
        }),
        &incomplete.sanitized,
        None,
        "Protected",
        &labels,
    )
    .is_err());
    // Revision mismatch: the session moved while the export was being prepared.
    let mut moved = reviewed_session();
    moved.revision += 1;
    assert!(check_export_authorization(
        Some(&moved),
        Some(&crate::pdf::PrivateExport {
            id: moved.id.clone(),
            revision: moved.revision - 1,
        }),
        &moved.sanitized,
        None,
        "Protected",
        &labels,
    )
    .is_err());
    // Text tampering still fails even with the hardcoded safe title/labels.
    let session = reviewed_session();
    let tampered = format!("{} DATO EXTRA", session.sanitized);
    let err = check_export_authorization(
        Some(&session),
        Some(&crate::pdf::PrivateExport {
            id: session.id.clone(),
            revision: session.revision,
        }),
        &tampered,
        None,
        "Protected",
        &labels,
    )
    .unwrap_err();
    assert!(err.contains("sin revisar"), "got: {err}");
}
