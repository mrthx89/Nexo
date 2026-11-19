//! Runtime configuration.
//!
//! Sources the server URL, `device_id`, and the bearer token from the
//! environment and/or a config file. **Credentials are never hardcoded**: the
//! bearer token is read from `NEXO_BEARER_TOKEN` (env) or the `token` field of
//! the config file, and falls back to the literal placeholder `"[REDACTED]"`
//! which must never authenticate against a real server.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Placeholder written into config templates / defaults. Not a real token.
pub const TOKEN_PLACEHOLDER: &str = "[REDACTED]";

/// Default server endpoint (placeholder for real deployments).
pub const DEFAULT_SERVER_URL: &str = "wss://nexo.yhdev.my.id/ws/appliance";

/// Environment variable that overrides the server URL.
pub const ENV_SERVER_URL: &str = "NEXO_SERVER_URL";
/// Environment variable that overrides the device id.
pub const ENV_DEVICE_ID: &str = "NEXO_DEVICE_ID";
/// Environment variable that carries the bearer token.
pub const ENV_BEARER_TOKEN: &str = "NEXO_BEARER_TOKEN";
/// Environment variable that points at the Silero VAD ONNX model.
pub const ENV_VAD_MODEL: &str = "NEXO_VAD_MODEL";
/// Environment variable that selects the config file path.
pub const ENV_CONFIG_FILE: &str = "NEXO_CONFIG";

/// On-disk config file schema. All fields optional so a partial file works.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub server_url: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    /// Bearer token. Never commit a real value — use the placeholder.
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub vad_model: Option<String>,
}

/// Resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub server_url: String,
    pub device_id: String,
    /// The bearer token, sourced from env or config file. May be the
    /// placeholder — callers must treat it as unusable until overridden.
    pub bearer_token: String,
    /// Path to the Silero VAD ONNX model, if known.
    pub vad_model: Option<PathBuf>,
}

impl Config {
    /// Load configuration with the precedence:
    ///   env > config file > built-in default.
    ///
    /// The config file path is taken from `$NEXO_CONFIG` (if set), else
    /// `./nexo.toml`. A missing file is not an error — defaults are used.
    pub fn load() -> Result<Self> {
        let path = config_path();
        let file = read_config_file(&path)?;
        Self::from_parts(file)
    }

    fn from_parts(file: ConfigFile) -> Result<Self> {
        let server_url = std::env::var(ENV_SERVER_URL)
            .ok()
            .filter(|s| !s.is_empty())
            .or(file.server_url)
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());

        let device_id = std::env::var(ENV_DEVICE_ID)
            .ok()
            .filter(|s| !s.is_empty())
            .or(file.device_id)
            .unwrap_or_else(|| "nexo-headless".to_string());

        let bearer_token = std::env::var(ENV_BEARER_TOKEN)
            .ok()
            .filter(|s| !s.is_empty())
            .or(file.token)
            .unwrap_or_else(|| TOKEN_PLACEHOLDER.to_string());

        let vad_model = std::env::var(ENV_VAD_MODEL)
            .ok()
            .filter(|s| !s.is_empty())
            .or(file.vad_model)
            .map(PathBuf::from);

        Ok(Self {
            server_url,
            device_id,
            bearer_token,
            vad_model,
        })
    }

    /// True when the bearer token is still the unusable placeholder.
    pub fn has_real_token(&self) -> bool {
        self.bearer_token != TOKEN_PLACEHOLDER && !self.bearer_token.is_empty()
    }
}

fn config_path() -> PathBuf {
    std::env::var(ENV_CONFIG_FILE)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("nexo.toml"))
}

fn read_config_file(path: &Path) -> Result<ConfigFile> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read config file {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse config file {}", path.display()))
}

/// Write a template config file with the placeholder token, so the operator
/// knows exactly which field to fill in. Refuses to overwrite an existing
/// file unless `force` is set.
pub fn write_template(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(anyhow!("config file {} already exists", path.display()));
    }
    let template = format!(
        "# Nexo headless client configuration\n\
         # Fill in `token` with a real bearer token before connecting.\n\
         server_url = \"{DEFAULT_SERVER_URL}\"\n\
         device_id = \"nexo-headless\"\n\
         token = \"{TOKEN_PLACEHOLDER}\"\n\
         # vad_model = \"/path/to/silero-v6.2.3.onnx\"\n"
    );
    std::fs::write(path, template)
        .with_context(|| format!("write config template to {}", path.display()))
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_not_a_real_token() {
        assert_eq!(TOKEN_PLACEHOLDER, "[REDACTED]");
    }

    #[test]
    fn defaults_apply_without_file() {
        // No env vars set in CI → defaults are used.
        let cfg = Config::from_parts(ConfigFile::default()).expect("defaults");
        assert_eq!(cfg.server_url, DEFAULT_SERVER_URL);
        assert_eq!(cfg.device_id, "nexo-headless");
        assert_eq!(cfg.bearer_token, TOKEN_PLACEHOLDER);
        assert!(!cfg.has_real_token());
    }

    #[test]
    fn file_values_are_used_when_env_absent() {
        let file = ConfigFile {
            server_url: Some("wss://example.test/ws".into()),
            device_id: Some("dev-1".into()),
            token: Some("real-token".into()),
            vad_model: Some("/models/silero.onnx".into()),
        };
        let cfg = Config::from_parts(file).expect("from file");
        assert_eq!(cfg.server_url, "wss://example.test/ws");
        assert_eq!(cfg.device_id, "dev-1");
        assert!(cfg.has_real_token());
        assert_eq!(cfg.vad_model.unwrap(), PathBuf::from("/models/silero.onnx"));
    }

    #[test]
    fn write_template_produces_placeholder() {
        let dir = std::env::temp_dir().join(format!("nexo-tpl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nexo.toml");
        write_template(&path, false).expect("write template");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(TOKEN_PLACEHOLDER));
        // Refuses to overwrite.
        assert!(write_template(&path, false).is_err());
        // Force overwrites.
        assert!(write_template(&path, true).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}