<p align="center"><img src="assets/icon.png" width="88" alt="ps5-rpc"></p>

<h1 align="center">ps5-rpc</h1>

<p align="center">Show what you're playing on your jailbroken PS5 as Discord Rich Presence.</p>

A small tray app that watches your PS5 for the running game and sets your Discord
status (box art, elapsed time, optional "View game" button). One ~1.9 MB binary,
no Python or Docker.

By default it reads the game from etaHEN's RPC server (port 8000) and falls back
to ps5debug-NG (port 744) if the RPC isn't enabled, so it works on either setup.

<p align="center"><img src="assets/demo.gif" width="800" alt="Loading etaHEN, etaHEN starting its RPC server, ps5-rpc connecting, starting Goat Simulator 3 and Discord showing it"></p>

On Discord it looks like this:

<p align="center"><img src="assets/discord.png" alt="Discord activity card: Playing Goat Simulator 3 with its box art and elapsed time"></p>

## Features

- Tray app with status, Reconnect, Open config folder, and a "Start on login" toggle.
- Two title sources with automatic fallback (etaHEN RPC ↔ ps5debug).
- Handles games whose process isn't `eboot.bin` (e.g. Minecraft LCE).
- Box art + names from orbispatches (PS4) / prosperopatches (PS5), cached locally.

## Download

Get the build for your system from the [Releases](https://github.com/smokeyxd/ps5-rust-rpc/releases) page:

- Windows: `ps5-rpc-<version>-windows-x64.zip`
- Linux (x64): `ps5-rpc-<version>-linux-x64.tar.gz`. Needs GTK 3, libxdo and an
  AppIndicator library, which most desktop distros already have.
- macOS (Apple Silicon and Intel): `ps5-rpc-<version>-macos-universal.tar.gz`

The builds aren't code-signed. On Windows, SmartScreen may warn the first time
(**More info** → **Run anyway**). On macOS, run
`xattr -dr com.apple.quarantine <extracted folder>` once, or allow it under
System Settings → Privacy & Security. `SHA256SUMS.txt` on each release lists the
checksums.

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
2. Run the app once to generate the config, then edit `config.json`. The tray's
   **Open config folder** opens the right place:
   - Windows: `%APPDATA%\jbps5\ps5-rpc\config\config.json`
   - Linux: `~/.config/ps5-rpc/config.json` (or `$XDG_CONFIG_HOME/ps5-rpc/` if set)
   - macOS: `~/Library/Application Support/dev.jbps5.ps5-rpc/config.json`
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
3. Make sure the Discord desktop app is open and either the etaHEN RPC
   ([how to turn it on](#turning-on-etahens-rpc-server)) or ps5debug is running
   on the console.
4. Launch it and hit **Reconnect** after editing the config.

### Turning on etaHEN's RPC server

etaHEN (1.4b or newer) has the RPC server built in, but it's off by default:

1. Connect to the PS5 over FTP (etaHEN's FTP server is on port 1337) and open
   `/data/etaHEN/config.ini`.
2. Set `discord_rpc=1` and save.
3. Restart the PS5 and load etaHEN again.

When it's on, the PS5 shows *"[etaHEN] Discord RPC server listening on port
8000"* while etaHEN loads, and *"[etaHEN] [RPC] New connection accepted"* when
ps5-rpc connects. With it off, ps5-rpc falls back to ps5debug-NG, if that's
running.

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
