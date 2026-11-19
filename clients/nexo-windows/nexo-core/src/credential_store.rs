//! Origin-bound local credential & identity storage mirroring `DeviceCredentialStore.kt`.
//!
//! On Windows, bearer tokens are encrypted per-user with Windows DPAPI (`CryptProtectData`)
//! before being saved to `%LOCALAPPDATA%\Nexo\credentials.json`. Never logs raw tokens.

use crate::config::{app_data_dir, url_origin};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredCredentials {
    device_id: Option<String>,
    server_url: Option<String>,
    origin: Option<String>,
    /// Hex-encoded DPAPI ciphertext on Windows (or restricted-file UTF-8 hex fallback elsewhere).
    protected_token_hex: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceCredentialStore {
    path: PathBuf,
}

impl DeviceCredentialStore {
    pub fn default_store() -> Self {
        Self {
            path: app_data_dir().join("credentials.json"),
        }
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_or_create_device_id(&self) -> Result<String> {
        let mut rec = self.read_record().unwrap_or_default();
        if let Some(id) = rec.device_id.filter(|s| !s.trim().is_empty()) {
            return Ok(id);
        }
        let id = format!("nexo_windows_{}", Uuid::new_v4().simple());
        rec.device_id = Some(id.clone());
        self.write_record(&rec)?;
        Ok(id)
    }

    pub fn save_server_url(&self, server_url: &str) -> Result<()> {
        let mut rec = self.read_record().unwrap_or_default();
        rec.server_url = Some(server_url.trim().to_string());
        self.write_record(&rec)
    }

    pub fn saved_server_url(&self) -> Option<String> {
        self.read_record()
            .ok()
            .and_then(|r| r.server_url)
            .filter(|s| !s.is_empty())
    }

    pub fn save_token(&self, server_url: &str, device_id: &str, token: &str) -> Result<()> {
        let origin = url_origin(server_url).ok_or_else(|| anyhow!("endpoint tidak valid"))?;
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("token kosong"));
        }
        let mut rec = self.read_record().unwrap_or_default();
        let payload = format!("{origin}|{device_id}|{trimmed}");
        let protected = protect_bytes(payload.as_bytes())?;
        rec.device_id = Some(device_id.to_string());
        rec.server_url = Some(server_url.trim().to_string());
        rec.origin = Some(origin);
        rec.protected_token_hex = Some(hex::encode(protected));
        self.write_record(&rec)
    }

    pub fn read_token(&self, server_url: &str, device_id: &str) -> Option<String> {
        let requested_origin = url_origin(server_url)?;
        let rec = self.read_record().ok()?;
        if rec.origin.as_deref() != Some(requested_origin.as_str()) {
            return None;
        }
        let cipher = hex::decode(rec.protected_token_hex?).ok()?;
        let plain = unprotect_bytes(&cipher).ok()?;
        let text = String::from_utf8(plain).ok()?;
        let mut parts = text.splitn(3, '|');
        let origin = parts.next()?;
        let saved_dev = parts.next()?;
        let tok = parts.next()?;
        if origin == requested_origin && saved_dev == device_id && !tok.is_empty() {
            Some(tok.to_string())
        } else {
            None
        }
    }

    pub fn clear_token(&self) -> Result<()> {
        let mut rec = self.read_record().unwrap_or_default();
        rec.origin = None;
        rec.protected_token_hex = None;
        self.write_record(&rec)
    }

    fn read_record(&self) -> Result<StoredCredentials> {
        if !self.path.exists() {
            return Ok(StoredCredentials::default());
        }
        let txt = std::fs::read_to_string(&self.path)
            .with_context(|| format!("read credentials {}", self.path.display()))?;
        Ok(serde_json::from_str(&txt)?)
    }

    fn write_record(&self, rec: &StoredCredentials) -> Result<()> {
        ensure_parent_dir(&self.path)?;
        let txt = serde_json::to_string_pretty(rec)?;
        std::fs::write(&self.path, txt)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(windows)]
fn protect_bytes(plain: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(anyhow!("DPAPI CryptProtectData failed"));
    }
    let slice = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(slice)
}

#[cfg(windows)]
fn unprotect_bytes(cipher: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: cipher.len() as u32,
        pbData: cipher.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(anyhow!("DPAPI CryptUnprotectData failed"));
    }
    let slice = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(slice)
}

#[cfg(not(windows))]
fn protect_bytes(plain: &[u8]) -> Result<Vec<u8>> {
    Ok(plain.to_vec())
}

#[cfg(not(windows))]
fn unprotect_bytes(cipher: &[u8]) -> Result<Vec<u8>> {
    Ok(cipher.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_binds_token_to_origin_and_device() {
        let dir = std::env::temp_dir().join(format!("nexo_cred_test_{}", std::process::id()));
        let store = DeviceCredentialStore::at(dir.join("credentials.json"));
        store
            .save_token("wss://nexo.yhdev.my.id/ws/appliance", "dev-1", "secret-tok-123")
            .unwrap();
        assert_eq!(
            store
                .read_token("wss://nexo.yhdev.my.id/ws/appliance", "dev-1")
                .as_deref(),
            Some("secret-tok-123")
        );
        // Different origin or device_id must fail closed
        assert!(store
            .read_token("wss://other.example/ws/appliance", "dev-1")
            .is_none());
        assert!(store
            .read_token("wss://nexo.yhdev.my.id/ws/appliance", "dev-2")
            .is_none());
        store.clear_token().unwrap();
        assert!(store
            .read_token("wss://nexo.yhdev.my.id/ws/appliance", "dev-1")
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
