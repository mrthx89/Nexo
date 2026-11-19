//! WebSocket client (tokio-tungstenite + rustls).
//!
//! Ported from `android/.../network/NexoWebSocketClient.kt`. The server is
//! `wss://nexo.yhdev.my.id/ws/appliance`. We use `rustls` for TLS (via
//! tokio-tungstenite's `rustls-tls-webpki-roots` feature) and `tokio` for the
//! async runtime.
//!
//! Responsibilities:
//!   * connect / reconnect with bounded exponential backoff
//!   * send `client_hello` immediately after the socket opens
//!   * pump control frames (JSON) and binary frames (utterance chunks) both ways
//!   * translate transport-level failures into the errors the upload state
//!     machine expects
//!
//! The socket layer owns the wire; protocol semantics live in `protocol.rs`
//! and the upload state machine in `upload.rs`.

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Default server endpoint. Placeholder — real deployments override via config.
pub const DEFAULT_URL: &str = "wss://nexo.yhdev.my.id/ws/appliance";

/// A live WebSocket connection plus the sink/stream halves.
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
    /// Connect to `url` and complete the WebSocket upgrade.
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws, _resp) = connect_async(url)
            .await
            .with_context(|| format!("websocket connect to {url}"))?;
        let (sink, stream) = ws.split();
        Ok(Self { sink, stream })
    }

    /// Send a JSON control frame.
    pub async fn send_json(&mut self, value: &Value) -> Result<()> {
        let text = serde_json::to_string(value)?;
        self.sink
            .send(Message::Text(text.into()))
            .await
            .context("send json control frame")
    }

    /// Send a binary utterance frame.
    pub async fn send_binary(&mut self, bytes: &[u8]) -> Result<()> {
        self.sink
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .context("send binary utterance frame")
    }

    /// Send a protocol-level ping.
    pub async fn ping(&mut self) -> Result<()> {
        self.sink
            .send(Message::Ping(Vec::new().into()))
            .await
            .context("send ping")
    }

    /// Receive the next message. Returns `None` on clean close, `Err` on a
    /// transport failure. Ping/pong frames are handled internally: pings are
    /// answered automatically and pongs are surfaced to the caller as
    /// [`Incoming::Pong`].
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
                Some(Ok(Message::Frame(_))) => {} // raw frames are not surfaced
                Some(Err(e)) => return Err(anyhow!(e)).context("websocket read error"),
                None => return Ok(None),
            }
        }
    }
}

/// A message received from the server, after transport-level framing has been
/// stripped and pings auto-answered.
#[derive(Debug, Clone)]
pub enum Incoming {
    Json(Value),
    Binary(Vec<u8>),
    Pong,
}

/// Connect with bounded exponential backoff. Returns the first successful
/// connection; gives up after `max_attempts`.
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
                let delay = base_delay * 2u32.saturating_pow(attempt - 1);
                let delay = delay.min(Duration::from_secs(10));
                tracing::warn!(attempt, %e, ?delay, "websocket connect failed; retrying");
                tokio::time::sleep(delay).await;
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

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
        // A port that is guaranteed closed on localhost; connect must fail and
        // return an Err, not panic.
        let result = WsConnection::connect("ws://127.0.0.1:1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connect_with_retry_gives_up() {
        let result =
            connect_with_retry("ws://127.0.0.1:1", 2, Duration::from_millis(1)).await;
        assert!(result.is_err());
    }
}