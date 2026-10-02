use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;

pub const DEFAULT_PS5_DEBUG_PORT: u16 = 744;
pub const DEFAULT_ETAHEN_RPC_PORT: u16 = 8000;
// Shared app so people don't have to make their own in the Discord developer portal.
pub const DEFAULT_APP_ID: &str = "1189341685168214056";
// 0.1.0 wrote this into new configs; treat it like an empty field.
pub const PLACEHOLDER_APP_ID: &str = "000000000000000000";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub ps5_ip: String,
    #[serde(default = "default_debug_port")]
    pub ps5_debug_port: u16,
    #[serde(default)]
    pub discord_app_id: String,
    #[serde(default = "default_true")]
    pub buttons: bool,
    #[serde(default = "default_poll")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_true")]
    pub use_etahen_rpc: bool,
    #[serde(default = "default_rpc_port")]
    pub etahen_rpc_port: u16,
}

fn default_debug_port() -> u16 {
    DEFAULT_PS5_DEBUG_PORT
}
fn default_rpc_port() -> u16 {
    DEFAULT_ETAHEN_RPC_PORT
}
fn default_poll() -> u64 {
    15
}
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ps5_ip: String::new(),
            ps5_debug_port: DEFAULT_PS5_DEBUG_PORT,
            discord_app_id: String::new(),
            buttons: true,
            poll_interval_secs: 15,
            use_etahen_rpc: true,
            etahen_rpc_port: DEFAULT_ETAHEN_RPC_PORT,
        }
    }
}

impl Config {
    pub fn app_id(&self) -> &str {
        let id = self.discord_app_id.trim();
        if id.is_empty() || id == PLACEHOLDER_APP_ID {
            DEFAULT_APP_ID
        } else {
            id
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.ps5_ip.parse::<IpAddr>().is_err() {
            let ok = !self.ps5_ip.is_empty()
                && self.ps5_ip.len() <= 253
                && self
                    .ps5_ip
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-');
            if !ok {
                return Err(anyhow!("ps5_ip is not a valid IP address or hostname"));
            }
        }
        if self.ps5_debug_port == 0 {
            return Err(anyhow!("ps5_debug_port must be non-zero"));
        }
        if self.etahen_rpc_port == 0 {
            return Err(anyhow!("etahen_rpc_port must be non-zero"));
        }
        if !crate::security::is_valid_app_id(self.app_id()) {
            return Err(anyhow!(
                "discord_app_id must be a numeric Discord Application ID (17-20 digits)"
            ));
        }
        if !(1..=3600).contains(&self.poll_interval_secs) {
            return Err(anyhow!("poll_interval_secs out of range (1..=3600)"));
        }
        Ok(())
    }
}

pub fn config_dir() -> Result<PathBuf> {
    let pd = directories::ProjectDirs::from("dev", "jbps5", "ps5-rpc")
        .ok_or_else(|| anyhow!("could not resolve a config directory for this platform"))?;
    Ok(pd.config_dir().to_path_buf())
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn cache_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("game_cache.json"))
}

pub fn load() -> Result<Option<Config>> {
    let p = config_path()?;
    if !p.exists() {
        return Ok(None);
    }
    let data = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
    if data.len() > 64 * 1024 {
        return Err(anyhow!("config file is implausibly large; refusing to parse"));
    }
    let cfg: Config = serde_json::from_slice(&data).context("parsing config.json")?;
    Ok(Some(cfg))
}

pub fn save(cfg: &Config) -> Result<()> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let p = config_path()?;
    let data = serde_json::to_vec_pretty(cfg)?;
    std::fs::write(&p, data).with_context(|| format!("writing {}", p.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_app_id(id: &str) -> Config {
        Config {
            ps5_ip: "192.168.1.2".into(),
            discord_app_id: id.into(),
            ..Config::default()
        }
    }

    #[test]
    fn app_id_falls_back_to_the_shared_app() {
        assert_eq!(with_app_id("").app_id(), DEFAULT_APP_ID);
        assert_eq!(with_app_id("  ").app_id(), DEFAULT_APP_ID);
        assert_eq!(with_app_id(PLACEHOLDER_APP_ID).app_id(), DEFAULT_APP_ID);
        assert_eq!(with_app_id("123456789012345678").app_id(), "123456789012345678");
        assert!(with_app_id("").validate().is_ok());
        assert!(with_app_id("not-an-id").validate().is_err());
    }

    #[test]
    fn config_without_app_id_still_parses() {
        let cfg: Config = serde_json::from_str(r#"{"ps5_ip":"192.168.1.2"}"#).unwrap();
        assert_eq!(cfg.app_id(), DEFAULT_APP_ID);
        assert!(cfg.validate().is_ok());
    }
}
