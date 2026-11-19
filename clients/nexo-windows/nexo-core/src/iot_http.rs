//! Server REST client for `/iot/*` and `/iot/ac/*` endpoints mirroring `IotDevicesActivity.kt`.

use crate::config::https_base_from_wss;
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct IotHttpClient {
    http: Client,
    base_url: String,
    bearer_token: String,
    device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcOtpVerifyPayload {
    pub phone: String,
    pub otp_code: String,
}

impl IotHttpClient {
    pub fn from_wss(wss_url: &str, bearer_token: &str, device_id: &str) -> Result<Self> {
        let base_url = https_base_from_wss(wss_url)
            .ok_or_else(|| anyhow!("tidak dapat membentuk URL HTTPS dari endpoint WSS"))?;
        let http = Client::builder()
            .timeout(Duration::from_secs(12))
            .build()?;
        Ok(Self {
            http,
            base_url,
            bearer_token: bearer_token.to_string(),
            device_id: device_id.to_string(),
        })
    }

    pub async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.bearer_token)
            .header("X-Device-ID", &self.device_id)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.context("parse JSON response")?;
        if !status.is_success() {
            return Err(anyhow!("HTTP {}: {}", status.as_u16(), body));
        }
        Ok(body)
    }

    pub async fn post_json(&self, path: &str, payload: Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.bearer_token)
            .header("X-Device-ID", &self.device_id)
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.context("parse JSON response")?;
        if !status.is_success() {
            return Err(anyhow!("HTTP {}: {}", status.as_u16(), body));
        }
        Ok(body)
    }

    pub async fn iot_status(&self) -> Result<Value> {
        self.get_json("/iot/status").await
    }

    pub async fn iot_control(&self, payload: Value) -> Result<Value> {
        self.post_json("/iot/control", payload).await
    }

    pub async fn ac_status(&self, serial: Option<&str>) -> Result<Value> {
        match serial {
            Some(s) if !s.is_empty() => self.get_json(&format!("/iot/ac/status?serial={s}")).await,
            _ => self.get_json("/iot/ac/status").await,
        }
    }

    pub async fn ac_devices(&self) -> Result<Value> {
        self.get_json("/iot/ac/devices").await
    }

    pub async fn ac_request_otp(&self, phone: &str) -> Result<Value> {
        self.post_json("/iot/ac/auth/request-otp", json!({ "phone": phone }))
            .await
    }

    pub async fn ac_verify_otp(&self, phone: &str, otp_code: &str) -> Result<Value> {
        self.post_json(
            "/iot/ac/auth/verify-otp",
            json!({ "phone": phone, "otp_code": otp_code }),
        )
        .await
    }
}
