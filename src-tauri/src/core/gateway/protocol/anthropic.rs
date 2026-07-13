use super::super::{
    anthropic_content_and_tool_calls, anthropic_tool_specs_from_value, normalize_message_role,
    numeric_field, push_message_if_useful, text_from_value,
};
use super::canonical::{GatewayMessage, GatewayRequestParts};
use serde_json::Value;

pub(in crate::core::gateway) fn decode_request(
    request_body: &Value,
    model: &str,
) -> GatewayRequestParts {
    let mut messages = Vec::new();
    for item in request_body
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
        let (content, tool_calls) =
            anthropic_content_and_tool_calls(item.get("content").unwrap_or(&Value::Null));
        push_message_if_useful(
            &mut messages,
            GatewayMessage {
                role: normalize_message_role(role),
                content,
                tool_call_id: None,
                tool_calls,
            },
        );
    }
    GatewayRequestParts {
        model: model.to_string(),
        system: request_body
            .get("system")
            .map(text_from_value)
            .filter(|value| !value.is_empty()),
        messages,
        tools: anthropic_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null)),
        tool_choice: request_body.get("tool_choice").cloned(),
        max_tokens: numeric_field(request_body, &["max_tokens"]),
        temperature: request_body.get("temperature").cloned(),
        top_p: request_body.get("top_p").cloned(),
    }
}
