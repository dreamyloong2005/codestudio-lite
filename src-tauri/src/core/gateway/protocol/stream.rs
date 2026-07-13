#[derive(Debug, Clone, Default)]
pub(in crate::core::gateway) struct SseBuffer {
    buffer: String,
}

impl SseBuffer {
    pub(in crate::core::gateway) fn push_chunk(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        self.drain(false)
    }
    pub(in crate::core::gateway) fn finish(&mut self) -> Vec<SseFrame> {
        self.drain(true)
    }
    fn drain(&mut self, include_remainder: bool) -> Vec<SseFrame> {
        let mut frames = Vec::new();
        while let Some((index, separator_len)) = next_separator(&self.buffer) {
            let raw = self.buffer[..index].to_string();
            self.buffer.drain(..index + separator_len);
            if let Some(frame) = parse_frame(&raw) {
                frames.push(frame)
            }
        }
        if include_remainder && !self.buffer.trim().is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            if let Some(frame) = parse_frame(&raw) {
                frames.push(frame)
            }
        }
        frames
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core::gateway) struct SseFrame {
    pub(in crate::core::gateway) event: Option<String>,
    pub(in crate::core::gateway) data: String,
}

fn next_separator(value: &str) -> Option<(usize, usize)> {
    let lf = value.find("\n\n").map(|index| (index, 2));
    let crlf = value.find("\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_frame(raw: &str) -> Option<SseFrame> {
    let mut event = None;
    let mut data = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string())
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string())
        }
    }
    (!data.is_empty()).then(|| SseFrame {
        event,
        data: data.join("\n"),
    })
}

#[derive(Debug, Clone, Default)]
pub(in crate::core::gateway) struct GatewayStreamUpdate {
    pub(in crate::core::gateway) text_delta: String,
    pub(in crate::core::gateway) tool_calls: Vec<GatewayToolCall>,
    pub(in crate::core::gateway) usage: Option<GatewayUsage>,
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) struct ClientStreamState {
    pub(in crate::core::gateway) protocol: GatewayProtocol,
    pub(in crate::core::gateway) id: String,
    pub(in crate::core::gateway) item_id: String,
    pub(in crate::core::gateway) model: String,
    pub(in crate::core::gateway) created: u64,
}

impl ClientStreamState {
    pub(in crate::core::gateway) fn new(protocol: GatewayProtocol, model: &str) -> Self {
        Self {
            protocol,
            id: format!("stream_codestudio_{}", Uuid::new_v4().simple()),
            item_id: format!("item_codestudio_{}", Uuid::new_v4().simple()),
            model: model.to_string(),
            created: unix_timestamp(),
        }
    }
}

pub(in crate::core::gateway) fn write_start(
    stream: &mut TcpStream,
    state: &ClientStreamState,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions | GatewayProtocol::GoogleGemini => Ok(()),
        GatewayProtocol::OpenAiResponses => {
            write_sse_json(
                stream,
                Some("response.created"),
                &json!({ "type": "response.created", "response": {
                "id": state.id, "object": "response", "created_at": state.created, "status": "in_progress", "model": state.model, "output": [] } }),
            )?;
            write_sse_json(
                stream,
                Some("response.output_item.added"),
                &json!({ "type": "response.output_item.added", "output_index": 0,
                "item": { "id": state.item_id, "type": "message", "status": "in_progress", "role": "assistant", "content": [] } }),
            )?;
            write_sse_json(
                stream,
                Some("response.content_part.added"),
                &json!({ "type": "response.content_part.added", "item_id": state.item_id,
                "output_index": 0, "content_index": 0, "part": { "type": "output_text", "text": "" } }),
            )
        }
        GatewayProtocol::AnthropicMessages => {
            write_sse_json(
                stream,
                Some("message_start"),
                &json!({ "type": "message_start", "message": { "id": state.id, "type": "message",
                "role": "assistant", "model": state.model, "content": [], "stop_reason": null, "stop_sequence": null,
                "usage": { "input_tokens": 0, "output_tokens": 0 } } }),
            )?;
            write_sse_json(
                stream,
                Some("content_block_start"),
                &json!({ "type": "content_block_start", "index": 0,
                "content_block": { "type": "text", "text": "" } }),
            )
        }
    }
}

pub(in crate::core::gateway) fn decode_update(
    protocol: GatewayProtocol,
    frame: &SseFrame,
    value: &Value,
) -> GatewayStreamUpdate {
    GatewayStreamUpdate {
        text_delta: text_delta(protocol, frame, value),
        tool_calls: tool_calls(protocol, frame, value),
        usage: usage(protocol, value),
    }
}

pub(in crate::core::gateway) fn text_delta(
    protocol: GatewayProtocol,
    frame: &SseFrame,
    value: &Value,
) -> String {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|choice| choice.get("delta").or_else(|| choice.get("message")))
            .and_then(|message| message.get("content"))
            .map(text_from_value)
            .unwrap_or_default(),
        GatewayProtocol::OpenAiResponses => {
            let event_type = value
                .get("type")
                .and_then(Value::as_str)
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event_type.contains("delta") {
                value
                    .get("delta")
                    .or_else(|| value.get("text"))
                    .map(text_from_value)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let event_type = value
                .get("type")
                .and_then(Value::as_str)
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event_type == "content_block_delta" {
                value
                    .get("delta")
                    .and_then(|delta| delta.get("text"))
                    .map(text_from_value)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
        GatewayProtocol::GoogleGemini => {
            assistant_text_from_response(GatewayProtocol::GoogleGemini, value)
        }
    }
}

fn tool_calls(protocol: GatewayProtocol, frame: &SseFrame, value: &Value) -> Vec<GatewayToolCall> {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|choice| choice.get("delta").or_else(|| choice.get("message")))
            .and_then(|message| message.get("tool_calls"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, item)| openai_tool_call(index, item))
            .collect(),
        GatewayProtocol::OpenAiResponses => {
            let event = value
                .get("type")
                .and_then(Value::as_str)
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if matches!(
                event,
                "response.output_item.added" | "response.output_item.done"
            ) {
                value
                    .get("item")
                    .filter(|item| {
                        item.get("type").and_then(Value::as_str) == Some("function_call")
                    })
                    .and_then(responses_tool_call)
                    .into_iter()
                    .collect()
            } else if event == "response.function_call_arguments.delta" {
                let delta = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if delta.is_empty() {
                    Vec::new()
                } else {
                    vec![GatewayToolCall {
                        id: value
                            .get("item_id")
                            .or_else(|| value.get("call_id"))
                            .and_then(Value::as_str)
                            .unwrap_or("call_codestudio_stream")
                            .to_string(),
                        name: value
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string(),
                        arguments: Value::String(delta.to_string()),
                    }]
                }
            } else {
                Vec::new()
            }
        }
        GatewayProtocol::AnthropicMessages => {
            let event = value
                .get("type")
                .and_then(Value::as_str)
                .or(frame.event.as_deref())
                .unwrap_or_default();
            if event == "content_block_start" {
                value
                    .get("content_block")
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
                    .and_then(anthropic_tool_call_from_value)
                    .into_iter()
                    .collect()
            } else if event == "content_block_delta" {
                let delta = value.get("delta").unwrap_or(&Value::Null);
                delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(|text| GatewayToolCall {
                        id: "call_codestudio_stream".to_string(),
                        name: "tool".to_string(),
                        arguments: Value::String(text.to_string()),
                    })
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            }
        }
        GatewayProtocol::GoogleGemini => {
            assistant_response_from_protocol(protocol, value).tool_calls
        }
    }
}

fn openai_tool_call(index: usize, value: &Value) -> Option<GatewayToolCall> {
    let function = value.get("function").unwrap_or(value);
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("tool")
        .to_string();
    let arguments = function.get("arguments");
    if name == "tool" && arguments.is_none() {
        return None;
    }
    Some(GatewayToolCall {
        id: value
            .get("id")
            .or_else(|| value.get("call_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("call_codestudio_stream_{index}")),
        name,
        arguments: argument_value(arguments),
    })
}

fn responses_tool_call(value: &Value) -> Option<GatewayToolCall> {
    Some(GatewayToolCall {
        id: value
            .get("call_id")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("call_codestudio_{}", Uuid::new_v4().simple())),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("tool")
            .to_string(),
        arguments: argument_value(value.get("arguments")),
    })
}

fn usage(protocol: GatewayProtocol, value: &Value) -> Option<GatewayUsage> {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => value
            .get("usage")
            .map(|_| usage_from_response(protocol, value)),
        GatewayProtocol::OpenAiResponses => {
            if value.get("usage").is_some() {
                Some(usage_from_response(protocol, value))
            } else {
                value
                    .get("response")
                    .filter(|item| item.get("usage").is_some())
                    .map(|item| usage_from_response(protocol, item))
            }
        }
        GatewayProtocol::AnthropicMessages => {
            if value.get("usage").is_some() {
                Some(usage_from_response(protocol, value))
            } else {
                value
                    .get("message")
                    .filter(|item| item.get("usage").is_some())
                    .map(|item| usage_from_response(protocol, item))
            }
        }
        GatewayProtocol::GoogleGemini => value
            .get("usageMetadata")
            .map(|_| usage_from_response(protocol, value)),
    }
}

pub(in crate::core::gateway) fn merge_usage(target: &mut GatewayUsage, update: GatewayUsage) {
    if update.input_tokens > 0 {
        target.input_tokens = update.input_tokens
    }
    if update.output_tokens > 0 {
        target.output_tokens = update.output_tokens
    }
    if update.total_tokens > 0 {
        target.total_tokens = update.total_tokens
    } else if target.total_tokens == 0 {
        target.total_tokens = target.input_tokens + target.output_tokens
    }
    target.cached_input_tokens = update.cached_input_tokens.or(target.cached_input_tokens);
    target.cache_creation_input_tokens = update
        .cache_creation_input_tokens
        .or(target.cache_creation_input_tokens);
    target.cache_read_input_tokens = update
        .cache_read_input_tokens
        .or(target.cache_read_input_tokens);
    target.reasoning_tokens = update.reasoning_tokens.or(target.reasoning_tokens);
    target.audio_input_tokens = update.audio_input_tokens.or(target.audio_input_tokens);
    target.audio_output_tokens = update.audio_output_tokens.or(target.audio_output_tokens);
    target.image_input_tokens = update.image_input_tokens.or(target.image_input_tokens);
    target.image_output_tokens = update.image_output_tokens.or(target.image_output_tokens);
    target.raw_prompt_details = update
        .raw_prompt_details
        .or_else(|| target.raw_prompt_details.clone());
    target.raw_completion_details = update
        .raw_completion_details
        .or_else(|| target.raw_completion_details.clone());
}

pub(in crate::core::gateway) fn usage_has_values(usage: &GatewayUsage) -> bool {
    usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.total_tokens > 0
        || usage.cached_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some()
        || usage.cache_read_input_tokens.is_some()
        || usage.reasoning_tokens.is_some()
        || usage.audio_input_tokens.is_some()
        || usage.audio_output_tokens.is_some()
        || usage.image_input_tokens.is_some()
        || usage.image_output_tokens.is_some()
        || usage.raw_prompt_details.is_some()
        || usage.raw_completion_details.is_some()
}
pub(in crate::core::gateway) fn write_delta(
    stream: &mut TcpStream,
    state: &ClientStreamState,
    delta: &str,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions => write_sse_data(
            stream,
            &json!({
                "id": state.id, "object": "chat.completion.chunk", "created": state.created, "model": state.model,
                "choices": [{ "index": 0, "delta": { "content": delta }, "finish_reason": null }]
            }),
        ),
        GatewayProtocol::OpenAiResponses => write_sse_json(
            stream,
            Some("response.output_text.delta"),
            &json!({
                "type": "response.output_text.delta", "item_id": state.item_id, "output_index": 0, "content_index": 0, "delta": delta
            }),
        ),
        GatewayProtocol::AnthropicMessages => write_sse_json(
            stream,
            Some("content_block_delta"),
            &json!({
                "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": delta }
            }),
        ),
        GatewayProtocol::GoogleGemini => write_sse_data(
            stream,
            &json!({
                "candidates": [{ "content": { "role": "model", "parts": [{ "text": delta }] }, "index": 0 }]
            }),
        ),
    }
}

pub(in crate::core::gateway) fn write_tool_call(
    stream: &mut TcpStream,
    state: &ClientStreamState,
    tool_call: &GatewayToolCall,
    index: usize,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions => write_sse_data(
            stream,
            &json!({
                "id": state.id, "object": "chat.completion.chunk", "created": state.created,
                "model": state.model, "choices": [{ "index": 0, "delta": { "tool_calls": [{
                    "index": index, "id": tool_call.id, "type": "function", "function": {
                        "name": tool_call.name, "arguments": arguments_as_string(&tool_call.arguments)
                    }}]}, "finish_reason": null }]
            }),
        ),
        GatewayProtocol::OpenAiResponses => {
            write_sse_json(
                stream,
                Some("response.output_item.added"),
                &json!({
                    "type": "response.output_item.added", "output_index": index + 1,
                    "item": { "id": tool_call.id, "type": "function_call", "status": "in_progress",
                        "call_id": tool_call.id, "name": tool_call.name, "arguments": "" }
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.function_call_arguments.delta"),
                &json!({
                    "type": "response.function_call_arguments.delta", "item_id": tool_call.id,
                    "output_index": index + 1, "delta": arguments_as_string(&tool_call.arguments)
                }),
            )?;
            write_sse_json(
                stream,
                Some("response.output_item.done"),
                &json!({
                    "type": "response.output_item.done", "output_index": index + 1,
                    "item": { "id": tool_call.id, "type": "function_call", "status": "completed",
                        "call_id": tool_call.id, "name": tool_call.name,
                        "arguments": arguments_as_string(&tool_call.arguments) }
                }),
            )
        }
        GatewayProtocol::AnthropicMessages => {
            let block_index = index + 1;
            write_sse_json(
                stream,
                Some("content_block_start"),
                &json!({
                    "type": "content_block_start", "index": block_index,
                    "content_block": { "type": "tool_use", "id": tool_call.id,
                        "name": tool_call.name, "input": {} }
                }),
            )?;
            write_sse_json(
                stream,
                Some("content_block_delta"),
                &json!({
                    "type": "content_block_delta", "index": block_index,
                    "delta": { "type": "input_json_delta",
                        "partial_json": arguments_as_string(&tool_call.arguments) }
                }),
            )?;
            write_sse_json(
                stream,
                Some("content_block_stop"),
                &json!({
                    "type": "content_block_stop", "index": block_index
                }),
            )
        }
        GatewayProtocol::GoogleGemini => write_sse_data(
            stream,
            &json!({
                "candidates": [{ "content": { "role": "model", "parts": [{
                    "functionCall": { "name": tool_call.name,
                        "args": arguments_as_object(&tool_call.arguments) }
                }]}, "index": 0 }]
            }),
        ),
    }
}

pub(in crate::core::gateway) fn write_done(
    stream: &mut TcpStream,
    state: &ClientStreamState,
    full_text: &str,
    tool_calls: &[GatewayToolCall],
    usage: &GatewayUsage,
) -> Result<(), String> {
    match state.protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            let mut done = json!({ "id": state.id, "object": "chat.completion.chunk", "created": state.created, "model": state.model,
                "choices": [{ "index": 0, "delta": {}, "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" } }] });
            if usage_has_values(usage) {
                done["usage"] = usage_value_for_protocol(state.protocol, usage);
            }
            write_sse_data(stream, &done)?;
            write_sse_done(stream)
        }
        GatewayProtocol::OpenAiResponses => {
            write_sse_json(
                stream,
                Some("response.output_text.done"),
                &json!({ "type": "response.output_text.done", "item_id": state.item_id, "output_index": 0, "content_index": 0, "text": full_text }),
            )?;
            write_sse_json(
                stream,
                Some("response.content_part.done"),
                &json!({ "type": "response.content_part.done", "item_id": state.item_id, "output_index": 0, "content_index": 0, "part": { "type": "output_text", "text": full_text } }),
            )?;
            write_sse_json(
                stream,
                Some("response.output_item.done"),
                &json!({ "type": "response.output_item.done", "output_index": 0, "item": { "id": state.item_id, "type": "message", "status": "completed", "role": "assistant", "content": [{ "type": "output_text", "text": full_text }] } }),
            )?;
            write_sse_json(
                stream,
                Some("response.completed"),
                &json!({ "type": "response.completed", "response": response_body_for_protocol(state.protocol, &state.model, &GatewayAssistantResponse { content: vec![GatewayContentPart::Text(full_text.to_string())], tool_calls: tool_calls.to_vec(), finish_reason: Some(if tool_calls.is_empty() { "stop" } else { "tool_calls" }.to_string()), usage: usage.clone() }) }),
            )
        }
        GatewayProtocol::AnthropicMessages => {
            write_sse_json(
                stream,
                Some("content_block_stop"),
                &json!({ "type": "content_block_stop", "index": 0 }),
            )?;
            write_sse_json(
                stream,
                Some("message_delta"),
                &json!({ "type": "message_delta", "delta": { "stop_reason": if tool_calls.is_empty() { "end_turn" } else { "tool_use" }, "stop_sequence": null }, "usage": { "output_tokens": usage.output_tokens } }),
            )?;
            write_sse_json(
                stream,
                Some("message_stop"),
                &json!({ "type": "message_stop" }),
            )
        }
        GatewayProtocol::GoogleGemini => {
            let mut done = json!({ "candidates": [{ "content": { "role": "model", "parts": gemini_assistant_parts(&GatewayAssistantResponse { content: if full_text.is_empty() { Vec::new() } else { vec![GatewayContentPart::Text(full_text.to_string())] }, tool_calls: tool_calls.to_vec(), finish_reason: Some("STOP".to_string()), usage: usage.clone() }) }, "finishReason": if tool_calls.is_empty() { "STOP" } else { "TOOL_CALL" }, "index": 0 }] });
            if usage_has_values(usage) {
                done["usageMetadata"] = usage_value_for_protocol(state.protocol, usage);
            }
            write_sse_data(stream, &done)
        }
    }
}

fn write_sse_done(stream: &mut TcpStream) -> Result<(), String> {
    use std::io::Write;
    stream
        .write_all(b"data: [DONE]\n\n")
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())
}

fn write_sse_json(
    stream: &mut TcpStream,
    event: Option<&str>,
    value: &Value,
) -> Result<(), String> {
    use std::io::Write;
    if let Some(event) = event {
        stream
            .write_all(format!("event: {event}\n").as_bytes())
            .map_err(|err| err.to_string())?;
    }
    write_sse_data(stream, value)
}

fn write_sse_data(stream: &mut TcpStream, value: &Value) -> Result<(), String> {
    use std::io::Write;
    let data = serde_json::to_string(value).map_err(|err| err.to_string())?;
    stream
        .write_all(format!("data: {data}\n\n").as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())
}

use super::super::{
    anthropic_tool_call_from_value, argument_value, arguments_as_object, arguments_as_string,
    assistant_response_from_protocol, assistant_text_from_response, gemini_assistant_parts,
    response_body_for_protocol, text_from_value, unix_timestamp, usage_from_response,
    usage_value_for_protocol,
};
use super::canonical::{
    GatewayAssistantResponse, GatewayContentPart, GatewayProtocol, GatewayToolCall, GatewayUsage,
};
use serde_json::{json, Value};
use std::net::TcpStream;
use uuid::Uuid;
