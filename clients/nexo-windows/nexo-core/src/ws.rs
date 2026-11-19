//! WebSocket client (tokio-tungstenite + rustls) with bearer auth & device headers.
//!
//! Ported from `clients/nexo-android/.../network/NexoWebSocketClient.kt`.

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use serde_json::Value;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub const DEFAULT_URL: &str = "wss://nexo.yhdev.my.id/ws/appliance";

pub struct WsConnection {
    sink: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
}

impl WsConnection {
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws, _resp) = connect_async(url)
            .await
            .with_context(|| format!("websocket connect to {url}"))?;
        let (sink, stream) = ws.split();
        Ok(Self { sink, stream })
    }

    /// Connect with required `Authorization: Bearer <token>` and `X-Device-ID` headers.
    pub async fn connect_with_auth(
        url: &str,
        bearer_token: &str,
        device_id: &str,
    ) -> Result<Self> {
        let mut req = url
            .into_client_request()
            .with_context(|| format!("invalid websocket URL: {url}"))?;
        let auth_val = HeaderValue::from_str(&format!("Bearer {bearer_token}"))
            .context("invalid bearer token header")?;
        let dev_val = HeaderValue::from_str(device_id)
            .context("invalid device_id header")?;
        req.headers_mut().insert("Authorization", auth_val);
        req.headers_mut().insert("X-Device-ID", dev_val);

        let (ws, _resp) = tokio::time::timeout(Duration::from_secs(15), connect_async(req))
            .await
            .context("websocket handshake timeout (15s)")?
            .with_context(|| format!("websocket connect to {url}"))?;
        let (sink, stream) = ws.split();
        Ok(Self { sink, stream })
    }

    pub async fn send_json(&mut self, value: &Value) -> Result<()> {
        let text = serde_json::to_string(value)?;
        self.sink
            .send(Message::Text(text.into()))
            .await
            .context("send json control frame")
    }

    pub async fn send_binary(&mut self, bytes: &[u8]) -> Result<()> {
        self.sink
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .context("send binary utterance frame")
    }

    pub async fn ping(&mut self) -> Result<()> {
        self.sink
            .send(Message::Ping(Vec::new().into()))
            .await
            .context("send ping")
    }

    pub async fn close(&mut self) -> Result<()> {
        let _ = self.sink.send(Message::Close(None)).await;
        Ok(())
    }

    pub async fn next_message(&mut self) -> Result<Option<Incoming>> {
        loop {
            match self.stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text)
                        .with_context(|| format!("parse text frame: {text}"))?;
                    return Ok(Some(Incoming::Json(value)));
                }
                Some(Ok(Message::Binary(bytes))) => {
                    return Ok(Some(Incoming::Binary(bytes.to_vec())));
                }
                Some(Ok(Message::Ping(payload))) => {
                    self.sink
                        .send(Message::Pong(payload))
                        .await
                        .context("auto-pong")?;
                }
                Some(Ok(Message::Pong(_))) => {
                    return Ok(Some(Incoming::Pong));
                }
                Some(Ok(Message::Close(_))) => return Ok(None),
                Some(Ok(Message::Frame(_))) => {}
                Some(Err(e)) => return Err(anyhow!(e)).context("websocket read error"),
                None => return Ok(None),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Incoming {
    Json(Value),
    Binary(Vec<u8>),
    Pong,
}

pub async fn connect_with_retry(
    url: &str,
    max_attempts: u32,
    base_delay: Duration,
) -> Result<WsConnection> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match WsConnection::connect(url).await {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                if attempt >= max_attempts {
                    return Err(e).context(format!(
                        "websocket connect failed after {max_attempts} attempts"
                    ));
                }
                let delay = (base_delay * 2u32.saturating_pow(attempt - 1)).min(Duration::from_secs(10));
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_url_is_wss_appliance() {
        assert_eq!(DEFAULT_URL, "wss://nexo.yhdev.my.id/ws/appliance");
        assert!(DEFAULT_URL.starts_with("wss://"));
    }

    #[tokio::test]
    async fn connect_to_unroutable_host_errors() {
        let result = WsConnection::connect("ws://127.0.0.1:1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connect_with_retry_gives_up() {
        let result = connect_with_retry("ws://127.0.0.1:1", 2, Duration::from_millis(1)).await;
        assert!(result.is_err());
    }
}
