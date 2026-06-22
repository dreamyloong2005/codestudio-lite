use serde_json::Value;

pub struct UpstreamResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct UpstreamResponseMeta {
    pub status: u16,
    pub content_type: &'static str,
}

pub enum UpstreamStreamEvent<'a> {
    Headers(UpstreamResponseMeta),
    Chunk(&'a [u8]),
}

pub fn post_json(
    url: &str,
    bearer_token: &str,
    json_body: &Value,
    timeout_seconds: u16,
) -> Result<UpstreamResponse, String> {
    post_json_with_headers(
        url,
        &bearer_json_headers(bearer_token),
        json_body,
        timeout_seconds,
    )
}

pub fn post_json_with_headers(
    url: &str,
    headers: &str,
    json_body: &Value,
    timeout_seconds: u16,
) -> Result<UpstreamResponse, String> {
    let mut body = Vec::new();
    let meta = post_json_stream_with_headers(url, headers, json_body, timeout_seconds, |event| {
        if let UpstreamStreamEvent::Chunk(chunk) = event {
            body.extend_from_slice(chunk);
        }

        Ok(())
    })?;

    Ok(UpstreamResponse {
        status: meta.status,
        content_type: meta.content_type,
        body,
    })
}

pub fn post_json_stream<F>(
    url: &str,
    bearer_token: &str,
    json_body: &Value,
    timeout_seconds: u16,
    on_event: F,
) -> Result<UpstreamResponseMeta, String>
where
    F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
{
    post_json_stream_with_headers(
        url,
        &bearer_json_headers(bearer_token),
        json_body,
        timeout_seconds,
        on_event,
    )
}

pub fn post_json_stream_with_headers<F>(
    url: &str,
    headers: &str,
    json_body: &Value,
    timeout_seconds: u16,
    on_event: F,
) -> Result<UpstreamResponseMeta, String>
where
    F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
{
    platform::post_json_stream(url, headers, json_body, timeout_seconds, on_event)
}

pub fn bearer_json_headers(bearer_token: &str) -> String {
    format!(
        "Authorization: Bearer {bearer_token}\r\nContent-Type: application/json\r\nAccept: application/json\r\n"
    )
}

pub fn anthropic_json_headers(api_key: &str) -> String {
    format!(
        "x-api-key: {api_key}\r\nanthropic-version: 2023-06-01\r\nContent-Type: application/json\r\nAccept: application/json\r\n"
    )
}

pub fn gemini_json_headers(api_key: &str) -> String {
    format!(
        "x-goog-api-key: {api_key}\r\nContent-Type: application/json\r\nAccept: application/json\r\n"
    )
}

#[derive(Debug)]
struct ParsedUrl {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "Upstream URL must start with http:// or https://".to_string())?;
    let secure = match scheme {
        "https" => true,
        "http" => false,
        _ => return Err("Upstream URL must start with http:// or https://".to_string()),
    };
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err(
            "Upstream URL must include a host and must not include credentials.".to_string(),
        );
    }

    let (host, port) = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| "Upstream IPv6 host is missing a closing bracket.".to_string())?;
        let host = &authority[1..end];
        let port = if authority.len() > end + 1 {
            authority[end + 2..]
                .parse::<u16>()
                .map_err(|_| "Upstream URL port is invalid.".to_string())?
        } else if secure {
            443
        } else {
            80
        };
        (host.to_string(), port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() {
            return Err("Upstream URL host is missing.".to_string());
        }
        let port = port
            .parse::<u16>()
            .map_err(|_| "Upstream URL port is invalid.".to_string())?;
        (host.to_string(), port)
    } else {
        (authority.to_string(), if secure { 443 } else { 80 })
    };

    if host.trim().is_empty() {
        return Err("Upstream URL host is missing.".to_string());
    }

    Ok(ParsedUrl {
        secure,
        host,
        port,
        path: path.to_string(),
    })
}

#[cfg(any(target_os = "macos", test))]
fn find_curl_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .or_else(|| {
            buffer
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2)
        })
}

#[cfg(any(target_os = "macos", test))]
fn parse_curl_response_meta(headers: &[u8]) -> Result<(UpstreamResponseMeta, bool), String> {
    let text = String::from_utf8_lossy(headers);
    let mut lines = text.lines();
    let status_line = lines
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| "Upstream response did not include a status line.".to_string())?;
    let mut parts = status_line.split_whitespace();
    let protocol = parts.next().unwrap_or_default();
    if !protocol.starts_with("HTTP/") {
        return Err("Upstream response status line was not HTTP.".to_string());
    }
    let status = parts
        .next()
        .ok_or_else(|| "Upstream response did not include a status code.".to_string())?
        .parse::<u16>()
        .map_err(|_| "Upstream response status code was invalid.".to_string())?;
    let mut content_type = "application/json";
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-type")
            && value
                .trim()
                .to_ascii_lowercase()
                .contains("text/event-stream")
        {
            content_type = "text/event-stream";
        }
    }
    let skip_block = status < 200
        || status_line
            .to_ascii_lowercase()
            .contains("connection established");

    Ok((
        UpstreamResponseMeta {
            status,
            content_type,
        },
        skip_block,
    ))
}

#[cfg(any(target_os = "macos", test))]
fn curl_header_lines(headers: &str) -> Vec<String> {
    headers
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn curl_config_quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\r' | '\n' => {}
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(any(target_os = "macos", test))]
fn curl_config_content(url: &str, headers: &str, timeout_seconds: u16) -> String {
    let timeout = timeout_seconds.max(1).to_string();
    let mut config = vec![
        "request = \"POST\"".to_string(),
        format!("url = {}", curl_config_quote(url)),
        format!("connect-timeout = {timeout}"),
        "speed-limit = 1".to_string(),
        format!("speed-time = {timeout}"),
        "data-binary = \"@-\"".to_string(),
    ];
    for header in curl_header_lines(headers) {
        config.push(format!("header = {}", curl_config_quote(&header)));
    }
    config.join("\n")
}

#[cfg(windows)]
mod platform {
    use super::{parse_url, UpstreamResponseMeta, UpstreamStreamEvent};
    use serde_json::Value;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER};
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_CONTENT_TYPE,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    pub fn post_json_stream<F>(
        url: &str,
        headers: &str,
        json_body: &Value,
        timeout_seconds: u16,
        mut on_event: F,
    ) -> Result<UpstreamResponseMeta, String>
    where
        F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
    {
        let parsed = parse_url(url)?;
        let body = serde_json::to_vec(json_body)
            .map_err(|err| format!("Could not serialize upstream request body: {err}"))?;
        let body_len = u32::try_from(body.len())
            .map_err(|_| "Upstream request body is too large.".to_string())?;
        let timeout_ms = i32::from(timeout_seconds) * 1000;

        let agent = to_wide_null("CodeStudio Lite Gateway");
        let session = Handle::new(
            unsafe {
                WinHttpOpen(
                    agent.as_ptr(),
                    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                    null(),
                    null(),
                    0,
                )
            },
            "WinHttpOpen",
        )?;

        let timeout_ok = unsafe {
            WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms)
        };
        if timeout_ok == 0 {
            return Err(format!("Could not set upstream timeout: {}", last_error()));
        }

        let host = to_wide_null(&parsed.host);
        let connection = Handle::new(
            unsafe { WinHttpConnect(session.0, host.as_ptr(), parsed.port, 0) },
            "WinHttpConnect",
        )?;

        let method = to_wide_null("POST");
        let path = to_wide_null(&parsed.path);
        let flags = if parsed.secure {
            WINHTTP_FLAG_SECURE
        } else {
            0
        };
        let request = Handle::new(
            unsafe {
                WinHttpOpenRequest(
                    connection.0,
                    method.as_ptr(),
                    path.as_ptr(),
                    null(),
                    null(),
                    null(),
                    flags,
                )
            },
            "WinHttpOpenRequest",
        )?;

        let header_chars = u32::try_from(headers.encode_utf16().count())
            .map_err(|_| "Upstream request headers are too large.".to_string())?;
        let headers = to_wide_null(&headers);
        let body_ptr = if body.is_empty() {
            null()
        } else {
            body.as_ptr().cast::<c_void>()
        };
        let send_ok = unsafe {
            WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                header_chars,
                body_ptr,
                body_len,
                body_len,
                0,
            )
        };
        if send_ok == 0 {
            return Err(format!("Could not send upstream request: {}", last_error()));
        }

        let receive_ok = unsafe { WinHttpReceiveResponse(request.0, null_mut()) };
        if receive_ok == 0 {
            return Err(format!(
                "Could not receive upstream response: {}",
                last_error()
            ));
        }

        let status = query_status(request.0)?;
        let content_type = query_content_type(request.0);
        let meta = UpstreamResponseMeta {
            status,
            content_type,
        };
        on_event(UpstreamStreamEvent::Headers(meta))?;
        read_response_body(request.0, &mut on_event)?;

        Ok(meta)
    }

    struct Handle(*mut c_void);

    impl Handle {
        fn new(handle: *mut c_void, label: &str) -> Result<Self, String> {
            if handle.is_null() {
                Err(format!("{label} failed: {}", last_error()))
            } else {
                Ok(Self(handle))
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn query_status(request: *mut c_void) -> Result<u16, String> {
        let mut status: u32 = 0;
        let mut length = u32::try_from(size_of::<u32>()).unwrap_or(4);
        let ok = unsafe {
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast::<c_void>(),
                &mut length,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(format!(
                "Could not read upstream status code: {}",
                last_error()
            ));
        }

        u16::try_from(status).map_err(|_| format!("Upstream status code {status} is invalid."))
    }

    fn query_content_type(request: *mut c_void) -> &'static str {
        let mut length = 0_u32;
        let first_ok = unsafe {
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_CONTENT_TYPE,
                null(),
                null_mut(),
                &mut length,
                null_mut(),
            )
        };
        if first_ok != 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return "application/json";
        }

        let mut buffer = vec![0_u16; (length as usize / size_of::<u16>()).saturating_add(1)];
        let ok = unsafe {
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_CONTENT_TYPE,
                null(),
                buffer.as_mut_ptr().cast::<c_void>(),
                &mut length,
                null_mut(),
            )
        };
        if ok == 0 {
            return "application/json";
        }

        let units = (length as usize / size_of::<u16>()).min(buffer.len());
        let value = String::from_utf16_lossy(&buffer[..units]).to_ascii_lowercase();
        if value.contains("text/event-stream") {
            "text/event-stream"
        } else {
            "application/json"
        }
    }

    fn read_response_body<F>(request: *mut c_void, on_event: &mut F) -> Result<(), String>
    where
        F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
    {
        let mut buffer = [0_u8; 8192];

        loop {
            let mut read = 0_u32;
            let ok = unsafe {
                WinHttpReadData(
                    request,
                    buffer.as_mut_ptr().cast::<c_void>(),
                    u32::try_from(buffer.len()).unwrap_or(8192),
                    &mut read,
                )
            };
            if ok == 0 {
                return Err(format!(
                    "Could not read upstream response body: {}",
                    last_error()
                ));
            }
            if read == 0 {
                break;
            }

            on_event(UpstreamStreamEvent::Chunk(&buffer[..read as usize]))?;
        }

        Ok(())
    }

    fn to_wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }

    fn last_error() -> String {
        format!("error {}", unsafe { GetLastError() })
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{
        curl_config_content, find_curl_header_end, parse_curl_response_meta, UpstreamResponseMeta,
        UpstreamStreamEvent,
    };
    use serde_json::Value;
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn post_json_stream<F>(
        url: &str,
        headers: &str,
        json_body: &Value,
        timeout_seconds: u16,
        mut on_event: F,
    ) -> Result<UpstreamResponseMeta, String>
    where
        F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
    {
        let body = serde_json::to_vec(json_body)
            .map_err(|err| format!("Could not serialize upstream request body: {err}"))?;
        let config_path = write_curl_config(url, headers, timeout_seconds)?;
        let mut child = Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--include",
                "--no-buffer",
                "--config",
            ])
            .arg(&config_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("Could not start curl for upstream request: {err}"))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Could not open curl stdin.".to_string())?;
        stdin
            .write_all(&body)
            .and_then(|_| stdin.flush())
            .map_err(|err| format!("Could not write upstream request body to curl: {err}"))?;
        drop(stdin);

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Could not open curl stdout.".to_string())?;
        let stderr_handle = child.stderr.take().map(|mut stderr| {
            thread::spawn(move || {
                let mut text = String::new();
                let _ = stderr.read_to_string(&mut text);
                text
            })
        });

        let stream_result = stream_curl_stdout(&mut stdout, &mut on_event);
        if stream_result.is_err() {
            let _ = child.kill();
        }
        let status = child
            .wait()
            .map_err(|err| format!("Could not wait for curl upstream request: {err}"))?;
        let _ = fs::remove_file(&config_path);
        let stderr = stderr_handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();

        let meta = stream_result?;
        if !status.success() {
            let detail = stderr.trim();
            if detail.is_empty() {
                return Err(format!(
                    "curl upstream request failed with status {status}."
                ));
            }
            return Err(format!("curl upstream request failed: {detail}"));
        }

        Ok(meta)
    }

    fn write_curl_config(
        url: &str,
        headers: &str,
        timeout_seconds: u16,
    ) -> Result<PathBuf, String> {
        let path = temporary_curl_config_path();
        let content = curl_config_content(url, headers, timeout_seconds);
        fs::write(&path, content)
            .map_err(|err| format!("Could not write temporary curl config: {err}"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("Could not secure temporary curl config: {err}"))?;
        Ok(path)
    }

    fn temporary_curl_config_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "codestudio-lite-curl-{}-{nanos}.conf",
            std::process::id()
        ))
    }

    fn stream_curl_stdout<F>(
        stdout: &mut impl Read,
        on_event: &mut F,
    ) -> Result<UpstreamResponseMeta, String>
    where
        F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
    {
        let (meta, initial_body) = read_curl_response_headers(stdout, on_event)?;
        if !initial_body.is_empty() {
            on_event(UpstreamStreamEvent::Chunk(&initial_body))?;
        }

        let mut buffer = [0_u8; 8192];
        loop {
            let read = stdout
                .read(&mut buffer)
                .map_err(|err| format!("Could not read curl upstream body: {err}"))?;
            if read == 0 {
                break;
            }
            on_event(UpstreamStreamEvent::Chunk(&buffer[..read]))?;
        }

        Ok(meta)
    }

    fn read_curl_response_headers<F>(
        stdout: &mut impl Read,
        on_event: &mut F,
    ) -> Result<(UpstreamResponseMeta, Vec<u8>), String>
    where
        F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
    {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4096];

        loop {
            if let Some(header_end) = find_curl_header_end(&buffer) {
                let (headers, rest) = buffer.split_at(header_end);
                let (meta, skip_block) = parse_curl_response_meta(headers)?;
                let rest = rest.to_vec();
                if skip_block {
                    buffer = rest;
                    continue;
                }
                on_event(UpstreamStreamEvent::Headers(meta))?;
                return Ok((meta, rest));
            }

            let read = stdout
                .read(&mut chunk)
                .map_err(|err| format!("Could not read curl upstream headers: {err}"))?;
            if read == 0 {
                return Err(
                    "curl ended before upstream response headers were received.".to_string()
                );
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.len() > 128 * 1024 {
                return Err("Upstream response headers are too large.".to_string());
            }
        }
    }

    #[allow(dead_code)]
    fn path_exists(path: &Path) -> bool {
        path.exists()
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod platform {
    use super::{UpstreamResponseMeta, UpstreamStreamEvent};
    use serde_json::Value;

    pub fn post_json_stream<F>(
        _url: &str,
        _headers: &str,
        _json_body: &Value,
        _timeout_seconds: u16,
        _on_event: F,
    ) -> Result<UpstreamResponseMeta, String>
    where
        F: FnMut(UpstreamStreamEvent<'_>) -> Result<(), String>,
    {
        Err("Upstream HTTP forwarding is not implemented on this platform yet.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_config_escapes_headers_without_losing_secrets() {
        let config = curl_config_content(
            "https://api.example.test/v1/messages",
            "Authorization: Bearer sk-test\r\nX-Title: Code\"Studio\r\n",
            15,
        );

        assert!(config.contains("request = \"POST\""));
        assert!(config.contains("url = \"https://api.example.test/v1/messages\""));
        assert!(config.contains("connect-timeout = 15"));
        assert!(config.contains("header = \"Authorization: Bearer sk-test\""));
        assert!(config.contains("header = \"X-Title: Code\\\"Studio\""));
        assert!(config.contains("data-binary = \"@-\""));
    }

    #[test]
    fn curl_response_meta_parses_status_and_sse_content_type() {
        let (meta, skip) = parse_curl_response_meta(
            b"HTTP/2 200\r\ncontent-type: text/event-stream; charset=utf-8\r\n\r\n",
        )
        .expect("headers should parse");

        assert_eq!(meta.status, 200);
        assert_eq!(meta.content_type, "text/event-stream");
        assert!(!skip);
    }

    #[test]
    fn curl_response_meta_skips_interim_header_blocks() {
        let (_, skip) = parse_curl_response_meta(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("interim headers should parse");
        assert!(skip);

        let (_, skip) = parse_curl_response_meta(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .expect("proxy headers should parse");
        assert!(skip);
    }

    #[test]
    fn curl_header_end_accepts_crlf_and_lf() {
        assert_eq!(
            find_curl_header_end(b"HTTP/2 200\r\nx: y\r\n\r\nbody"),
            Some(20)
        );
        assert_eq!(find_curl_header_end(b"HTTP/2 200\nx: y\n\nbody"), Some(17));
    }
}
