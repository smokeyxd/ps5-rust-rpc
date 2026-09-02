use anyhow::{anyhow, Result};
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

use crate::{metadata, security};

pub struct Presence {
    client: DiscordIpcClient,
}

impl Presence {
    pub fn new(app_id: &str) -> Result<Self> {
        if !security::is_valid_app_id(app_id) {
            return Err(anyhow!("invalid Discord application id"));
        }
        let client = DiscordIpcClient::new(app_id);
        Ok(Self { client })
    }

    pub fn connect(&mut self) -> Result<()> {
        self.client
            .connect()
            .map_err(|e| anyhow!("discord connect: {e}"))
    }

    pub fn set_game(
        &mut self,
        name: &str,
        image: &str,
        game_url: Option<&str>,
        start_ts: i64,
        buttons: bool,
    ) -> Result<()> {
        let name = security::sanitize_text(name, 128);
        let name = if name.is_empty() {
            "Unknown title".to_string()
        } else {
            name
        };
        let img = if security::is_safe_https_url(image) {
            image
        } else {
            metadata::FALLBACK_IMAGE
        };

        let assets = activity::Assets::new().large_image(img).large_text(&name);
        let timestamps = activity::Timestamps::new().start(start_ts);
        let mut act = activity::Activity::new()
            .details(&name)
            .assets(assets)
            .timestamps(timestamps);

        let btns = match game_url {
            Some(u) if buttons && security::is_safe_https_url(u) => {
                vec![activity::Button::new("View game", u)]
            }
            _ => Vec::new(),
        };
        if !btns.is_empty() {
            act = act.buttons(btns);
        }

        self.client
            .set_activity(act)
            .map_err(|e| anyhow!("discord set_activity: {e}"))
    }

    pub fn set_idle(&mut self) -> Result<()> {
        let act = activity::Activity::new()
            .details("Idle")
            .state("No game running");
        self.client
            .set_activity(act)
            .map_err(|e| anyhow!("discord idle: {e}"))
    }

    pub fn close(&mut self) {
        let _ = self.client.close();
    }
}
