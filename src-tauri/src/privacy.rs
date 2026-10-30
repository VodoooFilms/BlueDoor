//! The privacy boundary is deterministic and owns the original document.
//! LLM output is only a list of suggestions, never the sanitized document.
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager};

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
    let explicitly_empty = items.is_empty();
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
    if entities.is_empty() && !explicitly_empty {
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
    /// False when local analysis could not read every fragment. A partial map
    /// is still shown to the lawyer, but it must never be approvable: the
    /// uncovered text was never inspected, so nothing proves it is clean.
    pub analysis_complete: bool,
    pub reviewed: bool,
    pub revision: u64,
    pub layout_available: bool,
    /// Immutable page pixels and exact extraction offsets remain in memory.
    #[serde(skip_serializing)]
    pub layout: Option<std::sync::Arc<crate::layout::DocumentLayout>>,
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
            analysis_complete: false,
            reviewed: false,
            revision: 0,
            layout_available: false,
            layout: None,
        })
    }
}
#[derive(Default)]
pub(crate) struct PrivacyState(pub Mutex<Option<Session>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OcrSuggestion {
    pub original: String,
    pub replacement: String,
}

fn valid_ocr_prefix(s: &OcrSuggestion, text: &str) -> bool {
    s.original.chars().count() >= 3
        && s.original.chars().all(char::is_alphabetic)
        && s.replacement.chars().all(char::is_alphabetic)
        && s.replacement.chars().count() == s.original.chars().count() + 1
        && s.replacement.ends_with(&s.original)
        && text
            .match_indices(&s.original)
            .any(|(start, value)| boundary(text, start, start + value.len()))
}
pub(crate) fn parse_ocr_suggestions(raw: &str, text: &str) -> Result<Vec<OcrSuggestion>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        suggestions: Vec<OcrSuggestion>,
    }
    let response: Response = serde_json::from_str(raw.trim())
        .map_err(|_| "Qwen no devolvió sugerencias OCR válidas. El texto no se modificó.")?;
    if response.suggestions.len() > 50 {
        return Err("Demasiadas sugerencias OCR; revisa un documento más corto.".into());
    }
    let mut seen = HashSet::new();
    Ok(response
        .suggestions
        .into_iter()
        .filter(|s| valid_ocr_prefix(s, text) && seen.insert(s.original.clone()))
        .collect())
}

#[tauri::command]
pub(crate) async fn suggest_private_ocr(
    id: String,
    revision: u64,
    model_id: String,
    app: tauri::AppHandle,
) -> Result<Vec<OcrSuggestion>, String> {
    let session = get_privacy_session(app.clone())?
        .filter(|s| s.id == id && s.revision == revision)
        .ok_or("El documento cambió. Recarga la sesión.")?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::llm::LlmState>();
        let mut cache = state
            .0
            .try_lock()
            .map_err(|_| "Ya hay un análisis local en curso.")?;
        if cache.as_ref().is_none_or(|(loaded, _)| loaded != &model_id) {
            let path = crate::paths::models_dir(&app).join(crate::models::qwen_file(&model_id)?);
            crate::models::verify_model_file(&path)
                .map_err(|_| "Descarga Qwen desde Modelos antes de revisar OCR.")?;
            *cache = Some((
                model_id,
                crate::llm::LlmEngine::load(path.to_str().ok_or("Ruta no válida")?)?,
            ));
        }
        let mut suggestions = vec![];
        let mut seen = HashSet::new();
        for chunk in chunks(&session.original) {
            for suggestion in cache.as_ref().unwrap().1.suggest_ocr_prefixes(&chunk)? {
                if seen.insert(suggestion.original.clone()) {
                    suggestions.push(suggestion);
                }
            }
        }
        if get_privacy_session(app.clone())?.is_none_or(|s| s.id != id || s.revision != revision) {
            return Err("El documento cambió. Se descartaron las sugerencias OCR.".into());
        }
        Ok(suggestions)
    })
    .await
    .map_err(|_| "Se interrumpió la revisión OCR local.")?
}

#[tauri::command]
pub(crate) fn apply_private_ocr(
    id: String,
    revision: u64,
    suggestion: OcrSuggestion,
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
        .ok_or("La sugerencia está desactualizada.")?;
    if session.layout_available {
        return Err("Para conservar el formato, revisa los valores exactos en el mapa; las coordenadas corresponden a la extracción original.".into());
    }
    if !valid_ocr_prefix(&suggestion, &session.original) {
        return Err(
            "La corrección solo puede añadir una letra inicial a una palabra existente.".into(),
        );
    }
    session.original = corrected_ocr_text(&session.original, &suggestion)?;
    session.entities.clear();
    session.sanitized.clear();
    session.warnings.clear();
    session.advisories.clear();
    session.analyzed = false;
    session.analysis_complete = false;
    session.reviewed = false;
    session.edited = false;
    session.revision += 1;
    Ok(session.clone())
}

fn corrected_ocr_text(text: &str, suggestion: &OcrSuggestion) -> Result<String, String> {
    if !valid_ocr_prefix(suggestion, text) {
        return Err(
            "La corrección solo puede añadir una letra inicial a una palabra existente.".into(),
        );
    }
    let pattern =
        Regex::new(&regex::escape(&suggestion.original)).map_err(|_| "Palabra OCR no válida")?;
    let updated = pattern
        .replace_all(text, |capture: &regex::Captures| {
            let hit = capture.get(0).unwrap();
            if boundary(text, hit.start(), hit.end()) {
                suggestion.replacement.clone()
            } else {
                hit.as_str().to_string()
            }
        })
        .into_owned();
    if updated.len() > MAX_DOCUMENT_BYTES {
        return Err("El texto corregido supera 64 KB.".into());
    }
    Ok(updated)
}

fn needs_ocr_cleanup(session: &Session) -> bool {
    let extension = std::path::Path::new(&session.file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    !session.layout_available
        && session.original == session.extracted_original
        && matches!(
            extension.as_str(),
            "pdf" | "png" | "jpg" | "jpeg" | "tiff" | "tif" | "heic"
        )
}

fn patterns() -> &'static Vec<(Kind, Regex)> {
    static RULES: OnceLock<Vec<(Kind, Regex)>> = OnceLock::new();
    RULES.get_or_init(|| vec![
        // Bank names are organizations even when Qwen calls them addresses.
        (Kind::Company, Regex::new(r"(?m)^[ \t]*((?:BANCO|BANK|COOPERATIVA)[ \t]+[\p{Lu}][\p{L}]*(?:[ \t]+[\p{Lu}][\p{L}]*){0,3})[ \t]*$").unwrap()),
        // Explicit receipt fields carry names even when English NER misses
        // uppercase Spanish names. Values remain exact source spans.
        (Kind::Person, Regex::new(r"(?im)^[ \t]*(?:titular|beneficiario|ordenante|remitente|destinatario|nombre(?:s)?(?:[ \t]+y[ \t]+apellidos|[ \t]+del[ \t]+(?:titular|beneficiario|ordenante)))(?:[ \t]*:[ \t]*(?:\r?\n[ \t]*)?|[ \t]*\r?\n[ \t]*)([\p{L}][\p{L}\p{M}.'’\-]*(?:[ \t]+[\p{L}][\p{L}\p{M}.'’\-]*){1,5})[ \t]*\r?$").unwrap()),
        // Explicit name/surname fields are private even when the English NER
        // emits no PERSON anchor. Keep the exact value (including a single
        // surname); never join a following form row or erase its field label.
        (Kind::Person, Regex::new(r"(?im)^[ \t]*(?:apellidos?(?:[ \t]+(?:paterno|materno))?|(?:primer|segundo)[ \t]+apellido|apellidos[ \t]+y[ \t]+nombres|nombre[ \t]+completo|full[ \t]+name|(?:first|middle|last|family|given)[ \t]+names?|surnames?)[ \t]*:[ \t]*([\p{L}\p{M}][\p{L}\p{M}.'’\-]*(?:[ \t]+[\p{L}\p{M}][\p{L}\p{M}.'’\-]*){0,7})[ \t]*\r?$").unwrap()),
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
            if *kind == Kind::Person {
                let value = normalized(hit);
                if [
                    "cuenta",
                    "ahorros",
                    "monto",
                    "transferencia",
                    "comprobante",
                    "saldo",
                    "identificacion",
                    "cedula",
                ]
                .iter()
                .any(|word| value.split_whitespace().any(|part| part == *word))
                    || matches!(
                        value.as_str(),
                        "no disponible" | "sin datos" | "consumidor final"
                    )
                {
                    continue;
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
pub(crate) fn matching_pattern(original: &str) -> Result<Regex, String> {
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
pub(crate) fn boundary(text: &str, start: usize, end: usize) -> bool {
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
#[cfg(test)]
pub(crate) fn is_non_identifying_for_test(text: &str) -> bool {
    is_non_identifying(text)
}

/// Values that identify no party: law citations, articles, bare dates, amounts
/// and similar. Redacting them destroys the legal argument without protecting
/// anyone, so the model is not allowed to spend an alias on them.
/// Markers of a legal citation rather than of a party.
const ARTICLE_WORDS: [&str; 10] = [
    "articulo",
    "art.",
    "numeral",
    "inciso",
    "codigo",
    "ley organica",
    "constitucion",
    "codigo del trabajo",
    "codigo civil",
    "codigo de procedimiento",
];
/// Currency markers, used to recognise an amount rather than an identifier.
const CURRENCIES: [&str; 8] = ["usd", "eur", "gbp", "dolar", "euro", "$", "us$", "€"];
/// Month names, used to recognise a bare date.
const DATE_WORDS: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];

/// Label prefixes the model copies along with the value, such as
/// "CEDULA: 1712345678". The label belongs to the document's layout, not to the
/// identifier, and aliasing it would hide what kind of value was redacted.
fn strip_field_label(text: &str) -> &str {
    match text.split_once(':') {
        // Only a short all-caps label counts, never "Escritura publica: 0452-...".
        Some((label, value))
            if label.len() <= 28 && label.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') =>
        {
            value.trim()
        }
        _ => text,
    }
}

fn is_non_identifying(text: &str) -> bool {
    // Compare without accents: the document may write "Artículo" or "articulo".
    // The label is dropped here too, so the check does not depend on the caller.
    let lowered: String = normalized(strip_field_label(text))
        .chars()
        .map(|c| match c {
            'á' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'Ú' => 'u',
            'ñ' | 'Ñ' => 'n',
            other => other,
        })
        .collect();
    if matches!(
        lowered.as_str(),
        "autoridad"
            | "autoridad competente"
            | "juez"
            | "jueza"
            | "notario"
            | "notaria"
            | "abogado"
            | "abogada"
            | "compareciente"
            | "solicitante"
    ) {
        return true;
    }
    // Article and code citations, with or without the surrounding law name.
    if ARTICLE_WORDS.iter().any(|w| lowered.contains(w)) {
        return true;
    }
    // A date with no other identifying content.
    if DATE_WORDS.iter().any(|w| lowered.contains(w)) && !lowered.contains('@') {
        let digits: usize = lowered.chars().filter(|c| c.is_ascii_digit()).count();
        let letters: usize = lowered.chars().filter(|c| c.is_alphabetic()).count();
        // "15 de marzo de 2025": mostly a date and nothing else.
        return digits >= 2 && letters <= 16;
    }
    // An amount with a currency: "USD 4.826,00". The value is money, not a party,
    // so it stays readable in the protected document.
    if CURRENCIES.iter().any(|c| lowered.contains(c)) {
        let letters: usize = lowered.chars().filter(|c| c.is_alphabetic()).count();
        let digits: usize = lowered.chars().filter(|c| c.is_ascii_digit()).count();
        if digits > 0 && letters <= CURRENCIES.iter().map(|c| c.len()).sum::<usize>() + 4 {
            return true;
        }
    }
    // A bare number. Long digit runs are national IDs, RUCs, account or case
    // numbers and must stay; only short grouped amounts are money.
    let stripped: String = lowered
        .chars()
        .filter(|c| !c.is_ascii_digit() && *c != '.' && *c != ',')
        .collect();
    let digits: usize = lowered.chars().filter(|c| c.is_ascii_digit()).count();
    if stripped.is_empty() && digits > 0 && digits <= 7 {
        return true;
    }
    false
}

/// Accept only a vocabulary of generic bank labels plus identifiers that have
/// independently been found in the source. Any other word still blocks output.
fn receipt_fields(original: &str, proposed: &Detection) -> Option<Vec<Detection>> {
    let label = normalized(&proposed.text);
    if matches!(
        label.as_str(),
        "ahorros"
            | "ahorros socic"
            | "ahorros socios"
            | "cuenta corriente"
            | "cuenta de ahorros"
            | "cuenta origen"
            | "tipo de cuenta"
    ) {
        return Some(vec![]);
    }
    if proposed.kind != Kind::Address {
        return None;
    }
    let mut remainder = proposed.text.clone();
    let mut found = vec![];
    for detection in deterministic_detections(&proposed.text) {
        if detection.kind != Kind::Other || !grounded(original, &detection.text) {
            return None;
        }
        remainder = remainder.replace(&detection.text, " ");
        found.push(detection);
    }
    if remainder.chars().any(|c| c.is_numeric()) {
        return None;
    }
    let words: Vec<_> = remainder
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(name_token)
        .collect();
    if words.is_empty()
        || !words.iter().any(|w| w == "cuenta" || w == "cuonta")
        || !words.iter().all(|w| {
            matches!(
                w.as_str(),
                "cuenta"
                    | "cuonta"
                    | "origen"
                    | "origеn"
                    | "tipo"
                    | "tpo"
                    | "tpо"
                    | "ahorros"
                    | "socic"
                    | "socios"
            )
        })
    {
        return None;
    }
    Some(found)
}

/// Recover accent-only model normalization by selecting a unique source span.
/// No edit distance, guessed OCR letters or reconstructed names are accepted.
fn ground_model_detections(
    original: &str,
    model: Vec<Detection>,
) -> Result<Vec<Detection>, String> {
    let mut result = vec![];
    let proposed = model.len();
    let mut dropped = 0usize;
    for mut detection in model {
        if let Some(fields) = receipt_fields(original, &detection) {
            result.extend(fields);
            continue;
        }
        let trimmed = detection.text.trim();
        // Drop a leading field label so the alias replaces only the value.
        let trimmed = strip_field_label(trimmed);
        let text = trimmed;
        if text.len() < 2 || text.len() > 500 {
            dropped += 1;
            continue;
        }
        // A 1.7B model still files law citations and dates as entities even when
        // told not to. They identify nobody, and redacting them would gut the
        // legal argument, so they are dropped here instead of trusted.
        if is_non_identifying(text) {
            continue;
        }
        if grounded(original, text) {
            detection.text = text.into();
            result.push(detection);
            continue;
        }
        let mut pattern = String::new();
        for (i, word) in text.split_whitespace().enumerate() {
            if i > 0 {
                pattern.push_str(r"\s+");
            }
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
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build()
            .map_err(|_| "Entidad no válida")?;
        let spans: HashSet<&str> = re
            .find_iter(original)
            .filter(|m| boundary(original, m.start(), m.end()))
            .map(|m| m.as_str())
            .collect();
        if spans.len() != 1 {
            // The model dropped or added a character, as it does with degraded
            // OCR. The span is not in the document, so it cannot leak; drop it.
            // If most of the answer is unusable the model is off the rails and
            // the caller splits the fragment instead of trusting the rest.
            dropped += 1;
            continue;
        }
        detection.text = spans.into_iter().next().unwrap().into();
        result.push(detection);
    }
    if proposed > 0 && dropped * 2 > proposed {
        return Err("Qwen propuso una entidad que no coincide con el documento. No se aprobará la salida; vuelve a analizar.".into());
    }
    Ok(result)
}

fn merge_detections(original: &str, model: Vec<Detection>) -> Result<Vec<Entity>, String> {
    merge_detection_sources(original, model, vec![])
}
pub(crate) fn merge_detection_sources(
    original: &str,
    model: Vec<Detection>,
    local_names: Vec<Detection>,
) -> Result<Vec<Entity>, String> {
    let model = ground_model_detections(original, model)?;
    let local_names = ground_model_detections(original, local_names)?;
    let mut entities = vec![];
    let mut seen = HashSet::new();
    let mut counts: HashMap<Kind, usize> = HashMap::new();
    // Deterministic patterns take precedence on the same exact value.
    for (source, detections) in [
        ("pattern", deterministic_detections(original)),
        ("local-ner", local_names),
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
    complete_detected_names(original, &mut entities);
    add_ocr_name_variants(original, &mut entities);
    if entities.len() > MAX_ENTITIES {
        return Err("El documento tiene demasiadas entidades para este demo.".into());
    }
    Ok(entities)
}

fn name_token(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            _ => c,
        })
        .collect()
}

/// Extend only grounded person detections inside a bounded source name run.
/// This covers omitted middle names and OCR-spelled surnames without inventing
/// words, crossing table cells, joining parties, or consuming legal prose.
/// A bounded adjacent OCR line can complete a standalone anchored name.
fn complete_detected_names(original: &str, entities: &mut Vec<Entity>) {
    let people: Vec<Entity> = entities
        .iter()
        .filter(|e| e.kind == Kind::Person)
        .cloned()
        .collect();
    if people.is_empty() {
        return;
    }
    let words_re = Regex::new(r"\p{L}+(?:[-'’]\p{L}+)*").expect("name words");
    let connector = |word: &str| matches!(word, "de" | "del" | "la" | "las" | "los" | "da" | "dos");
    let reserved = |word: &str| {
        matches!(
            word,
            "nombre"
                | "nombres"
                | "apellido"
                | "apellidos"
                | "firma"
                | "firmado"
                | "titular"
                | "ordenante"
                | "beneficiario"
                | "compareciente"
                | "senor"
                | "señor"
                | "señora"
                | "dr"
                | "dra"
                | "sr"
                | "sra"
                | "senora"
                | "abogado"
                | "abogada"
                | "doctor"
                | "doctora"
                | "notario"
                | "notaria"
                | "juez"
                | "jueza"
                | "comparece"
                | "comparecen"
                | "presenta"
                | "presentan"
                | "solicita"
                | "solicitan"
                | "declara"
                | "declaran"
                | "manifiesta"
                | "recibe"
                | "paga"
                | "transfiere"
                | "suscribe"
                | "ante"
                | "con"
                | "para"
                | "por"
                | "en"
                | "y"
                | "e"
                | "o"
                | "el"
                | "un"
                | "una"
                | "como"
                | "quien"
                | "que"
                | "a"
                | "al"
                | "es"
                | "son"
                | "se"
                | "su"
                | "sus"
                | "banco"
                | "bank"
                | "cooperativa"
                | "sociedad"
                | "compania"
                | "empresa"
                | "total"
                | "usd"
                | "cedula"
                | "identificacion"
                | "proceso"
                | "email"
                | "correo"
                | "telefono"
                | "direccion"
                | "fecha"
                | "comprobante"
                | "transferencia"
                | "contrato"
                | "documento"
                | "republica"
                | "ecuador"
                | "provincia"
                | "canton"
                | "ciudad"
                | "domicilio"
        )
    };
    let other_values: Vec<String> = entities
        .iter()
        .filter(|e| {
            e.kind == Kind::Company
                && (e.source == "pattern"
                    || e.original.split_whitespace().any(|word| {
                        matches!(
                            name_token(word).as_str(),
                            "banco" | "bank" | "cooperativa" | "sociedad" | "compania" | "empresa"
                        )
                    }))
        })
        .map(|e| e.original.clone())
        .collect();
    let other_words: HashSet<String> = other_values
        .iter()
        .flat_map(|value| {
            words_re
                .find_iter(value)
                .map(|word| name_token(word.as_str()))
        })
        .collect();
    let other_word = |word: &str| other_words.contains(&name_token(word));
    let person_patterns: Vec<_> = people
        .iter()
        .filter(|person| person.original.chars().count() >= 3)
        .filter_map(|person| {
            matching_pattern(&person.original)
                .ok()
                .map(|pattern| (person, pattern))
        })
        .collect();
    let mut seen: HashSet<String> = entities.iter().map(|e| normalized(&e.original)).collect();
    let mut next = entities.iter().filter(|e| e.kind == Kind::Person).count() + 1;
    let mut name_lines: Vec<(&str, bool)> = original.lines().map(|line| (line, false)).collect();
    // OCR and narrow pages can place the final surname on the next line.
    // Only join an already anchored, standalone multiword name with a short
    // adjacent name-like continuation. Blank lines, tabs, field labels, prose,
    // organizations and separately detected people remain boundaries.
    for (split, _) in original.match_indices('\n') {
        let begin = original[..split].rfind('\n').map_or(0, |i| i + 1);
        let end = original[split + 1..]
            .find('\n')
            .map_or(original.len(), |i| split + 1 + i);
        let first = original[begin..split].trim();
        let value = first
            .split_once(':')
            .map_or(first, |(_, value)| value.trim());
        let next_line = original[split + 1..end].trim();
        let name_like = |text: &str| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_alphabetic() || matches!(c, ' ' | '\u{a0}' | '-' | '\'' | '’'))
                && text.split_whitespace().all(|word| {
                    !reserved(&name_token(word))
                        && !other_word(word)
                        && (word.chars().next().is_some_and(char::is_uppercase)
                            || connector(&name_token(word)))
                })
        };
        if !(2..=7).contains(&value.split_whitespace().count())
            || !(1..=3).contains(&next_line.split_whitespace().count())
            || !name_like(value)
            || !name_like(next_line)
            || connector(&name_token(
                next_line.split_whitespace().last().unwrap_or(""),
            ))
            || !person_patterns.iter().any(|(_, pattern)| {
                pattern
                    .find_iter(value)
                    .any(|hit| boundary(value, hit.start(), hit.end()))
            })
            || entities.iter().any(|e| {
                matching_pattern(&e.original).ok().is_some_and(|pattern| {
                    pattern
                        .find_iter(next_line)
                        .any(|hit| boundary(next_line, hit.start(), hit.end()))
                })
            })
        {
            continue;
        }
        let offset = original[begin..split].find(value).unwrap();
        name_lines.push((original[begin + offset..end].trim(), true));
    }
    for (line, wrapped) in name_lines {
        let line = line.trim();
        let field = line.split_once(':').filter(|(label, _)| {
            matches!(
                name_token(label.trim()).as_str(),
                "nombre"
                    | "nombres"
                    | "nombre completo"
                    | "nombres y apellidos"
                    | "apellidos y nombres"
                    | "firma"
                    | "titular"
                    | "ordenante"
                    | "beneficiario"
                    | "compareciente"
            )
        });
        let source = field.map_or(line, |(_, value)| value.trim());
        let words: Vec<_> = words_re.find_iter(source).collect();
        // Lowercase completion requires an explicit name field; ordinary prose
        // cannot be distinguished safely from an unlabeled lowercase name.
        let standalone = (2..=8).contains(&words.len())
            && source.len() <= 160
            && source.chars().all(|c| {
                c.is_alphabetic()
                    || matches!(c, ' ' | '\u{a0}' | '-' | '\'' | '’')
                    || (wrapped && matches!(c, '\n' | '\r'))
            })
            && words
                .iter()
                .all(|w| !reserved(&name_token(w.as_str())) && !other_word(w.as_str()))
            && (field.is_some()
                || words.iter().all(|w| {
                    w.as_str().chars().next().is_some_and(char::is_uppercase)
                        || connector(&name_token(w.as_str()))
                }))
            && !connector(&name_token(words[0].as_str()))
            && !connector(&name_token(words.last().unwrap().as_str()));
        let mut runs = vec![];
        if standalone {
            runs.push((0, source.len()));
        } else {
            let mut start: Option<usize> = None;
            let mut end = 0;
            let mut count = 0;
            for word in &words {
                let token = name_token(word.as_str());
                let capital = word.as_str().chars().next().is_some_and(char::is_uppercase);
                let gap = &source[end..word.start()];
                let contiguous =
                    gap.chars().all(|c| matches!(c, ' ' | '\u{a0}')) && gap.chars().count() <= 2;
                if !contiguous
                    || reserved(&token)
                    || other_word(word.as_str())
                    || (!capital && !connector(&token))
                {
                    if let Some(a) = start.take() {
                        if (2..=8).contains(&count) {
                            runs.push((a, end));
                        }
                    }
                    count = 0;
                }
                if !reserved(&token)
                    && !other_word(word.as_str())
                    && (capital || (start.is_some() && connector(&token)))
                {
                    if start.is_none() {
                        start = Some(word.start());
                    }
                    count += 1;
                    end = word.end();
                } else {
                    end = word.end();
                }
            }
            if let Some(a) = start {
                if (2..=8).contains(&count) {
                    runs.push((a, end));
                }
            }
        }
        for (start, end) in runs {
            let span = source[start..end].trim();
            let span_words: Vec<_> = span.split_whitespace().collect();
            if span_words.len() < 2
                || span.len() > 160
                || span_words.last().is_some_and(|w| connector(&name_token(w)))
            {
                continue;
            }
            let anchors: Vec<_> = person_patterns
                .iter()
                .filter(|(_, pattern)| {
                    pattern
                        .find_iter(span)
                        .any(|hit| boundary(span, hit.start(), hit.end()))
                })
                .map(|(person, _)| *person)
                .collect();
            if anchors.is_empty() || !seen.insert(normalized(span)) {
                continue;
            }
            let aliases: HashSet<_> = anchors.iter().map(|e| e.alias.as_str()).collect();
            let alias = if aliases.len() == 1 {
                anchors[0].alias.clone()
            } else {
                let alias = format!("PERSON_{next:02}");
                next += 1;
                alias
            };
            entities.push(Entity {
                kind: Kind::Person,
                original: span.into(),
                alias,
                source: "name-coverage".into(),
            });
        }
    }
}

/// Receipt OCR often truncates surnames and loses accents. Only standalone
/// uppercase name lines sharing two complete tokens with a detected person
/// qualify. Ambiguous prefixes receive a distinct alias rather than implying
/// that two people are the same person.
fn add_ocr_name_variants(original: &str, entities: &mut Vec<Entity>) {
    let people: Vec<Entity> = entities
        .iter()
        .filter(|e| e.kind == Kind::Person)
        .cloned()
        .collect();
    let mut seen: HashSet<String> = entities.iter().map(|e| normalized(&e.original)).collect();
    let mut next = entities.iter().filter(|e| e.kind == Kind::Person).count() + 1;
    for line in original.lines() {
        let line = line.trim();
        let words: Vec<&str> = line.split_whitespace().collect();
        if !(2..=6).contains(&words.len())
            || line.len() > 120
            || !words
                .iter()
                .all(|word| word.chars().all(|c| c.is_uppercase()))
            || words.iter().any(|word| {
                matches!(
                    *word,
                    "PRESENTA"
                        | "SOLICITA"
                        | "COMPARECE"
                        | "MANIFIESTA"
                        | "DECLARA"
                        | "RECIBE"
                        | "TRANSFIERE"
                        | "PAGA"
                        | "PARA"
                        | "CON"
                        | "DESDE"
                        | "HASTA"
                )
            })
        {
            continue;
        }
        let candidates: HashSet<String> = people
            .iter()
            .filter(|person| {
                let known: Vec<&str> = person.original.split_whitespace().collect();
                known.len() >= 2
                    && known[0].chars().count() >= 3
                    && known[1].chars().count() >= 3
                    && name_token(words[0]) == name_token(known[0])
                    && name_token(words[1]) == name_token(known[1])
            })
            .map(|e| e.alias.clone())
            .collect();
        if candidates.is_empty() || !seen.insert(normalized(line)) {
            continue;
        }
        let alias = if candidates.len() == 1 {
            candidates.into_iter().next().unwrap()
        } else {
            let alias = format!("PERSON_{next:02}");
            next += 1;
            alias
        };
        entities.push(Entity {
            kind: Kind::Person,
            original: line.into(),
            alias,
            source: "pattern".into(),
        });
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
fn sanitize_for_file(
    original: &str,
    entities: &[Entity],
    file_name: &str,
) -> Result<String, String> {
    let sanitized = sanitize(original, entities)?;
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "pdf" | "png" | "jpg" | "jpeg" | "tiff" | "tif" | "heic"
    ) {
        return Ok(sanitized);
    }
    let cyrillic = |c: char| ('\u{0400}'..='\u{052f}').contains(&c);
    let latin = original
        .chars()
        .filter(|c| c.is_alphabetic() && !cyrillic(*c))
        .count();
    let foreign = original.chars().filter(|c| cyrillic(*c)).count();
    if latin < 200 || latin < foreign * 5 {
        return Ok(sanitized);
    }
    Ok(sanitized
        .lines()
        .map(|line| match line.trim() {
            "Tpо:" | "Tpo:" => "Tipo:".into(),
            "Cuonta:" => "Cuenta:".into(),
            _ if line.chars().any(cyrillic)
                && !line.chars().any(char::is_numeric)
                && !entities.iter().any(|e| line.contains(&e.alias)) =>
            {
                "[Fragmento OCR ilegible]".into()
            }
            _ => line.to_string(),
        })
        .collect::<Vec<String>>()
        .join("\n"))
}

fn validate_entities(original: &str, entities: &[Entity]) -> Result<(), String> {
    if entities.len() > MAX_ENTITIES {
        return Err("Demasiadas entidades.".into());
    }
    let mut seen = HashSet::new();
    for e in entities {
        if e.original.trim().len() < 2 || e.original.len() > 500 || !grounded(original, &e.original)
        {
            return Err("Cada entidad debe coincidir con texto del documento original.".into());
        }
        if !seen.insert(normalized(&e.original)) {
            return Err("Una misma entidad no puede tener dos entradas.".into());
        }
        if e.alias.trim().is_empty()
            || e.alias.chars().count() > 120
            || e.alias.chars().any(char::is_control)
        {
            return Err("Escribe un alias de 1 a 120 caracteres, sin saltos de línea.".into());
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
/// Shown when the local pass could not read the whole document. Approval stays
/// closed until the lawyer re-runs the analysis or splits the file.
const INCOMPLETE_ANALYSIS: &str = "El análisis local no cubrió todo el documento. Vuelve a protegerlo o divide el archivo antes de aprobar la revisión.";

fn require_sanitized(original: &str, sanitized: &str, entities: &[Entity]) -> Result<(), String> {
    if entities.is_empty() {
        return Err(
            "El mapa no tiene entidades: la versión protegida sería el documento original. Detecta con Qwen o añade las entidades a mano en «Mapa privado» antes de aprobar.".into(),
        );
    }
    if sanitized.trim() == original.trim() {
        return Err(
            "La versión protegida es idéntica al original. Revisa el mapa: no se ha pseudonymizado nada.".into(),
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
    if entities.iter().any(|e| e.source == "name-coverage") {
        advisories.push("Se completaron nombres parciales con palabras contiguas del original. Revisa los nombres compuestos y las zonas OCR antes de aprobar.".into());
    }
    advisories
}

pub(crate) fn leak_warnings(sanitized: &str, entities: &[Entity]) -> Vec<String> {
    let mut warnings = vec![];
    // Only aliases actually present in the reviewed map are exempted. A random
    // identifier with alias-like syntax is not automatically trusted.
    let mut scanned = sanitized.to_string();
    for entity in entities {
        let pattern = Regex::new(&regex::escape(&entity.alias)).unwrap();
        let mut next = String::new();
        let mut cursor = 0;
        for hit in pattern.find_iter(&scanned) {
            if !boundary(&scanned, hit.start(), hit.end())
                || scanned[..hit.start()].ends_with('@')
                || scanned[hit.end()..].starts_with('@')
            {
                continue;
            }
            next.push_str(&scanned[cursor..hit.start()]);
            next.push(' ');
            cursor = hit.end();
        }
        next.push_str(&scanned[cursor..]);
        scanned = next;
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
///
/// Bound the response as well as the prompt. A real five-page PDF with tables
/// exhausted 2048 answer tokens for a 4000-character block despite its prompt
/// fitting comfortably in the context. Smaller tasks avoid repeated full-budget
/// generations; the original source is still covered without truncation.
pub(crate) const CHUNK_CHARS: usize = 2000;
/// Characters repeated between neighbouring fragments so an entity split by a
/// fragment boundary is still seen whole.
const CHUNK_OVERLAP: usize = 300;
/// A fragment shorter than this is not split again: below it the model has no
/// room left to fail, so it is reported as a coverage gap instead.
const MIN_SPLIT_CHARS: usize = 600;

#[cfg(test)]
pub(crate) fn min_split_chars_for_test() -> usize {
    MIN_SPLIT_CHARS
}

#[cfg(test)]
pub(crate) fn chunk_chars_for_test() -> usize {
    CHUNK_CHARS
}

fn chunks(text: &str) -> Vec<String> {
    let chunk_chars = crate::llm::CHUNK_SIZE.load(std::sync::atomic::Ordering::Relaxed);
    let chars: Vec<char> = text.chars().collect();
    let mut result = vec![];
    let mut start = 0;
    while start < chars.len() {
        let end = (start + chunk_chars).min(chars.len());
        result.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start = end - CHUNK_OVERLAP.min(chunk_chars - 1);
    }
    result
}
/// Result of a local analysis pass. A fragment the model could not finish is
/// recorded instead of aborting: on a long document a single rambling fragment
/// used to discard every entity found in the rest of the file, which left the
/// lawyer with no map at all and looked like Qwen had not run.
#[derive(Debug)]
pub(crate) struct Analysis {
    pub entities: Vec<Entity>,
    pub fragments: usize,
    pub failed_fragments: usize,
}
impl Analysis {
    fn complete(&self) -> bool {
        self.failed_fragments == 0
    }
    /// Shown to the lawyer when the map is not trustworthy on its own.
    fn coverage_warning(&self) -> Option<String> {
        (!self.complete()).then(|| {
            format!(
                "El análisis local no pudo leer {} de {} fragmentos del documento. La versión protegida no está verificada en esa parte: revisa el texto a mano o divide el documento.",
                self.failed_fragments, self.fragments
            )
        })
    }
}

/// Same pass, reporting `(done, total)` per fragment. A multi-fragment document
/// takes several model round trips; without this the UI shows one frozen label
/// for minutes and the app looks like it never started.
fn analyze_with_progress(
    extractor: &(impl EntityExtractor + ?Sized),
    original: &str,
    on_fragment: &mut dyn FnMut(usize, usize, usize),
) -> Result<Analysis, String> {
    let all = chunks(original);
    let mut detections = vec![];
    let mut failed = 0usize;
    // Fragments still to read. A fragment the model cannot answer inside the
    // budget is split and retried instead of being dropped: measured on the real
    // model, two fragments with nearly identical prompts (1674 and 1684 tokens)
    // either finished or ran out, so no fixed size can prevent it.
    let mut pending: Vec<String> = all;
    let mut read = 0usize;
    while let Some(chunk) = pending.pop() {
        crate::llm::check_cancelled()?;
        // Completed blocks plus queued work. A split increases the total; do
        // not show "1 of 1", "2 of 2" throughout a multi-block document.
        let position = read + 1;
        let total = read + pending.len() + 1;
        let mut report = |tokens: usize| on_fragment(position, total, tokens);
        report(0);
        // Two attempts: the general pass, then the exact pass. Both may fail on
        // a fragment the model cannot finish; that is local to this fragment.
        // Anchor against the whole document, not the fragment: splitting a
        // fragment can cut an entity in half, and text the model read is still
        // real text of the document even if it straddles the split.
        let first = extractor.extract(&chunk, &mut report);
        // Repeating an overflowing task with a longer instruction spends the
        // same budget again. Split it immediately instead. The exact-copy retry
        // is reserved for malformed or ungrounded suggestions.
        let overflowed = first
            .as_ref()
            .is_err_and(|reason| reason.contains("no terminó dentro del límite"));
        let grounded = first
            .and_then(|suggested| ground_model_detections(original, suggested))
            .or_else(|reason| {
                if overflowed {
                    return Err(reason);
                }
                extractor
                    .extract_exact(&chunk, &mut report)
                    .and_then(|suggested| ground_model_detections(original, suggested))
            });
        match grounded {
            Ok(entities) => {
                detections.extend(entities);
                read += 1;
            }
            Err(_) => {
                // Too dense for one answer: read the same text in two halves.
                let chars: Vec<char> = chunk.chars().collect();
                if chars.len() > MIN_SPLIT_CHARS {
                    let middle = chars.len() / 2;
                    // A name or identifier crossing the split must remain whole
                    // in at least one child. Preserve context on both sides.
                    let context = CHUNK_OVERLAP / 2;
                    let left: String = chars[..(middle + context).min(chars.len())]
                        .iter()
                        .collect();
                    let right: String = chars[middle.saturating_sub(context)..].iter().collect();
                    pending.push(right);
                    pending.push(left);
                } else {
                    failed += 1;
                    read += 1;
                }
            }
        }
    }
    Ok(Analysis {
        entities: merge_detections(original, detections)?,
        fragments: read,
        failed_fragments: failed,
    })
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
    let llm = app.state::<crate::llm::LlmState>();
    #[allow(unused_mut)]
    let mut cache = llm
        .0
        .try_lock()
        .map_err(|_| "Espera a que termine el análisis antes de cerrar el documento.")?;
    if let Some((_, engine)) = cache.as_ref() {
        engine.clear_extractions();
    }
    #[cfg(target_os = "android")]
    {
        *cache = None;
        crate::android::remember_source("")?;
    }
    *app.state::<PrivacyState>()
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")? = None;
    crate::session_status::clear(&app)?;
    Ok(())
}

fn reset_analysis(session: &mut Session) {
    session.entities.clear();
    session.sanitized.clear();
    session.warnings.clear();
    session.advisories.clear();
    session.analyzed = false;
    session.analysis_complete = false;
    session.reviewed = false;
    session.edited = false;
    session.revision += 1;
}

#[tauri::command]
pub(crate) fn reset_private_analysis(
    id: String,
    revision: u64,
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
    let analysis_lease = crate::llm::begin_analysis()?;
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
        current.analysis_complete = false;
        current.revision += 1;
        current.clone()
    };
    let requested_revision = revision;
    let revision = session.revision;
    tauri::async_runtime::spawn_blocking(move || {
        let _analysis_lease = analysis_lease;
        let state = app.state::<crate::llm::LlmState>();
        let mut cache = state
            .0
            .try_lock()
            .map_err(|_| "Ya hay un análisis local en curso.")?;
        let _ = app.emit("private-analysis-progress", (id.as_str(), requested_revision, "model_verify"));
        #[cfg(target_os = "android")]
        let gliner = if crate::models::gliner_files(&model_id).is_some() {
            *cache = None;
            Some(crate::gliner::GlinerEngine::load(&app, &model_id)?)
        } else { None };
        if crate::models::gliner_files(&model_id).is_none() && cache.as_ref().is_none_or(|(loaded, _)| loaded != &model_id) {
            let path = crate::paths::models_dir(&app).join(crate::models::qwen_file(&model_id)?);
            let verify_timer = crate::performance::Timer::new("model_verify");
            crate::models::verify_model_file(&path)
                .map_err(|_| "Qwen no está instalado o está dañado. Descárgalo desde Modelos.")?;
            drop(verify_timer);
            let _ = app.emit("private-analysis-progress", (id.as_str(), requested_revision, "model_load"));
            let _load_timer = crate::performance::Timer::new("model_load");
            *cache = Some((
                model_id.clone(),
                crate::llm::LlmEngine::load(path.to_str().ok_or("Ruta del modelo no válida")?)?,
            ));
        }
        let _analysis_timer = crate::performance::Timer::new("analysis");
        #[cfg(target_os = "android")]
        let engine: &dyn EntityExtractor = match gliner.as_ref() { Some(engine) => engine, None => &cache.as_ref().ok_or("Modelo no disponible")?.1 };
        #[cfg(not(target_os = "android"))]
        let engine: &dyn EntityExtractor = &cache.as_ref().ok_or("Modelo no disponible")?.1;
        let mut original = session.original.clone();
        // OCR cleanup is part of the local pipeline; keep the extraction intact.
        // Gather suggestions from one snapshot so corrections cannot cascade.
        // On mobile the extracted source stays exact; optional generative OCR
        // cleanup otherwise doubles inference before entity detection even starts.
        if !cfg!(target_os = "android") && needs_ocr_cleanup(&session) {
            let mut seen = HashSet::new();
            let ocr_chunks = chunks(&session.original);
            let ocr_total = ocr_chunks.len();
            for (index, chunk) in ocr_chunks.into_iter().enumerate() {
                let _ = app.emit(
                    "private-analysis-progress",
                    (
                        id.as_str(),
                        requested_revision,
                        format!("ocr:{}/{}", index + 1, ocr_total),
                    ),
                );
                // A fragment Qwen cannot finish must not discard the OCR fixes
                // it did produce for the rest of the document.
                crate::llm::check_cancelled()?;
                let Ok(suggestions) = cache.as_ref().ok_or("Modelo no disponible")?.1.suggest_ocr_prefixes(&chunk) else {
                    continue;
                };
                for suggestion in suggestions {
                    if valid_ocr_prefix(&suggestion, &original)
                        && seen.insert(suggestion.original.clone())
                    {
                        original = corrected_ocr_text(&original, &suggestion)?;
                    }
                }
            }
        }
        // Progress carries only counts, never document content.
        let mut report = |done: usize, total: usize, tokens: usize| {
            let _ = app.emit(
                "private-analysis-progress",
                (
                    id.as_str(),
                    requested_revision,
                    if crate::models::gliner_files(&model_id).is_some() { format!("ner:{done}/{total}") } else if tokens >= 1_000_000 { format!("prefill:{done}/{total}:{}", tokens - 1_000_000) } else { format!("reading:{done}/{total}:{tokens}") },
                ),
            );
        };
        let mut analysis = analyze_with_progress(engine, &original, &mut report)?;
        crate::llm::check_cancelled()?;
        // A specialized on-device name detector complements the generative
        // model; names must not depend exclusively on its response budget.
        #[cfg(target_os = "macos")]
        let local_names = crate::layout::executable(&app)
            .and_then(|helper| crate::layout::named_entities(&original, &helper));
        #[cfg(not(target_os = "macos"))]
        let local_names: Result<Vec<Detection>, String> = Ok(vec![]);
        let names_unavailable = local_names.is_err();
        analysis.entities = merge_detection_sources(&original,
            analysis.entities.iter().map(|e| Detection { kind: e.kind, text: e.original.clone() }).collect(),
            local_names.unwrap_or_default())?;
        #[cfg(any(target_os = "macos", target_os = "android"))]
        if let Some(document) = session.layout.as_ref() {
            crate::layout::fit_generated_aliases(document, &mut analysis.entities, &crate::layout::executable(&app)?)?;
        }
        let complete = analysis.complete();
        // Progress carries only operation identity; private document data stays local.
        let _ = app.emit(
            "private-analysis-progress",
            (id.as_str(), requested_revision, "sanitizing"),
        );
        let sanitized = sanitize_for_file(&original, &analysis.entities, &session.file_name)?;
        // Deliberately no `require_sanitized` here. Finding nothing is a valid
        // analysis result that the lawyer must see; it becomes a refusal at
        // review and export time, where it can no longer be mistaken for a
        // clean document. Failing here would hide the empty map entirely.
        let mut warnings = leak_warnings(&sanitized, &analysis.entities);
        warnings.extend(analysis.coverage_warning());
        let mut advisories = privacy_advisories(&analysis.entities);
        if model_id == crate::models::GLINER_ID { advisories.push("GLiNER INT8 es experimental; su ficha declara inglés. Revisa especialmente nombres, organizaciones e identificadores en español.".into()); }
        if names_unavailable { advisories.push("El detector complementario de nombres no estuvo disponible. Revisa nombres y organizaciones en todo el documento.".into()); }
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
        current.entities = analysis.entities;
        current.sanitized = sanitized;
        current.edited = false;
        current.warnings = warnings;
        current.advisories = advisories;
        current.analyzed = true;
        current.analysis_complete = complete;
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
    let map_changed = session.entities.len() != entities.len()
        || session
            .entities
            .iter()
            .zip(entities.iter())
            .any(|(a, b)| a.kind != b.kind || a.original != b.original || a.alias != b.alias);
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
    if !session.analysis_complete {
        // Recomputing `warnings` above drops the coverage notice, so the
        // refusal has to come from the flag itself or a second click on
        // "apply and confirm" would approve a map that never saw every fragment.
        session.reviewed = false;
        session.revision += 1;
        return Err(INCOMPLETE_ANALYSIS.into());
    }
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
    let mut guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    let session = guard
        .as_mut()
        .filter(|s| s.id == id && s.revision == revision)
        .ok_or("La edición está desactualizada. Recarga el documento.")?;
    if !session.analyzed {
        return Err("Analiza el documento antes de editarlo.".into());
    }
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
    if !session.analysis_complete {
        return Err(INCOMPLETE_ANALYSIS.into());
    }
    let sanitized = sanitize_for_file(&session.original, &session.entities, &session.file_name)?;
    if (!session.edited && sanitized != session.sanitized)
        || !leak_warnings(&session.sanitized, &session.entities).is_empty()
    {
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
    private: Option<&crate::pdf::PrivateExport>,
    text: &str,
    html: Option<&str>,
    title: &str,
    labels: &[(&str, &str)],
) -> Result<(), String> {
    let privacy = app.state::<PrivacyState>();
    let guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    check_export_authorization(guard.as_ref(), private, text, html, title, labels)
}

/// The decision the writer makes, independent of Tauri state so it can be tested
/// directly like `check_export_payload` below.
fn check_export_authorization(
    session: Option<&Session>,
    private: Option<&crate::pdf::PrivateExport>,
    text: &str,
    html: Option<&str>,
    title: &str,
    labels: &[(&str, &str)],
) -> Result<(), String> {
    let Some(session) = session else {
        // No private document is loaded, so this is an ordinary transcript
        // export and there is nothing private in it to gate.
        return Ok(());
    };
    let Some(private) = private else {
        // Hay una sesión privada cargada en memoria (aunque no se envíe el payload
        // de exportación). Esto bloquea cualquier exportación desde Edit/Transcript
        // mientras Private Legal está activo, para evitar mezclar el contenido.
        return Err("Hay un documento privado cargado: solo se puede exportar su versión revisada. Para exportar una transcripción o audio, vuelve a la vista principal y cierra Private Legal antes de exportar.".into());
    };
    if session.id != private.id || session.revision != private.revision || !session.analyzed {
        return Err("La sesión cambió. Recarga el documento privado antes de exportar.".into());
    }
    check_export_payload(session, text, html, title)?;
    check_export_labels(session, labels)
}

/// The writer prints the duration, language and model line into the page body.
/// Those strings come from the renderer as well, so they clear the same gate:
/// an identifier tucked into `model` would otherwise leave the process exactly
/// like one tucked into the text.
fn check_export_labels(session: &Session, labels: &[(&str, &str)]) -> Result<(), String> {
    for (name, value) in labels {
        if value.trim().is_empty() {
            continue;
        }
        if !leak_warnings(value, &session.entities).is_empty() {
            return Err(format!(
                "PDF bloqueado: {name} contiene posibles datos privados."
            ));
        }
    }
    Ok(())
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
    if !session.analysis_complete {
        return Err(INCOMPLETE_ANALYSIS.into());
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
    // This one is always about a specific session, so a missing or stale
    // session is an error here even though the writer treats it as "no private
    // document loaded".
    let privacy = app.state::<PrivacyState>();
    let guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    let session = guard
        .as_ref()
        .filter(|s| s.id == id && s.revision == revision && s.analyzed)
        .ok_or("La revisión está desactualizada.")?;
    check_export_payload(session, &text, None, title.as_deref().unwrap_or_default())?;
    check_export_labels(
        session,
        &[("el título", title.as_deref().unwrap_or_default())],
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod benchmark;

/// Human correction of extracted text. Original extraction stays in memory for
/// comparison; every correction discards analysis and its export approval.
#[tauri::command]
pub(crate) fn update_private_source(
    id: String,
    revision: u64,
    text: String,
    app: tauri::AppHandle,
) -> Result<Session, String> {
    let llm = app.state::<crate::llm::LlmState>();
    let _inference = llm
        .0
        .try_lock()
        .map_err(|_| "Cancela el análisis antes de corregir la extracción.")?;
    let privacy = app.state::<PrivacyState>();
    let mut guard = privacy
        .0
        .lock()
        .map_err(|_| "Sesión privada no disponible")?;
    let current = guard
        .as_ref()
        .filter(|s| s.id == id && s.revision == revision)
        .ok_or("El documento ha cambiado. Recarga la sesión.")?;
    if current.layout_available {
        return Err("Corrige este documento en el editor de formato original.".into());
    }
    let mut next = Session::new(current.file_name.clone(), text)?;
    next.id = current.id.clone();
    next.revision = current.revision + 1;
    next.extracted_original = current.extracted_original.clone();
    *guard = Some(next.clone());
    Ok(next)
}
