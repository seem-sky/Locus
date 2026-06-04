use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct HeadroomProxyClient {
    base_url: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomProxyHealth {
    pub available: bool,
    pub status: String,
    pub error: Option<String>,
}

impl HeadroomProxyClient {
    pub fn from_settings() -> Self {
        Self::new(crate::headroom::settings::base_url())
    }

    pub fn new(base_url: String) -> Self {
        Self {
            base_url: normalize_loopback_base_url(base_url.trim_end_matches('/')),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn health(&self) -> HeadroomProxyHealth {
        for path in ["/livez", "/health"] {
            match self.probe(path) {
                Ok(()) => {
                    return HeadroomProxyHealth {
                        available: true,
                        status: "healthy".to_string(),
                        error: None,
                    };
                }
                Err(error) => {
                    if path == "/health" {
                        return HeadroomProxyHealth {
                            available: false,
                            status: "unavailable".to_string(),
                            error: Some(error),
                        };
                    }
                }
            }
        }
        HeadroomProxyHealth {
            available: false,
            status: "unavailable".to_string(),
            error: Some("headroom proxy health probe failed".to_string()),
        }
    }

    fn probe(&self, path: &str) -> Result<(), String> {
        let url = format!("{}{}", self.base_url, path);
        let status = http_get_status_code(&url, HEALTH_PROBE_TIMEOUT)
            .map_err(|error| probe_error_message(&url, &error))?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(format!(
                "headroom proxy probe {} returned HTTP {status}",
                url
            ))
        }
    }
}

fn http_get_status_code(url: &str, timeout: Duration) -> Result<u16, String> {
    let parsed = url::Url::parse(url).map_err(|error| error.to_string())?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "headroom proxy URL missing host".to_string())?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "headroom proxy URL missing port".to_string())?;
    let request_path = match parsed.query() {
        Some(query) => format!("{}?{query}", parsed.path()),
        None => parsed.path().to_string(),
    };
    let request_path = if request_path.is_empty() {
        "/".to_string()
    } else {
        request_path
    };

    let default_port = match parsed.scheme() {
        "https" => 443,
        _ => 80,
    };
    let host_header = if port == default_port {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };

    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?;
    let mut last_error = String::from("no socket addresses resolved");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(mut stream) => {
                let _ = stream.set_read_timeout(Some(timeout));
                let _ = stream.set_write_timeout(Some(timeout));
                let request = format!(
                    "GET {request_path} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nUser-Agent: locus-headroom-probe\r\n\r\n"
                );
                stream
                    .write_all(request.as_bytes())
                    .map_err(|error| error.to_string())?;
                let mut buf = [0u8; 512];
                let read = stream.read(&mut buf).map_err(|error| error.to_string())?;
                let response = String::from_utf8_lossy(&buf[..read]);
                return parse_http_status_code(&response);
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }
    }
    Err(last_error)
}

fn parse_http_status_code(response: &str) -> Result<u16, String> {
    let status_line = response
        .lines()
        .next()
        .ok_or_else(|| "empty HTTP response from headroom proxy".to_string())?;
    let mut parts = status_line.split_whitespace();
    let _version = parts
        .next()
        .ok_or_else(|| format!("invalid HTTP status line: {status_line}"))?;
    let code = parts
        .next()
        .ok_or_else(|| format!("invalid HTTP status line: {status_line}"))?
        .parse::<u16>()
        .map_err(|error| format!("invalid HTTP status code in {status_line}: {error}"))?;
    Ok(code)
}

fn normalize_loopback_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim();
    let lower = trimmed.to_ascii_lowercase();
    for (prefix, replacement) in [
        ("http://localhost", "http://127.0.0.1"),
        ("https://localhost", "https://127.0.0.1"),
        ("http://[::1]", "http://127.0.0.1"),
        ("https://[::1]", "https://127.0.0.1"),
    ] {
        if lower.starts_with(prefix) {
            return format!("{replacement}{}", &trimmed[prefix.len()..]);
        }
    }
    trimmed.to_string()
}

fn probe_error_message(url: &str, error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("10061")
        || lower.contains("failed to connect")
        || lower.contains("target machine actively refused")
        || lower.contains("os error 10061")
    {
        return format!("headroom proxy is not listening at {url}");
    }
    format!("headroom proxy probe failed for {url}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_loopback_to_ipv4() {
        assert_eq!(
            normalize_loopback_base_url("http://localhost:8787"),
            "http://127.0.0.1:8787"
        );
        assert_eq!(
            normalize_loopback_base_url("http://[::1]:8787"),
            "http://127.0.0.1:8787"
        );
    }

    #[test]
    fn parse_http_status_code_reads_first_line() {
        assert_eq!(
            parse_http_status_code("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap(),
            200
        );
        assert_eq!(
            parse_http_status_code("HTTP/1.0 503 Service Unavailable").unwrap(),
            503
        );
    }
}
