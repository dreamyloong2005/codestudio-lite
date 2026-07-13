use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::gateway) enum GatewayProtocol {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
    GoogleGemini,
}

impl GatewayProtocol {
    pub(in crate::core::gateway) fn from_profile_protocol(value: &str) -> Result<Self, String> {
        match value {
            "openai-chat-completions" => Ok(Self::OpenAiChatCompletions),
            "openai-responses" => Ok(Self::OpenAiResponses),
            "anthropic-messages" => Ok(Self::AnthropicMessages),
            "google-gemini" => Ok(Self::GoogleGemini),
            _ => Err("Unsupported Provider API protocol.".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) enum GatewayContentPart {
    Text(String),
    ImageUrl(String),
    ImageBase64 {
        mime_type: String,
        data: String,
    },
    ToolResult {
        tool_call_id: Option<String>,
        content: String,
    },
    Unknown(Value),
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) struct GatewayToolCall {
    pub(in crate::core::gateway) id: String,
    pub(in crate::core::gateway) name: String,
    pub(in crate::core::gateway) arguments: Value,
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) struct GatewayToolSpec {
    pub(in crate::core::gateway) name: String,
    pub(in crate::core::gateway) description: Option<String>,
    pub(in crate::core::gateway) schema: Option<Value>,
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) struct GatewayMessage {
    pub(in crate::core::gateway) role: String,
    pub(in crate::core::gateway) content: Vec<GatewayContentPart>,
    pub(in crate::core::gateway) tool_call_id: Option<String>,
    pub(in crate::core::gateway) tool_calls: Vec<GatewayToolCall>,
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) struct GatewayRequestParts {
    pub(in crate::core::gateway) model: String,
    pub(in crate::core::gateway) system: Option<String>,
    pub(in crate::core::gateway) messages: Vec<GatewayMessage>,
    pub(in crate::core::gateway) tools: Vec<GatewayToolSpec>,
    pub(in crate::core::gateway) tool_choice: Option<Value>,
    pub(in crate::core::gateway) max_tokens: Option<u64>,
    pub(in crate::core::gateway) temperature: Option<Value>,
    pub(in crate::core::gateway) top_p: Option<Value>,
}

#[derive(Debug, Clone)]
pub(in crate::core::gateway) struct GatewayAssistantResponse {
    pub(in crate::core::gateway) content: Vec<GatewayContentPart>,
    pub(in crate::core::gateway) tool_calls: Vec<GatewayToolCall>,
    pub(in crate::core::gateway) finish_reason: Option<String>,
    pub(in crate::core::gateway) usage: GatewayUsage,
}

#[derive(Debug)]
pub(in crate::core::gateway) struct ConvertedGatewayRequest {
    pub(in crate::core::gateway) endpoint: String,
    pub(in crate::core::gateway) headers: String,
    pub(in crate::core::gateway) body: Value,
    pub(in crate::core::gateway) model: String,
}

#[derive(Debug, Clone, Default)]
pub(in crate::core::gateway) struct GatewayUsage {
    pub(in crate::core::gateway) input_tokens: u64,
    pub(in crate::core::gateway) output_tokens: u64,
    pub(in crate::core::gateway) total_tokens: u64,
    pub(in crate::core::gateway) cached_input_tokens: Option<u64>,
    pub(in crate::core::gateway) cache_creation_input_tokens: Option<u64>,
    pub(in crate::core::gateway) cache_read_input_tokens: Option<u64>,
    pub(in crate::core::gateway) reasoning_tokens: Option<u64>,
    pub(in crate::core::gateway) audio_input_tokens: Option<u64>,
    pub(in crate::core::gateway) audio_output_tokens: Option<u64>,
    pub(in crate::core::gateway) image_input_tokens: Option<u64>,
    pub(in crate::core::gateway) image_output_tokens: Option<u64>,
    pub(in crate::core::gateway) raw_prompt_details: Option<Value>,
    pub(in crate::core::gateway) raw_completion_details: Option<Value>,
}
