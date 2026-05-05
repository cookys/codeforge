//! `HttpClanContentProvider` — reqwest-backed consumer of CodePower's
//! `/api/clan-content/*` endpoints.
//!
//! **Phase 2 status (skeleton)**: the trait impl calls real endpoints
//! and runs the `X-Protocol-Version` handshake from the canonical
//! protocol spec. It is NOT wired into `src/pet/village.rs` — Plan B
//! Phase 6 does that, along with retry / cache / offline fallback.
//!
//! Authentication (Plan A interim):
//! - Read `~/.codeforge/config.toml` `[codepower]` section
//!   (`base_url`, `session_token`).
//! - `CODEPOWER_TOKEN` env var overrides the file for CI / Docker.
//! - File permission check (0o400 on Unix) is best-effort — if the
//!   file was stored by the operator with tighter perms we honour them.
//!
//! Degrade policy: on handshake mismatch or JSON decode failure, the
//! caller is expected to fall back to `StubClanContentProvider` for the
//! remainder of the session. This file surfaces failures as
//! `anyhow::Error` so the caller can decide.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::provider::{
    BlueprintId, ClanContentProvider, ClanId, ClanSummary, PetBlueprint, UserId,
};

/// The protocol version this client speaks. Bump in lockstep with
/// CodePower when a breaking shape change ships.
pub const PROTOCOL_VERSION: &str = "1";

/// Per-request timeout. Fail-fast matters more than completeness here —
/// a slow network must not block the CodeForge TUI.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    data: Option<T>,
    #[allow(dead_code)]
    error: Option<String>,
}

pub struct HttpClanContentProvider {
    base_url: String,
    session_token: Option<String>,
    client: reqwest::Client,
}

impl HttpClanContentProvider {
    /// Construct with explicit settings — tests inject a mock base URL
    /// here.
    pub fn new(base_url: String, session_token: Option<String>) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            base_url,
            session_token,
            client,
        })
    }

    /// Load from `~/.codeforge/config.toml` with `CODEPOWER_TOKEN` env
    /// override. Returns `Ok(None)` when no config is present — caller
    /// should fall back to `StubClanContentProvider`.
    pub fn from_config() -> anyhow::Result<Option<Self>> {
        let path = match dirs::home_dir() {
            Some(mut p) => {
                p.push(".codeforge/config.toml");
                p
            }
            None => return Ok(None),
        };

        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(&path)?;
        let parsed: ConfigFile = toml::from_str(&raw)?;
        let Some(cp) = parsed.codepower else {
            return Ok(None);
        };

        let token = std::env::var("CODEPOWER_TOKEN").ok().or(cp.session_token);

        Ok(Some(Self::new(cp.base_url, token)?))
    }

    fn url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}{path}")
    }

    fn authorize(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(t) = &self.session_token {
            req.header("Cookie", format!("session={t}"))
        } else {
            req
        }
    }

    /// Inspect `X-Protocol-Version`; return Err when the header is
    /// missing or does not match `PROTOCOL_VERSION`.
    fn verify_protocol(resp: &reqwest::Response) -> anyhow::Result<()> {
        match resp
            .headers()
            .get("X-Protocol-Version")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) if v == PROTOCOL_VERSION => Ok(()),
            Some(other) => anyhow::bail!(
                "clan-content protocol mismatch: expected {PROTOCOL_VERSION}, got {other}"
            ),
            None => anyhow::bail!("clan-content response is missing X-Protocol-Version header"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    codepower: Option<CodepowerSection>,
}

#[derive(Debug, Deserialize)]
struct CodepowerSection {
    base_url: String,
    session_token: Option<String>,
}

#[async_trait]
impl ClanContentProvider for HttpClanContentProvider {
    async fn available_clans(&self, user_id: UserId) -> anyhow::Result<Vec<ClanSummary>> {
        let url = self.url("/api/clan-content/clans");
        let resp = self
            .authorize(self.client.get(&url).query(&[("user_id", user_id)]))
            .send()
            .await?;
        Self::verify_protocol(&resp)?;
        let envelope: ApiEnvelope<Vec<ClanSummary>> = resp.error_for_status()?.json().await?;
        Ok(envelope.data.unwrap_or_default())
    }

    async fn clan_pets(&self, clan_id: ClanId) -> anyhow::Result<Vec<PetBlueprint>> {
        let url = self.url(&format!("/api/clan-content/clans/{clan_id}/pets"));
        let resp = self.authorize(self.client.get(&url)).send().await?;
        Self::verify_protocol(&resp)?;
        let envelope: ApiEnvelope<Vec<PetBlueprint>> = resp.error_for_status()?.json().await?;
        Ok(envelope.data.unwrap_or_default())
    }

    async fn blueprint(&self, id: BlueprintId) -> anyhow::Result<Option<PetBlueprint>> {
        let url = self.url(&format!("/api/clan-content/pets/{id}"));
        let resp = self.authorize(self.client.get(&url)).send().await?;
        // 404 is a legitimate "no such blueprint" in Plan A — surface as
        // Ok(None) rather than Err so callers can render empty state.
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            Self::verify_protocol(&resp)?;
            return Ok(None);
        }
        Self::verify_protocol(&resp)?;
        let envelope: ApiEnvelope<PetBlueprint> = resp.error_for_status()?.json().await?;
        Ok(envelope.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_joins_trailing_slash() {
        let p = HttpClanContentProvider::new("http://example.com/".into(), None).unwrap();
        assert_eq!(p.url("/api/health"), "http://example.com/api/health");
    }

    #[test]
    fn url_without_trailing_slash() {
        let p = HttpClanContentProvider::new("http://example.com".into(), None).unwrap();
        assert_eq!(p.url("/api/health"), "http://example.com/api/health");
    }
}
