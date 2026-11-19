//! Philips WiZ Smart Lighting LAN UDP controller mirroring `WizUdpManager.kt`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const WIZ_PORT: u16 = 38899;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredBulb {
    pub ip: String,
    pub mac: String,
    pub name: String,
    pub state: bool,
    pub dimming: u8,
    pub temp: Option<u16>,
    pub r: Option<u8>,
    pub g: Option<u8>,
    pub b: Option<u8>,
    pub scene_id: Option<u16>,
}

pub fn load_cached_bulbs(cache_file: &Path) -> Vec<DiscoveredBulb> {
    if !cache_file.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(cache_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_cached_bulbs(cache_file: &Path, bulbs: &[DiscoveredBulb]) -> Result<()> {
    if let Some(parent) = cache_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(bulbs)?;
    std::fs::write(cache_file, body)?;
    Ok(())
}

pub fn default_bulb_cache_path() -> PathBuf {
    crate::config::app_data_dir().join("wiz_bulbs.json")
}

/// Broadcast UDP probe on port 38899 and query `getPilot` for discovered bulbs.
pub fn scan_bulbs(cache_file: &Path, timeout_ms: u64) -> Result<Vec<DiscoveredBulb>> {
    let mut map: BTreeMap<String, DiscoveredBulb> = load_cached_bulbs(cache_file)
        .into_iter()
        .map(|b| (b.ip.clone(), b))
        .collect();

    let socket = UdpSocket::bind("0.0.0.0:0").context("bind WiZ UDP socket")?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(450)))?;

    let probe = serde_json::to_vec(&json!({ "method": "getSystemConfig", "params": {} }))?;
    socket.send_to(&probe, ("255.255.255.255", WIZ_PORT))?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut buf = [0u8; 2048];
    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let ip = addr.ip().to_string();
                let Ok(val) = serde_json::from_slice::<serde_json::Value>(&buf[..len]) else {
                    continue;
                };
                let Some(result) = val.get("result") else {
                    continue;
                };
                let mac = result
                    .get("mac")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let short = if mac.len() >= 4 {
                    mac[mac.len() - 4..].to_uppercase()
                } else {
                    ip.clone()
                };
                map.entry(ip.clone()).or_insert_with(|| DiscoveredBulb {
                    ip,
                    mac,
                    name: format!("Philips WiZ ({short})"),
                    state: true,
                    dimming: 100,
                    temp: None,
                    r: None,
                    g: None,
                    b: None,
                    scene_id: None,
                });
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                break;
            }
            Err(_) => break,
        }
    }

    let get_pilot = serde_json::to_vec(&json!({ "method": "getPilot", "params": {} }))?;
    let ips: Vec<String> = map.keys().cloned().collect();
    for ip in ips {
        if socket.send_to(&get_pilot, (ip.as_str(), WIZ_PORT)).is_ok() {
            if let Ok((len, _)) = socket.recv_from(&mut buf) {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&buf[..len]) {
                    if let Some(res) = val.get("result") {
                        if let Some(b) = map.get_mut(&ip) {
                            b.state = res.get("state").and_then(|v| v.as_bool()).unwrap_or(b.state);
                            b.dimming = res
                                .get("dimming")
                                .and_then(|v| v.as_u64())
                                .map(|v| v.clamp(10, 100) as u8)
                                .unwrap_or(b.dimming);
                            b.temp = res.get("temp").and_then(|v| v.as_u64()).map(|v| v as u16);
                            b.r = res.get("r").and_then(|v| v.as_u64()).map(|v| v as u8);
                            b.g = res.get("g").and_then(|v| v.as_u64()).map(|v| v as u8);
                            b.b = res.get("b").and_then(|v| v.as_u64()).map(|v| v as u8);
                            b.scene_id = res.get("sceneId").and_then(|v| v.as_u64()).map(|v| v as u16);
                        }
                    }
                }
            }
        }
    }

    let bulbs: Vec<DiscoveredBulb> = map.into_values().collect();
    let _ = save_cached_bulbs(cache_file, &bulbs);
    Ok(bulbs)
}

#[derive(Debug, Clone, Deserialize)]
pub struct WizControlParams {
    pub ip: String,
    pub state: Option<bool>,
    pub dimming: Option<u8>,
    pub temp: Option<u16>,
    pub r: Option<u8>,
    pub g: Option<u8>,
    pub b: Option<u8>,
    pub scene_id: Option<u16>,
}

pub fn build_set_pilot_payload(params: &WizControlParams) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    if let Some(s) = params.state {
        map.insert("state".into(), json!(s));
    }
    if let Some(d) = params.dimming {
        map.insert("dimming".into(), json!(d.clamp(10, 100)));
    }
    if let Some(t) = params.temp {
        map.insert("temp".into(), json!(t.clamp(2200, 6500)));
    }
    if let (Some(r), Some(g), Some(b)) = (params.r, params.g, params.b) {
        map.insert("r".into(), json!(r));
        map.insert("g".into(), json!(g));
        map.insert("b".into(), json!(b));
    }
    if let Some(scene) = params.scene_id {
        map.insert("sceneId".into(), json!(scene));
    }
    json!({
        "method": "setPilot",
        "params": map
    })
}

pub fn set_pilot(params: &WizControlParams) -> Result<bool> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(800)))?;
    let payload = serde_json::to_vec(&build_set_pilot_payload(params))?;
    socket.send_to(&payload, (params.ip.as_str(), WIZ_PORT))?;
    let mut buf = [0u8; 512];
    match socket.recv_from(&mut buf) {
        Ok((len, _)) => {
            let v: serde_json::Value = serde_json::from_slice(&buf[..len])?;
            Ok(v.get("result")
                .and_then(|r| r.get("success"))
                .and_then(|s| s.as_bool())
                .unwrap_or(false))
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pilot_clamps_dimming_and_temp() {
        let payload = build_set_pilot_payload(&WizControlParams {
            ip: "192.168.1.50".into(),
            state: Some(true),
            dimming: Some(2),
            temp: Some(9000),
            r: None,
            g: None,
            b: None,
            scene_id: None,
        });
        assert_eq!(payload["method"], "setPilot");
        assert_eq!(payload["params"]["dimming"], 10);
        assert_eq!(payload["params"]["temp"], 6500);
        assert_eq!(payload["params"]["state"], true);
    }
}
