use super::super::{
    gemini_parts_and_tool_calls, gemini_tool_specs_from_value, numeric_field,
    push_message_if_useful, text_from_value,
};
use super::canonical::{GatewayMessage, GatewayRequestParts};
use serde_json::Value;

pub(in crate::core::gateway) fn decode_request(
    request_body: &Value,
    model: &str,
) -> GatewayRequestParts {
    let mut messages = Vec::new();
    for item in request_body
        .get("contents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
        let (content, tool_calls) =
            gemini_parts_and_tool_calls(item.get("parts").unwrap_or(&Value::Null));
        push_message_if_useful(
            &mut messages,
            GatewayMessage {
                role: if role == "model" {
                    "assistant".to_string()
                } else {
                    "user".to_string()
                },
                content,
                tool_call_id: None,
                tool_calls,
            },
        );
    }
    let generation = request_body.get("generationConfig").unwrap_or(&Value::Null);
    GatewayRequestParts {
        model: model.to_string(),
        system: request_body
            .get("systemInstruction")
            .map(text_from_value)
            .filter(|value| !value.is_empty()),
        messages,
        tools: gemini_tool_specs_from_value(request_body.get("tools").unwrap_or(&Value::Null)),
        tool_choice: request_body.get("toolConfig").cloned(),
        max_tokens: numeric_field(generation, &["maxOutputTokens"]),
        temperature: generation.get("temperature").cloned(),
        top_p: generation.get("topP").cloned(),
    }
}
