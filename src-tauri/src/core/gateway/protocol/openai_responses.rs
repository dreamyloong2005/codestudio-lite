use super::super::{
    append_system, content_parts_from_value, normalize_message_role, numeric_field,
    push_message_if_useful, responses_function_call_from_value, responses_tool_specs_from_value,
    text_from_value,
};
use super::canonical::{GatewayContentPart, GatewayMessage, GatewayRequestParts};
use serde_json::Value;

pub(in crate::core::gateway) fn decode_request(
    request_body: &Value,
    model: &str,
) -> GatewayRequestParts {
    let mut system = request_body
        .get("instructions")
        .map(text_from_value)
        .filter(|value| !value.is_empty());
    let mut messages = Vec::new();
    match request_body.get("input") {
        Some(Value::String(input)) if !input.trim().is_empty() => messages.push(GatewayMessage {
            role: "user".to_string(),
            content: vec![GatewayContentPart::Text(input.trim().to_string())],
            tool_call_id: None,
            tool_calls: Vec::new(),
        }),
        Some(Value::Array(items)) => {
            for item in items {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
                if item_type == "function_call" {
                    let tool_call = responses_function_call_from_value(item);
                    push_message_if_useful(
                        &mut messages,
                        GatewayMessage {
                            role: "assistant".to_string(),
                            content: Vec::new(),
                            tool_call_id: None,
                            tool_calls: tool_call.into_iter().collect(),
                        },
                    );
                    continue;
                }
                if item_type == "function_call_output" {
                    let call_id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let content = text_from_value(
                        item.get("output")
                            .or_else(|| item.get("content"))
                            .unwrap_or(&Value::Null),
                    );
                    push_message_if_useful(
                        &mut messages,
                        GatewayMessage {
                            role: "tool".to_string(),
                            content: vec![GatewayContentPart::ToolResult {
                                tool_call_id: call_id.clone(),
                                content,
                            }],
                            tool_call_id: call_id,
                            tool_calls: Vec::new(),
                        },
                    );
                    continue;
                }
                let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                if role == "system" {
                    append_system(
                        &mut system,
                        text_from_value(
                            item.get("content")
                                .or_else(|| item.get("text"))
                                .unwrap_or(&Value::Null),
                        ),
                    );
                } else {
                    push_message_if_useful(
                        &mut messages,
                        GatewayMessage {
                            role: normalize_message_role(role),
                            content: content_parts_from_value(
                                item.get("content")
                                    .or_else(|| item.get("text"))
                                    .unwrap_or(&Value::Null),
                            ),
                            tool_call_id: None,
                            tool_calls: Vec::new(),
                        },
                    );
                }
            }
        }
        Some(value) => {
            let content = text_from_value(value);
            if !content.is_empty() {
                messages.push(GatewayMessage {
                    role: "user".to_string(),
                    content: vec![GatewayContentPart::Text(content)],
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                });
            }
        }
        None => {}
    }
    GatewayRequestParts {
        model: model.to_string(),
        system,
        messages,
        tools: responses_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null)),
        tool_choice: request_body.get("tool_choice").cloned(),
        max_tokens: numeric_field(request_body, &["max_output_tokens", "max_tokens"]),
        temperature: request_body.get("temperature").cloned(),
        top_p: request_body.get("top_p").cloned(),
    }
}
