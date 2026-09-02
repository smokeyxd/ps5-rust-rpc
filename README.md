<p align="center"><img src="assets/icon.png" width="88" alt="ps5-rpc"></p>

<h1 align="center">ps5-rpc</h1>

<p align="center">Show what you're playing on your jailbroken PS5 as Discord Rich Presence.</p>

A small tray app that watches your PS5 for the running game and sets your Discord
status (box art, elapsed time, optional "View game" button). One ~1.9 MB binary,
no Python or Docker.

By default it reads the game from etaHEN's RPC server (port 8000) and falls back
to ps5debug-NG (port 744) if the RPC isn't enabled, so it works on either setup.

## Features

- Tray app with status, Reconnect, Open config folder, and a "Start on login" toggle.
- Two title sources with automatic fallback (etaHEN RPC ↔ ps5debug).
- Handles games whose process isn't `eboot.bin` (e.g. Minecraft LCE).
- Box art + names from orbispatches (PS4) / prosperopatches (PS5), cached locally.

## Build

```
cargo build --release
```

Output: `target/release/ps5-rpc(.exe)`. Debug builds keep a console for logs;
release builds run windowless.

## Setup

1. Create a Discord application at <https://discord.com/developers/applications>
   and copy its **Application ID**. (Art comes from the web, so you don't need to
   upload any assets.)
2. Run the app once to generate the config, then edit
   `%APPDATA%\jbps5\ps5-rpc\config\config.json`:
   ```json
   {
     "ps5_ip": "YOUR_PS5_IP",
     "ps5_debug_port": 744,
     "discord_app_id": "PASTE_YOUR_APP_ID_HERE",
     "buttons": true,
     "poll_interval_secs": 15,
     "use_etahen_rpc": true,
     "etahen_rpc_port": 8000
   }
   ```
3. Make sure the Discord desktop app is open and either the etaHEN RPC or
   ps5debug is running on the console.
4. Launch it and hit **Reconnect** after editing the config.

Autostart: use the tray's **Start on login** toggle (per-user, no admin). Move
the binary to a permanent location before enabling, since it registers the
current path.

## Config

| Key | Default | Notes |
|-----|---------|-------|
| `ps5_ip` | (discovered) | Console IP or hostname |
| `ps5_debug_port` | `744` | ps5debug-NG port |
| `discord_app_id` | (required) | Your Discord Application ID |
| `buttons` | `true` | Show a "View game" button |
| `poll_interval_secs` | `15` | 1–3600 |
| `use_etahen_rpc` | `true` | Prefer etaHEN RPC (8000); ps5debug (744) is the fallback |
| `etahen_rpc_port` | `8000` | etaHEN RPC port |

## Notes on security

The app treats everything the console and the web APIs send as untrusted: wire
lengths are bounded before reading, title IDs are validated before they go into a
URL, HTTP uses TLS with a size cap, and image/button URLs are checked before they
reach Discord. It only makes outbound connections and never opens a listening
port. No `unsafe`.

## Credits

Inspired by `jeroendev-one/ps5-rpc-client`. Talks to `ps5debug-NG` and etaHEN.

## License

MIT — see [LICENSE](LICENSE).
