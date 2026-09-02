use anyhow::{anyhow, Result};
use auto_launch::AutoLaunchBuilder;

const APP_NAME: &str = "ps5-rpc";

fn handle() -> Result<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe()?;
    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&exe.to_string_lossy())
        .build()
        .map_err(|e| anyhow!("autostart init: {e}"))
}

pub fn is_enabled() -> bool {
    handle()
        .and_then(|a| a.is_enabled().map_err(|e| anyhow!("{e}")))
        .unwrap_or(false)
}

pub fn set(enabled: bool) -> Result<()> {
    let a = handle()?;
    if enabled {
        a.enable().map_err(|e| anyhow!("autostart enable: {e}"))
    } else {
        a.disable().map_err(|e| anyhow!("autostart disable: {e}"))
    }
}
