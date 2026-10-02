use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

use crate::{config, security};

const MAX_HTTP_BODY: u64 = 256 * 1024;
const CACHE_TTL_SECS: u64 = 30 * 24 * 3600;
const RETRY_FAILED_SECS: u64 = 3600;
const MAX_CACHE_ENTRIES: usize = 5000;
const MAX_CACHE_FILE: usize = 8 * 1024 * 1024;

// served from this repo so nobody else decides what shows up on people's profiles
pub const FALLBACK_IMAGE: &str =
    "https://raw.githubusercontent.com/smokeyxd/ps5-rust-rpc/main/assets/icon.png";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GameMeta {
    pub name: String,
    pub image_url: String,
    pub game_url: Option<String>,
    #[serde(default)]
    pub fetched_at: u64,
    #[serde(default)]
    pub found: bool,
}

// a failed lookup (offline, site down) is retried after an hour instead of sticking for 30 days
fn is_fresh(m: &GameMeta, now: u64) -> bool {
    let ttl = if m.found { CACHE_TTL_SECS } else { RETRY_FAILED_SECS };
    now.saturating_sub(m.fetched_at) < ttl
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout(Duration::from_secs(12))
        .user_agent(concat!("ps5-rpc/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value> {
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| anyhow!("request failed: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_HTTP_BODY).read_to_end(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

fn fetch(title_id: &str) -> Result<GameMeta> {
    if !security::is_valid_title_id(title_id) {
        return Err(anyhow!("refusing to look up malformed title id"));
    }
    let agent = http_agent();

    let (api, game_url) = if title_id.starts_with("PPSA") {
        (
            format!("https://prosperopatches.com/api/lookup?titleid={title_id}"),
            format!("https://prosperopatches.com/{title_id}"),
        )
    } else {
        (
            format!("https://orbispatches.com/api/lookup?titleid={title_id}"),
            format!("https://orbispatches.com/{title_id}"),
        )
    };

    let v = get_json(&agent, &api)?;
    let name = v
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| security::sanitize_text(s, 128))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("no name in metadata response"))?;
    let image = v
        .get("metadata")
        .and_then(|m| m.get("icon"))
        .and_then(|i| i.as_str())
        .filter(|u| security::is_safe_https_url(u))
        .unwrap_or(FALLBACK_IMAGE)
        .to_string();
    let game_url = if security::is_safe_https_url(&game_url) {
        Some(game_url)
    } else {
        None
    };

    Ok(GameMeta {
        name,
        image_url: image,
        game_url,
        fetched_at: now_secs(),
        found: true,
    })
}

pub struct Cache {
    map: HashMap<String, GameMeta>,
}

impl Cache {
    pub fn load() -> Self {
        let map = (|| -> Result<HashMap<String, GameMeta>> {
            let p = config::cache_path()?;
            if !p.exists() {
                return Ok(HashMap::new());
            }
            let data = std::fs::read(&p)?;
            if data.len() > MAX_CACHE_FILE {
                return Ok(HashMap::new());
            }
            Ok(serde_json::from_slice(&data).unwrap_or_default())
        })()
        .unwrap_or_default();
        Self { map }
    }

    fn save(&self) {
        if let Ok(p) = config::cache_path() {
            if let Ok(dir) = config::config_dir() {
                let _ = std::fs::create_dir_all(&dir);
            }
            if let Ok(data) = serde_json::to_vec(&self.map) {
                let _ = std::fs::write(p, data);
            }
        }
    }

    pub fn get_or_fetch(&mut self, title_id: &str, fallback_name: &str) -> GameMeta {
        if let Some(m) = self.map.get(title_id) {
            if is_fresh(m, now_secs()) {
                return m.clone();
            }
        }

        let meta = if security::is_retail_title_id(title_id) {
            fetch(title_id).unwrap_or_else(|_| GameMeta {
                name: fallback_or(fallback_name, title_id),
                image_url: FALLBACK_IMAGE.to_string(),
                game_url: None,
                fetched_at: now_secs(),
                found: false,
            })
        } else {
            GameMeta {
                name: fallback_or(fallback_name, title_id),
                image_url: FALLBACK_IMAGE.to_string(),
                game_url: None,
                fetched_at: now_secs(),
                found: false,
            }
        };

        if self.map.len() >= MAX_CACHE_ENTRIES && !self.map.contains_key(title_id) {
            if let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, m)| m.fetched_at)
                .map(|(k, _)| k.clone())
            {
                self.map.remove(&oldest);
            }
        }
        self.map.insert(title_id.to_string(), meta.clone());
        self.save();
        meta
    }
}

fn fallback_or(name: &str, title_id: &str) -> String {
    let n = security::sanitize_text(name, 128);
    if n.is_empty() {
        title_id.to_string()
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(found: bool, fetched_at: u64) -> GameMeta {
        GameMeta {
            name: "Goat Simulator 3".into(),
            image_url: FALLBACK_IMAGE.into(),
            game_url: None,
            fetched_at,
            found,
        }
    }

    #[test]
    fn failed_lookups_expire_after_an_hour() {
        let now = 10_000_000;
        assert!(is_fresh(&meta(true, now - 29 * 24 * 3600), now));
        assert!(!is_fresh(&meta(true, now - 31 * 24 * 3600), now));
        assert!(is_fresh(&meta(false, now - 59 * 60), now));
        assert!(!is_fresh(&meta(false, now - 61 * 60), now));
    }

    #[test]
    fn old_cache_entries_without_found_are_retried() {
        let m: GameMeta = serde_json::from_str(
            r#"{"name":"x","image_url":"https://a.b/c","game_url":null,"fetched_at":1}"#,
        )
        .unwrap();
        assert!(!m.found);
    }

    #[test]
    fn fallback_image_passes_the_url_check() {
        assert!(security::is_safe_https_url(FALLBACK_IMAGE));
    }
}
