use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cache::DiskCache;

/// A minimal HTTP response.
pub struct HttpResponse {
    /// Whether the status code indicates success (2xx).
    pub ok: bool,
    /// The HTTP status code. `0` indicates a connection-level failure.
    pub status: u16,
    /// The response body as a UTF-8 string.
    pub body: String,
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
        log::debug!("GET {url}");
        let result = self.agent.get(url).call();
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
                let ok = (200..300).contains(&status);
                let body = resp
                    .into_body()
                    .read_to_string()
                    .unwrap_or_default();
                Ok(HttpResponse { ok, status, body })
            }
            Err(e) => {
                log::warn!("HTTP request failed: {e}");
                Ok(HttpResponse {
                    ok: false,
                    status: 0,
                    body: String::new(),
                })
            }
        }
    }
}
