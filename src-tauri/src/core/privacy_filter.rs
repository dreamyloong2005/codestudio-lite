use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyFilterMode {
    Off,
    Detect,
    Redact,
    Block,
}

impl Default for PrivacyFilterMode {
    fn default() -> Self {
        Self::Off
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyFilterAction {
    None,
    Detected,
    Redacted,
    Blocked,
}

impl Default for PrivacyFilterAction {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyFilterReport {
    pub hit_count: usize,
}

impl PrivacyFilterReport {
    pub fn action_for_mode(&self, mode: PrivacyFilterMode) -> PrivacyFilterAction {
        if self.hit_count == 0 {
            return PrivacyFilterAction::None;
        }
        match mode {
            PrivacyFilterMode::Off => PrivacyFilterAction::None,
            PrivacyFilterMode::Detect => PrivacyFilterAction::Detected,
            PrivacyFilterMode::Redact => PrivacyFilterAction::Redacted,
            PrivacyFilterMode::Block => PrivacyFilterAction::Blocked,
        }
    }
}

pub fn filter_json_value(value: &mut Value, mode: PrivacyFilterMode) -> PrivacyFilterReport {
    if matches!(mode, PrivacyFilterMode::Off) {
        return PrivacyFilterReport { hit_count: 0 };
    }

    let mut hit_count = 0;
    filter_value_at_key(value, None, mode, &mut hit_count);
    PrivacyFilterReport { hit_count }
}

fn filter_value_at_key(
    value: &mut Value,
    key: Option<&str>,
    mode: PrivacyFilterMode,
    hit_count: &mut usize,
) {
    match value {
        Value::String(text) => {
            if should_filter_string_key(key) {
                let filtered = filter_text(text, mode);
                *hit_count += filtered.hit_count;
                if matches!(mode, PrivacyFilterMode::Redact) && filtered.hit_count > 0 {
                    *text = filtered.text;
                }
            }
        }
        Value::Array(items) => {
            let parent_is_filterable = key.is_some_and(should_descend_filterable_key);
            for item in items {
                filter_value_at_key(
                    item,
                    if parent_is_filterable {
                        Some("text")
                    } else {
                        key
                    },
                    mode,
                    hit_count,
                );
            }
        }
        Value::Object(map) => {
            for (child_key, child_value) in map {
                if should_skip_key(child_key) {
                    continue;
                }
                filter_value_at_key(child_value, Some(child_key), mode, hit_count);
            }
        }
        _ => {}
    }
}

fn should_filter_string_key(key: Option<&str>) -> bool {
    let Some(key) = key.map(|value| value.to_ascii_lowercase()) else {
        return false;
    };

    matches!(
        key.as_str(),
        "content"
            | "text"
            | "input"
            | "output"
            | "instructions"
            | "system"
            | "prompt"
            | "query"
            | "message"
    )
}

fn should_descend_filterable_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "messages" | "content" | "input" | "output" | "parts" | "systeminstruction"
    )
}

fn should_skip_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "model"
            | "id"
            | "name"
            | "type"
            | "role"
            | "url"
            | "base_url"
            | "baseurl"
            | "api_key"
            | "apikey"
            | "authorization"
            | "headers"
            | "tools"
            | "tool_choice"
            | "response_format"
            | "json_schema"
            | "schema"
            | "function"
            | "functions"
    )
}

#[derive(Debug, Clone)]
struct FilteredText {
    text: String,
    hit_count: usize,
}

#[derive(Debug, Clone)]
struct Span {
    start: usize,
    end: usize,
    placeholder: &'static str,
}

fn filter_text(text: &str, mode: PrivacyFilterMode) -> FilteredText {
    let spans = merged_spans(detect_spans(text));
    if spans.is_empty() {
        return FilteredText {
            text: text.to_string(),
            hit_count: 0,
        };
    }

    if !matches!(mode, PrivacyFilterMode::Redact) {
        return FilteredText {
            text: text.to_string(),
            hit_count: spans.len(),
        };
    }

    let mut redacted = String::with_capacity(text.len());
    let mut cursor = 0;
    for span in &spans {
        redacted.push_str(&text[cursor..span.start]);
        redacted.push_str(span.placeholder);
        cursor = span.end;
    }
    redacted.push_str(&text[cursor..]);

    FilteredText {
        text: redacted,
        hit_count: spans.len(),
    }
}

fn detect_spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    collect_email_spans(text, &mut spans);
    collect_simple_spans(text, phone_regex(), "[电话]", &mut spans);
    collect_id_card_spans(text, &mut spans);
    collect_bank_card_spans(text, &mut spans);
    collect_simple_spans(text, ipv4_regex(), "[IP]", &mut spans);
    collect_secret_spans(text, &mut spans);
    spans
}

fn collect_simple_spans(
    text: &str,
    regex: &'static Regex,
    placeholder: &'static str,
    spans: &mut Vec<Span>,
) {
    for item in regex.find_iter(text) {
        spans.push(Span {
            start: item.start(),
            end: item.end(),
            placeholder,
        });
    }
}

fn collect_email_spans(text: &str, spans: &mut Vec<Span>) {
    for item in email_regex().find_iter(text) {
        if is_ssh_user_host_context(text, item.start(), item.end()) {
            continue;
        }
        spans.push(Span {
            start: item.start(),
            end: item.end(),
            placeholder: "[邮箱]",
        });
    }
}

fn is_ssh_user_host_context(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].to_ascii_lowercase();
    let prefix = before
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .trim_matches(|ch: char| ch == '\'' || ch == '"' || ch == '`');
    if matches!(prefix, "ssh" | "scp" | "sftp" | "rsync") {
        return true;
    }

    let after = text[end..].chars().next();
    if after == Some(':') {
        return true;
    }

    let url_prefix = before
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .trim_matches(|ch: char| ch == '\'' || ch == '"' || ch == '`');
    url_prefix.ends_with("git@")
}

fn collect_id_card_spans(text: &str, spans: &mut Vec<Span>) {
    for item in cn_id_regex().find_iter(text) {
        if valid_cn_id_checksum(item.as_str()) {
            spans.push(Span {
                start: item.start(),
                end: item.end(),
                placeholder: "[身份证]",
            });
        }
    }
}

fn valid_cn_id_checksum(value: &str) -> bool {
    if value.len() != 18 {
        return false;
    }
    let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let checks = ['1', '0', 'X', '9', '8', '7', '6', '5', '4', '3', '2'];
    let mut sum = 0_u32;
    for (index, ch) in value.chars().take(17).enumerate() {
        let Some(digit) = ch.to_digit(10) else {
            return false;
        };
        sum += digit * weights[index];
    }
    let expected = checks[(sum % 11) as usize];
    value
        .chars()
        .nth(17)
        .map(|ch| ch.to_ascii_uppercase() == expected)
        .unwrap_or(false)
}

fn collect_bank_card_spans(text: &str, spans: &mut Vec<Span>) {
    for item in bank_card_regex().find_iter(text) {
        let raw = item.as_str();
        let digits: String = raw.chars().filter(|ch| ch.is_ascii_digit()).collect();
        if (13..=19).contains(&digits.len()) && valid_luhn(&digits) {
            spans.push(Span {
                start: item.start(),
                end: item.end(),
                placeholder: "[银行卡]",
            });
        }
    }
}

fn valid_luhn(digits: &str) -> bool {
    let mut sum = 0_u32;
    let mut double = false;
    for ch in digits.chars().rev() {
        let Some(mut digit) = ch.to_digit(10) else {
            return false;
        };
        if double {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        sum += digit;
        double = !double;
    }
    sum > 0 && sum % 10 == 0
}

fn collect_secret_spans(text: &str, spans: &mut Vec<Span>) {
    for regex in [
        openai_secret_regex(),
        assignment_secret_regex(),
        bearer_secret_regex(),
    ] {
        for item in regex.find_iter(text) {
            if is_template_placeholder(item.as_str()) {
                continue;
            }
            spans.push(Span {
                start: item.start(),
                end: item.end(),
                placeholder: "[密钥]",
            });
        }
    }

    for item in high_entropy_regex().find_iter(text) {
        let candidate = item.as_str();
        if should_skip_entropy_candidate(text, item.start(), item.end(), candidate) {
            continue;
        }
        spans.push(Span {
            start: item.start(),
            end: item.end(),
            placeholder: "[密钥]",
        });
    }
}

fn is_template_placeholder(value: &str) -> bool {
    let trimmed = value.trim();
    (trimmed.starts_with("{{") && trimmed.ends_with("}}"))
        || (trimmed.starts_with("${") && trimmed.ends_with('}'))
}

fn should_skip_entropy_candidate(text: &str, start: usize, end: usize, candidate: &str) -> bool {
    if candidate.len() < 24 {
        return true;
    }
    if uuid_regex().is_match(candidate) || hex_hash_regex().is_match(candidate) {
        return true;
    }
    if candidate.contains('/') || candidate.contains('\\') || candidate.starts_with("http") {
        return true;
    }
    if text[..start]
        .chars()
        .last()
        .is_some_and(is_pathish_boundary)
        || text[end..].chars().next().is_some_and(is_pathish_boundary)
    {
        return true;
    }
    entropy(candidate) < 4.0
}

fn is_pathish_boundary(ch: char) -> bool {
    matches!(ch, '/' | '\\' | '.' | '-' | '_' | ':')
}

fn entropy(value: &str) -> f64 {
    let mut counts = [0_usize; 256];
    let mut total = 0_usize;
    for byte in value.bytes() {
        counts[byte as usize] += 1;
        total += 1;
    }
    if total == 0 {
        return 0.0;
    }
    counts
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let p = *count as f64 / total as f64;
            -p * p.log2()
        })
        .sum()
}

fn merged_spans(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by_key(|span| (span.start, std::cmp::Reverse(span.end)));
    let mut merged: Vec<Span> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start < last.end {
                if span.end > last.end {
                    last.end = span.end;
                    last.placeholder = span.placeholder;
                }
                continue;
            }
        }
        merged.push(span);
    }
    merged
}

fn email_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}\b").expect("email regex")
    })
}

fn phone_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b1[3-9]\d{9}\b").expect("phone regex"))
}

fn cn_id_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b\d{17}[\dXx]\b").expect("cn id regex"))
}

fn bank_card_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(?:\d[ -]?){13,19}\b").expect("bank card regex"))
}

fn ipv4_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\b",
        )
        .expect("ipv4 regex")
    })
}

fn openai_secret_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\bsk-[A-Za-z0-9_\-]{16,}\b").expect("openai secret regex"))
}

fn assignment_secret_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(?:api[_-]?key|token|secret|password|passwd|access[_-]?token)\b\s*[:=]\s*["']?[A-Za-z0-9_\-./+=]{12,}["']?"#,
        )
        .expect("assignment secret regex")
    })
}

fn bearer_secret_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9_\-./+=]{16,}\b").expect("bearer secret regex")
    })
}

fn high_entropy_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b[A-Za-z0-9_\-+=]{28,}\b").expect("entropy regex"))
}

fn uuid_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .expect("uuid regex")
    })
}

fn hex_hash_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:[0-9a-f]{32}|[0-9a-f]{40}|[0-9a-f]{64})$").expect("hex hash regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_email_phone_and_secret() {
        let mut value = json!({
            "model": "gpt-5.5",
            "messages": [{
                "role": "user",
                "content": "Email alice@example.com phone 13800138000 key sk-test1234567890abcdef"
            }]
        });

        let report = filter_json_value(&mut value, PrivacyFilterMode::Redact);
        let content = value["messages"][0]["content"].as_str().unwrap();

        assert_eq!(report.hit_count, 3);
        assert_eq!(value["model"].as_str(), Some("gpt-5.5"));
        assert!(content.contains("[邮箱]"));
        assert!(content.contains("[电话]"));
        assert!(content.contains("[密钥]"));
        assert!(!content.contains("alice@example.com"));
        assert!(!content.contains("13800138000"));
        assert!(!content.contains("sk-test"));
    }

    #[test]
    fn skips_ssh_user_host_email_like_text() {
        let mut value = json!({
            "messages": [{
                "role": "user",
                "content": "Run ssh deploy@example.com and git clone git@example.com:org/repo.git"
            }]
        });

        let report = filter_json_value(&mut value, PrivacyFilterMode::Redact);
        let content = value["messages"][0]["content"].as_str().unwrap();

        assert_eq!(report.hit_count, 0);
        assert!(content.contains("deploy@example.com"));
        assert!(content.contains("git@example.com:org/repo.git"));
    }

    #[test]
    fn luhn_filters_bank_cards() {
        let mut value = json!({
            "input": "Valid card 4111111111111111 but not 4111111111111112"
        });

        let report = filter_json_value(&mut value, PrivacyFilterMode::Redact);
        let input = value["input"].as_str().unwrap();

        assert_eq!(report.hit_count, 1);
        assert!(input.contains("[银行卡]"));
        assert!(input.contains("4111111111111112"));
        assert!(!input.contains("4111111111111111"));
    }

    #[test]
    fn detect_mode_counts_without_changing_content() {
        let mut value = json!({ "input": "Contact bob@example.com" });

        let report = filter_json_value(&mut value, PrivacyFilterMode::Detect);

        assert_eq!(report.hit_count, 1);
        assert_eq!(value["input"].as_str(), Some("Contact bob@example.com"));
    }
}
