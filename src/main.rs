#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod config;
mod discord;
mod metadata;
mod ps5;
mod rpc;
mod security;
mod source;
mod worker;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

#[derive(Debug)]
pub enum UserEvent {
    Menu(MenuEvent),
    Status(worker::WorkerStatus),
}

fn make_icon() -> Result<Icon> {
    let (w, h) = (32u32, 32u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let in_play = fx >= 11.0 && fx <= 23.0 && {
                let t = (fx - 11.0) / 12.0;
                let half = 8.0 * (1.0 - t);
                (fy - 16.0).abs() <= half
            };
            if in_play {
                rgba.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            } else {
                rgba.extend_from_slice(&[0x00, 0x54, 0xCC, 0xFF]);
            }
        }
    }
    Ok(Icon::from_rgba(rgba, w, h)?)
}

fn open_path(p: &Path) {
    #[cfg(target_os = "windows")]
    let cmd = ("explorer", p);
    #[cfg(target_os = "macos")]
    let cmd = ("open", p);
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = ("xdg-open", p);
    let _ = std::process::Command::new(cmd.0).arg(cmd.1).spawn();
}

fn main() -> Result<()> {
    let cfg = match config::load()? {
        Some(c) => c,
        None => {
            let mut c = config::Config::default();
            if let Some(ip) = worker::discover_ps5() {
                c.ps5_ip = ip;
            }
            config::save(&c)?;
            if let Ok(p) = config::config_path() {
                eprintln!(
                    "First run: wrote {}. Set your Discord Application ID (and PS5 IP if not auto-detected), then use the tray's Reconnect.",
                    p.display()
                );
            }
            c
        }
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Menu(e));
        }));
    }

    let menu = Menu::new();
    let status_item = MenuItem::new("Starting…", false, None);
    let reconnect_item = MenuItem::new("Reconnect", true, None);
    let open_cfg_item = MenuItem::new("Open config folder", true, None);
    let autostart_item = CheckMenuItem::new("Start on login", true, autostart::is_enabled(), None);
    let quit_item = MenuItem::new("Quit", true, None);
    menu.append(&status_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&reconnect_item)?;
    menu.append(&open_cfg_item)?;
    menu.append(&autostart_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("PS5 RPC — starting…")
        .with_icon(make_icon()?)
        .build()?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let reconnect = Arc::new(AtomicBool::new(false));
    {
        let cfg = cfg.clone();
        let p = proxy.clone();
        let sd = shutdown.clone();
        let rc = reconnect.clone();
        std::thread::spawn(move || worker::run(cfg, p, sd, rc));
    }

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _ = &tray;
        match event {
            Event::UserEvent(UserEvent::Status(s)) => {
                let (label, tip) = worker::status_labels(&s);
                status_item.set_text(&label);
                let _ = tray.set_tooltip(Some(&tip));
            }
            Event::UserEvent(UserEvent::Menu(me)) => {
                if me.id == *quit_item.id() {
                    shutdown.store(true, Ordering::SeqCst);
                    *control_flow = ControlFlow::Exit;
                } else if me.id == *reconnect_item.id() {
                    reconnect.store(true, Ordering::SeqCst);
                } else if me.id == *open_cfg_item.id() {
                    if let Ok(dir) = config::config_dir() {
                        open_path(&dir);
                    }
                } else if me.id == *autostart_item.id() {
                    let want = !autostart::is_enabled();
                    let _ = autostart::set(want);
                    autostart_item.set_checked(autostart::is_enabled());
                }
            }
            _ => {}
        }
    });
}
