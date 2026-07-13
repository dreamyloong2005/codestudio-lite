use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const DOWNLOAD_HTTP_MAX_ATTEMPTS: usize = 4;
const DOWNLOAD_HTTP_RETRY_DELAY_MS: u64 = 500;
const DOWNLOAD_HTTP_USER_AGENT: &str = "CodeStudio Lite";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownloadHttpTransport {
    PlatformDefault,
    Direct,
    MacosSystemProxy,
}

impl DownloadHttpTransport {
    fn label(self) -> &'static str {
        match self {
            Self::PlatformDefault => "platform default connection",
            Self::Direct => "direct connection",
            Self::MacosSystemProxy => "macOS system proxy",
        }
    }
}

pub(crate) fn download_http_transports(target_is_macos: bool) -> Vec<DownloadHttpTransport> {
    if target_is_macos {
        vec![
            DownloadHttpTransport::Direct,
            DownloadHttpTransport::MacosSystemProxy,
        ]
    } else {
        vec![DownloadHttpTransport::PlatformDefault]
    }
}

pub(crate) fn download_http_client(
    timeout: Duration,
    transport: DownloadHttpTransport,
) -> Result<Client, String> {
    let mut builder = Client::builder()
        .timeout(timeout)
        .user_agent(DOWNLOAD_HTTP_USER_AGENT)
        .use_rustls_tls();
    if transport == DownloadHttpTransport::Direct {
        builder = builder.no_proxy();
    }
    if cfg!(target_os = "macos") {
        builder = builder.http1_only();
    }
    builder
        .build()
        .map_err(|err| format!("Failed to create HTTP client: {err}"))
}

pub(crate) fn download_http_should_retry_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn download_http_retry_delay(attempt: usize) -> Duration {
    Duration::from_millis(
        (DOWNLOAD_HTTP_RETRY_DELAY_MS * attempt.max(1) as u64)
            .min(DOWNLOAD_HTTP_RETRY_DELAY_MS * DOWNLOAD_HTTP_MAX_ATTEMPTS as u64),
    )
}

fn transport_attempt_error(
    transport: DownloadHttpTransport,
    attempts: usize,
    detail: &str,
) -> String {
    format!("{} after {attempts} attempts: {detail}", transport.label())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownloadResponseMode {
    Truncate,
    Append,
    Complete,
}

pub(crate) fn download_response_mode(
    status: StatusCode,
    resume_offset: u64,
    expected_total: Option<u64>,
) -> Result<DownloadResponseMode, String> {
    if status == StatusCode::RANGE_NOT_SATISFIABLE
        && resume_offset > 0
        && expected_total == Some(resume_offset)
    {
        return Ok(DownloadResponseMode::Complete);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        return Ok(if resume_offset > 0 {
            DownloadResponseMode::Append
        } else {
            DownloadResponseMode::Truncate
        });
    }
    if status == StatusCode::OK {
        return Ok(DownloadResponseMode::Truncate);
    }
    Err(format!("HTTP {status}"))
}

pub(crate) fn fetch_text(
    url: &str,
    timeout: Duration,
    max_attempts: usize,
) -> Result<String, String> {
    let host = url_host(url);
    let attempts = max_attempts.max(1);
    let mut transport_errors = Vec::new();

    for transport in download_http_transports(cfg!(target_os = "macos")) {
        let client = match download_http_client(timeout, transport) {
            Ok(client) => client,
            Err(err) => {
                transport_errors.push(format!("{}: {err}", transport.label()));
                continue;
            }
        };
        let mut last_error = "unknown transfer failure".to_string();
        for attempt in 1..=attempts {
            match client.get(url).send() {
                Ok(response) if !response.status().is_success() => {
                    let status = response.status();
                    let detail = format!("HTTP {status}");
                    if !download_http_should_retry_status(status) {
                        return Err(format!(
                            "Failed to read {host} via {}: {detail}",
                            transport.label()
                        ));
                    }
                    last_error = detail;
                }
                Ok(response) => match response.text() {
                    Ok(text) => return Ok(text),
                    Err(err) => last_error = err.to_string(),
                },
                Err(err) => last_error = err.to_string(),
            }
            if attempt < attempts {
                thread::sleep(download_http_retry_delay(attempt));
            }
        }
        transport_errors.push(transport_attempt_error(transport, attempts, &last_error));
    }

    Err(format!(
        "Failed to read {host}: {}",
        transport_errors.join("; ")
    ))
}

pub(crate) fn download_to_file<F>(
    url: &str,
    path: &Path,
    temp: &Path,
    expected_total: Option<u64>,
    timeout: Duration,
    max_attempts: usize,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create download directory: {err}"))?;
    }
    if temp.exists() {
        let _ = fs::remove_file(temp);
    }

    let host = url_host(url);
    let attempts = max_attempts.max(1);
    let mut transport_errors = Vec::new();
    let mut completed = false;

    for transport in download_http_transports(cfg!(target_os = "macos")) {
        let client = match download_http_client(timeout, transport) {
            Ok(client) => client,
            Err(err) => {
                transport_errors.push(format!("{}: {err}", transport.label()));
                continue;
            }
        };
        let mut last_error = "unknown transfer failure".to_string();

        for attempt in 1..=attempts {
            let resume_offset = fs::metadata(temp)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let mut request = client.get(url);
            if resume_offset > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={resume_offset}-"));
            }

            let mut response = match request.send() {
                Ok(response) => response,
                Err(err) => {
                    last_error = err.to_string();
                    if attempt < attempts {
                        thread::sleep(download_http_retry_delay(attempt));
                    }
                    continue;
                }
            };
            let status = response.status();
            let mode = match download_response_mode(status, resume_offset, expected_total) {
                Ok(mode) => mode,
                Err(detail) if download_http_should_retry_status(status) => {
                    last_error = detail;
                    if attempt < attempts {
                        thread::sleep(download_http_retry_delay(attempt));
                    }
                    continue;
                }
                Err(detail) => {
                    let _ = fs::remove_file(temp);
                    return Err(format!(
                        "Failed to download {host} via {}: {detail}",
                        transport.label()
                    ));
                }
            };
            if mode == DownloadResponseMode::Complete {
                completed = true;
                break;
            }

            let append = mode == DownloadResponseMode::Append;
            let mut downloaded = if append { resume_offset } else { 0 };
            let total = expected_total.or_else(|| {
                response
                    .content_length()
                    .map(|remaining| downloaded.saturating_add(remaining))
            });
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(temp)
                .map_err(|err| format!("Failed to create download file: {err}"))?;
            let mut buffer = [0_u8; 64 * 1024];
            let mut last_emit = Instant::now() - Duration::from_secs(2);
            let mut read_failed = false;
            loop {
                let size = match response.read(&mut buffer) {
                    Ok(size) => size,
                    Err(err) => {
                        last_error =
                            format!("transfer interrupted after {downloaded} bytes: {err}");
                        read_failed = true;
                        break;
                    }
                };
                if size == 0 {
                    break;
                }
                file.write_all(&buffer[..size])
                    .map_err(|err| format!("Failed to write installer download: {err}"))?;
                downloaded = downloaded.saturating_add(size as u64);
                if last_emit.elapsed() >= Duration::from_millis(500) {
                    on_progress(downloaded, total);
                    last_emit = Instant::now();
                }
            }
            file.flush()
                .map_err(|err| format!("Failed to finish installer download: {err}"))?;
            drop(file);

            if !read_failed {
                let final_size = fs::metadata(temp)
                    .map_err(|err| format!("Failed to inspect installer download: {err}"))?
                    .len();
                if let Some(total) = total {
                    if final_size != total {
                        last_error =
                            format!("transfer ended at {final_size} bytes; expected {total} bytes");
                    } else {
                        completed = true;
                    }
                } else {
                    completed = true;
                }
                if completed {
                    break;
                }
            }
            if attempt < attempts {
                thread::sleep(download_http_retry_delay(attempt));
            }
        }

        if completed {
            break;
        }
        transport_errors.push(transport_attempt_error(transport, attempts, &last_error));
    }

    if !completed {
        let _ = fs::remove_file(temp);
        return Err(format!(
            "Failed to download {host}: {}",
            transport_errors.join("; ")
        ));
    }

    let downloaded = fs::metadata(temp)
        .map_err(|err| format!("Failed to inspect completed download: {err}"))?
        .len();
    on_progress(downloaded, expected_total.or(Some(downloaded)));
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            format!(
                "Failed to replace staged download {}: {err}",
                path.display()
            )
        })?;
    }
    fs::rename(temp, path).map_err(|err| {
        let _ = fs::remove_file(temp);
        format!(
            "Failed to save downloaded file to {}: {err}",
            path.display()
        )
    })?;
    Ok(downloaded)
}

fn url_host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_downloads_try_direct_then_system_proxy_without_changing_other_platforms() {
        assert_eq!(
            download_http_transports(true),
            vec![
                DownloadHttpTransport::Direct,
                DownloadHttpTransport::MacosSystemProxy,
            ]
        );
        assert_eq!(
            download_http_transports(false),
            vec![DownloadHttpTransport::PlatformDefault]
        );
    }

    #[test]
    fn download_retry_policy_is_bounded_and_only_retries_transient_statuses() {
        assert_eq!(DOWNLOAD_HTTP_MAX_ATTEMPTS, 4);
        assert!(download_http_should_retry_status(
            StatusCode::REQUEST_TIMEOUT
        ));
        assert!(download_http_should_retry_status(
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(download_http_should_retry_status(StatusCode::BAD_GATEWAY));
        assert!(!download_http_should_retry_status(StatusCode::NOT_FOUND));
    }

    #[test]
    fn transport_errors_identify_direct_and_macos_proxy_attempts() {
        assert_eq!(
            transport_attempt_error(DownloadHttpTransport::Direct, 4, "connection reset"),
            "direct connection after 4 attempts: connection reset"
        );
        assert_eq!(
            transport_attempt_error(
                DownloadHttpTransport::MacosSystemProxy,
                4,
                "proxy unavailable"
            ),
            "macOS system proxy after 4 attempts: proxy unavailable"
        );
    }

    #[test]
    fn resumed_download_response_modes_never_append_a_full_response() {
        assert_eq!(
            download_response_mode(StatusCode::OK, 1024, Some(2048)).unwrap(),
            DownloadResponseMode::Truncate
        );
        assert_eq!(
            download_response_mode(StatusCode::PARTIAL_CONTENT, 1024, Some(2048)).unwrap(),
            DownloadResponseMode::Append
        );
        assert_eq!(
            download_response_mode(StatusCode::RANGE_NOT_SATISFIABLE, 2048, Some(2048)).unwrap(),
            DownloadResponseMode::Complete
        );
        assert!(download_response_mode(StatusCode::NOT_FOUND, 0, Some(2048)).is_err());
    }

    #[test]
    fn cargo_enables_macos_system_proxy_without_replacing_rustls() {
        let cargo_manifest = include_str!("../../Cargo.toml");
        assert!(cargo_manifest.contains("\"rustls-tls\""));
        assert!(cargo_manifest.contains("\"macos-system-configuration\""));
    }

    #[test]
    fn app_owned_download_callers_use_the_shared_transport() {
        let chatgpt = include_str!("chatgpt_desktop.rs");
        let chatgpt_fetch = chatgpt
            .split("fn fetch_text")
            .nth(1)
            .and_then(|body| body.split("fn download_to_file").next())
            .expect("ChatGPT metadata fetch should exist");
        let chatgpt_download = chatgpt
            .split("fn download_to_file")
            .nth(1)
            .and_then(|body| body.split("fn emit_step_progress").next())
            .expect("ChatGPT package download should exist");

        let installer = include_str!("tool_installer.rs");
        let claude_metadata = installer
            .split("fn read_claude_desktop_latest_metadata")
            .nth(1)
            .and_then(|body| body.split("fn claude_desktop_macos_dmg_url").next())
            .expect("Claude metadata fetch should exist");
        let claude_download = installer
            .split("fn download_url_to_file")
            .nth(1)
            .and_then(|body| body.split("fn sha256_file").next())
            .expect("Claude package download should exist");

        let detector = include_str!("detector.rs");
        let detector_fetch = detector
            .split("fn read_claude_desktop_latest_version_from_url")
            .nth(1)
            .and_then(|body| body.split("fn normalized_version_label").next())
            .expect("Claude update detection fetch should exist");

        for body in [
            chatgpt_fetch,
            chatgpt_download,
            claude_metadata,
            claude_download,
            detector_fetch,
        ] {
            assert!(body.contains("download_http::"));
            assert!(!body.contains("reqwest::blocking::Client::builder"));
        }
    }
}
