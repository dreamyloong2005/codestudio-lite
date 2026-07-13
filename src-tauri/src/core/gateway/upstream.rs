use super::protocol::canonical::GatewayProtocol;
use crate::core::profile;
use crate::core::types::ProfileDraft;
use crate::core::upstream_http;
use std::collections::{HashMap, HashSet};
use url::Url;

const PROTOCOL_GOOGLE_GEMINI: &str = "google-gemini";

pub(super) fn endpoint(
    protocol: GatewayProtocol,
    profile: &ProfileDraft,
    model: &str,
    stream: bool,
) -> String {
    match protocol {
        GatewayProtocol::OpenAiChatCompletions => {
            api_endpoint(&profile.base_url, "/v1/chat/completions")
        }
        GatewayProtocol::OpenAiResponses => api_endpoint(&profile.base_url, "/v1/responses"),
        GatewayProtocol::AnthropicMessages => api_endpoint(&profile.base_url, "/messages"),
        GatewayProtocol::GoogleGemini => {
            let runtime_base_url = profile::profile_runtime_base_url_for_protocol(
                PROTOCOL_GOOGLE_GEMINI,
                &profile.base_url,
            );
            let base_url = runtime_base_url.trim_end_matches('/');
            let model = normalize_gemini_model(model);
            if stream {
                format!("{base_url}/models/{model}:streamGenerateContent?alt=sse")
            } else {
                format!("{base_url}/models/{model}:generateContent")
            }
        }
    }
}

pub(super) fn api_endpoint(base_url: &str, path: &str) -> String {
    let trimmed_base = base_url.trim_end_matches('/');
    let clean_path = format!("/{}", path.trim().trim_start_matches('/'));
    let fallback = || format!("{trimmed_base}{clean_path}");
    let Ok(mut parsed) = Url::parse(trimmed_base) else {
        return fallback();
    };
    if parsed.scheme().is_empty() || parsed.host_str().is_none() {
        return fallback();
    }

    let base_path = parsed.path().trim_end_matches('/').to_string();
    let endpoint_path = path_without_v1(&clean_path);
    let clean_suffix = clean_path.trim_end_matches('/');
    if base_path.ends_with(clean_suffix) {
        parsed.set_path(&base_path);
    } else if endpoint_path
        .as_deref()
        .is_some_and(|endpoint| base_path.ends_with(endpoint.trim_end_matches('/')))
    {
        parsed.set_path(&base_path);
    } else if let Some(endpoint) = endpoint_path
        .as_deref()
        .filter(|_| base_has_version_segment(&base_path))
    {
        parsed.set_path(&format!("{base_path}{endpoint}"));
    } else {
        parsed.set_path(&format!("{base_path}{clean_path}"));
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

fn path_without_v1(path: &str) -> Option<String> {
    let clean_path = format!("/{}", path.trim().trim_start_matches('/'));
    clean_path
        .strip_prefix("/v1/")
        .map(|path| format!("/{path}"))
}

fn base_has_version_segment(base_path: &str) -> bool {
    base_path
        .trim_matches('/')
        .split('/')
        .any(path_segment_is_version)
}

fn path_segment_is_version(segment: &str) -> bool {
    let mut chars = segment.trim().chars();
    if !matches!(chars.next(), Some('v' | 'V')) {
        return false;
    }
    let mut has_digit = false;
    for ch in chars {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if !has_digit || !(ch.is_ascii_alphabetic() || matches!(ch, '-' | '_' | '.')) {
            return false;
        }
    }
    has_digit
}

fn normalize_gemini_model(model: &str) -> String {
    model
        .trim()
        .trim_start_matches("models/")
        .trim_start_matches('/')
        .to_string()
}

pub(super) fn headers(
    protocol: GatewayProtocol,
    api_key: &str,
    request_headers: &HashMap<String, String>,
) -> String {
    let mut headers = match protocol {
        GatewayProtocol::AnthropicMessages => upstream_http::anthropic_json_headers(api_key),
        GatewayProtocol::GoogleGemini => upstream_http::gemini_json_headers(api_key),
        GatewayProtocol::OpenAiChatCompletions | GatewayProtocol::OpenAiResponses => {
            upstream_http::bearer_json_headers(api_key)
        }
    };
    let mut generated = generated_header_names(&headers);
    let mut passthrough = safe_passthrough_headers(request_headers)
        .into_iter()
        .filter(|(name, _)| !generated.contains(*name))
        .collect::<Vec<_>>();
    passthrough.sort_by(|left, right| left.0.cmp(right.0));
    for (name, value) in passthrough {
        generated.insert(name.to_string());
        headers.push_str(canonical_header_name(name));
        headers.push_str(": ");
        headers.push_str(value);
        headers.push_str("\r\n");
    }
    headers
}

fn generated_header_names(headers: &str) -> HashSet<String> {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_ascii_lowercase())
        .collect()
}

fn safe_passthrough_headers(headers: &HashMap<String, String>) -> Vec<(&str, &str)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let (name, value) = (name.trim(), value.trim());
            (!value.is_empty() && is_safe_passthrough_header(name, value)).then_some((name, value))
        })
        .collect()
}

fn is_safe_passthrough_header(name: &str, value: &str) -> bool {
    is_valid_header_name(name)
        && is_safe_header_value(value)
        && !is_forbidden_header(name)
        && (name.starts_with("x-")
            || name.starts_with("anthropic-")
            || name.starts_with("openai-")
            || name.starts_with("cf-")
            || name.starts_with("helicone-")
            || matches!(name, "http-referer" | "referer" | "user-agent"))
}

fn is_forbidden_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-encoding"
            | "authorization"
            | "connection"
            | "content-length"
            | "content-type"
            | "cookie"
            | "expect"
            | "host"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "x-api-key"
            | "x-codestudio-client"
            | "x-codestudio-client-tool"
            | "x-codestudio-tool"
            | "x-goog-api-key"
            | "anthropic-version"
    )
}

fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            matches!(byte, b'!' | b'#' | b'$' | b'%' | b'&' | b'\''
        | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~' | b'0'..=b'9' | b'a'..=b'z')
        })
}

fn is_safe_header_value(value: &str) -> bool {
    !value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

fn canonical_header_name(name: &str) -> &str {
    match name {
        "http-referer" => "HTTP-Referer",
        "referer" => "Referer",
        "user-agent" => "User-Agent",
        "x-title" => "X-Title",
        "x-stainless-lang" => "X-Stainless-Lang",
        "x-stainless-package-version" => "X-Stainless-Package-Version",
        "x-stainless-os" => "X-Stainless-OS",
        "x-stainless-arch" => "X-Stainless-Arch",
        "x-stainless-runtime" => "X-Stainless-Runtime",
        "x-stainless-runtime-version" => "X-Stainless-Runtime-Version",
        "x-codestudio-client" => "X-CodeStudio-Client",
        "x-codestudio-tool" => "X-CodeStudio-Tool",
        "x-codestudio-client-tool" => "X-CodeStudio-Client-Tool",
        "anthropic-beta" => "anthropic-beta",
        "openai-beta" => "OpenAI-Beta",
        "cf-ray" => "CF-Ray",
        _ => name,
    }
}
