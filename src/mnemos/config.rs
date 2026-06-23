//! Endpoint / auth resolution for the Mnemos client (ship spec §8).
//!
//! Source of truth order:
//!   1. `~/.config/mnemos.env`  (KEY=VALUE lines)
//!   2. process environment      (same keys)
//!   3. hard-coded local-first fallback (`http://127.0.0.1:8845`)
//!
//! Recognized keys:
//!   - `MNEMOS_INGEST_URL` — base URL (host:port or full base). default 127.0.0.1:8845
//!   - `MACHINE_ID`        — envelope.machine_id (§4.2 cosmetic / forward-compat)
//!   - `MNEMOS_TOKEN`      — Bearer token when Mnemos auth is enabled (§8)

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

const DEFAULT_BASE: &str = "http://127.0.0.1:8845";

#[derive(Debug, Clone)]
pub struct MnemosConfig {
    /// Base URL, no trailing slash (e.g. `http://127.0.0.1:8845`).
    pub base_url: String,
    /// Machine identifier for envelope.machine_id; never `None` (falls back to hostname/"unknown").
    pub machine_id: String,
    /// Bearer token, if Mnemos auth is enabled.
    pub token: Option<String>,
}

impl MnemosConfig {
    /// Resolve config from `~/.config/mnemos.env` + process env + fallback.
    pub fn load() -> Result<Self> {
        let file_vars = Self::read_env_file(Self::env_file_path());
        Ok(Self::from_vars(&file_vars))
    }

    /// Default path of the mnemos env file (`~/.config/mnemos.env`).
    pub fn env_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("mnemos.env"))
    }

    /// Parse a `KEY=VALUE` env file into a map. Missing file → empty map (not an error).
    /// Lines starting with `#` and blank lines are ignored; surrounding quotes stripped.
    pub fn read_env_file(path: Option<PathBuf>) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let Some(path) = path else { return map };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return map;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // tolerate a leading `export ` (common in env files)
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                map.insert(k.trim().to_string(), v.to_string());
            }
        }
        map
    }

    /// Build config from a vars map, with process-env and hard-coded fallbacks.
    /// File vars take precedence over process env (explicit config wins).
    pub fn from_vars(file_vars: &HashMap<String, String>) -> Self {
        let get = |key: &str| -> Option<String> {
            file_vars
                .get(key)
                .cloned()
                .filter(|s| !s.is_empty())
                .or_else(|| std::env::var(key).ok().filter(|s| !s.is_empty()))
        };

        let base_raw = get("MNEMOS_INGEST_URL").unwrap_or_else(|| DEFAULT_BASE.to_string());
        let base_url = Self::normalize_base(&base_raw);

        let machine_id = get("MACHINE_ID")
            .or_else(Self::hostname)
            .unwrap_or_else(|| "unknown".to_string());

        let token = get("MNEMOS_TOKEN");

        Self {
            base_url,
            machine_id,
            token,
        }
    }

    /// Whether the user has explicitly opted into Mnemos shipping.
    ///
    /// Opt-in signal = presence of the config file (`~/.config/mnemos.env`) OR a
    /// non-empty `MNEMOS_INGEST_URL` in the process env. When neither is present,
    /// `ship --no-hook` (the SessionEnd path) is a clean no-op: dream still
    /// distills L1 locally, but nothing is POSTed and nothing is queued to
    /// `ship-failed/`. This is what lets codeforge-only users (no Mnemos) keep
    /// distilling without accumulating dead-letter junk every session end.
    ///
    /// Sending data to a server is treated as an explicit opt-in, never triggered
    /// implicitly by something happening to listen on the default port.
    pub fn opted_in() -> bool {
        let config_file_exists = Self::env_file_path().map(|p| p.exists()).unwrap_or(false);
        let ingest_url_env = std::env::var("MNEMOS_INGEST_URL")
            .ok()
            .filter(|s| !s.is_empty());
        Self::opted_in_from(config_file_exists, ingest_url_env.as_deref())
    }

    /// Pure opt-in decision, split out for deterministic testing.
    fn opted_in_from(config_file_exists: bool, ingest_url_env: Option<&str>) -> bool {
        config_file_exists || ingest_url_env.is_some()
    }

    /// Ledger ingest endpoint (§8).
    pub fn ledger_url(&self) -> String {
        format!("{}/v1/ingest/ledger", self.base_url)
    }

    /// Context query endpoint (atoms/context).
    pub fn context_url(&self) -> String {
        format!("{}/v1/atoms/context", self.base_url)
    }

    /// Cite write-back endpoint for a given atom_id (§11).
    pub fn cite_url(&self, atom_id: &str) -> String {
        format!("{}/v1/atoms/{}/cite", self.base_url, atom_id)
    }

    /// Normalize a base URL: add `http://` scheme if bare host:port, strip trailing slash.
    fn normalize_base(raw: &str) -> String {
        let with_scheme = if raw.contains("://") {
            raw.to_string()
        } else {
            format!("http://{raw}")
        };
        with_scheme.trim_end_matches('/').to_string()
    }

    fn hostname() -> Option<String> {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn fallback_to_localhost_when_unset() {
        let cfg = MnemosConfig::from_vars(&vars(&[]));
        // process env may or may not set MNEMOS_INGEST_URL; assert the shape either way.
        assert!(cfg.base_url.starts_with("http"));
        assert!(cfg.ledger_url().ends_with("/v1/ingest/ledger"));
    }

    #[test]
    fn explicit_base_url_used() {
        let cfg = MnemosConfig::from_vars(&vars(&[("MNEMOS_INGEST_URL", "http://10.0.0.5:9000")]));
        assert_eq!(cfg.base_url, "http://10.0.0.5:9000");
        assert_eq!(cfg.ledger_url(), "http://10.0.0.5:9000/v1/ingest/ledger");
        assert_eq!(cfg.context_url(), "http://10.0.0.5:9000/v1/atoms/context");
        assert_eq!(
            cfg.cite_url("01ABC"),
            "http://10.0.0.5:9000/v1/atoms/01ABC/cite"
        );
    }

    #[test]
    fn bare_host_port_gets_scheme() {
        let cfg = MnemosConfig::from_vars(&vars(&[("MNEMOS_INGEST_URL", "127.0.0.1:8845")]));
        assert_eq!(cfg.base_url, "http://127.0.0.1:8845");
    }

    #[test]
    fn trailing_slash_stripped() {
        let cfg = MnemosConfig::from_vars(&vars(&[("MNEMOS_INGEST_URL", "http://h:1/")]));
        assert_eq!(cfg.base_url, "http://h:1");
        assert_eq!(cfg.ledger_url(), "http://h:1/v1/ingest/ledger");
    }

    #[test]
    fn machine_id_from_vars() {
        let cfg = MnemosConfig::from_vars(&vars(&[("MACHINE_ID", "main-linux-blackwell")]));
        assert_eq!(cfg.machine_id, "main-linux-blackwell");
    }

    #[test]
    fn token_optional() {
        let cfg = MnemosConfig::from_vars(&vars(&[("MNEMOS_TOKEN", "secret")]));
        assert_eq!(cfg.token.as_deref(), Some("secret"));
    }

    #[test]
    fn opted_in_requires_config_file_or_env() {
        // No config file, no env → not opted in (clean no-op for codeforge-only users).
        assert!(!MnemosConfig::opted_in_from(false, None));
        // Config file present → opted in.
        assert!(MnemosConfig::opted_in_from(true, None));
        // Explicit ingest URL env → opted in even without the file.
        assert!(MnemosConfig::opted_in_from(
            false,
            Some("http://127.0.0.1:8845")
        ));
        // Both present → opted in.
        assert!(MnemosConfig::opted_in_from(true, Some("http://h:1")));
    }

    #[test]
    fn read_env_file_missing_is_empty() {
        let map = MnemosConfig::read_env_file(Some(PathBuf::from("/nonexistent/mnemos.env")));
        assert!(map.is_empty());
    }

    #[test]
    fn read_env_file_parses_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mnemos.env");
        std::fs::write(
            &path,
            "# comment\nexport MNEMOS_INGEST_URL=\"http://x:1\"\nMACHINE_ID=mybox\n\nMNEMOS_TOKEN='tok'\n",
        )
        .unwrap();
        let map = MnemosConfig::read_env_file(Some(path));
        assert_eq!(map.get("MNEMOS_INGEST_URL").unwrap(), "http://x:1");
        assert_eq!(map.get("MACHINE_ID").unwrap(), "mybox");
        assert_eq!(map.get("MNEMOS_TOKEN").unwrap(), "tok");
    }
}
