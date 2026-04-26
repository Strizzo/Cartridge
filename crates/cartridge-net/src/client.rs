use std::fs;
use std::io::Read;
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
pub struct HttpClient {
    cache: DiskCache,
    agent: ureq::Agent,
}

impl HttpClient {
    /// Create a new `HttpClient` that stores cached responses under
    /// `cache_dir`.
    pub fn new(cache_dir: PathBuf) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent("Cartridge/0.1.0")
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

    /// Download a URL to a local file path.
    pub fn download(&self, url: &str, dest: &Path) -> Result<(), String> {
        log::debug!("Downloading {url} -> {}", dest.display());

        // Ensure parent directory exists.
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
        }

        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("Download request failed for {url}: {e}"))?;

        let mut reader = response.into_body().into_reader();
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read download body for {url}: {e}"))?;

        fs::write(dest, &bytes)
            .map_err(|e| format!("Failed to write file {}: {e}", dest.display()))?;

        Ok(())
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
                    resp.into_body().read_to_string().unwrap_or_default()
                };
                Ok(HttpResponse { ok, status, body, etag })
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
