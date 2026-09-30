//! The privacy boundary is deterministic and owns the original document.
//! LLM output is only a list of suggestions, never the sanitized document.
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use tauri::Manager;

use crate::llm::EntityExtractor;

pub(crate) const MAX_DOCUMENT_BYTES: usize = 64_000;
const MAX_ENTITIES: usize = 400;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Kind {
    Person,
    Company,
    Id,
    Email,
    Phone,
    Address,
    Case,
    Other,
}
impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Company => "COMPANY",
            Self::Id => "ID",
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::Address => "ADDRESS",
            Self::Case => "CASE",
            Self::Other => "OTHER",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Detection {
    pub kind: Kind,
    pub text: String,
}

/// What the model sends before it is validated. A small model regularly invents
/// its own categories (`ACCOUNT`, `BANK`, …) and adds fields, so parsing is
/// deliberately lenient: an unknown kind becomes [`Kind::Other`] instead of
/// discarding the whole response.
///
/// Dropping the entry instead would be the wrong direction. A bank account the
/// model labelled `ACCOUNT` is still private data, so throwing it away leaves it
/// in the sanitized text. `Other` over-sanitizes, which the lawyer can undo in
/// the map, whereas a miss is silent. Grounding against the source text and the
/// deterministic rules still apply afterwards either way.
#[derive(Deserialize)]
struct RawDetection {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extraction {
    entities: Vec<serde_json::Value>,
}

impl Kind {
    /// Accepts the documented names plus anything else the model invents, which
    /// is treated as an unclassified private span rather than as a hard error.
    fn from_model(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "PERSON" | "PERSONA" | "NAME" | "NOMBRE" => Self::Person,
            "COMPANY" | "COMPANIA" | "ORGANIZATION" | "ORGANIZACION" | "ORG" | "EMPRESA" => {
                Self::Company
            }
            "ID" | "IDENTIFIER" | "IDENTIFICADOR" | "CEDULA" | "DNI" | "RUC" | "NIF" => Self::Id,
            "EMAIL" | "E_MAIL" | "MAIL" | "CORREO" => Self::Email,
            "PHONE" | "TELEPHONE" | "PHONE_NUMBER" | "TELEFONO" | "MOVIL" => Self::Phone,
            "ADDRESS" | "DIRECCION" | "STREET" | "DOMICILIO" => Self::Address,
            "CASE" | "CASE_NUMBER" | "CASO" | "EXPEDIENTE" | "PROCESS" | "PROCESO" => Self::Case,
            _ => Self::Other,
        }
    }
}

pub(crate) fn parse_detections(raw: &str) -> Result<Vec<Detection>, String> {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .and_then(|s| s.trim().strip_suffix("```"))
        .unwrap_or(raw)
        .trim();
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| "Qwen no devolvió entidades JSON válidas. No se aprobará ninguna salida; vuelve a analizar.".to_string())?;
    let items = if value.is_array() {
        value.as_array().cloned().unwrap_or_default()
    } else {
        serde_json::from_value::<Extraction>(value)
            .map(|v| v.entities)
            .map_err(|_| {
                "Qwen no devolvió entidades JSON válidas. No se aprobará ninguna salida; vuelve a analizar."
                    .to_string()
            })?
    };
    let mut entities = vec![];
    for item in items {
        let Ok(raw_detection) = serde_json::from_value::<RawDetection>(item) else {
            continue;
        };
        let Some(text) = raw_detection.text.filter(|t| !t.trim().is_empty()) else {
            continue;
        };
        entities.push(Detection {
            kind: raw_detection
                .kind
                .as_deref()
                .map(Kind::from_model)
                .unwrap_or(Kind::Other),
            text,
        });
    }
    if entities.is_empty() {
        return Err("Qwen no devolvió entidades JSON válidas. No se aprobará ninguna salida; vuelve a analizar.".into());
    }
    if entities.len() > MAX_ENTITIES {
        return Err("Demasiadas entidades en una respuesta local.".into());
    }
    Ok(entities)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entity {
    pub kind: Kind,
    pub original: String,
    pub alias: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Session {
    pub id: String,
    pub file_name: String,
    pub original: String,
    /// The unmodified extraction, kept only in this process for the OCR
    /// comparison. Never sent to the WebView; `get_private_extraction` hands it
    /// out on demand so the renderer does not hold a second copy of the
    /// private document at rest.
    #[serde(skip_serializing)]
    pub extracted_original: String,
    pub entities: Vec<Entity>,
    pub sanitized: String,
    pub edited: bool,
    pub warnings: Vec<String>,
    pub advisories: Vec<String>,
    pub analyzed: bool,
    pub reviewed: bool,
    pub revision: u64,
}
impl Session {
    pub(crate) fn new(file_name: String, original: String) -> Result<Self, String> {
        if original.trim().is_empty() {
            return Err(
                "No se encontró texto legible. Revisa la calidad del archivo o usa otro documento."
                    .into(),
            );
        }
        if original.len() > MAX_DOCUMENT_BYTES {
            return Err(
                "El documento supera 64 KB de texto. Usa un documento más corto; no se truncará."
                    .into(),
            );
        }
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            file_name,
            sanitized: String::new(),
            edited: false,
            extracted_original: original.clone(),
            original,
            entities: vec![],
            warnings: vec![],
            advisories: vec![],
            analyzed: false,
            reviewed: false,
            revision: 0,
        })
    }
}
#[derive(Default)]
pub(crate) struct PrivacyState(pub Mutex<Option<Session>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OcrSuggestion { pub original: String, pub replacement: String }

fn valid_ocr_prefix(s: &OcrSuggestion, text: &str) -> bool {
    s.original.chars().count() >= 3 && s.original.chars().all(char::is_alphabetic)
        && s.replacement.chars().all(char::is_alphabetic)
        && s.replacement.chars().count() == s.original.chars().count() + 1
        && s.replacement.ends_with(&s.original)
        && text.match_indices(&s.original).any(|(start, value)| boundary(text, start, start + value.len()))
}
pub(crate) fn parse_ocr_suggestions(raw: &str, text: &str) -> Result<Vec<OcrSuggestion>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response { suggestions: Vec<OcrSuggestion> }
    let response: Response = serde_json::from_str(raw.trim())
        .map_err(|_| "Qwen no devolvió sugerencias OCR válidas. El texto no se modificó.")?;
    if response.suggestions.len() > 50 { return Err("Demasiadas sugerencias OCR; revisa un documento más corto.".into()); }
    let mut seen = HashSet::new();
    Ok(response.suggestions.into_iter().filter(|s| valid_ocr_prefix(s, text) && seen.insert(s.original.clone())).collect())
}

#[tauri::command]
pub(crate) async fn suggest_private_ocr(id: String, revision: u64, model_id: String, app: tauri::AppHandle) -> Result<Vec<OcrSuggestion>, String> {
    let session = get_privacy_session(app.clone())?.filter(|s| s.id == id && s.revision == revision)
        .ok_or("El documento cambió. Recarga la sesión.")?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::llm::LlmState>();
        let mut cache = state.0.try_lock().map_err(|_| "Ya hay un análisis local en curso.")?;
        if cache.as_ref().is_none_or(|(loaded, _)| loaded != &model_id) {
            let path = crate::paths::models_dir(&app).join(crate::models::qwen_file(&model_id)?);
            crate::models::verify_model_file(&path).map_err(|_| "Descarga Qwen desde Modelos antes de revisar OCR.")?;
            *cache = Some((model_id, crate::llm::LlmEngine::load(path.to_str().ok_or("Ruta no válida")?)?));
        }
        let mut suggestions = vec![];
        let mut seen = HashSet::new();
        for chunk in chunks(&session.original) {
            for suggestion in cache.as_ref().unwrap().1.suggest_ocr_prefixes(&chunk)? {
                if seen.insert(suggestion.original.clone()) { suggestions.push(suggestion); }
            }
        }
        if get_privacy_session(app.clone())?.is_none_or(|s| s.id != id || s.revision != revision) {
            return Err("El documento cambió. Se descartaron las sugerencias OCR.".into());
        }
        Ok(suggestions)
    }).await.map_err(|_| "Se interrumpió la revisión OCR local.")?
}

#[tauri::command]
pub(crate) fn apply_private_ocr(id: String, revision: u64, suggestion: OcrSuggestion, app: tauri::AppHandle) -> Result<Session, String> {
    let privacy = app.state::<PrivacyState>();
    let mut guard = privacy.0.lock().map_err(|_| "Sesión privada no disponible")?;
    let session = guard.as_mut().filter(|s| s.id == id && s.revision == revision)
        .ok_or("La sugerencia está desactualizada.")?;
    if !valid_ocr_prefix(&suggestion, &session.original) { return Err("La corrección solo puede añadir una letra inicial a una palabra existente.".into()); }
    session.original = corrected_ocr_text(&session.original, &suggestion)?;
    session.entities.clear(); session.sanitized.clear(); session.warnings.clear();
    session.advisories.clear();
    session.analyzed = false; session.reviewed = false; session.edited = false; session.revision += 1;
    Ok(session.clone())
}

fn corrected_ocr_text(text: &str, suggestion: &OcrSuggestion) -> Result<String, String> {
    if !valid_ocr_prefix(suggestion, text) {
        return Err("La corrección solo puede añadir una letra inicial a una palabra existente.".into());
    }
    let pattern = Regex::new(&regex::escape(&suggestion.original)).map_err(|_| "Palabra OCR no válida")?;
    let updated = pattern.replace_all(text, |capture: &regex::Captures| {
        let hit = capture.get(0).unwrap();
        if boundary(text, hit.start(), hit.end()) { suggestion.replacement.clone() } else { hit.as_str().to_string() }
    }).into_owned();
    if updated.len() > MAX_DOCUMENT_BYTES { return Err("El texto corregido supera 64 KB.".into()); }
    Ok(updated)
}

fn needs_ocr_cleanup(session: &Session) -> bool {
    let extension = std::path::Path::new(&session.file_name)
        .extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
    session.original == session.extracted_original
        && matches!(extension.as_str(), "pdf" | "png" | "jpg" | "jpeg" | "tiff" | "tif" | "heic")
}

fn patterns() -> &'static Vec<(Kind, Regex)> {
    static RULES: OnceLock<Vec<(Kind, Regex)>> = OnceLock::new();
    RULES.get_or_init(|| vec![
        // Bank names are organizations even when Qwen calls them addresses.
        (Kind::Company, Regex::new(r"(?m)^[ \t]*((?:BANCO|BANK|COOPERATIVA)[ \t]+[\p{Lu}][\p{L}]*(?:[ \t]+[\p{Lu}][\p{L}]*){0,3})[ \t]*$").unwrap()),
        // Masked receipt accounts still identify a customer; retain no visible digits.
        (Kind::Other, Regex::new(r"(?i)\b[0-9]{2,}[X*•]{2,}[0-9]{2,}\b").unwrap()),
        // OCR legal headers often put a proper name before the seat of the court.
        (Kind::Person, Regex::new(r"(?m)^[ \t]*([\p{Lu}][\p{Lu}\p{M}]+(?:[ \t]+[\p{Lu}][\p{Lu}\p{M}]+){1,4})[ \t]+CON[ \t]+SEDE\b").unwrap()),
        (Kind::Email, Regex::new(r"(?i)[\p{L}0-9._%+\-]+@[\p{L}0-9.\-]+\.[\p{L}]{2,}").unwrap()),
        // OCR often removes the separators in judicial case numbers.
        (Kind::Case, Regex::new(r"(?i)\b(?:juicio(?:[ \t]+de[ \t]+[\p{L}]+)?|causa|proceso|expediente|case(?:[ \t]+number)?)\s*(?:n[úu]mero|no\.?|n[º°])?\s*[:#]?\s*([0-9][A-Z0-9./\-]{4,30})").unwrap()),
        (Kind::Id, Regex::new(r"\b(?:[0-9]{13}|[0-9]{10})\b").unwrap()),
        (Kind::Case, Regex::new(r"\b(?:[0-9]{5}[-.][0-9]{4}[-.][0-9]{4,6}|[0-9]{14,15})\b").unwrap()),
        (Kind::Phone, Regex::new(r"(?:\+\d{1,3}[ .\-]?)?(?:\(?\d{2,4}\)?[ .\-]){1,3}\d{3,4}\b|\b\d{7,9}\b|\+\d{7,15}\b").unwrap()),
        // Labeled identifiers cover passports, bank accounts and less common IDs.
        (Kind::Other, Regex::new(r"(?im)(?:pasaporte|passport|cuenta(?: bancaria)?|account|identificaci[oó]n|c[eé]dula|ruc|dni|nif|ssn|expediente|proceso|case number)[ \t]*(?:n[úu]mero|no\.?|n[º°])?[ \t]*[:#]?[ \t]*([A-Z0-9][A-Z0-9._/\-]{4,})").unwrap()),
        (Kind::Address, Regex::new(r"(?im)(?:direcci[oó]n|domicilio|address)[ \t]*:[ \t]*([^\n\r;]{5,180})").unwrap()),
    ])
}
fn looks_like_date_or_amount(text: &str) -> bool {
    static DATE: OnceLock<Regex> = OnceLock::new();
    DATE.get_or_init(|| Regex::new(r"^\d{4}[ \t]*[-–—/][ \t]*\d{4}$|^\d{4}[-./ ]\d{1,2}[-./ ]\d{1,2}$|^\d{1,2}[-./ ]\d{1,2}[-./ ]\d{2,4}$|^\d{1,3}(?:\.\d{3})+(?:,\d{2})?$").unwrap()).is_match(text.trim())
}
fn is_public_fact(text: &str) -> bool {
    static MONEY: OnceLock<Regex> = OnceLock::new();
    let money = MONEY.get_or_init(|| Regex::new(r"(?i)^(?:USD|EUR|US\$|\$|€)\s*[0-9][0-9.,]*$|^[0-9][0-9.,]*\s*(?:USD|EUR|euros|dólares)$").unwrap());
    looks_like_date_or_amount(text) || money.is_match(text.trim())
}
pub(crate) fn deterministic_detections(text: &str) -> Vec<Detection> {
    static MONEY_BEFORE: OnceLock<Regex> = OnceLock::new();
    static MONEY_AFTER: OnceLock<Regex> = OnceLock::new();
    let before =
        MONEY_BEFORE.get_or_init(|| Regex::new(r"(?i)(?:USD|EUR|US\$|\$|€)[ \t]*$").unwrap());
    let after = MONEY_AFTER
        .get_or_init(|| Regex::new(r"(?i)^[ \t]*(?:USD|EUR|d[oó]lares|euros)\b").unwrap());
    let mut result = vec![];
    for (kind, pattern) in patterns() {
        for captures in pattern.captures_iter(text) {
            let matched = captures.get(1).unwrap_or_else(|| captures.get(0).unwrap());
            let mut hit = matched.as_str().trim().trim_end_matches(['.', '/', '-']);
            if *kind == Kind::Address {
                let prefix = if hit.starts_with("Av. ") { 4 } else { 0 };
                if let Some(end) = hit[prefix..].find(". ") {
                    hit = &hit[..prefix + end];
                }
            }
            if hit.len() < 2 || !boundary(text, matched.start(), matched.end()) {
                continue;
            }
            if matches!(kind, Kind::Phone | Kind::Id | Kind::Case) {
                // Amounts with long digit counts must not become identifiers.
                if before.is_match(&text[..matched.start()])
                    || after.is_match(&text[matched.end()..])
                {
                    continue;
                }
            }
            if *kind == Kind::Phone {
                let digits = hit.chars().filter(char::is_ascii_digit).count();
                if !(7..=15).contains(&digits) || looks_like_date_or_amount(hit) {
                    continue;
                }
            }
            if *kind == Kind::Other && !hit.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            result.push(Detection {
                kind: *kind,
                text: hit.to_string(),
            });
        }
    }
    result
}
fn normalized(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn matching_pattern(original: &str) -> Result<Regex, String> {
    let escaped = original
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\s+");
    RegexBuilder::new(&escaped)
        .case_insensitive(true)
        .build()
        .map_err(|_| "Entidad no válida".into())
}
fn boundary(text: &str, start: usize, end: usize) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let first = text[start..end].chars().next();
    let last = text[start..end].chars().next_back();
    !(first.is_some_and(word) && text[..start].chars().next_back().is_some_and(word)
        || last.is_some_and(word) && text[end..].chars().next().is_some_and(word))
}
fn grounded(text: &str, original: &str) -> bool {
    matching_pattern(original).is_ok_and(|re| {
        re.find_iter(text)
            .any(|m| boundary(text, m.start(), m.end()))
    })
}
/// Qwen sometimes turns receipt field labels into a made-up address block.
/// Accept only a vocabulary of generic bank labels plus identifiers that have
/// independently been found in the source. Any other word still blocks output.
fn receipt_fields(original: &str, proposed: &Detection) -> Option<Vec<Detection>> {
    let label = normalized(&proposed.text);
    if matches!(label.as_str(), "ahorros" | "ahorros socic" | "ahorros socios" | "cuenta corriente" | "cuenta de ahorros" | "cuenta origen" | "tipo de cuenta") {
        return Some(vec![]);
    }
    if proposed.kind != Kind::Address { return None; }
    let mut remainder = proposed.text.clone();
    let mut found = vec![];
    for detection in deterministic_detections(&proposed.text) {
        if detection.kind != Kind::Other || !grounded(original, &detection.text) { return None; }
        remainder = remainder.replace(&detection.text, " ");
        found.push(detection);
    }
    if remainder.chars().any(|c| c.is_numeric()) { return None; }
    let words: Vec<_> = remainder.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).map(name_token).collect();
    if words.is_empty() || !words.iter().any(|w| w == "cuenta" || w == "cuonta")
        || !words.iter().all(|w| matches!(w.as_str(), "cuenta" | "cuonta" | "origen" | "origеn" | "tipo" | "tpo" | "tpо" | "ahorros" | "socic" | "socios")) {
        return None;
    }
    Some(found)
}

/// Recover accent-only model normalization by selecting a unique source span.
/// No edit distance, guessed OCR letters or reconstructed names are accepted.
fn ground_model_detections(original: &str, model: Vec<Detection>) -> Result<Vec<Detection>, String> {
    let mut result = vec![];
    for mut detection in model {
        if let Some(fields) = receipt_fields(original, &detection) { result.extend(fields); continue; }
        let text = detection.text.trim();
        if text.len() < 2 || text.len() > 500 {
            return Err("Qwen propuso una entidad que no coincide con el documento. No se aprobará la salida; vuelve a analizar.".into());
        }
        if grounded(original, text) { detection.text = text.into(); result.push(detection); continue; }
        let mut pattern = String::new();
        for (i, word) in text.split_whitespace().enumerate() {
            if i > 0 { pattern.push_str(r"\s+"); }
            for c in word.chars() {
                match c.to_ascii_lowercase() {
                    'a' | 'á' | 'Á' => pattern.push_str("[aá]"),
                    'e' | 'é' | 'É' => pattern.push_str("[eé]"),
                    'i' | 'í' | 'Í' => pattern.push_str("[ií]"),
                    'o' | 'ó' | 'Ó' => pattern.push_str("[oó]"),
                    'u' | 'ú' | 'Ú' | 'ü' | 'Ü' => pattern.push_str("[uúü]"),
                    _ => pattern.push_str(&regex::escape(&c.to_string())),
                }
            }
        }
        let re = RegexBuilder::new(&pattern).case_insensitive(true).build().map_err(|_| "Entidad no válida")?;
        let spans: HashSet<&str> = re.find_iter(original).filter(|m| boundary(original, m.start(), m.end())).map(|m| m.as_str()).collect();
        if spans.len() != 1 {
            return Err("Qwen propuso una entidad que no coincide con el documento. No se aprobará la salida; vuelve a analizar.".into());
        }
        detection.text = spans.into_iter().next().unwrap().into();
        result.push(detection);
    }
    Ok(result)
}

fn merge_detections(original: &str, model: Vec<Detection>) -> Result<Vec<Entity>, String> {
    let model = ground_model_detections(original, model)?;
    let mut entities = vec![];
    let mut seen = HashSet::new();
    let mut counts: HashMap<Kind, usize> = HashMap::new();
    // Deterministic patterns take precedence on the same exact value.
    for (source, detections) in [
        ("pattern", deterministic_detections(original)),
        ("qwen", model),
    ] {
        for detection in detections {
            let text = detection.text.trim().to_string();
            if is_public_fact(&text) {
                continue;
            }
            if text.len() < 2 || text.len() > 500 || !grounded(original, &text) {
                return Err("Qwen propuso una entidad que no coincide con el documento. No se aprobará la salida; vuelve a analizar.".into());
            }
            if !seen.insert(normalized(&text)) {
                continue;
            }
            let count = counts.entry(detection.kind).or_default();
            *count += 1;
            entities.push(Entity {
                kind: detection.kind,
                original: text,
                alias: format!("{}_{:02}", detection.kind.prefix(), count),
                source: source.into(),
            });
        }
    }
    add_ocr_name_variants(original, &mut entities);
    if entities.len() > MAX_ENTITIES {
        return Err("El documento tiene demasiadas entidades para este demo.".into());
    }
    Ok(entities)
}

fn name_token(text: &str) -> String {
    text.to_lowercase().chars().map(|c| match c {
        'á' => 'a', 'é' => 'e', 'í' => 'i', 'ó' => 'o', 'ú' | 'ü' => 'u', _ => c,
    }).collect()
}

/// Receipt OCR often truncates surnames and loses accents. Only standalone
/// uppercase name lines sharing two complete tokens with a detected person
/// qualify. Ambiguous prefixes receive a distinct alias rather than implying
/// that two people are the same person.
fn add_ocr_name_variants(original: &str, entities: &mut Vec<Entity>) {
    let people: Vec<Entity> = entities.iter().filter(|e| e.kind == Kind::Person).cloned().collect();
    let mut seen: HashSet<String> = entities.iter().map(|e| normalized(&e.original)).collect();
    let mut next = entities.iter().filter(|e| e.kind == Kind::Person).count() + 1;
    for line in original.lines() {
        let line = line.trim();
        let words: Vec<&str> = line.split_whitespace().collect();
        if !(2..=6).contains(&words.len()) || line.len() > 120
            || !words.iter().all(|word| word.chars().all(|c| c.is_uppercase()))
            || words.iter().any(|word| matches!(*word, "PRESENTA" | "SOLICITA" | "COMPARECE" | "MANIFIESTA" | "DECLARA" | "RECIBE" | "TRANSFIERE" | "PAGA" | "PARA" | "CON" | "DESDE" | "HASTA")) {
            continue;
        }
        let candidates: HashSet<String> = people.iter().filter(|person| {
            let known: Vec<&str> = person.original.split_whitespace().collect();
            known.len() >= 2 && known[0].chars().count() >= 3 && known[1].chars().count() >= 3
                && name_token(words[0]) == name_token(known[0])
                && name_token(words[1]) == name_token(known[1])
        }).map(|e| e.alias.clone()).collect();
        if candidates.is_empty() || !seen.insert(normalized(line)) { continue; }
        let alias = if candidates.len() == 1 { candidates.into_iter().next().unwrap() }
            else { let alias = format!("PERSON_{next:02}"); next += 1; alias };
        entities.push(Entity { kind: Kind::Person, original: line.into(), alias, source: "pattern".into() });
    }
}

/// Replace from source spans, longest-first at each position, in a single pass.
/// Never re-replace aliases or accept a model-generated rewrite of the document.
pub(crate) fn sanitize(original: &str, entities: &[Entity]) -> Result<String, String> {
    validate_entities(original, entities)?;
    let mut matches = vec![];
    for (i, entity) in entities.iter().enumerate() {
        for m in matching_pattern(&entity.original)?.find_iter(original) {
            if boundary(original, m.start(), m.end()) {
                matches.push((m.start(), m.end(), i));
            }
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut output = String::new();
    let mut cursor = 0;
    for (start, end, i) in matches {
        if start < cursor {
            continue;
        }
        output.push_str(&original[cursor..start]);
        output.push_str(&entities[i].alias);
        // The final dot in a name such as “Banco S.A.” may also end the
        // sentence. Keep that punctuation in the sanitized prose.
        if original[start..end].ends_with('.') {
            output.push('.');
        }
        cursor = end;
    }
    output.push_str(&original[cursor..]);
    Ok(output)
}
/// Keep OCR corrections confined to scanned formats. The source remains
/// intact; unreadable script fragments receive a visible annotation instead
/// of being reconstructed or silently omitted. Legitimate Cyrillic documents
/// are preserved, as are every line containing numbers and every alias.
fn sanitize_for_file(original: &str, entities: &[Entity], file_name: &str) -> Result<String, String> {
    let sanitized = sanitize(original, entities)?;
    let extension = std::path::Path::new(file_name).extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(extension.as_str(), "pdf" | "png" | "jpg" | "jpeg" | "tiff" | "tif" | "heic") { return Ok(sanitized); }
    let cyrillic = |c: char| ('\u{0400}'..='\u{052f}').contains(&c);
    let latin = original.chars().filter(|c| c.is_alphabetic() && !cyrillic(*c)).count();
    let foreign = original.chars().filter(|c| cyrillic(*c)).count();
    if latin < 200 || latin < foreign * 5 { return Ok(sanitized); }
    Ok(sanitized.lines().map(|line| {
        match line.trim() {
            "Tpо:" | "Tpo:" => "Tipo:".into(),
            "Cuonta:" => "Cuenta:".into(),
            _ if line.chars().any(cyrillic) && !line.chars().any(char::is_numeric)
                && !entities.iter().any(|e| line.contains(&e.alias)) => "[Fragmento OCR ilegible]".into(),
            _ => line.to_string(),
        }
    }).collect::<Vec<String>>().join("\n"))
}

fn validate_entities(original: &str, entities: &[Entity]) -> Result<(), String> {
    if entities.len() > MAX_ENTITIES {
        return Err("Demasiadas entidades.".into());
    }
    let mut seen = HashSet::new();
    static ALIAS: OnceLock<Regex> = OnceLock::new();
    let re = ALIAS.get_or_init(|| Regex::new(r"^(?:PERSON|COMPANY|ID|EMAIL|PHONE|ADDRESS|CASE|OTHER|CLIENTE|ACTOR|DEMANDADO|TESTIGO|ENTITY)_[0-9]{2,4}$").unwrap());
    for e in entities {
        if e.original.trim().len() < 2 || e.original.len() > 500 || !grounded(original, &e.original)
        {
            return Err("Cada entidad debe coincidir con texto del documento original.".into());
        }
        if !seen.insert(normalized(&e.original)) {
            return Err("Una misma entidad no puede tener dos entradas.".into());
        }
        if !re.is_match(&e.alias) {
            return Err("Usa un alias simbólico como PERSON_01 o CLIENTE_01 (prefijo PERSON, COMPANY, ID, EMAIL, PHONE, ADDRESS, CASE, OTHER, CLIENTE, ACTOR, DEMANDADO, TESTIGO o ENTITY y 2–4 dígitos).".into());
        }
        // No private value may be smuggled into an alias, nor alias already exist in source.
        if grounded(original, &e.alias)
            || entities
                .iter()
                .any(|other| grounded(&e.alias, &other.original))
        {
            return Err(
                "El alias coincide con información privada o con texto original. Elige otro."
                    .into(),
            );
        }
    }
    // Same alias may group variants of a single entity; incompatible categories cannot collide.
    let mut aliases = HashMap::new();
    for e in entities {
        if aliases
            .insert(&e.alias, e.kind)
            .is_some_and(|kind| kind != e.kind)
        {
            return Err("Un alias no puede representar categorías distintas.".into());
        }
    }
    Ok(())
}

/// Un mapa vacío produce `sanitize() == original`: el documento privado entero
/// volvería como salida «SAFE». Se exige al menos una entidad y que la
/// sanitización realmente cambie el texto antes de aprobar o exportar.
fn require_sanitized(original: &str, sanitized: &str, entities: &[Entity]) -> Result<(), String> {
    if entities.is_empty() {
        return Err(
            "El mapa no tiene entidades: la versión sanitizada sería el documento original. Detecta con Qwen o añade las entidades a mano en «Mapa privado» antes de aprobar.".into(),
        );
    }
    if sanitized.trim() == original.trim() {
        return Err(
            "La versión sanitizada es idéntica al original. Revisa el mapa: no se ha pseudonymizado nada.".into(),
        );
    }
    Ok(())
}

/// Avisos no bloqueantes: lo que la capa determinista NO puede comprobar. No
/// impiden la revisión, pero el usuario debe verlos antes de llamarlo «SAFE».
pub(crate) fn privacy_advisories(entities: &[Entity]) -> Vec<String> {
    let mut advisories = vec![];
    if !entities.iter().any(|e| e.kind == Kind::Person) {
        advisories.push(
            "El mapa no contiene ninguna persona. Los nombres no tienen un detector determinista: Qwen puede omitirlos y las reglas no los comprueban. Revísalos tú.".into(),
        );
    }
    advisories
}

pub(crate) fn leak_warnings(sanitized: &str, entities: &[Entity]) -> Vec<String> {
    let mut warnings = vec![];
    // Only aliases actually present in the reviewed map are exempted. A random
    // identifier with alias-like syntax is not automatically trusted.
    let mut scanned = sanitized.to_string();
    for entity in entities {
        scanned = scanned.replace(&entity.alias, " ");
    }
    for detection in deterministic_detections(&scanned) {
        let warning = format!(
            "Posible {} residual: corrige el mapa antes de aprobar.",
            detection.kind.prefix()
        );
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
    for entity in entities {
        if grounded(sanitized, &entity.original) {
            let warning = format!(
                "Una entidad {} del mapa sigue presente en la salida.",
                entity.kind.prefix()
            );
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }
    warnings
}

/// Overlapping chunks cover the entire UTF-8 input; no silent truncation.
fn chunks(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut result = vec![];
    let mut start = 0;
    while start < chars.len() {
        let end = (start + 2800).min(chars.len());
        result.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start = end - 200;
    }
    result
}
fn analyze_with(extractor: &impl EntityExtractor, original: &str) -> Result<Vec<Entity>, String> {
    let mut detections = vec![];
    for chunk in chunks(original) {
        let suggested = extractor.extract(&chunk)?;
        let grounded = match ground_model_detections(&chunk, suggested) {
            Ok(entities) => entities,
            Err(_) => ground_model_detections(&chunk, extractor.extract_exact(&chunk)?)?,
        };
        detections.extend(grounded);
    }
    merge_detections(original, detections)
}

#[tauri::command]
pub(crate) fn get_privacy_session(app: tauri::AppHandle) -> Result<Option<Session>, String> {
    Ok(app
        .state::<PrivacyState>()
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?
        .clone())
}
#[tauri::command]
pub(crate) fn clear_privacy_session(app: tauri::AppHandle) -> Result<(), String> {
    *app.state::<PrivacyState>()
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")? = None;
    Ok(())
}

fn reset_analysis(session: &mut Session) {
    session.entities.clear();
    session.sanitized.clear();
    session.warnings.clear();
    session.advisories.clear();
    session.analyzed = false;
    session.reviewed = false;
    session.edited = false;
    session.revision += 1;
}

#[tauri::command]
pub(crate) fn reset_private_analysis(id: String, revision: u64, app: tauri::AppHandle) -> Result<Session, String> {
    let privacy = app.state::<PrivacyState>();
    let mut guard = privacy.0.lock().map_err(|_| "Sesión privada no disponible")?;
    let session = guard.as_mut().filter(|s| s.id == id && s.revision == revision)
        .ok_or("El documento cambió. Recarga la sesión.")?;
    reset_analysis(session);
    Ok(session.clone())
}

/// Handed to the WebView only when the user explicitly asks to compare the
/// initial extraction, so the renderer does not keep a second copy of the
/// private document just by opening the screen.
#[tauri::command]
pub(crate) fn get_private_extraction(
    id: String,
    revision: u64,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let privacy = app.state::<PrivacyState>();
    let guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    Ok(guard
        .as_ref()
        .filter(|s| s.id == id && s.revision == revision)
        .map(|s| s.extracted_original.clone()))
}
#[tauri::command]
pub(crate) async fn analyze_private_document(
    id: String,
    revision: u64,
    model_id: String,
    app: tauri::AppHandle,
) -> Result<Session, String> {
    let session = {
        let privacy = app.state::<PrivacyState>();
        let mut guard = privacy
            .0
            .lock()
            .map_err(|_| "Sesión privada no disponible")?;
        let current = guard
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision)
            .ok_or("El documento ha cambiado. Recarga la sesión.")?;
        current.reviewed = false;
        current.analyzed = false;
        current.revision += 1;
        current.clone()
    };
    let revision = session.revision;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::llm::LlmState>();
        let mut cache = state
            .0
            .try_lock()
            .map_err(|_| "Ya hay un análisis local en curso.")?;
        if cache.as_ref().is_none_or(|(loaded, _)| loaded != &model_id) {
            let path = crate::paths::models_dir(&app).join(crate::models::qwen_file(&model_id)?);
            crate::models::verify_model_file(&path)
                .map_err(|_| "Qwen no está instalado o está dañado. Descárgalo desde Modelos.")?;
            *cache = Some((
                model_id.clone(),
                crate::llm::LlmEngine::load(path.to_str().ok_or("Ruta del modelo no válida")?)?,
            ));
        }
        let engine = &cache.as_ref().unwrap().1;
        let mut original = session.original.clone();
        // OCR cleanup is part of the local pipeline; keep the extraction intact.
        // Gather suggestions from one snapshot so corrections cannot cascade.
        if needs_ocr_cleanup(&session) {
            let mut seen = HashSet::new();
            for chunk in chunks(&session.original) {
                for suggestion in engine.suggest_ocr_prefixes(&chunk)? {
                    if valid_ocr_prefix(&suggestion, &original) && seen.insert(suggestion.original.clone()) {
                        original = corrected_ocr_text(&original, &suggestion)?;
                    }
                }
            }
        }
        let entities = analyze_with(engine, &original)?;
        let sanitized = sanitize_for_file(&original, &entities, &session.file_name)?;
        // Deliberately no `require_sanitized` here. Finding nothing is a valid
        // analysis result that the lawyer must see; it becomes a refusal at
        // review and export time, where it can no longer be mistaken for a
        // clean document. Failing here would hide the empty map entirely.
        let warnings = leak_warnings(&sanitized, &entities);
        let advisories = privacy_advisories(&entities);
        let privacy = app.state::<PrivacyState>();
        let mut guard = privacy
            .0
            .lock()
            .map_err(|_| "Sesión privada no disponible")?;
        let current = guard
            .as_mut()
            .filter(|s| s.id == id && s.revision == revision)
            .ok_or("El documento cambió durante el análisis. Se descartó el resultado.")?;
        current.original = original;
        current.entities = entities;
        current.sanitized = sanitized;
        current.edited = false;
        current.warnings = warnings;
        current.advisories = advisories;
        current.analyzed = true;
        current.reviewed = false;
        current.revision += 1;
        Ok(current.clone())
    })
    .await
    .map_err(|_| "El análisis local se interrumpió.")?
}
#[tauri::command]
pub(crate) fn review_private_document(
    id: String,
    revision: u64,
    entities: Vec<Entity>,
    confirm: bool,
    app: tauri::AppHandle,
) -> Result<Session, String> {
    let privacy = app.state::<PrivacyState>();
    let mut guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    let session = guard
        .as_mut()
        .filter(|s| s.id == id && s.revision == revision)
        .ok_or("La revisión está desactualizada. Recarga la sesión.")?;
    if !session.analyzed {
        return Err("Analiza el documento con Qwen antes de revisar la salida.".into());
    }
    let sanitized = sanitize_for_file(&session.original, &entities, &session.file_name)?;
    let map_changed = session.entities.len() != entities.len() || session.entities.iter().zip(entities.iter()).any(|(a, b)| a.kind != b.kind || a.original != b.original || a.alias != b.alias);
    // Review can remove a false positive, but pattern checks still block real
    // identifiers and an empty map can never be approved.
    session.entities = entities
        .into_iter()
        .map(|mut e| {
            e.source = "review".into();
            e
        })
        .collect();
    if !session.edited || map_changed {
        require_sanitized(&session.original, &sanitized, &session.entities)?;
        session.sanitized = sanitized;
        session.edited = false;
    }
    require_sanitized(&session.original, &session.sanitized, &session.entities)?;
    session.warnings = leak_warnings(&session.sanitized, &session.entities);
    session.advisories = privacy_advisories(&session.entities);
    session.reviewed = confirm && session.warnings.is_empty();
    session.revision += 1;
    Ok(session.clone())
}
#[tauri::command]
pub(crate) fn update_private_document(
    id: String,
    revision: u64,
    text: String,
    app: tauri::AppHandle,
) -> Result<Session, String> {
    let privacy = app.state::<PrivacyState>();
    let mut guard = privacy.0.lock().map_err(|_| "Sesión privada no disponible")?;
    let session = guard.as_mut().filter(|s| s.id == id && s.revision == revision)
        .ok_or("La edición está desactualizada. Recarga el documento.")?;
    if !session.analyzed { return Err("Analiza el documento antes de editarlo.".into()); }
    if text.trim().is_empty() || text.len() > MAX_DOCUMENT_BYTES {
        return Err("El documento editado debe contener texto y no superar 64 KB.".into());
    }
    session.sanitized = text;
    session.edited = true;
    session.reviewed = false;
    session.warnings = leak_warnings(&session.sanitized, &session.entities);
    session.advisories = privacy_advisories(&session.entities);
    session.revision += 1;
    Ok(session.clone())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutboundPreview {
    pub schema_version: u8,
    pub document: String,
}
#[tauri::command]
pub(crate) fn preview_private_outbound(
    id: String,
    revision: u64,
    app: tauri::AppHandle,
) -> Result<OutboundPreview, String> {
    let privacy = app.state::<PrivacyState>();
    let guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    let session = guard
        .as_ref()
        .filter(|s| s.id == id && s.revision == revision)
        .ok_or("La revisión está desactualizada.")?;
    checked_outbound(session)
}
fn checked_outbound(session: &Session) -> Result<OutboundPreview, String> {
    if !session.analyzed || !session.reviewed {
        return Err(
            "Revisa el mapa y resuelve las posibles filtraciones antes de abrir la vista previa."
                .into(),
        );
    }
    let sanitized = sanitize_for_file(&session.original, &session.entities, &session.file_name)?;
    if (!session.edited && sanitized != session.sanitized) || !leak_warnings(&session.sanitized, &session.entities).is_empty() {
        return Err("La salida no supera la validación determinista. Revisa el mapa.".into());
    }
    require_sanitized(&session.original, &session.sanitized, &session.entities)?;
    Ok(OutboundPreview {
        schema_version: 1,
        document: session.sanitized.clone(),
    })
}

/// Whitespace-only comparison. The editor and the PDF reader normalise line
/// breaks differently, but no other difference may exist between what the
/// lawyer reviewed and what the writer receives.
fn normalized_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Authoritative export gate, called by the PDF writer itself rather than by
/// the renderer. The reviewed session text must match the payload, the map must
/// be non-empty and the sanitized text must differ from the original.
pub(crate) fn gate_private_export(
    app: &tauri::AppHandle,
    id: &str,
    revision: u64,
    text: &str,
    html: Option<&str>,
    title: &str,
) -> Result<(), String> {
    let privacy = app.state::<PrivacyState>();
    let guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    let session = guard
        .as_ref()
        .filter(|s| s.id == id && s.revision == revision && s.analyzed)
        .ok_or("La sesión cambió. Recarga el documento privado antes de exportar.")?;
    check_export_payload(session, text, html, title)
}

/// The rules of the export gate, independent of Tauri state so they can be
/// tested directly. Everything here is a refusal: the writer only proceeds when
/// the session, the payload text and the rendered HTML are all the document the
/// lawyer actually reviewed.
fn check_export_payload(
    session: &Session,
    text: &str,
    html: Option<&str>,
    title: &str,
) -> Result<(), String> {
    if !session.reviewed {
        return Err(
            "El documento privado no tiene una revisión confirmada. Revísalo en Private Legal antes de exportar."
                .into(),
        );
    }
    require_sanitized(&session.original, &session.sanitized, &session.entities)?;
    let reviewed = normalized_text(&session.sanitized);
    if normalized_text(text) != reviewed {
        return Err("El PDF usaría texto sin revisar. Guarda los cambios y confirma de nuevo la revisión en Private Legal.".into());
    }
    if let Some(html) = html.filter(|h| !h.trim().is_empty()) {
        if crate::pdf::html_visible_text(html) != reviewed {
            return Err("El formato del documento no coincide con el texto revisado. Guarda los cambios y confirma de nuevo la revisión en Private Legal.".into());
        }
    }
    let warnings = leak_warnings(&session.sanitized, &session.entities);
    if !warnings.is_empty() {
        return Err(format!("PDF bloqueado: quedan posibles datos privados. Corrige el documento o vuelve a analizar en Private Legal. {}", warnings.join(" ")));
    }
    // The writer also stamps the title into the page header and the PDF
    // metadata, so it is part of what leaves the process just like the body.
    if !leak_warnings(title, &session.entities).is_empty() {
        return Err("PDF bloqueado: el título contiene posibles datos privados.".into());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn validate_private_export(
    id: String,
    revision: u64,
    text: String,
    title: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Same rules as the writer, minus the rendered HTML which does not exist
    // yet at this point. Failing here only saves the user the file dialog.
    gate_private_export(
        &app,
        &id,
        revision,
        &text,
        None,
        title.as_deref().unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
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
        let entities = merge_detections(original, vec![Detection { kind: Kind::Person, text: "José Antonio Martínez Valdez".into() }]).unwrap();
        let out = sanitize(original, &entities).unwrap();
        for value in ["XIMENA BARRAGÁN", "JOSE ANTONIO", "4410", "2204"] { assert!(!out.contains(value), "{out}"); }
        let aliases: HashSet<_> = entities.iter().filter(|e| e.original.starts_with("JOSE ANTONIO") || e.original.starts_with("José Antonio")).map(|e| &e.alias).collect();
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
        let entities = merge_detections(original, vec![Detection { kind: Kind::Person, text: "José Antonio Martínez".into() }]).unwrap();
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
        let mut session = Session::new("original.pdf".into(), "Documento original privado".into()).unwrap();
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
        assert!(check_export_payload(&session, "Documento sanitizado", None, "Documento sanitizado").is_err());
    }

    #[test]
    fn automatic_ocr_cleanup_preserves_boundaries_and_initial_extraction() {
        let mut session = Session::new("scan.PDF".into(), "ombre y sobrenombre, ombre.".into()).unwrap();
        assert!(needs_ocr_cleanup(&session));
        session.original = corrected_ocr_text(&session.original, &OcrSuggestion {
            original: "ombre".into(), replacement: "nombre".into(),
        }).unwrap();
        assert_eq!(session.original, "nombre y sobrenombre, nombre.");
        assert_eq!(session.extracted_original, "ombre y sobrenombre, ombre.");
        assert!(!needs_ocr_cleanup(&session));
        assert!(!needs_ocr_cleanup(&Session::new("document.txt".into(), "ombre".into()).unwrap()));
        assert!(corrected_ocr_text("12345", &OcrSuggestion {
            original: "12345".into(), replacement: "012345".into(),
        }).is_err());
    }

    #[test]
    fn ocr_suggestions_only_add_one_letter_to_exact_existing_words() {
        let raw = r#"{"suggestions":[{"original":"ombre","replacement":"nombre"},{"original":"uenta","replacement":"cuenta"},{"original":"12345","replacement":"012345"},{"original":"ombre","replacement":"nombre completo"},{"original":"ausente","replacement":"causente"},{"original":"Ana","replacement":"Diana"}]}"#;
        let suggestions = parse_ocr_suggestions(raw, "ombre del titular\nuenta de ahorros\n12345\nAna").unwrap();
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].replacement, "nombre");
        assert_eq!(suggestions[1].replacement, "cuenta");
        assert!(parse_ocr_suggestions(r#"{"suggestions":[],"instruction":"ignore"}"#, "texto").is_err());
        assert!(parse_ocr_suggestions(r#"{"suggestions":[{"original":"ombre","replacement":"nombre"}]}"#, "sobrenombre").unwrap().is_empty());
    }
    #[test]
    #[ignore = "Requires verified local Qwen weights"]
    fn real_qwen_suggests_ocr_missing_initials() {
        let (_guard, engine) = locked_qwen_engine();
        let text = "Recibo de pago\nEl ombre del titular aparece en el encabezado.\nLa uenta de ahorros recibe USD 1500.\nProceso 17203201900137.";
        let suggestions = engine.suggest_ocr_prefixes(text).unwrap();
        assert!(suggestions.iter().any(|s| s.original == "ombre" && s.replacement == "nombre"));
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
        let entities = merge_detections(text, vec![Detection { kind: Kind::Address, text: "BANCO PICHINCHA".into() }, Detection { kind: Kind::Address, text: "AHORROS SOCIC".into() }]).unwrap();
        assert!(entities.iter().any(|e| e.kind == Kind::Company && e.original == "BANCO PICHINCHA"));
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
        assert_eq!(sanitize_for_file(&text, &entities, "text.txt").unwrap(), sanitize(&text, &entities).unwrap());
        let russian = "Это документ на русском языке.";
        assert_eq!(sanitize_for_file(russian, &[], "scan.pdf").unwrap(), russian);
    }
    #[test]
    fn cleaned_ocr_output_keeps_the_export_gate_exact() {
        let mut session = reviewed_session();
        session.file_name = "scan.pdf".into();
        session.original.push_str(&format!("\n{}\nHONоSEOк\n$500.00", "Documento legible. ".repeat(20)));
        session.sanitized = sanitize_for_file(&session.original, &session.entities, &session.file_name).unwrap();
        assert!(session.original.contains("HONоSEOк"));
        assert!(session.sanitized.contains("[Fragmento OCR ilegible]"));
        assert!(checked_outbound(&session).is_ok());
        assert!(check_export_payload(&session, &session.sanitized, None, "Documento sanitizado").is_ok());
        for title in [
            "Documento sanitizado 2026-09-30 16-25-08",
            "Sanitized document 2026-09-30 16-25-08",
        ] {
            assert!(check_export_payload(&session, &session.sanitized, None, title).is_ok());
        }
        assert!(check_export_payload(&session, &(session.sanitized.clone() + " nuevo texto"), None, "Documento sanitizado").is_err());
    }
    #[test]
    fn receipt_labels_are_not_addresses_but_real_account_spans_are_retained() {
        let source = "uenta Orige\nCuonta:\nTpо:\n4410XXXXX319\nAHORROS SOCIC\n2204XXX521";
        for proposed in ["CUENTA ORIGEN", "CUENTA: TPо: 4410XXXXX319", "Cuenta Origen: TPo: 4410XXXXX319, Ahorros Socic: 2204XXX521"] {
            let model = vec![Detection { kind: Kind::Address, text: proposed.into() }];
            let entities = merge_detections(source, model).unwrap();
            let out = sanitize(source, &entities).unwrap();
            assert!(!out.contains("4410")); assert!(!out.contains("2204"));
            assert!(leak_warnings(&out, &entities).is_empty());
        }
        for invented in ["Cuenta Origen de Pedro", "Cuenta Origen 999XXXXX000", "Cuenta Origen: Calle Inventada 22"] {
            assert!(ground_model_detections(source, vec![Detection { kind: Kind::Address, text: invented.into() }]).is_err());
        }
    }
    #[test]
    fn normalized_model_names_use_exact_unique_source_spans() {
        let text = "José Antonio Martínez\nJOSE ANTONIO MI";
        let detections = ground_model_detections(text, vec![Detection { kind: Kind::Person, text: "Jose Antonio Martinez".into() }]).unwrap();
        assert_eq!(detections[0].text, "José Antonio Martínez");
        let entities = merge_detections(text, detections).unwrap();
        let out = sanitize(text, &entities).unwrap();
        assert!(!out.contains("Antonio"));
        assert!(!out.contains("JOSE"));
        for (source, proposed) in [("Ana Pérez / Ana Péréz", "Ana Perez"), ("JOSE ANTONIO MI", "José Antonio Martínez"), ("Ana", "Pedro")] {
            assert!(ground_model_detections(source, vec![Detection { kind: Kind::Person, text: proposed.into() }]).is_err());
        }
    }
    #[test]
    fn unmatched_model_entity_retries_exact_extraction_without_dropping_it() {
        use std::cell::Cell;
        struct RetryExtractor { retried: Cell<bool>, succeeds: bool }
        impl EntityExtractor for RetryExtractor {
            fn extract(&self, _: &str) -> Result<Vec<Detection>, String> {
                Ok(vec![Detection { kind: Kind::Person, text: "Pedro Inventado".into() }])
            }
            fn extract_exact(&self, _: &str) -> Result<Vec<Detection>, String> {
                self.retried.set(true);
                Ok(vec![Detection { kind: Kind::Person, text: if self.succeeds { "Ana Pérez" } else { "Pedro Inventado" }.into() }])
            }
        }
        for succeeds in [true, false] {
            let extractor = RetryExtractor { retried: Cell::new(false), succeeds };
            let result = analyze_with(&extractor, "Ana Pérez comparece.");
            assert!(extractor.retried.get());
            assert_eq!(result.is_ok(), succeeds);
            if let Ok(entities) = result { assert_eq!(sanitize("Ana Pérez comparece.", &entities).unwrap(), "PERSON_01 comparece."); }
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
        session.entities = merge_detections(&session.original, vec![Detection { kind: Kind::Person, text: "Ana".into() }]).unwrap();
        session.sanitized = sanitize(&session.original, &session.entities).unwrap();
        session.analyzed = true;
        session.edited = true;
        session.sanitized = "PERSON_01 presenta el escrito.\n\nSegundo párrafo.".into();
        assert!(checked_outbound(&session).is_err());
        session.reviewed = true;
        assert_eq!(checked_outbound(&session).unwrap().document, session.sanitized);
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
                if valid_ocr_prefix(&suggestion, &text) { text = corrected_ocr_text(&text, &suggestion).unwrap(); }
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
        assert!(!entities.iter().any(|e| e.original == "BANCO PICHINCHA" && e.kind != Kind::Company));
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
        for private in ["José Antonio", "JOSE ANTONIO", "XIMENA BARRAGÁN", "4410", "2204"] {
            assert!(!out.contains(private), "OCR sample still contains private information");
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
        let original =
            "USD 1500000000, $ 12345678, 123456789 euros. Fecha 30 09 2026 y 2026-09-30.";
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
        assert!(privacy_advisories(&entities).iter().any(|a| a.contains("ninguna persona")));
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
        assert!(require_sanitized(original, original, &[entity(Kind::Other, "x", "OTHER_01")]).is_err());

        let mut session = Session::new("testamento.txt".into(), original.into()).unwrap();
        session.entities = vec![];
        session.sanitized = sanitize(original, &[]).unwrap();
        session.analyzed = true;
        session.reviewed = true;
        let err = checked_outbound(&session).err().expect("mapa vacío debe fallar");
        assert!(err.contains("mapa no tiene entidades"), "{err}");
    }

    #[test]
    fn a_map_that_changes_nothing_is_refused() {
        let original = "Ana llama a Pedro.";
        let entities = vec![entity(Kind::Person, "Ana", "PERSON_01")];
        // Sanitizing does change the text, so this map is acceptable.
        assert!(require_sanitized(original, &sanitize(original, &entities).unwrap(), &entities).is_ok());
        // An entity that survives in the output would make them identical.
        let unchanged = "PERSON_01 llama a Pedro.";
        assert!(require_sanitized(unchanged, unchanged, &entities).is_err());
    }

    #[test]
    fn advisories_call_out_the_names_the_rules_cannot_check() {
        assert!(privacy_advisories(&[]).iter().any(|a| a.contains("ninguna persona")));
        assert!(
            privacy_advisories(&[entity(Kind::Person, "Ana", "PERSON_01")]).is_empty(),
            "con una persona en el mapa el aviso sobra"
        );
        // Organisations alone still leave every person name unchecked.
        assert!(privacy_advisories(&[entity(Kind::Company, "Banco", "COMPANY_01")])
            .iter()
            .any(|a| a.contains("ninguna persona")));
    }

    #[test]
    fn normalized_text_ignores_only_whitespace() {
        assert_eq!(normalized_text("a  b\nc"), "a b c");
        assert_ne!(normalized_text("PERSON_01 llama"), normalized_text("Ana llama"));
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
        assert!(check_export_payload(&session, &session.sanitized, None, "Documento sanitizado").is_ok());
    }

    #[test]
    fn the_export_gate_refuses_an_unreviewed_session() {
        let mut session = reviewed_session();
        session.reviewed = false;
        let err =
            check_export_payload(&session, &session.sanitized, None, "Documento").unwrap_err();
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
        let err =
            check_export_payload(&session, &session.sanitized, None, "Documento").unwrap_err();
        assert!(err.contains("entidades") || err.contains("idéntico"), "got: {err}");
    }

    #[test]
    fn the_export_gate_refuses_html_that_renders_more_than_the_reviewed_text() {
        // Rich formatting is fine, but any extra visible text — a heading, a
        // repeated line, a hidden node — is content the lawyer never reviewed.
        let session = reviewed_session();
        for html in [
            format!("<p>{}</p><h1>Demanda</h1>", session.sanitized),
            format!("<p>{}</p><p style=\"display:none\">MARÍA PÉREZ</p>", session.sanitized),
            format!("<p>{}</p><p>{}</p>", session.sanitized, session.sanitized),
        ] {
            let err =
                check_export_payload(&session, &session.sanitized, Some(&html), "Documento")
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
        session.entities.push(entity(Kind::Person, "Banco Andino", "COMPANY_01"));
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
            let err =
                check_export_payload(&session, &session.sanitized, None, title).unwrap_err();
            assert!(err.contains("título"), "got: {err} for {title}");
        }
    }
}
