use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::cache::DiskCache;

/// A minimal HTTP response.
pub struct HttpResponse {
    /// Whether the status code indicates success (2xx) or 304 Not Modified.
    pub ok: bool,
    /// The HTTP status code. `0` indicates a connection-level failure.
    pub status: u16,
    /// The response body as a UTF-8 string. Empty for 304.
    pub body: String,
    /// Server-provided ETag if the response carried one. Lets callers
    /// short-circuit subsequent requests with `If-None-Match`.
    pub etag: Option<String>,
}

impl HttpResponse {
    /// Attempt to parse the response body as JSON.
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.body).ok()
    }
}

/// Synchronous HTTP client with optional disk caching.
#[derive(Clone)]
pub struct HttpClient {
    cache: DiskCache,
    agent: ureq::Agent,
}

impl HttpClient {
    /// Create a new `HttpClient` that stores cached responses under
    /// `cache_dir`.
    pub fn new(cache_dir: PathBuf) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(concat!("Cartridge/", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        Self {
            cache: DiskCache::new(cache_dir),
            agent,
        }
    }

    /// Perform an HTTP GET request.
    pub fn get(&self, url: &str) -> Result<HttpResponse, String> {
        self.get_with_etag(url, None)
    }

    /// Perform an HTTP GET with an optional `If-None-Match` ETag.
    /// If the server returns 304 Not Modified, the response will have
    /// `ok=true`, `status=304`, and an empty body — callers can keep
    /// using their previously cached value.
    pub fn get_with_etag(&self, url: &str, etag: Option<&str>) -> Result<HttpResponse, String> {
        log::debug!("GET {url} (etag: {etag:?})");
        let req = self.agent.get(url);
        let result = if let Some(e) = etag {
            req.header("If-None-Match", e).call()
        } else {
            req.call()
        };
        Self::into_response(result)
    }

    /// Perform a cached HTTP GET. If a valid cached response exists (younger
    /// than `ttl_seconds`), it is returned without hitting the network.
    pub fn get_cached(&self, url: &str, ttl_seconds: u64) -> Result<HttpResponse, String> {
        if let Some(body) = self.cache.get(url, ttl_seconds) {
            log::debug!("Cache hit for {url}");
            return Ok(HttpResponse {
                ok: true,
                status: 200,
                body,
                etag: None,
            });
        }

        let response = self.get(url)?;
        if response.ok {
            self.cache.put(url, &response.body);
        }
        Ok(response)
    }

    /// Perform an HTTP POST with a string body (sent as `application/json`).
    pub fn post(&self, url: &str, body: &str) -> Result<HttpResponse, String> {
        log::debug!("POST {url}");
        let result = self
            .agent
            .post(url)
            .header("Content-Type", "application/json")
            .send(body.as_bytes());
        Self::into_response(result)
    }

    /// Download an HTTPS URL, with a 32 MiB streaming limit. The destination
    /// must not exist; failed downloads are removed.
    pub fn download(&self, url: &str, dest: &Path) -> Result<(), String> {
        self.download_bounded(url, dest, 32 * 1024 * 1024)
            .map(|_| ())
    }

    /// Used by the installer with the exact signed package size as the limit.
    pub(crate) fn download_bounded(
        &self,
        url: &str,
        dest: &Path,
        limit: u64,
    ) -> Result<u64, String> {
        let local_test = validate_artifact_url(url)?;
        let response = self
            .agent
            .get(url)
            .config()
            // Enforced on every redirect, including GitHub asset redirects.
            .https_only(!local_test)
            // A weak handheld Wi-Fi link may need longer for a bounded archive.
            .timeout_global(Some(std::time::Duration::from_secs(120)))
            // Unit-test HTTP cannot redirect out of the loopback server.
            .max_redirects(if local_test { 0 } else { 5 })
            .build()
            .header("Accept-Encoding", "identity")
            .call()
            .map_err(|e| format!("Download request failed: {e}"))?;
        if response.status().as_u16() != 200 {
            return Err(format!("Download HTTP status {}", response.status()));
        }
        if let Some(encoding) = response.headers().get("content-encoding") {
            if encoding.to_str().ok() != Some("identity") {
                return Err("Encoded download body is not a package artifact".into());
            }
        }
        if let Some(length) = response.headers().get("content-length") {
            let length = length
                .to_str()
                .ok()
                .and_then(|n| n.parse::<u64>().ok())
                .ok_or("Invalid download Content-Length")?;
            if length > limit {
                return Err("Download exceeds package size limit".into());
            }
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Download directory: {e}"))?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)
            .map_err(|e| format!("Create download: {e}"))?;
        let result = (|| {
            let mut reader = response.into_body().into_reader();
            let mut buffer = [0u8; 16 * 1024];
            let mut size = 0u64;
            loop {
                // Never buffer the complete package, even without Content-Length.
                let count = reader
                    .read(&mut buffer)
                    .map_err(|e| format!("Download body: {e}"))?;
                if count == 0 {
                    break;
                }
                size = size
                    .checked_add(count as u64)
                    .ok_or("Download size overflow")?;
                if size > limit {
                    return Err("Download exceeds package size limit".into());
                }
                file.write_all(&buffer[..count])
                    .map_err(|e| format!("Write download: {e}"))?;
            }
            file.sync_all().map_err(|e| format!("Sync download: {e}"))?;
            Ok(size)
        })();
        if result.is_err() {
            let _ = fs::remove_file(dest);
        }
        result
    }

    /// Convert a ureq result into our `HttpResponse`, handling errors
    /// gracefully by returning a response with `ok = false` rather than
    /// propagating the error.
    fn into_response(
        result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<HttpResponse, String> {
        match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                // Treat 304 Not Modified as success. Body is empty.
                let ok = (200..300).contains(&status) || status == 304;
                let etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let body = if status == 304 {
                    String::new()
                } else {
                    resp.into_body()
                        .with_config()
                        .limit(4 * 1024 * 1024)
                        .read_to_string()
                        .map_err(|e| format!("Response body failed (4 MiB limit): {e}"))?
                };
                Ok(HttpResponse {
                    ok,
                    status,
                    body,
                    etag,
                })
            }
            Err(e) => {
                log::warn!("HTTP request failed: {e}");
                Ok(HttpResponse {
                    ok: false,
                    status: 0,
                    body: String::new(),
                    etag: None,
                })
            }
        }
    }
}

/// No environment variable, feature flag, or public option can disable HTTPS.
/// Only this crate's unit tests may serve artifacts over literal loopback HTTP.
pub(crate) fn validate_artifact_url(url: &str) -> Result<bool, String> {
    let uri: ureq::http::Uri = url.parse().map_err(|_| "Invalid artifact URL")?;
    let authority = uri.authority().ok_or("Artifact URL needs a host")?;
    if authority.as_str().contains('@') || url.contains('#') {
        return Err("Artifact URL must not contain credentials or a fragment".into());
    }
    if uri.scheme_str() == Some("https") {
        return Ok(false);
    }
    #[cfg(test)]
    if uri.scheme_str() == Some("http")
        && matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]"))
    {
        return Ok(true);
    }
    Err("Artifact URL must use HTTPS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::time::Duration;

    fn reply(headers: &str, body: &str) -> Result<HttpResponse, String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let message = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n{body}");
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "HTTP client did not connect"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("HTTP accept failed: {e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).unwrap();
            // A limit rejection may close the connection before all bytes send.
            let _ = socket.write_all(message.as_bytes());
        });
        let result = HttpClient::new(std::env::temp_dir().join("unused-http-test-cache"))
            .get(&format!("http://{address}/"));
        server.join().unwrap();
        result
    }

    #[test]
    fn reports_body_failures_instead_of_success_with_empty_text() {
        let normal = reply("Content-Length: 2\r\nETag: sample\r\n", "ok").unwrap();
        assert!(normal.ok);
        assert_eq!(normal.body, "ok");
        assert_eq!(normal.etag.as_deref(), Some("sample"));
        assert!(
            reply("Content-Length: 4194305\r\n", "").is_err(),
            "oversized body accepted"
        );
        assert!(
            reply("Content-Length: 100\r\n", "short").is_err(),
            "truncated body accepted"
        );
    }
}
