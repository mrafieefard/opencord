//! HTTPS to a server's media endpoints (Phase 2 plan §4.4), through the same
//! pinned certificate as its gateway. Requests are few and small (sound
//! files of at most 256 KB), so each opens its own connection: HTTP/1.1,
//! `Connection: close`, the response read up to a limit.

use std::sync::Arc;
use std::time::Duration;

use opencord_common::address::{Fingerprint, ServerAddress};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use crate::tofu::TlsTrust;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A whole request, uploads included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HttpError {
    #[error("could not reach the server: {0}")]
    Unreachable(String),
    #[error("the server's certificate changed")]
    Certificate,
    #[error("the answer is larger than {0} bytes")]
    TooLarge(usize),
    #[error("the server's answer could not be read")]
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// One request to the server at `address`, whose certificate must match
/// `pin`, or, without one, be vouched for by an authority. `bearer` goes in the Authorization header; at most `limit` bytes
/// of body are read.
pub async fn request(
    address: &ServerAddress,
    pin: Option<Fingerprint>,
    method: &str,
    path: &str,
    bearer: &str,
    body: &[u8],
    limit: usize,
) -> Result<HttpResponse, HttpError> {
    timeout(
        REQUEST_TIMEOUT,
        send(address, pin, method, path, bearer, body, limit),
    )
    .await
    .map_err(|_| HttpError::Unreachable("the request timed out".to_owned()))?
}

async fn send(
    address: &ServerAddress,
    pin: Option<Fingerprint>,
    method: &str,
    path: &str,
    bearer: &str,
    body: &[u8],
    limit: usize,
) -> Result<HttpResponse, HttpError> {
    let unreachable = |error: std::io::Error| HttpError::Unreachable(error.to_string());
    let trust = TlsTrust::new(pin);
    let tcp = timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((address.host.as_str(), address.port)),
    )
    .await
    .map_err(|_| HttpError::Unreachable("the connection timed out".to_owned()))?
    .map_err(unreachable)?;
    let server_name = ServerName::try_from(address.host.clone())
        .map_err(|_| HttpError::Unreachable(format!("{} is not a valid host", address.host)))?;
    let mut tls = TlsConnector::from(Arc::clone(&trust.config))
        .connect(server_name, tcp)
        .await
        .map_err(|error| match trust.verdict() {
            Some(crate::tofu::Verdict::Pinned(_) | crate::tofu::Verdict::Authority(_)) | None => {
                unreachable(error)
            }
            Some(_) => HttpError::Certificate,
        })?;
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Authorization: Bearer {bearer}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    tls.write_all(head.as_bytes()).await.map_err(unreachable)?;
    // A server that refuses early may close before reading all of it.
    let _ = tls.write_all(body).await;
    let _ = tls.flush().await;
    let mut response = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = match tls.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => read,
            // Servers often end TLS without a close_notify; what came is
            // what there is.
            Err(_) if !response.is_empty() => break,
            Err(error) => return Err(unreachable(error)),
        };
        response.extend_from_slice(&chunk[..read]);
        if response.len() > limit + 16 * 1024 {
            return Err(HttpError::TooLarge(limit));
        }
    }
    parse(&response, limit)
}

/// The status and body of a whole HTTP/1.1 response.
fn parse(response: &[u8], limit: usize) -> Result<HttpResponse, HttpError> {
    let end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(HttpError::Malformed)?;
    let head = std::str::from_utf8(&response[..end]).map_err(|_| HttpError::Malformed)?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or(HttpError::Malformed)?;
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.parse::<usize>().map_err(|_| HttpError::Malformed)?);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }
    let rest = &response[end + 4..];
    let body = if chunked {
        unchunk(rest)?
    } else {
        match length {
            Some(length) if length > rest.len() => return Err(HttpError::Malformed),
            Some(length) => rest[..length].to_vec(),
            None => rest.to_vec(),
        }
    };
    if body.len() > limit {
        return Err(HttpError::TooLarge(limit));
    }
    Ok(HttpResponse { status, body })
}

fn unchunk(mut rest: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    loop {
        let line_end = rest
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or(HttpError::Malformed)?;
        let size_text = std::str::from_utf8(&rest[..line_end]).map_err(|_| HttpError::Malformed)?;
        let size_text = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| HttpError::Malformed)?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Ok(body);
        }
        if rest.len() < size + 2 {
            return Err(HttpError::Malformed);
        }
        body.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_response_with_a_length_is_read_to_it() {
        let response = b"HTTP/1.1 201 Created\r\ncontent-type: x\r\nContent-Length: 5\r\n\r\nhello";

        assert_eq!(
            parse(response, 100),
            Ok(HttpResponse {
                status: 201,
                body: b"hello".to_vec()
            })
        );
    }

    #[test]
    fn a_chunked_response_is_joined() {
        let response =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;x=y\r\n world\r\n0\r\n\r\n";

        assert_eq!(parse(response, 100).unwrap().body, b"hello world");
    }

    #[test]
    fn a_body_over_the_limit_or_cut_short_is_refused() {
        let long = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let short = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello";

        assert_eq!(parse(long, 4), Err(HttpError::TooLarge(4)));
        assert_eq!(parse(short, 100), Err(HttpError::Malformed));
        assert_eq!(parse(b"garbage", 100), Err(HttpError::Malformed));
    }
}
