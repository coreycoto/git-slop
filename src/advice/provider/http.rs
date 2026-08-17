use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

const MAXIMUM_RESPONSE_HEADER_BYTES: usize = 65_536;

pub(super) struct Endpoint {
    host: String,
    port: u16,
    path: String,
}

impl Endpoint {
    pub(super) fn parse(value: &str) -> Result<Self> {
        let Some(rest) = value.strip_prefix("http://") else {
            bail!("provider_endpoint_unsupported: V1 requires an explicit http:// endpoint");
        };
        if rest.contains('@')
            || rest.contains('?')
            || rest.contains('#')
            || rest
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
        {
            bail!(
                "provider_endpoint_invalid: credentials, queries, fragments, whitespace, and controls are not allowed"
            );
        }
        let (authority, path) = rest
            .split_once('/')
            .map_or((rest, "/".to_string()), |(authority, path)| {
                (authority, format!("/{path}"))
            });
        if authority.is_empty() {
            bail!("provider_endpoint_invalid: endpoint host is empty");
        }
        let (host, port) = if authority.starts_with('[') {
            let close = authority
                .find(']')
                .ok_or_else(|| anyhow::anyhow!("provider_endpoint_invalid: malformed IPv6 host"))?;
            let host = authority[1..close].to_string();
            let suffix = &authority[close + 1..];
            let port = if suffix.is_empty() {
                80
            } else {
                parse_port(suffix.strip_prefix(':').ok_or_else(|| {
                    anyhow::anyhow!("provider_endpoint_invalid: malformed IPv6 authority")
                })?)?
            };
            (host, port)
        } else if let Some((host, port)) = authority.rsplit_once(':') {
            (host.to_string(), parse_port(port)?)
        } else {
            (authority.to_string(), 80)
        };
        let local = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback());
        if !local {
            bail!(
                "provider_remote_unsupported: endpoint host {host:?} is not loopback. Advisor V1 has no authenticated TLS transport, so remote endpoints are refused"
            );
        }
        Ok(Self { host, port, path })
    }

    pub(super) fn classification(&self) -> &'static str {
        "loopback"
    }

    fn host_header(&self) -> String {
        if self
            .host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_ipv6())
        {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub(super) fn post(
        &self,
        body: &[u8],
        connect_timeout: Duration,
        timeout: Duration,
        maximum: usize,
        resource_guard: Option<crate::advice::RuntimeResourceGuard>,
    ) -> Result<Vec<u8>> {
        let mut stream = connect_endpoint(self, connect_timeout)?;
        let started = Instant::now();
        let poll_timeout = timeout.min(Duration::from_millis(250));
        stream.set_read_timeout(Some(poll_timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.path,
            self.host_header(),
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .context("provider_timeout: request headers could not be written")?;
        stream
            .write_all(body)
            .context("provider_timeout: request body could not be written")?;
        let mut response = Vec::new();
        let response_limit = maximum.saturating_add(MAXIMUM_RESPONSE_HEADER_BYTES);
        loop {
            if let Some(guard) = resource_guard {
                crate::advice::resources::enforce_resource_guard(guard)?;
            }
            if started.elapsed() >= timeout {
                bail!(
                    "provider_timeout: model loading and generation did not finish within {} seconds",
                    timeout.as_secs_f64()
                );
            }
            let mut chunk = [0_u8; 8192];
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    if response.len().saturating_add(read) > response_limit {
                        bail!("provider_response_too_large: response exceeds configured bounds");
                    }
                    response.extend_from_slice(&chunk[..read]);
                    if !response.windows(4).any(|window| window == b"\r\n\r\n")
                        && response.len() > MAXIMUM_RESPONSE_HEADER_BYTES
                    {
                        bail!(
                            "provider_http_invalid: response headers exceed {MAXIMUM_RESPONSE_HEADER_BYTES} bytes"
                        );
                    }
                    if framed_response_complete(&response, maximum)? {
                        break;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => {
                    return Err(error).context(
                        "provider_unavailable: response connection failed before completion",
                    );
                }
            }
        }
        decode_response(&response, maximum)
    }
}

fn parse_port(value: &str) -> Result<u16> {
    let port = value.parse::<u16>().map_err(|_| {
        anyhow::anyhow!(
            "provider_endpoint_invalid: endpoint port must be an integer from 1 to 65535"
        )
    })?;
    if port == 0 {
        bail!("provider_endpoint_invalid: endpoint port must be between 1 and 65535");
    }
    Ok(port)
}

pub(super) fn probe_endpoint(value: &str, timeout: Duration) -> Result<()> {
    let endpoint = Endpoint::parse(value)?;
    let _stream = connect_endpoint(&endpoint, timeout)?;
    Ok(())
}

fn connect_endpoint(endpoint: &Endpoint, timeout: Duration) -> Result<TcpStream> {
    let addresses = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .context("provider_unavailable: endpoint resolution failed")?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        bail!("provider_unavailable: endpoint resolved to no addresses");
    }
    if addresses.iter().any(|address| !address.ip().is_loopback()) {
        bail!("provider_endpoint_invalid: loopback endpoint resolved to a non-loopback address");
    }
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(anyhow::anyhow!(
        "provider_unavailable: {}",
        last_error.map_or_else(
            || "connection failed".to_string(),
            |error| error.to_string()
        )
    ))
}

struct ResponseFraming {
    header_end: usize,
    status: u16,
    chunked: bool,
    content_length: Option<usize>,
    content_type: Option<String>,
}

fn response_framing(response: &[u8]) -> Result<Option<ResponseFraming>> {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let headers = std::str::from_utf8(&response[..header_end])
        .context("provider_http_invalid: response headers are not UTF-8")?;
    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: response status is missing"))?;
    if !status_line.starts_with("HTTP/1.0 ") && !status_line.starts_with("HTTP/1.1 ") {
        bail!("provider_http_invalid: response status line is malformed");
    }
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: response status is malformed"))?;
    let mut transfer_encodings = Vec::new();
    let mut content_encodings = Vec::new();
    let mut content_lengths = Vec::new();
    let mut content_type = None;
    for line in headers.lines().skip(1) {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: malformed response header"))?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            transfer_encodings.extend(
                value
                    .split(',')
                    .map(|encoding| encoding.trim().to_ascii_lowercase()),
            );
        } else if name.eq_ignore_ascii_case("content-encoding") {
            content_encodings.extend(
                value
                    .split(',')
                    .map(|encoding| encoding.trim().to_ascii_lowercase()),
            );
        } else if name.eq_ignore_ascii_case("content-length") {
            for length in value.split(',') {
                content_lengths.push(
                    length
                        .trim()
                        .parse::<usize>()
                        .context("provider_http_invalid: malformed content length")?,
                );
            }
        } else if name.eq_ignore_ascii_case("content-type") {
            if content_type.is_some() {
                bail!("provider_http_invalid: duplicate Content-Type headers");
            }
            content_type = Some(value.trim().to_ascii_lowercase());
        }
    }
    if !transfer_encodings.is_empty() && transfer_encodings.as_slice() != ["chunked"] {
        bail!("provider_http_unsupported: only an exact chunked transfer encoding is supported");
    }
    if content_encodings
        .iter()
        .any(|encoding| encoding != "identity")
    {
        bail!("provider_http_unsupported: compressed response bodies are not supported");
    }
    if content_lengths.windows(2).any(|pair| pair[0] != pair[1]) {
        bail!("provider_http_invalid: conflicting content lengths");
    }
    let chunked = !transfer_encodings.is_empty();
    if chunked && !content_lengths.is_empty() {
        bail!("provider_http_invalid: ambiguous transfer framing");
    }
    Ok(Some(ResponseFraming {
        header_end,
        status,
        chunked,
        content_length: content_lengths.first().copied(),
        content_type,
    }))
}

fn framed_response_complete(response: &[u8], maximum: usize) -> Result<bool> {
    let Some(framing) = response_framing(response)? else {
        return Ok(false);
    };
    if framing
        .content_length
        .is_some_and(|length| length > maximum)
    {
        bail!("provider_response_too_large: Content-Length exceeds {maximum} bytes");
    }
    let body = &response[framing.header_end + 4..];
    if framing.chunked {
        return chunked_frame_complete(body, maximum);
    }
    Ok(framing
        .content_length
        .is_some_and(|length| body.len() >= length))
}

fn chunked_frame_complete(mut body: &[u8], maximum: usize) -> Result<bool> {
    let mut decoded = 0_usize;
    loop {
        let Some(line_end) = body.windows(2).position(|window| window == b"\r\n") else {
            return Ok(false);
        };
        let size = parse_chunk_size(&body[..line_end])?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(
                body.starts_with(b"\r\n") || body.windows(4).any(|window| window == b"\r\n\r\n")
            );
        }
        decoded = decoded
            .checked_add(size)
            .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: chunk size overflow"))?;
        if decoded > maximum {
            bail!("provider_response_too_large: response exceeds {maximum} bytes");
        }
        let framed_size = size
            .checked_add(2)
            .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: chunk size overflow"))?;
        if body.len() < framed_size {
            return Ok(false);
        }
        if &body[size..framed_size] != b"\r\n" {
            bail!("provider_http_invalid: malformed chunked response");
        }
        body = &body[framed_size..];
    }
}

fn parse_chunk_size(line: &[u8]) -> Result<usize> {
    let size_text = std::str::from_utf8(line)?
        .split(';')
        .next()
        .unwrap_or_default();
    usize::from_str_radix(size_text.trim(), 16).context("provider_http_invalid: invalid chunk size")
}

fn decode_chunked(mut body: &[u8], maximum: usize) -> Result<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let Some(line_end) = body.windows(2).position(|window| window == b"\r\n") else {
            bail!("provider_http_invalid: malformed chunk header");
        };
        let size = parse_chunk_size(&body[..line_end])?;
        body = &body[line_end + 2..];
        if size == 0 {
            let terminal_length = if body.starts_with(b"\r\n") {
                2
            } else {
                body.windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|position| position + 4)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "provider_http_invalid: chunked response terminator is incomplete"
                        )
                    })?
            };
            if body.len() != terminal_length {
                bail!("provider_http_invalid: trailing bytes after chunked response");
            }
            break;
        }
        let framed_size = size
            .checked_add(2)
            .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: chunk size overflow"))?;
        if body.len() < framed_size || &body[size..framed_size] != b"\r\n" {
            bail!("provider_http_invalid: truncated chunked response");
        }
        if decoded.len().saturating_add(size) > maximum {
            bail!("provider_response_too_large: response exceeds {maximum} bytes");
        }
        decoded.extend_from_slice(&body[..size]);
        body = &body[framed_size..];
    }
    Ok(decoded)
}

fn decode_response(response: &[u8], maximum: usize) -> Result<Vec<u8>> {
    let framing = response_framing(response)?
        .ok_or_else(|| anyhow::anyhow!("provider_http_invalid: response headers are incomplete"))?;
    let raw_body = &response[framing.header_end + 4..];
    let body = if framing.chunked {
        decode_chunked(raw_body, maximum)?
    } else {
        if framing
            .content_length
            .is_some_and(|length| raw_body.len() != length)
        {
            bail!("provider_http_invalid: response body length does not match Content-Length");
        }
        if raw_body.len() > maximum {
            bail!("provider_response_too_large: response exceeds {maximum} bytes");
        }
        raw_body.to_vec()
    };
    if !(200..300).contains(&framing.status) {
        let diagnostic = String::from_utf8_lossy(&body);
        let bounded = diagnostic.chars().take(1000).collect::<String>();
        bail!(
            "provider_http_error: endpoint returned HTTP {}: {bounded}",
            framing.status
        );
    }
    let content_type = framing
        .content_type
        .as_deref()
        .and_then(|value| value.split(';').next())
        .unwrap_or_default();
    if content_type != "application/json" && !content_type.ends_with("+json") {
        bail!(
            "provider_http_unsupported: successful response Content-Type must be application/json or +json"
        );
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parser_keeps_loopback_and_host_headers_exact() {
        let local = Endpoint::parse("http://localhost:11434/v1/chat/completions")
            .expect("loopback endpoint");
        assert_eq!(local.classification(), "loopback");
        assert_eq!(local.host_header(), "localhost:11434");
        let ipv6 = Endpoint::parse("http://[::1]:11434/v1/chat/completions")
            .expect("IPv6 loopback endpoint");
        assert_eq!(ipv6.host_header(), "[::1]:11434");
        assert!(Endpoint::parse("http://localhost:0/v1/chat/completions").is_err());
        assert!(Endpoint::parse("http://localhost:nope/v1/chat/completions").is_err());
        assert!(Endpoint::parse("http://example.com/v1/chat/completions").is_err());
        assert!(Endpoint::parse("http://localhost\r\nX-Test: unsafe/path").is_err());
        assert!(Endpoint::parse("https://localhost/v1/chat/completions").is_err());
        assert!(Endpoint::parse("http://[::1]evil/v1/chat/completions").is_err());
    }

    #[test]
    fn framing_is_bounded_and_rejects_ambiguous_or_unsupported_encodings() {
        assert_eq!(
            decode_chunked(b"4\r\ntest\r\n0\r\n\r\n", 4).expect("chunked response"),
            b"test"
        );
        assert!(decode_chunked(b"4\r\ntest", 4).is_err());
        assert!(decode_chunked(b"4\r\ntest\r\n0\r\n\r\n", 3).is_err());
        assert!(!chunked_frame_complete(b"4\r\ntest\r\n0\r\n", 4).expect("partial frame"));
        assert!(
            response_framing(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\n\r\n"
            )
            .is_err()
        );
        assert!(
            response_framing(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n")
                .is_err()
        );
        assert!(
            response_framing(
                b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\n\r\n"
            )
            .is_err()
        );
        assert!(
            framed_response_complete(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n", 4).is_err()
        );
        assert!(
            decode_response(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{}",
                4
            )
            .is_err()
        );
    }
}
