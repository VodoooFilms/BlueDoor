use super::*;

/// llama.cpp initialises one Metal device per context and refuses a second
/// concurrent one, so the real-inference tests must not run in parallel.
/// The app itself never has this problem: `analyze_private_document`
/// serialises work behind a `try_lock` on the engine cache.
static REAL_INFERENCE: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    session.sanitized = "Documento sanitizado".into();
    session.analyzed = true;
    session.reviewed = true;
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
    assert!(check_export_payload(
        &session,
        "Documento sanitizado",
        None,
        "Documento sanitizado"
    )
    .is_err());
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
    assert!(parse_detections(r#"{"entities":[]}"#).is_err());
    // One bad element must not cost us the good ones.
    let mixed = parse_detections(
        r#"[{"kind":"ACCOUNT","text":"EC1234567890"},{"kind":"PERSON","text":"Ana"}]"#,
    )
    .unwrap();
    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed[0].kind, Kind::Other);
    assert_eq!(mixed[1].kind, Kind::Person);
    assert!(parse_detections("```json\n{\"entities\":[]}\n```").is_err());
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
    assert_eq!(parts.len(), 3);
    assert!(parts.last().unwrap().ends_with("FINAL_PRIVATE"));
    assert_eq!(parts[0].chars().count(), 2800);
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
        check_export_payload(&session, &session.sanitized, None, "Documento sanitizado").is_ok()
    );
    assert!(check_export_payload(
        &session,
        &(session.sanitized.clone() + " nuevo texto"),
        None,
        "Documento sanitizado"
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
        fn extract(&self, _: &str) -> Result<Vec<Detection>, String> {
            Ok(vec![Detection {
                kind: Kind::Person,
                text: "Pedro Inventado".into(),
            }])
        }
        fn extract_exact(&self, _: &str) -> Result<Vec<Detection>, String> {
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
        let result = analyze_with(&extractor, "Ana Pérez comparece.");
        assert!(extractor.retried.get());
        assert_eq!(result.is_ok(), succeeds);
        if let Ok(entities) = result {
            assert_eq!(
                sanitize("Ana Pérez comparece.", &entities).unwrap(),
                "PERSON_01 comparece."
            );
        }
    }
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
    assert_eq!(
        checked_outbound(&session).unwrap().document,
        session.sanitized
    );
    session.sanitized.push_str(" ana@example.com");
    assert!(checked_outbound(&session).is_err());
}
#[test]
#[ignore = "Requires a local QA text path and verified Qwen weights"]
fn real_qwen_analyzes_local_qa_document() {
    let input = std::env::var("BELEGAL_QA_TEXT").expect("set a local OCR text path");
    let mut text = std::fs::read_to_string(input).unwrap();
    let (_guard, engine) = locked_qwen_engine();
    let snapshot = text.clone();
    for chunk in chunks(&snapshot) {
        for suggestion in engine.suggest_ocr_prefixes(&chunk).unwrap() {
            if valid_ocr_prefix(&suggestion, &text) {
                text = corrected_ocr_text(&text, &suggestion).unwrap();
            }
        }
    }
    let entities = analyze_with(&engine, &text).unwrap();
    let sanitized = sanitize_for_file(&text, &entities, "scan.pdf").unwrap();
    require_sanitized(&text, &sanitized, &entities).unwrap();
    assert!(leak_warnings(&sanitized, &entities).is_empty());
    assert!(!sanitized.contains("JOSE ANTONIO"));
    assert!(!sanitized.contains("XIMENA BARRAGÁN"));
    assert!(!sanitized.contains("4410XXXXX319"));
    assert!(sanitized.contains("AHORROS SOCIC"));
    assert!(!entities
        .iter()
        .any(|e| e.original == "BANCO PICHINCHA" && e.kind != Kind::Company));
    let mut session = Session::new("scan.pdf".into(), text).unwrap();
    session.entities = entities;
    session.sanitized = sanitized;
    session.analyzed = true;
    session.reviewed = true;
    assert!(checked_outbound(&session).is_ok());
}
#[test]
#[ignore = "Requires verified local Qwen weights"]
fn real_qwen_handles_ocr_receipt_fragments() {
    let (_guard, engine) = locked_qwen_engine();
    let text = "OFICIO – DOCUMENTO DE REFERENCIA\nDe: José Antonio Martínez Valdez\nAsunto: Pagos Septiembre 2025-2026\nXIMENA BARRAGÁN CON SEDE EN LA PARROQUIA\nJosé Antonio Martínez Valdez comparece.\nANEXOS\nnombre\nJOSE ANTONIO MI\nE\n$500.00\nJOSE ANTONIO MARTIN\nTRANCO PICHNCHE\nJOSE ANTONIO MARTA\nCuonta:\n4410XXXXX319\n2204XXX521";
    let entities = analyze_with(&engine, text).unwrap();
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
        .extract(text)
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
    session
}

#[test]
fn the_export_gate_approves_only_the_reviewed_document() {
    let session = reviewed_session();
    assert!(
        check_export_payload(&session, &session.sanitized, None, "Documento sanitizado").is_ok()
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
    let clean = check_export_payload(&session, &session.sanitized, None, "Documento sanitizado");
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
