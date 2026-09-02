use anyhow::{anyhow, bail, Result};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::security;

const PACKET_MAGIC: u32 = 0xFFAA_BBCC;
const STATUS_SUCCESS: u32 = 0x8000_0000;

const CMD_PROC_LIST: u32 = 0xBDAA_0001;
const CMD_PROC_INFO: u32 = 0xBDAA_000A;
const CMD_FOREGROUND_APP: u32 = 0xBDDD_0006;

const MAX_PROCS: u32 = 8192;
const PROC_ENTRY: usize = 36;
const PROC_INFO_LEN: usize = 188;
const FOREGROUND_LEN: usize = 140;
const MAX_INFO_CALLS: usize = 48;

#[derive(Debug, Clone)]
pub struct TitleInfo {
    pub title_id: String,
    pub name: String,
}

pub struct Ps5Client {
    host: String,
    port: u16,
    timeout: Duration,
}

impl Ps5Client {
    pub fn new(host: impl Into<String>, port: u16, timeout: Duration) -> Self {
        Self {
            host: host.into(),
            port,
            timeout,
        }
    }

    fn connect(&self) -> Result<TcpStream> {
        let target = format!("{}:{}", self.host, self.port);
        let mut last: anyhow::Error = anyhow!("no resolved address for {target}");
        for sa in target.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sa, self.timeout) {
                Ok(s) => {
                    s.set_read_timeout(Some(self.timeout))?;
                    s.set_write_timeout(Some(self.timeout))?;
                    let _ = s.set_nodelay(true);
                    return Ok(s);
                }
                Err(e) => last = e.into(),
            }
        }
        Err(last)
    }

    fn send_cmd(s: &mut TcpStream, cmd: u32, body: &[u8]) -> Result<()> {
        let mut hdr = [0u8; 12];
        hdr[0..4].copy_from_slice(&PACKET_MAGIC.to_le_bytes());
        hdr[4..8].copy_from_slice(&cmd.to_le_bytes());
        hdr[8..12].copy_from_slice(&(body.len() as u32).to_le_bytes());
        s.write_all(&hdr)?;
        if !body.is_empty() {
            s.write_all(body)?;
        }
        Ok(())
    }

    fn read_u32(s: &mut TcpStream) -> Result<u32> {
        let mut b = [0u8; 4];
        s.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn read_capped(s: &mut TcpStream, n: usize, cap: usize) -> Result<Vec<u8>> {
        if n > cap {
            bail!("server declared {n} bytes (cap {cap}); refusing");
        }
        let mut buf = vec![0u8; n];
        s.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn expect_success(s: &mut TcpStream) -> Result<()> {
        let st = Self::read_u32(s)?;
        if st != STATUS_SUCCESS {
            bail!("ps5debug returned status 0x{st:08X}");
        }
        Ok(())
    }

    fn field_str(field: &[u8], max_chars: usize) -> String {
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        let raw = String::from_utf8_lossy(&field[..end]);
        security::sanitize_text(&raw, max_chars)
    }

    fn foreground_app(&self) -> Result<Option<TitleInfo>> {
        let mut s = self.connect()?;
        Self::send_cmd(&mut s, CMD_FOREGROUND_APP, &[])?;
        Self::expect_success(&mut s)?;
        let buf = Self::read_capped(&mut s, FOREGROUND_LEN, FOREGROUND_LEN)?;
        let pid = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if pid == 0 {
            return Ok(None);
        }
        let title = Self::field_str(&buf[4..20], 9);
        let name = Self::field_str(&buf[84..124], 128);
        if security::is_valid_title_id(&title) {
            Ok(Some(TitleInfo {
                title_id: title,
                name,
            }))
        } else {
            Ok(None)
        }
    }

    fn list_processes(&self) -> Result<Vec<(String, i32)>> {
        let mut s = self.connect()?;
        Self::send_cmd(&mut s, CMD_PROC_LIST, &[])?;
        Self::expect_success(&mut s)?;
        let num = Self::read_u32(&mut s)?;
        if num > MAX_PROCS {
            bail!("process count {num} exceeds cap {MAX_PROCS}");
        }
        let total = (num as usize)
            .checked_mul(PROC_ENTRY)
            .ok_or_else(|| anyhow!("process table size overflow"))?;
        let buf = Self::read_capped(&mut s, total, (MAX_PROCS as usize) * PROC_ENTRY)?;
        let mut out = Vec::with_capacity(num as usize);
        for chunk in buf.chunks_exact(PROC_ENTRY) {
            let name = Self::field_str(&chunk[..32], 64);
            let pid = i32::from_le_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]);
            out.push((name, pid));
        }
        Ok(out)
    }

    fn proc_title(&self, pid: i32) -> Result<Option<TitleInfo>> {
        if pid <= 0 {
            return Ok(None);
        }
        let mut s = self.connect()?;
        Self::send_cmd(&mut s, CMD_PROC_INFO, &(pid as u32).to_le_bytes())?;
        Self::expect_success(&mut s)?;
        let buf = Self::read_capped(&mut s, PROC_INFO_LEN, PROC_INFO_LEN)?;
        let name = Self::field_str(&buf[4..44], 128);
        let title = Self::field_str(&buf[108..124], 9);
        if security::is_valid_title_id(&title) {
            Ok(Some(TitleInfo {
                title_id: title,
                name,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn current_title(&self) -> Result<Option<TitleInfo>> {
        if let Some(t) = self.foreground_app()? {
            if security::is_retail_title_id(&t.title_id) {
                return Ok(Some(t));
            }
        }
        let procs = self.list_processes()?;
        let mut calls = 0usize;
        for (name, pid) in procs {
            if calls >= MAX_INFO_CALLS {
                break;
            }
            if !(name == "eboot.bin" || name.ends_with(".elf")) {
                continue;
            }
            calls += 1;
            if let Ok(Some(t)) = self.proc_title(pid) {
                if security::is_retail_title_id(&t.title_id) {
                    let name = if t.name.is_empty() { name } else { t.name };
                    return Ok(Some(TitleInfo {
                        title_id: t.title_id,
                        name,
                    }));
                }
            }
        }
        Ok(None)
    }
}
