use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use url::Url;

pub const VIEW_WIDTH: u32 = 720;
pub const VIEW_HEIGHT: u32 = 576;
pub const VIEW_Y: i32 = 100;
pub const MAX_URL: usize = 4096;
pub const ZOOM_LEVELS: &[u16] = &[60, 75, 90, 100, 110, 120, 130, 140, 150, 160, 175, 200];
pub const DEFAULT_ZOOM: u16 = 100;
const MAX_SITE_VIEWS: usize = 128;

pub fn site_key(address: &str) -> Option<String> {
    let url = Url::parse(address).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.origin().ascii_serialization())
}

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub site_zoom: BTreeMap<String, u16>,
}
impl Library {
    pub fn load(path: &Path) -> Self {
        let mut library: Self = std::fs::File::open(path)
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
                Ok(bytes)
            })
            .ok()
            .filter(|b| b.len() < 1024 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        library.bookmarks.retain(|b| address(&b.url).is_ok());
        library.history.retain(|b| address(&b.url).is_ok());
        library.bookmarks.truncate(64);
        library.history.truncate(100);
        library.site_zoom.retain(|origin, zoom| {
            site_key(origin).as_deref() == Some(origin.as_str()) && ZOOM_LEVELS.contains(zoom)
        });
        while library.site_zoom.len() > MAX_SITE_VIEWS {
            library.site_zoom.pop_first();
        }
        library
    }
    pub fn zoom_for(&self, url: &str) -> u16 {
        site_key(url)
            .and_then(|key| self.site_zoom.get(&key).copied())
            .unwrap_or(DEFAULT_ZOOM)
    }
    pub fn remember_zoom(&mut self, url: &str, zoom: u16) -> bool {
        let Some(key) = site_key(url).filter(|_| ZOOM_LEVELS.contains(&zoom)) else {
            return false;
        };
        if self.site_zoom.get(&key) == Some(&zoom) {
            return false;
        }
        if self.site_zoom.len() >= MAX_SITE_VIEWS && !self.site_zoom.contains_key(&key) {
            self.site_zoom.pop_first();
        }
        self.site_zoom.insert(key, zoom);
        true
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
    fn zoom_is_per_origin_and_old_libraries_keep_bookmarks() {
        let mut library: Library = serde_json::from_str(
            r#"{"bookmarks":[{"title":"saved","url":"https://example.org/a"}],"history":[]}"#,
        )
        .unwrap();
        assert_eq!(library.bookmarks.len(), 1);
        assert_eq!(library.zoom_for("https://example.org/a"), 100);
        assert!(library.remember_zoom("https://example.org/a?q=x#y", 130));
        assert_eq!(library.zoom_for("https://example.org/other"), 130);
        assert_eq!(library.zoom_for("http://example.org/"), 100);
        assert_eq!(library.zoom_for("https://example.org:444/"), 100);
        assert!(!library.remember_zoom("file:///private", 130));
        assert!(!library.remember_zoom("https://example.org/", 999));
        let saved = serde_json::to_string(&library).unwrap();
        let restored: Library = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.zoom_for("https://example.org/"), 130);
        for index in 0..200 {
            library.remember_zoom(&format!("https://s{index}.example.org/"), 110);
        }
        assert_eq!(library.site_zoom.len(), MAX_SITE_VIEWS);
    }
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
