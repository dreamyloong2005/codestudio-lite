use super::super::{
    append_system, content_parts_from_value, content_text, normalize_message_role, numeric_field,
    openai_legacy_function_specs, openai_tool_calls_from_value, openai_tool_specs_from_value,
    push_message_if_useful, text_from_value,
};
use super::canonical::{GatewayContentPart, GatewayMessage, GatewayRequestParts};
use serde_json::Value;

pub(in crate::core::gateway) fn decode_request(
    request_body: &Value,
    model: &str,
) -> GatewayRequestParts {
    let mut system = None;
    let mut messages = Vec::new();
    for item in request_body
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
        if role == "system" {
            let content = text_from_value(item.get("content").unwrap_or(&Value::Null));
            if !content.is_empty() {
                append_system(&mut system, content)
            }
            continue;
        }
        let mut message = GatewayMessage {
            role: normalize_message_role(role),
            content: content_parts_from_value(item.get("content").unwrap_or(&Value::Null)),
            tool_call_id: item
                .get("tool_call_id")
                .or_else(|| item.get("call_id"))
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            tool_calls: openai_tool_calls_from_value(
                item.get("tool_calls").unwrap_or(&Value::Null),
            ),
        };
        if message.role == "tool" {
            let content = content_text(&message.content);
            message.content = vec![GatewayContentPart::ToolResult {
                tool_call_id: message.tool_call_id.clone(),
                content,
            }];
        }
        push_message_if_useful(&mut messages, message);
    }
    GatewayRequestParts {
        model: model.to_string(),
        system,
        messages,
        tools: openai_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null))
            .into_iter()
            .chain(openai_legacy_function_specs(
                request_body.get("functions").unwrap_or(&Value::Null),
            ))
            .collect(),
        tool_choice: request_body
            .get("tool_choice")
            .or_else(|| request_body.get("function_call"))
            .cloned(),
        max_tokens: numeric_field(request_body, &["max_completion_tokens", "max_tokens"]),
        temperature: request_body.get("temperature").cloned(),
        top_p: request_body.get("top_p").cloned(),
    }
}
