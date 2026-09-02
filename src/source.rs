use anyhow::Result;
use std::time::Duration;

use crate::config::Config;
use crate::ps5::{Ps5Client, TitleInfo};
use crate::rpc::RpcClient;

pub struct Sources {
    debug: Ps5Client,
    rpc: RpcClient,
    prefer_rpc: bool,
    pub active: &'static str,
}

impl Sources {
    pub fn from_config(cfg: &Config) -> Self {
        let timeout = Duration::from_secs(6);
        Sources {
            debug: Ps5Client::new(cfg.ps5_ip.clone(), cfg.ps5_debug_port, timeout),
            rpc: RpcClient::new(cfg.ps5_ip.clone(), cfg.etahen_rpc_port, timeout),
            prefer_rpc: cfg.use_etahen_rpc,
            active: if cfg.use_etahen_rpc {
                "etaHEN RPC"
            } else {
                "ps5debug"
            },
        }
    }

    pub fn current_title(&mut self) -> Result<Option<TitleInfo>> {
        if self.prefer_rpc {
            match self.rpc.current_title() {
                Ok(rpc_state) => {
                    if self.rpc.synced() {
                        self.active = "etaHEN RPC";
                        Ok(rpc_state)
                    } else {
                        match self.debug.current_title() {
                            Ok(seed) => {
                                self.active = "etaHEN RPC (seeded via ps5debug)";
                                Ok(seed)
                            }
                            Err(_) => {
                                self.active = "etaHEN RPC";
                                Ok(rpc_state)
                            }
                        }
                    }
                }
                Err(rpc_err) => match self.debug.current_title() {
                    Ok(t) => {
                        self.active = "ps5debug (fallback)";
                        Ok(t)
                    }
                    Err(_) => Err(rpc_err),
                },
            }
        } else {
            match self.debug.current_title() {
                Ok(t) => {
                    self.active = "ps5debug";
                    Ok(t)
                }
                Err(debug_err) => match self.rpc.current_title() {
                    Ok(t) => {
                        self.active = "etaHEN RPC (fallback)";
                        Ok(t)
                    }
                    Err(_) => Err(debug_err),
                },
            }
        }
    }
}
