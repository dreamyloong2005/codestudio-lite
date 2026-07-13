mod anthropic;
pub(super) mod canonical;
mod gemini;
mod openai_chat;
mod openai_responses;
mod stream;

pub(in crate::core::gateway) use openai_chat::decode_request as decode_openai_chat_request;
pub(in crate::core::gateway) use openai_responses::decode_request as decode_openai_responses_request;
pub(in crate::core::gateway) use stream::{
    decode_update as stream_update_from_event, merge_usage as merge_stream_usage,
    text_delta as stream_text_delta_from_event, write_delta as write_protocol_stream_delta,
    write_done as write_protocol_stream_done, write_start as write_protocol_stream_start,
    write_tool_call as write_protocol_stream_tool_call, ClientStreamState, SseBuffer, SseFrame,
};

use super::{
    anthropic_assistant_content_blocks, anthropic_content_and_tool_calls, anthropic_content_value,
    arguments_as_string, content_parts_from_value, content_text, gemini_assistant_parts,
    gemini_parts_and_tool_calls, gemini_parts_value, openai_chat_content_value,
    openai_chat_message_value, openai_tool_call_value, openai_tool_calls_from_value,
    responses_function_call_from_value, responses_input_items_for_message,
    responses_output_content_parts, set_optional_u64, set_optional_value, set_tools_for_protocol,
    unix_timestamp, usage_from_response, usage_value_for_protocol,
};
use canonical::{GatewayAssistantResponse, GatewayProtocol, GatewayRequestParts};
use serde_json::{json, Map, Value};
use uuid::Uuid;

pub(in crate::core::gateway) fn from_route_path(route_path: &str) -> Option<GatewayProtocol> {
    match route_path {
        "/v1/responses" => Some(GatewayProtocol::OpenAiResponses),
        "/v1/chat/completions" => Some(GatewayProtocol::OpenAiChatCompletions),
        "/v1/messages" => Some(GatewayProtocol::AnthropicMessages),
        path if gemini_route(path).is_some() => Some(GatewayProtocol::GoogleGemini),
        _ => None,
    }
}

pub(in crate::core::gateway) fn gemini_route(route_path: &str) -> Option<(String, bool)> {
    let rest = route_path
        .strip_prefix("/v1beta/models/")
        .or_else(|| route_path.strip_prefix("/v1/models/"))?;
    if let Some(model) = rest.strip_suffix(":generateContent") {
        return (!model.trim().is_empty()).then(|| (model.to_string(), false));
    }
    let model = rest.strip_suffix(":streamGenerateContent")?;
    (!model.trim().is_empty()).then(|| (model.to_string(), true))
}

pub(in crate::core::gateway) fn encode_request(
    protocol: GatewayProtocol,
    parts: &GatewayRequestParts,
    stream: bool,
) -> Value {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut messages = Vec::new();
            if let Some(system) = parts.system.as_deref() {
                messages.push(json!({ "role": "system", "content": system }))
            }
            messages.extend(parts.messages.iter().map(openai_chat_message_value));
            if messages.is_empty() {
                messages.push(json!({ "role": "user", "content": "" }))
            }
            let mut body = json!({ "model": parts.model, "messages": messages });
            if stream {
                body["stream"] = Value::Bool(true)
            }
            set_optional_u64(&mut body, "max_tokens", parts.max_tokens);
            set_optional_value(&mut body, "temperature", parts.temperature.clone());
            set_optional_value(&mut body, "top_p", parts.top_p.clone());
            set_tools_for_protocol(&mut body, protocol, parts);
            body
        }
        GatewayProtocol::OpenAiResponses => {
            let input: Vec<Value> = if parts.messages.is_empty() {
                vec![json!({ "role": "user", "content": [{ "type": "input_text", "text": "" }] })]
            } else {
                parts
                    .messages
                    .iter()
                    .flat_map(responses_input_items_for_message)
                    .collect()
            };
            let mut body = json!({ "model": parts.model, "input": input });
            if stream {
                body["stream"] = Value::Bool(true)
            }
            if let Some(system) = parts.system.as_deref() {
                body["instructions"] = Value::String(system.to_string())
            }
            set_optional_u64(&mut body, "max_output_tokens", parts.max_tokens);
            set_optional_value(&mut body, "temperature", parts.temperature.clone());
            set_optional_value(&mut body, "top_p", parts.top_p.clone());
            set_tools_for_protocol(&mut body, protocol, parts);
            body
        }
        GatewayProtocol::AnthropicMessages => {
            let messages: Vec<Value> =
                if parts.messages.is_empty() {
                    vec![json!({ "role": "user", "content": "" })]
                } else {
                    parts.messages.iter().map(|message| json!({
                "role": if message.role == "assistant" { "assistant" } else { "user" },
                "content": anthropic_content_value(message),
            })).collect()
                };
            let mut body = json!({ "model": parts.model, "messages": messages, "max_tokens": parts.max_tokens.unwrap_or(4096) });
            if stream {
                body["stream"] = Value::Bool(true)
            }
            if let Some(system) = parts.system.as_deref() {
                body["system"] = Value::String(system.to_string())
            }
            set_optional_value(&mut body, "temperature", parts.temperature.clone());
            set_optional_value(&mut body, "top_p", parts.top_p.clone());
            set_tools_for_protocol(&mut body, protocol, parts);
            body
        }
        GatewayProtocol::GoogleGemini => {
            let contents: Vec<Value> = if parts.messages.is_empty() {
                vec![json!({ "role": "user", "parts": [{ "text": "" }] })]
            } else {
                parts
                    .messages
                    .iter()
                    .map(|message| {
                        json!({
                            "role": if message.role == "assistant" { "model" } else { "user" },
                            "parts": gemini_parts_value(message),
                        })
                    })
                    .collect()
            };
            let mut generation = json!({});
            set_optional_u64(&mut generation, "maxOutputTokens", parts.max_tokens);
            set_optional_value(&mut generation, "temperature", parts.temperature.clone());
            set_optional_value(&mut generation, "topP", parts.top_p.clone());
            let mut body = json!({ "contents": contents });
            if generation
                .as_object()
                .is_some_and(|object| !object.is_empty())
            {
                body["generationConfig"] = generation
            }
            if let Some(system) = parts.system.as_deref() {
                body["systemInstruction"] = json!({ "parts": [{ "text": system }] })
            }
            set_tools_for_protocol(&mut body, protocol, parts);
            body
        }
    }
}
pub(in crate::core::gateway) use anthropic::decode_request as decode_anthropic_request;
pub(in crate::core::gateway) use gemini::decode_request as decode_gemini_request;

pub(in crate::core::gateway) fn decode_response(
    protocol: GatewayProtocol,
    value: &Value,
) -> GatewayAssistantResponse {
    let usage = usage_from_response(protocol, value);
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let choice = value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .unwrap_or(&Value::Null);
            let message = choice
                .get("message")
                .or_else(|| choice.get("delta"))
                .unwrap_or(&Value::Null);
            GatewayAssistantResponse {
                content: content_parts_from_value(message.get("content").unwrap_or(&Value::Null)),
                tool_calls: openai_tool_calls_from_value(
                    message.get("tool_calls").unwrap_or(&Value::Null),
                ),
                finish_reason: choice
                    .get("finish_reason")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                usage,
            }
        }
        GatewayProtocol::OpenAiResponses => {
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();
            for item in value
                .get("output")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                match item.get("type").and_then(Value::as_str).unwrap_or_default() {
                    "message" => content.extend(content_parts_from_value(
                        item.get("content").unwrap_or(&Value::Null),
                    )),
                    "function_call" => {
                        if let Some(call) = responses_function_call_from_value(item) {
                            tool_calls.push(call)
                        }
                    }
                    _ => {}
                }
            }
            if content.is_empty() {
                content.extend(content_parts_from_value(
                    value.get("output_text").unwrap_or(&Value::Null),
                ))
            }
            GatewayAssistantResponse {
                content,
                tool_calls,
                finish_reason: value
                    .get("status")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                usage,
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let (content, tool_calls) =
                anthropic_content_and_tool_calls(value.get("content").unwrap_or(&Value::Null));
            GatewayAssistantResponse {
                content,
                tool_calls,
                finish_reason: value
                    .get("stop_reason")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                usage,
            }
        }
        GatewayProtocol::GoogleGemini => {
            let candidate = value
                .get("candidates")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .unwrap_or(&Value::Null);
            let (content, tool_calls) = gemini_parts_and_tool_calls(
                candidate
                    .get("content")
                    .and_then(|content| content.get("parts"))
                    .unwrap_or(&Value::Null),
            );
            GatewayAssistantResponse {
                content,
                tool_calls,
                finish_reason: candidate
                    .get("finishReason")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                usage,
            }
        }
    }
}

pub(in crate::core::gateway) fn encode_response(
    protocol: GatewayProtocol,
    model: &str,
    response: &GatewayAssistantResponse,
) -> Value {
    let text = content_text(&response.content);
    let finish_reason = response
        .finish_reason
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(if response.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        });
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut message = Map::new();
            message.insert("role".into(), Value::String("assistant".into()));
            message.insert(
                "content".into(),
                openai_chat_content_value(&response.content),
            );
            if !response.tool_calls.is_empty() {
                message.insert(
                    "tool_calls".into(),
                    Value::Array(
                        response
                            .tool_calls
                            .iter()
                            .map(openai_tool_call_value)
                            .collect(),
                    ),
                );
            }
            json!({
                "id": format!("chatcmpl-codestudio-{}", Uuid::new_v4().simple()), "object": "chat.completion",
                "created": unix_timestamp(), "model": model,
                "choices": [{ "index": 0, "message": Value::Object(message),
                    "finish_reason": if response.tool_calls.is_empty() { finish_reason } else { "tool_calls" } }],
                "usage": usage_value_for_protocol(protocol, &response.usage)
            })
        }
        GatewayProtocol::OpenAiResponses => {
            let mut output = Vec::new();
            if !response.content.is_empty() || response.tool_calls.is_empty() {
                output.push(json!({ "id": format!("msg-codestudio-{}", Uuid::new_v4().simple()), "type": "message",
                    "status": "completed", "role": "assistant", "content": responses_output_content_parts(&response.content) }));
            }
            output.extend(response.tool_calls.iter().map(|call| json!({
                "id": call.id, "type": "function_call", "status": "completed", "call_id": call.id,
                "name": call.name, "arguments": arguments_as_string(&call.arguments)
            })));
            json!({
                "id": format!("resp-codestudio-{}", Uuid::new_v4().simple()), "object": "response",
                "created_at": unix_timestamp(), "status": "completed", "model": model, "output": output,
                "output_text": text, "usage": usage_value_for_protocol(protocol, &response.usage)
            })
        }
        GatewayProtocol::AnthropicMessages => json!({
            "id": format!("msg_codestudio_{}", Uuid::new_v4().simple()), "type": "message", "role": "assistant",
            "model": model, "content": anthropic_assistant_content_blocks(response),
            "stop_reason": if response.tool_calls.is_empty() { "end_turn" } else { "tool_use" },
            "usage": usage_value_for_protocol(protocol, &response.usage)
        }),
        GatewayProtocol::GoogleGemini => json!({
            "candidates": [{ "content": { "role": "model", "parts": gemini_assistant_parts(response) },
                "finishReason": if response.tool_calls.is_empty() { "STOP" } else { "TOOL_CALL" }, "index": 0 }],
            "usageMetadata": usage_value_for_protocol(protocol, &response.usage), "modelVersion": model
        }),
    }
}
