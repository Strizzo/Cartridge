use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

pub const VIEW_WIDTH: u32 = 720;
pub const VIEW_HEIGHT: u32 = 576;
pub const VIEW_Y: i32 = 100;
pub const MAX_URL: usize = 4096;

pub fn address(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() || input.len() > MAX_URL || input.chars().any(char::is_control) {
        return Err("Enter a web address or search terms (at most 4096 bytes).".into());
    }
    let candidate = if input.contains("://") {
        input.to_string()
    } else if input.starts_with("localhost:") || input.starts_with("127.0.0.1:") {
        format!("http://{input}")
    } else if !input.contains(' ') && input.contains('.') {
        format!("https://{input}")
    } else if input.contains(':') && !input.contains(' ') {
        return Err("Only http:// and https:// web addresses are supported.".into());
    } else {
        let mut url = Url::parse("https://duckduckgo.com/").unwrap();
        url.query_pairs_mut().append_pair("q", input);
        return Ok(url.to_string());
    };
    let url = Url::parse(&candidate).map_err(|_| "That web address is not valid.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an http:// or https:// address without embedded credentials.".into());
    }
    Ok(url.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<Bookmark>,
}
impl Library {
    pub fn load(path: &Path) -> Self {
        let mut library: Self = std::fs::read(path)
            .ok()
            .filter(|b| b.len() < 1024 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        library.bookmarks.retain(|b| address(&b.url).is_ok());
        library.history.retain(|b| address(&b.url).is_ok());
        library.bookmarks.truncate(64);
        library.history.truncate(100);
        library
    }
    pub fn remember(&mut self, title: &str, url: &str) {
        if address(url).is_err() {
            return;
        }
        let base = url.split('#').next().unwrap_or(url);
        self.history
            .retain(|b| b.url.split('#').next().unwrap_or(&b.url) != base);
        self.history.insert(
            0,
            Bookmark {
                title: title.chars().take(160).collect(),
                url: url.into(),
            },
        );
        self.history.truncate(100);
    }
    pub fn toggle_bookmark(&mut self, title: &str, url: &str) {
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
        } else if address(url).is_ok() && self.bookmarks.len() < 64 {
            self.bookmarks.push(Bookmark {
                title: title.chars().take(160).collect(),
                url: url.into(),
            });
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("Missing browser data directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = path.with_extension("tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_and_search_are_distinct() {
        assert_eq!(
            address("example.com/a?q=x").unwrap(),
            "https://example.com/a?q=x"
        );
        assert_eq!(
            address("http://localhost:1234/a").unwrap(),
            "http://localhost:1234/a"
        );
        assert_eq!(
            address("localhost:1234/a").unwrap(),
            "http://localhost:1234/a"
        );
        assert_eq!(
            address("127.0.0.1:8765/").unwrap(),
            "http://127.0.0.1:8765/"
        );
        assert!(
            address("game consoles")
                .unwrap()
                .contains("q=game+consoles")
        );
    }
    #[test]
    fn privileged_schemes_and_credentials_are_not_addresses() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "chrome://settings",
            "data:text/html,x",
            "https://u:p@example.org",
            "http://",
            "",
            "https://example.org/\nfoo",
        ] {
            assert!(address(url).is_err(), "accepted {url}");
        }
    }
    #[test]
    fn bookmarks_and_history_are_bounded_and_deduplicated() {
        let mut library = Library::default();
        for i in 0..120 {
            library.remember("A", &format!("https://example.org/{i}"));
        }
        assert_eq!(library.history.len(), 100);
        library.remember("New title", "https://example.org/119");
        assert_eq!(library.history.len(), 100);
        library.toggle_bookmark("A", "https://example.org/");
        assert_eq!(library.bookmarks.len(), 1);
        library.toggle_bookmark("A", "https://example.org/");
        assert!(library.bookmarks.is_empty());
    }
}
