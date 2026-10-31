//! Conservative additions: checksummed unlabelled values and explicit fields.
use super::{Detection, Kind};
use regex::Regex;
use std::sync::OnceLock;

pub(super) fn rut_valid(value: &str) -> bool {
    let compact: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if !(8..=9).contains(&compact.len()) {
        return false;
    }
    let (body, check) = compact.split_at(compact.len() - 1);
    if !body.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = body
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * (2 + (i as u32 % 6)))
        .sum();
    let expected = match 11 - sum % 11 {
        11 => '0',
        10 => 'K',
        n => char::from(b'0' + n as u8),
    };
    check.eq_ignore_ascii_case(&expected.to_string())
}
pub(super) fn luhn_valid(value: &str) -> bool {
    let digits: Vec<_> = value
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    if !(13..=19).contains(&digits.len()) || digits.iter().all(|d| *d == digits[0]) {
        return false;
    }
    digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            let d = if i % 2 == 1 { d * 2 } else { d };
            u32::from(if d > 9 { d - 9 } else { d })
        })
        .sum::<u32>()
        % 10
        == 0
}
pub(super) fn iban_valid(value: &str) -> bool {
    let s: String = value.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if !s.is_ascii() || s.len() < 15 || s.len() > 34 {
        return false;
    }
    let b = s.as_bytes();
    if !b[..2].iter().all(u8::is_ascii_uppercase) || !b[2..4].iter().all(u8::is_ascii_digit) {
        return false;
    }
    // Country-specific lengths prevent checksum-valid truncated prefixes.
    let length = match &s[..2] {
        "ES" => 24,
        "GB" => 22,
        "DE" => 22,
        "FR" => 27,
        "CH" => 21,
        "NL" => 18,
        "BE" => 16,
        "IT" => 27,
        "PT" => 25,
        "BR" => 29,
        "CR" => 22,
        "GT" => 28,
        "DO" => 28,
        "SV" => 28,
        _ => return false,
    };
    if s.len() != length {
        return false;
    }
    let mut remainder = 0u32;
    for c in b[4..].iter().chain(&b[..4]) {
        if c.is_ascii_digit() {
            remainder = (remainder * 10 + u32::from(c - b'0')) % 97;
        } else if c.is_ascii_uppercase() {
            remainder = (remainder * 100 + u32::from(c - b'A' + 10)) % 97;
        } else {
            return false;
        }
    }
    remainder == 1
}
pub(super) fn detections(text: &str) -> Vec<Detection> {
    static RULES: OnceLock<Vec<(Kind, Regex, u8)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| vec![
        (Kind::Id, Regex::new(r"\b(?:[0-9]{1,2}\.[0-9]{3}\.[0-9]{3}|[0-9]{7,8})-[0-9kK]\b").unwrap(), 1),
        (Kind::Other, Regex::new(r"\b[0-9](?:[ \t\-]*[0-9]){12,18}\b").unwrap(), 2),
        (Kind::Other, Regex::new(r"\b[A-Z]{2}[0-9]{2}(?:[ \t]?[A-Z0-9]){11,30}\b").unwrap(), 3),
        (Kind::Other, Regex::new(r"(?i)(?:fecha[ \t]+de[ \t]+nacimiento|naci[oó][ \t]+el|birth[ \t]*date|date[ \t]+of[ \t]+birth|dob)[ \t]*[:=]?[ \t`*]*([0-9]{1,4}[-/.][0-9]{1,2}[-/.][0-9]{2,4})\b").unwrap(), 0),
        (Kind::Phone, Regex::new(r"(?:\+[0-9]{1,3}[ \t]*)?\([0-9]{2,4}\)[ \t]*[0-9]{3,4}[- ][0-9]{4}\b").unwrap(), 0),
        (Kind::Phone, Regex::new(r"(?:\+56[ \t-]*9[ \t-]*[0-9]{4}[ \t-]*[0-9]{4}|\+593[ \t-]*9[0-9][ \t-]*[0-9]{3}[ \t-]*[0-9]{4}|\b09[ \t-]+[0-9]{4}[ \t-]+[0-9]{4})\b").unwrap(), 0),
        // Capitalized name runs only after explicit conversational person cues.
        (Kind::Person, Regex::new(r"(?i:habl[eé] con|contacta a|llama a|call|spoke to)[ \t]+([\p{Lu}][\p{L}\p{M}'’-]+(?:[ \t]+[\p{Lu}][\p{L}\p{M}'’-]+){1,5})").unwrap(), 0),
        (Kind::Person, Regex::new(r"(?m)^([\p{Lu}][\p{L}\p{M}'’-]+(?:[ \t]+[\p{Lu}][\p{L}\p{M}'’-]+){1,5})[ \t]+(?:me (?:pidi[oó]|dijo|llam[oó])|asked me|called me)\b").unwrap(), 0),
        (Kind::Person, Regex::new(r"(?i)(?:nombre|full name)[ \t]*:[ \t`*]*([\p{L}\p{M}]+(?:[ \t]+[\p{L}\p{M}]+){1,5})").unwrap(), 0),
    ]);
    let mut result = vec![];
    for (kind, re, validator) in rules {
        for caps in re.captures_iter(text) {
            let m = caps.get(1).unwrap_or_else(|| caps.get(0).unwrap());
            let value = m.as_str();
            if !super::boundary(text, m.start(), m.end()) {
                continue;
            }
            let valid = match validator {
                1 => rut_valid(value),
                2 => luhn_valid(value),
                3 => iban_valid(value),
                _ => true,
            };
            if valid {
                result.push(Detection {
                    kind: *kind,
                    text: value.into(),
                });
            }
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksums_reject_near_misses() {
        assert!(rut_valid("12.345.678-5"));
        assert!(!rut_valid("12.345.678-4"));
        assert!(luhn_valid("4111 1111 1111 1111"));
        assert!(!luhn_valid("4111 1111 1111 1112"));
        assert!(iban_valid("ES91 2100 0418 4502 0005 1332"));
        assert!(!iban_valid("ES90 2100 0418 4502 0005 1332"));
        assert!(!iban_valid("ES91 2100 0418 4502"));
    }
}
