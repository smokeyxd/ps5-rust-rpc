use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;

pub const DEFAULT_PS5_DEBUG_PORT: u16 = 744;
pub const DEFAULT_ETAHEN_RPC_PORT: u16 = 8000;
pub const PLACEHOLDER_APP_ID: &str = "000000000000000000";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub ps5_ip: String,
    #[serde(default = "default_debug_port")]
    pub ps5_debug_port: u16,
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
            discord_app_id: PLACEHOLDER_APP_ID.to_string(),
            buttons: true,
            poll_interval_secs: 15,
            use_etahen_rpc: true,
            etahen_rpc_port: DEFAULT_ETAHEN_RPC_PORT,
        }
    }
}

impl Config {
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
        if !crate::security::is_valid_app_id(&self.discord_app_id) {
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
