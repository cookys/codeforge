//! Account aliases (G36 plan §1). The alias is the only account identifier
//! G36 ever writes: `AUTOPILOT_ACCOUNT` when it is a valid opaque alias (set by
//! autopilot's account-pinned dispatch, G37), else a hash of the logged-in account
//! (organizationUuid + emailAddress from the Claude config JSON), else a hash of the config dir.
//! Never an email, account uuid, token or path. Of the login file only three fields are read
//! (`oauthAccount.{organizationUuid, organizationName, emailAddress}`) and none is kept: the
//! alias is a hash and the org is a short display name.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read as _;
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

/// The account alias of a Claude session without a login file: the G37 alias when valid,
/// else `acct-` + hash8(config dir).
pub fn claude_alias(env_account: Option<&str>, config_dir: &Path) -> String {
    match env_account {
        Some(a) if valid_alias(a) => a.to_string(),
        _ => format!("acct-{}", path_hash8(config_dir)),
    }
}

/// Org display sentinel for a personal subscription.
pub const ORG_PERSONAL: &str = "personal";
/// Longest org display accepted.
pub const ORG_MAX: usize = 40;
/// The login file is never read above this size.
const LOGIN_MAX_BYTES: u64 = 32 << 20;

/// `1..=40` chars, no control char, no `@` (an email must never pass as an org).
pub fn valid_org(s: &str) -> bool {
    let n = s.chars().count();
    (1..=ORG_MAX).contains(&n) && !s.chars().any(|c| c.is_control() || c == '@')
}

/// A logged-in account as shown: an opaque alias and the org display (if known).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub alias: String,
    pub org: Option<String>,
}

/// The only three fields ever read from the login file; serde ignores everything else.
#[derive(Deserialize)]
struct Login {
    #[serde(rename = "oauthAccount")]
    oauth: Option<Acct>,
}
#[derive(Deserialize)]
struct Acct {
    #[serde(rename = "organizationUuid")]
    org_uuid: Option<String>,
    #[serde(rename = "organizationName")]
    org_name: Option<String>,
    #[serde(rename = "emailAddress")]
    email: Option<String>,
}

/// `personal` for a personal subscription (Claude names those "<name>'s Organization", or the
/// name contains an email), else the sanitized name; `None` when nothing displayable is left.
pub fn org_display(name: &str) -> Option<String> {
    let n = name.trim();
    if n.contains('@') || n.ends_with("'s Organization") || n.ends_with("\u{2019}s Organization") {
        return Some(ORG_PERSONAL.to_string());
    }
    let clean: String = n
        .chars()
        .filter(|c| !c.is_control())
        .take(ORG_MAX)
        .collect();
    let clean = clean.trim().to_string();
    (!clean.is_empty()).then_some(clean)
}

/// The identity in a login file's bytes: needs organizationUuid and emailAddress.
fn derive(bytes: &[u8]) -> Option<Identity> {
    let acct = serde_json::from_slice::<Login>(bytes).ok()?.oauth?;
    let (uuid, email) = (acct.org_uuid?, acct.email?);
    if uuid.is_empty() || email.is_empty() {
        return None;
    }
    let d = Sha256::digest(format!("{uuid}\n{email}").as_bytes());
    let hex: String = d.iter().take(4).map(|b| format!("{b:02x}")).collect();
    Some(Identity {
        alias: format!("acct-{hex}"),
        org: acct.org_name.as_deref().and_then(org_display),
    })
}

/// The login file: `<CLAUDE_CONFIG_DIR>/.claude.json` when set and non-empty, else
/// `<home>/.claude.json` (outside `~/.claude/`).
pub fn login_path(env_config_dir: Option<&str>, home: &Path) -> PathBuf {
    match env_config_dir {
        Some(d) if !d.is_empty() => PathBuf::from(d).join(".claude.json"),
        _ => home.join(".claude.json"),
    }
}

/// The login file of a transcript root: the default `~/.claude` has `~/.claude.json`, any
/// other root `<root>/.claude.json`.
pub fn login_path_for_root(root: &Path, home: &Path) -> PathBuf {
    let default = home.join(".claude");
    let same = std::fs::canonicalize(&default).is_ok_and(|d| d == root);
    if same || root == default {
        home.join(".claude.json")
    } else {
        root.join(".claude.json")
    }
}

/// `<live_base>/codeforge/usage/identity-<hash8 of the config dir>.json`.
pub fn identity_cache_path(live_base: &Path, config_dir: &Path) -> PathBuf {
    super::summary::summary_dir(live_base).join(format!("identity-{}.json", path_hash8(config_dir)))
}

/// The cache file: the login file's (mtime_ns, size) and what was derived from it.
#[derive(Serialize, Deserialize)]
struct Cached {
    mtime_ns: i64,
    size: u64,
    alias: Option<String>,
    org: Option<String>,
}

fn read_small(path: &Path, cap: u64) -> Option<Vec<u8>> {
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(cap + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= cap).then_some(buf)
}

fn write_cache(path: &Path, c: &Cached) {
    use std::io::Write as _;
    let Some(dir) = path.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let ok = o.open(&tmp).is_ok_and(|mut f| {
        f.write_all(&serde_json::to_vec(c).unwrap_or_default())
            .is_ok()
    }) && std::fs::rename(&tmp, path).is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The identity of the login file at `login`, via the cache at `cache` while the file's
/// (mtime_ns, size) is unchanged (one stat + one small read). `None`: no file, over the cap,
/// unparseable or no login in it. `read` is the reader of the login file (a seam for tests).
fn login_identity_with(
    login: &Path,
    cache: &Path,
    read: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> Option<Identity> {
    let meta = std::fs::metadata(login).ok()?;
    if !meta.is_file() || meta.len() > LOGIN_MAX_BYTES {
        return None;
    }
    let mtime_ns = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos() as i64;
    let from = |alias: Option<String>, org: Option<String>| {
        alias.filter(|a| valid_alias(a)).map(|alias| Identity {
            alias,
            org: org.filter(|o| valid_org(o)),
        })
    };
    if let Some(c) = read_small(cache, 4096).and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
    {
        if c.mtime_ns == mtime_ns && c.size == meta.len() {
            return from(c.alias, c.org);
        }
    }
    let id = read(login).and_then(|b| derive(&b));
    write_cache(
        cache,
        &Cached {
            mtime_ns,
            size: meta.len(),
            alias: id.as_ref().map(|i| i.alias.clone()),
            org: id.as_ref().and_then(|i| i.org.clone()),
        },
    );
    id
}

/// [`login_identity_with`] reading the real file.
pub fn login_identity(login: &Path, cache: &Path) -> Option<Identity> {
    login_identity_with(login, cache, &|p| read_small(p, LOGIN_MAX_BYTES))
}

/// A session's account: the G37 alias wins; else the login identity; else the config dir hash.
/// The org comes from the login whenever there is one.
pub fn account_from(
    env_account: Option<&str>,
    config_dir: &Path,
    login: Option<Identity>,
) -> Identity {
    let org = login.as_ref().and_then(|i| i.org.clone());
    let alias = match (env_account, login) {
        (Some(a), _) if valid_alias(a) => a.to_string(),
        (_, Some(i)) => i.alias,
        _ => claude_alias(None, config_dir),
    };
    Identity { alias, org }
}

/// This process's config dir and account, read from the environment (the one place G36
/// reads `AUTOPILOT_ACCOUNT`; the collector never does). Never fails: any login-file
/// problem means the config dir hash.
pub fn current_claude_account(live_base: &Path) -> (PathBuf, Identity) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cfg = std::env::var("CLAUDE_CONFIG_DIR").ok();
    let acct = std::env::var(ACCOUNT_ENV).ok();
    let dir = claude_config_dir(cfg.as_deref(), &home);
    let login = login_identity(
        &login_path(cfg.as_deref(), &home),
        &identity_cache_path(live_base, &dir),
    );
    let id = account_from(acct.as_deref(), &dir, login);
    (dir, id)
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

    fn login(uuid: &str, email: &str, name: &str) -> String {
        format!(
            r#"{{"numStartups":3,"oauthAccount":{{"accountUuid":"ACCT-SECRET","organizationUuid":"{uuid}","organizationName":"{name}","emailAddress":"{email}","organizationType":"ORGTYPE-SECRET"}},"accessToken":"sk-ant-FAKE-TOKEN"}}"#
        )
    }

    #[test]
    fn same_email_two_orgs_two_aliases_and_same_login_same_alias() {
        let a = derive(login("org-1", "me@x.com", "Acme").as_bytes()).unwrap();
        let b = derive(login("org-2", "me@x.com", "Acme").as_bytes()).unwrap();
        let a2 = derive(login("org-1", "me@x.com", "Acme").as_bytes()).unwrap();
        assert_ne!(a.alias, b.alias);
        assert_eq!(a.alias, a2.alias);
        assert!(valid_alias(&a.alias) && a.alias.len() == 13);
        // two config dirs with the same login: same alias (the hash never sees the path)
        let d1 = tempfile::tempdir().unwrap();
        let d2 = tempfile::tempdir().unwrap();
        for d in [&d1, &d2] {
            std::fs::write(
                d.path().join(".claude.json"),
                login("org-1", "me@x.com", "Acme"),
            )
            .unwrap();
        }
        let c = tempfile::tempdir().unwrap();
        let id = |d: &tempfile::TempDir| {
            login_identity(
                &d.path().join(".claude.json"),
                &c.path().join(format!("{}.json", path_hash8(d.path()))),
            )
        };
        assert_eq!(id(&d1), id(&d2));
        assert_eq!(id(&d1).unwrap().alias, a.alias);
    }

    #[test]
    fn env_alias_wins_and_missing_or_garbage_falls_back_to_the_path_hash() {
        let dir = tempfile::tempdir().unwrap();
        let id = derive(login("o", "e@x.com", "Acme").as_bytes());
        let won = account_from(Some("acct-pinned"), dir.path(), id.clone());
        assert_eq!(won.alias, "acct-pinned");
        assert_eq!(won.org.as_deref(), Some("Acme"));
        let fb = account_from(None, dir.path(), None);
        assert_eq!(fb.alias, claude_alias(None, dir.path()));
        assert_eq!(fb.org, None);
        assert_eq!(
            account_from(None, dir.path(), id.clone()).alias,
            id.unwrap().alias
        );
        // missing, garbage, no oauthAccount, missing fields
        let cache = dir.path().join("c.json");
        let f = dir.path().join(".claude.json");
        assert_eq!(login_identity(&f, &cache), None);
        for bad in [
            "not json",
            "{}",
            r#"{"oauthAccount":{"emailAddress":"a@b.c"}}"#,
            r#"{"oauthAccount":{"organizationUuid":"","emailAddress":"a@b.c"}}"#,
        ] {
            std::fs::write(&f, bad).unwrap();
            assert_eq!(login_identity(&f, &cache), None, "{bad}");
        }
    }

    #[test]
    fn personal_detection_and_org_sanitizing() {
        for p in [
            "Jo Doe's Organization",
            "me@x.com's Organization",
            "Jo\u{2019}s Organization",
            "someone@x.com",
            "  Jo's Organization ",
        ] {
            assert_eq!(org_display(p).as_deref(), Some("personal"), "{p}");
        }
        assert_eq!(org_display("Acme Corp").as_deref(), Some("Acme Corp"));
        assert_eq!(org_display("Ac\nme\u{7}").as_deref(), Some("Acme"));
        let cjk = "鍛".repeat(60);
        assert_eq!(org_display(&cjk).unwrap().chars().count(), 40);
        assert_eq!(org_display("\n\t "), None);
        assert!(valid_org("personal") && valid_org("公司"));
        assert!(
            !valid_org("")
                && !valid_org("a@b")
                && !valid_org("a\nb")
                && !valid_org(&"a".repeat(41))
        );
    }

    #[test]
    fn typed_parse_never_keeps_other_fields_and_the_cache_holds_no_secret() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".claude.json");
        std::fs::write(&f, login("o", "me@x.com", "Acme")).unwrap();
        let cache = dir.path().join("sub").join("identity.json");
        let id = login_identity(&f, &cache).unwrap();
        let out = format!("{id:?}");
        let cached = std::fs::read_to_string(&cache).unwrap();
        for text in [out, cached] {
            for secret in [
                "FAKE-TOKEN",
                "ACCT-SECRET",
                "ORGTYPE-SECRET",
                "me@x.com",
                "\"o\"",
            ] {
                assert!(!text.contains(secret), "{secret} in {text}");
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&cache).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn cache_skips_the_parse_while_mtime_and_size_are_unchanged() {
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".claude.json");
        let cache = dir.path().join("identity.json");
        std::fs::write(&f, login("o1", "me@x.com", "Acme")).unwrap();
        let n = Cell::new(0);
        let rd = |p: &Path| {
            n.set(n.get() + 1);
            std::fs::read(p).ok()
        };
        let a = login_identity_with(&f, &cache, &rd).unwrap();
        let b = login_identity_with(&f, &cache, &rd).unwrap();
        assert_eq!((n.get(), &a), (1, &b));
        // a different size reparses and yields the new identity
        std::fs::write(&f, login("o22", "me@x.com", "Acme")).unwrap();
        let c = login_identity_with(&f, &cache, &rd).unwrap();
        assert_eq!(n.get(), 2);
        assert_ne!(a.alias, c.alias);
        // "no login" is cached too
        std::fs::write(&f, "{}").unwrap();
        assert_eq!(login_identity_with(&f, &cache, &rd), None);
        assert_eq!(login_identity_with(&f, &cache, &rd), None);
        assert_eq!(n.get(), 3);
    }

    #[test]
    fn over_the_cap_is_a_fallback_and_login_paths_follow_the_rule() {
        let home = Path::new("/home/t");
        assert_eq!(login_path(None, home), home.join(".claude.json"));
        assert_eq!(login_path(Some(""), home), home.join(".claude.json"));
        assert_eq!(
            login_path(Some("/x/c"), home),
            PathBuf::from("/x/c/.claude.json")
        );
        assert_eq!(
            login_path_for_root(&home.join(".claude"), home),
            home.join(".claude.json")
        );
        assert_eq!(
            login_path_for_root(Path::new("/x/c"), home),
            PathBuf::from("/x/c/.claude.json")
        );
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".claude.json");
        let file = std::fs::File::create(&f).unwrap();
        file.set_len(LOGIN_MAX_BYTES + 1).unwrap();
        assert_eq!(login_identity(&f, &dir.path().join("c.json")), None);
    }
}
