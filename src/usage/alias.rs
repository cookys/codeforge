//! Account aliases (G36 plan §1). The alias is the only account identifier
//! G36 ever writes: `AUTOPILOT_ACCOUNT` when it is a valid opaque alias (set by
//! autopilot's account-pinned dispatch, G37), else a hash of the config dir.
//! Never an email, account uuid, token or path.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The env var autopilot G37 sets in a spawned session (agreed with autopilot,
/// fleet msg 01M6FKM6GMYRRV2S1269KEJWAC).
pub const ACCOUNT_ENV: &str = "AUTOPILOT_ACCOUNT";
/// Longest alias accepted.
pub const ALIAS_MAX: usize = 24;

/// `^[a-z0-9-]{1,24}$`. Anything else (an email, a uuid with capitals, a path,
/// an empty string) is not an alias and is never written.
pub fn valid_alias(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= ALIAS_MAX
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// First 8 hex chars of sha256 of the canonical path (the path as given when it
/// cannot be canonicalized, e.g. not created yet).
pub fn path_hash8(path: &Path) -> String {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let digest = Sha256::digest(canon.as_os_str().as_encoded_bytes());
    digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

/// The Claude config dir: `CLAUDE_CONFIG_DIR` when set and non-empty, else
/// `<home>/.claude`. Env values are parameters (no `std::env` here: env-var test
/// seams race under `cargo test`).
pub fn claude_config_dir(env_config_dir: Option<&str>, home: &Path) -> PathBuf {
    match env_config_dir {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => home.join(".claude"),
    }
}

/// The account alias of a Claude session: the G37 alias when valid, else
/// `acct-` + hash8(config dir).
pub fn claude_alias(env_account: Option<&str>, config_dir: &Path) -> String {
    match env_account {
        Some(a) if valid_alias(a) => a.to_string(),
        _ => format!("acct-{}", path_hash8(config_dir)),
    }
}

/// This process's config dir and alias, read from the environment (the one place G36
/// reads `AUTOPILOT_ACCOUNT`; the collector never does).
pub fn current_claude_account() -> (PathBuf, String) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cfg = std::env::var("CLAUDE_CONFIG_DIR").ok();
    let acct = std::env::var(ACCOUNT_ENV).ok();
    let dir = claude_config_dir(cfg.as_deref(), &home);
    let alias = claude_alias(acct.as_deref(), &dir);
    (dir, alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_alias_accepts_only_the_opaque_form() {
        for ok in ["acct-a", "a", "0", "acct-1a2b3c4d", &"a".repeat(24)] {
            assert!(valid_alias(ok), "{ok}");
        }
        for bad in [
            "",
            &"a".repeat(25),
            "Acct-A",
            "me@example.com",
            "acct_a",
            "acct a",
            "/home/x/.claude",
            "acct-ä",
            "12345678-ABCD-4ef0-9abc-0123456789ab",
        ] {
            assert!(!valid_alias(bad), "{bad}");
        }
    }

    #[test]
    fn alias_takes_a_valid_env_and_falls_back_to_the_dir_hash() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(claude_alias(Some("acct-b"), dir.path()), "acct-b");
        let h = claude_alias(None, dir.path());
        assert!(
            h.starts_with("acct-") && h.len() == 13 && valid_alias(&h),
            "{h}"
        );
        // an invalid value is never used, and never leaks into the alias
        let bad = claude_alias(Some("me@example.com"), dir.path());
        assert_eq!(bad, h);
        assert_eq!(claude_alias(Some(""), dir.path()), h);
        // two dirs, two aliases; a symlink to the same dir, the same alias
        let other = tempfile::tempdir().unwrap();
        assert_ne!(claude_alias(None, other.path()), h);
        #[cfg(unix)]
        {
            let link = other.path().join("link");
            std::os::unix::fs::symlink(dir.path(), &link).unwrap();
            assert_eq!(claude_alias(None, &link), h);
        }
    }

    #[test]
    fn config_dir_defaults_to_home_dot_claude() {
        let home = Path::new("/home/t");
        assert_eq!(claude_config_dir(None, home), home.join(".claude"));
        assert_eq!(claude_config_dir(Some(""), home), home.join(".claude"));
        assert_eq!(
            claude_config_dir(Some("/x/cfg"), home),
            PathBuf::from("/x/cfg")
        );
    }
}
