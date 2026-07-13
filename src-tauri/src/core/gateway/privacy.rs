use super::protocol::canonical::GatewayProtocol;
use crate::core::privacy_filter::{
    self, PrivacyFilterAction, PrivacyFilterMode, PrivacyFilterReport,
};
use serde_json::{json, Value};

pub(super) fn apply(
    protocol: GatewayProtocol,
    request_body: &mut Value,
    mode: PrivacyFilterMode,
) -> Result<PrivacyFilterReport, usize> {
    if matches!(mode, PrivacyFilterMode::Block) {
        let latest_report = latest_report(protocol, request_body);
        if latest_report.hit_count > 0 {
            return Err(latest_report.hit_count);
        }
        return Ok(privacy_filter::filter_json_value(
            request_body,
            PrivacyFilterMode::Redact,
        ));
    }
    Ok(privacy_filter::filter_json_value(request_body, mode))
}

pub(super) fn filter_metadata(
    protocol: GatewayProtocol,
    request_body: &Value,
    mode: PrivacyFilterMode,
) -> (usize, PrivacyFilterAction) {
    if matches!(mode, PrivacyFilterMode::Off) {
        return (0, PrivacyFilterAction::None);
    }
    if matches!(mode, PrivacyFilterMode::Block) {
        let latest_report = latest_report(protocol, request_body);
        if latest_report.hit_count > 0 {
            return (latest_report.hit_count, PrivacyFilterAction::Blocked);
        }
        let mut redacted = request_body.clone();
        let report = privacy_filter::filter_json_value(&mut redacted, PrivacyFilterMode::Redact);
        return if report.hit_count > 0 {
            (report.hit_count, PrivacyFilterAction::Redacted)
        } else {
            (0, PrivacyFilterAction::None)
        };
    }
    let mut value = request_body.clone();
    let report = privacy_filter::filter_json_value(&mut value, mode);
    (report.hit_count, report.action_for_mode(mode))
}

fn latest_report(protocol: GatewayProtocol, request_body: &Value) -> PrivacyFilterReport {
    let mut scope = latest_scope(protocol, request_body).unwrap_or_else(|| request_body.clone());
    privacy_filter::filter_json_value(&mut scope, PrivacyFilterMode::Detect)
}

fn latest_scope(protocol: GatewayProtocol, request_body: &Value) -> Option<Value> {
    match protocol {
        GatewayProtocol::OpenAiResponses => latest_responses_scope(request_body),
        GatewayProtocol::OpenAiChatCompletions | GatewayProtocol::AnthropicMessages => {
            latest_array_item(request_body, "messages")
        }
        GatewayProtocol::GoogleGemini => latest_array_item(request_body, "contents"),
    }
}

fn latest_array_item(request_body: &Value, key: &str) -> Option<Value> {
    request_body
        .get(key)?
        .as_array()?
        .iter()
        .rev()
        .find(|item| has_filterable_text(item))
        .cloned()
        .map(|item| json!({ key: [item] }))
}

fn latest_responses_scope(request_body: &Value) -> Option<Value> {
    match request_body.get("input") {
        Some(Value::String(input)) => Some(json!({ "input": input })),
        Some(Value::Array(items)) => items
            .iter()
            .rev()
            .find(|item| has_filterable_text(item))
            .cloned()
            .map(|item| json!({ "input": [item] })),
        Some(value) => Some(json!({ "input": value })),
        None => None,
    }
}

fn has_filterable_text(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => items.iter().any(has_filterable_text),
        Value::Object(map) => map.iter().any(|(key, child)| {
            !matches!(
                key.to_ascii_lowercase().as_str(),
                "role" | "type" | "id" | "name"
            ) && has_filterable_text(child)
        }),
        _ => false,
    }
}
