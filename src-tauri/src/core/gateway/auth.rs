use std::collections::HashMap;
use std::net::IpAddr;

pub(super) fn bearer_authorized(headers: &HashMap<String, String>, token: &str) -> bool {
    headers
        .get("authorization")
        .map(|value| value == &format!("Bearer {token}"))
        .unwrap_or(false)
}

pub(super) fn scoped_request_can_skip_local_auth(
    strict_tool: bool,
    tool_id: Option<&str>,
    host: &str,
) -> bool {
    strict_tool && tool_id == Some("codex") && host_is_loopback(host)
}

fn host_is_loopback(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_auth_requires_the_exact_managed_token() {
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer expected".to_string());
        assert!(bearer_authorized(&headers, "expected"));
        assert!(!bearer_authorized(&headers, "other"));
    }

    #[test]
    fn only_loopback_codex_scopes_skip_local_auth() {
        assert!(scoped_request_can_skip_local_auth(
            true,
            Some("codex"),
            "127.0.0.1"
        ));
        assert!(scoped_request_can_skip_local_auth(
            true,
            Some("codex"),
            "[::1]"
        ));
        assert!(!scoped_request_can_skip_local_auth(
            true,
            Some("claude"),
            "127.0.0.1"
        ));
        assert!(!scoped_request_can_skip_local_auth(
            true,
            Some("codex"),
            "0.0.0.0"
        ));
        assert!(!scoped_request_can_skip_local_auth(
            false,
            Some("codex"),
            "localhost"
        ));
    }
}
