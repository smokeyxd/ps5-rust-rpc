use anyhow::{anyhow, Result};
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

use crate::{metadata, security};

const BADGE_KEY: &str = "ps5";

pub struct Presence {
    client: DiscordIpcClient,
    badge: bool,
}

impl Presence {
    pub fn new(app_id: &str) -> Result<Self> {
        if !security::is_valid_app_id(app_id) {
            return Err(anyhow!("invalid Discord application id"));
        }
        let client = DiscordIpcClient::new(app_id);
        // the badge image is uploaded to the built-in app only; another app id would show a broken image
        let badge = app_id == crate::config::DEFAULT_APP_ID;
        Ok(Self { client, badge })
    }

    pub fn connect(&mut self) -> Result<()> {
        self.client
            .connect()
            .map_err(|e| anyhow!("discord connect: {e}"))
    }

    pub fn set_game(
        &mut self,
        title_id: &str,
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

        let mut assets = activity::Assets::new().large_image(img).large_text(&name);
        if self.badge {
            assets = assets.small_image(BADGE_KEY).small_text("PS5");
        }
        let timestamps = activity::Timestamps::new().start(start_ts);
        // without this the member list says "Playing <app name>" instead of the game
        let mut act = activity::Activity::new()
            .details(&name)
            .state(platform_line(title_id))
            .status_display_type(activity::StatusDisplayType::Details)
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

    pub fn clear(&mut self) -> Result<()> {
        self.client
            .clear_activity()
            .map_err(|e| anyhow!("discord clear: {e}"))
    }

    pub fn close(&mut self) {
        let _ = self.client.close();
    }
}

fn platform_line(title_id: &str) -> &'static str {
    if title_id.starts_with("CUSA") {
        "PS4 game on PS5"
    } else {
        "on PS5"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_line_tells_ps4_games_apart() {
        assert_eq!(platform_line("PPSA01234"), "on PS5");
        assert_eq!(platform_line("CUSA00265"), "PS4 game on PS5");
        assert_eq!(platform_line(""), "on PS5");
    }
}
