use anyhow::{anyhow, Result};
use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::ps5::TitleInfo;
use crate::security;

const MAX_LINE_CHARS: usize = 64;
const MAX_BUF_BYTES: usize = 4096;
const MAX_READ_PER_POLL: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_millis(400);

pub struct RpcClient {
    host: String,
    port: u16,
    connect_timeout: Duration,
    stream: Option<TcpStream>,
    buf: String,
    last: Option<TitleInfo>,
    synced: bool,
}

impl RpcClient {
    pub fn new(host: impl Into<String>, port: u16, connect_timeout: Duration) -> Self {
        Self {
            host: host.into(),
            port,
            connect_timeout,
            stream: None,
            buf: String::new(),
            last: None,
            synced: false,
        }
    }

    pub fn synced(&self) -> bool {
        self.synced
    }

    fn ensure(&mut self) -> Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }
        let target = format!("{}:{}", self.host, self.port);
        let mut last_err: anyhow::Error = anyhow!("no resolved address for {target}");
        for sa in target.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sa, self.connect_timeout) {
                Ok(s) => {
                    s.set_read_timeout(Some(READ_TIMEOUT))?;
                    let _ = s.set_nodelay(true);
                    self.stream = Some(s);
                    self.buf.clear();
                    self.synced = false;
                    return Ok(());
                }
                Err(e) => last_err = e.into(),
            }
        }
        Err(last_err)
    }

    fn drop_conn(&mut self) {
        self.stream = None;
        self.buf.clear();
    }

    fn parse_line(line: &str) -> Option<Option<TitleInfo>> {
        let t = security::sanitize_text(line, MAX_LINE_CHARS);
        if t.is_empty() {
            return None;
        }
        let low = t.to_ascii_lowercase();
        if low.contains("no game") || low.contains("not running") || low == "none" {
            return Some(None);
        }
        let tok = t.split_whitespace().next().unwrap_or(t.as_str());
        if security::is_valid_title_id(tok) {
            return Some(Some(TitleInfo {
                title_id: tok.to_string(),
                name: String::new(),
            }));
        }
        None
    }

    pub fn current_title(&mut self) -> Result<Option<TitleInfo>> {
        self.ensure()?;

        let mut tmp = [0u8; 1024];
        let mut total = 0usize;
        loop {
            let stream = match self.stream.as_mut() {
                Some(s) => s,
                None => break,
            };
            match stream.read(&mut tmp) {
                Ok(0) => {
                    self.drop_conn();
                    return Err(anyhow!("rpc: connection closed by console"));
                }
                Ok(n) => {
                    total += n;
                    let chunk = String::from_utf8_lossy(&tmp[..n]);
                    self.buf.push_str(&chunk);
                    if self.buf.len() > MAX_BUF_BYTES {
                        self.drop_conn();
                        return Err(anyhow!("rpc: oversized line, dropping connection"));
                    }
                    if total >= MAX_READ_PER_POLL {
                        break;
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                Err(e) => {
                    self.drop_conn();
                    return Err(anyhow!("rpc read: {e}"));
                }
            }
        }

        while let Some(idx) = self.buf.find(|c| c == '\n' || c == '\r') {
            let line = self.buf[..idx].to_string();
            let mut rest = self.buf[idx + 1..].to_string();
            if rest.starts_with('\n') {
                rest.remove(0);
            }
            self.buf = rest;
            if let Some(new_state) = Self::parse_line(&line) {
                self.last = new_state;
                self.synced = true;
            }
        }

        Ok(self.last.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::RpcClient;

    #[test]
    fn understands_etahen_and_payload_lines() {
        let game = RpcClient::parse_line("CUSA46679").unwrap().unwrap();
        assert_eq!(game.title_id, "CUSA46679");
        assert!(RpcClient::parse_line("No game running.").unwrap().is_none());
        assert!(RpcClient::parse_line("Failed to get title ID.").is_none());
        assert!(RpcClient::parse_line("").is_none());
        assert!(RpcClient::parse_line("../etc/passwd").is_none());
    }
}
