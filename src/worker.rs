use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tao::event_loop::EventLoopProxy;

use crate::config::{self, Config};
use crate::discord::Presence;
use crate::metadata::Cache;
use crate::security;
use crate::source::Sources;

#[derive(Debug, Clone)]
pub enum WorkerStatus {
    Connecting,
    Playing(String),
    Idle,
    Error(String),
}

pub fn status_labels(s: &WorkerStatus) -> (String, String) {
    match s {
        WorkerStatus::Connecting => ("Connecting…".into(), "PS5 RPC — connecting".into()),
        WorkerStatus::Playing(name) => (format!("Playing: {name}"), format!("PS5 RPC — {name}")),
        WorkerStatus::Idle => ("Idle (no game)".into(), "PS5 RPC — idle".into()),
        WorkerStatus::Error(e) => (format!("Error: {e}"), format!("PS5 RPC — {e}")),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn short(e: impl std::fmt::Display) -> String {
    security::sanitize_text(&e.to_string(), 80)
}

fn nap(secs: u64, shutdown: &AtomicBool) {
    let steps = secs.saturating_mul(5);
    for _ in 0..steps {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn send(proxy: &EventLoopProxy<crate::UserEvent>, s: WorkerStatus) {
    let _ = proxy.send_event(crate::UserEvent::Status(s));
}

pub fn discover_ps5() -> Option<String> {
    const MAGIC: u32 = 0xFFFF_AAAA;
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_broadcast(true).ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    sock.send_to(&MAGIC.to_le_bytes(), "255.255.255.255:1010").ok()?;
    let mut buf = [0u8; 16];
    for _ in 0..4 {
        match sock.recv_from(&mut buf) {
            Ok((n, addr)) if n >= 4 => {
                let got = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if got == MAGIC {
                    return Some(addr.ip().to_string());
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    None
}

fn report_bad_config(cfg: &Config, proxy: &EventLoopProxy<crate::UserEvent>) {
    if cfg.validate().is_err() {
        send(proxy, WorkerStatus::Error("check ps5_ip in config.json".into()));
    }
}

pub fn run(
    cfg: Config,
    proxy: EventLoopProxy<crate::UserEvent>,
    shutdown: Arc<AtomicBool>,
    reconnect: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        let cfg2 = cfg.clone();
        let proxy2 = proxy.clone();
        let sd2 = shutdown.clone();
        let rc2 = reconnect.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            worker_loop(cfg2, &proxy2, &sd2, &rc2);
        }));
        if result.is_err() {
            send(&proxy, WorkerStatus::Error("internal error; restarting".into()));
            nap(3, &shutdown);
        } else {
            break;
        }
    }
}

fn worker_loop(
    mut cfg: Config,
    proxy: &EventLoopProxy<crate::UserEvent>,
    shutdown: &AtomicBool,
    reconnect: &AtomicBool,
) {
    let mut sources = Sources::from_config(&cfg);
    let mut cache = Cache::load();
    let mut discord: Option<Presence> = None;
    let mut last_title: Option<String> = None;
    let mut backoff = 2u64;
    let mut ps5_failures = 0u32;

    report_bad_config(&cfg, proxy);

    loop {
        if shutdown.load(Ordering::SeqCst) {
            if let Some(mut d) = discord.take() {
                d.close();
            }
            return;
        }
        if reconnect.swap(false, Ordering::SeqCst) {
            if let Some(mut d) = discord.take() {
                d.close();
            }
            if let Ok(Some(fresh)) = config::load() {
                cfg = fresh;
                report_bad_config(&cfg, proxy);
            }
            sources = Sources::from_config(&cfg);
            last_title = None;
        }

        if discord.is_none() {
            send(proxy, WorkerStatus::Connecting);
            match Presence::new(cfg.app_id()).and_then(|mut d| {
                d.connect()?;
                Ok(d)
            }) {
                Ok(d) => {
                    discord = Some(d);
                    backoff = 2;
                }
                Err(e) => {
                    send(proxy, WorkerStatus::Error(format!("Discord: {}", short(e))));
                    nap(backoff, shutdown);
                    backoff = (backoff * 2).min(60);
                    continue;
                }
            }
        }

        let polled = sources.current_title();
        if polled.is_ok() {
            ps5_failures = 0;
        }
        match polled {
            Ok(Some(t)) => {
                if last_title.as_deref() != Some(t.title_id.as_str()) {
                    last_title = Some(t.title_id.clone());
                    let start_ts = now_secs();
                    let meta = cache.get_or_fetch(&t.title_id, &t.name);
                    if let Some(d) = discord.as_mut() {
                        if let Err(e) = d.set_game(
                            &t.title_id,
                            &meta.name,
                            &meta.image_url,
                            meta.game_url.as_deref(),
                            start_ts,
                            cfg.buttons,
                        ) {
                            send(proxy, WorkerStatus::Error(format!("Discord: {}", short(e))));
                            discord = None;
                            continue;
                        }
                    }
                    send(proxy, WorkerStatus::Playing(meta.name));
                }
            }
            Ok(None) => {
                if last_title.is_some() {
                    last_title = None;
                    if let Some(d) = discord.as_mut() {
                        let _ = d.set_idle();
                    }
                    send(proxy, WorkerStatus::Idle);
                }
            }
            Err(e) => {
                ps5_failures += 1;
                // one failed poll can be a hiccup; two in a row usually means the PS5 is off,
                // and Discord would otherwise keep showing the last game forever
                if ps5_failures == 2 && last_title.take().is_some() {
                    if let Some(d) = discord.as_mut() {
                        let _ = d.clear();
                    }
                }
                send(proxy, WorkerStatus::Error(format!("PS5: {}", short(e))));
            }
        }

        nap(cfg.poll_interval_secs.max(1), shutdown);
    }
}
