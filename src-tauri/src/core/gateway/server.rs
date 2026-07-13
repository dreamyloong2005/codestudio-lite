use std::collections::HashMap;
use std::io::Read;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;

pub(in crate::core::gateway) fn spawn_accept_loop<C, E>(
    listener: TcpListener,
    shutdown: Receiver<()>,
    on_connection: C,
    on_error: E,
) -> JoinHandle<()>
where
    C: Fn(TcpStream) + Send + Sync + 'static,
    E: Fn(String) + Send + Sync + 'static,
{
    let on_connection = Arc::new(on_connection);
    thread::spawn(move || loop {
        if shutdown.try_recv().is_ok() {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let on_connection = Arc::clone(&on_connection);
                thread::spawn(move || on_connection(stream));
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(35));
            }
            Err(err) => {
                on_error(format!("Gateway accept failed: {err}"));
                thread::sleep(Duration::from_millis(100));
            }
        }
    })
}

pub(in crate::core::gateway) struct HttpRequest {
    pub(in crate::core::gateway) method: String,
    pub(in crate::core::gateway) path: String,
    pub(in crate::core::gateway) headers: HashMap<String, String>,
    pub(in crate::core::gateway) body: Vec<u8>,
}

pub(in crate::core::gateway) struct HttpResponse {
    pub(in crate::core::gateway) status: u16,
    pub(in crate::core::gateway) reason: &'static str,
    pub(in crate::core::gateway) content_type: &'static str,
    pub(in crate::core::gateway) headers: Vec<(&'static str, &'static str)>,
    pub(in crate::core::gateway) body: Vec<u8>,
}

pub(in crate::core::gateway) enum RouteResponse {
    Buffered(HttpResponse),
    Stream(StreamingResponse),
}

impl RouteResponse {
    pub(in crate::core::gateway) fn status(&self) -> u16 {
        match self {
            Self::Buffered(response) => response.status,
            Self::Stream(response) => response.expected_status,
        }
    }
}

pub(in crate::core::gateway) struct StreamingResponse {
    pub(in crate::core::gateway) expected_status: u16,
    pub(in crate::core::gateway) run: Box<dyn FnOnce(&mut TcpStream) -> Result<u16, String> + Send>,
}

pub(in crate::core::gateway) fn write_buffered_response(
    stream: &mut TcpStream,
    response: HttpResponse,
) -> Result<u16, String> {
    use std::io::Write;
    let status = response.status;
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.reason,
        response.content_type,
        response.body.len()
    );
    append_cors_headers(&mut head);
    for (name, value) in response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(&response.body))
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())?;
    Ok(status)
}

pub(in crate::core::gateway) fn write_route_response(
    stream: &mut TcpStream,
    response: RouteResponse,
) -> Result<u16, String> {
    match response {
        RouteResponse::Buffered(response) => write_buffered_response(stream, response),
        RouteResponse::Stream(response) => (response.run)(stream),
    }
}

pub(in crate::core::gateway) fn write_stream_headers(
    stream: &mut TcpStream,
    status: u16,
    reason: &'static str,
    content_type: &'static str,
) -> Result<(), String> {
    use std::io::Write;
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nCache-Control: no-cache\r\nConnection: close\r\nX-Accel-Buffering: no\r\n",
        status, reason, content_type
    );
    append_cors_headers(&mut head);
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|err| err.to_string())
}

pub(in crate::core::gateway) fn append_cors_headers(head: &mut String) {
    head.push_str("Access-Control-Allow-Origin: *\r\n");
    head.push_str("Access-Control-Allow-Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS\r\n");
    head.push_str("Access-Control-Allow-Headers: *\r\n");
}

pub(in crate::core::gateway) fn read_request(
    stream: &mut TcpStream,
) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let read = stream.read(&mut chunk).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_REQUEST_BYTES {
            return Err("Request is too large".to_string());
        }
        if header_end.is_none() {
            if let Some(index) = find_header_end(&buffer) {
                header_end = Some(index + 4);
                content_length = parse_content_length(&String::from_utf8_lossy(&buffer[..index]));
            }
        }
        if header_end.is_some_and(|end| buffer.len() >= end + content_length) {
            break;
        }
    }

    let header_end = header_end.ok_or_else(|| "Invalid HTTP request".to_string())?;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let mut request_line = lines
        .next()
        .ok_or_else(|| "Missing HTTP request line".to_string())?
        .split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    let end = header_end + content_length;
    let body = buffer
        .get(header_end..end.min(buffer.len()))
        .unwrap_or_default()
        .to_vec();

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> usize {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}
