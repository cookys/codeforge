//! Account aliases (G36 plan §1). The alias is the only account identifier
//! G36 ever writes: `AUTOPILOT_ACCOUNT` when it is a valid opaque alias (set by
//! autopilot's account-pinned dispatch, G37), else a hash of the logged-in account
//! (organizationUuid + emailAddress from the Claude config JSON), else none: a session with no
//! login is unattributed, never a hash of its config dir (PM 01M7052H79: such a key showed as a
//! ghost account on every host). The config dir hash is kept only as the collector's root key.
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

/// The alias an old writer gave a session without a login: `acct-` + its root's hash8. Such a
/// key names no account, so readers treat it as unattributed.
pub fn is_dir_hash_alias(alias: &str, root_hash: &str) -> bool {
    alias.strip_prefix("acct-") == Some(root_hash)
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

/// Longest email accepted (RFC 5321 path limit).
pub const EMAIL_MAX: usize = 254;

/// A login email: 3..=254 chars, exactly one `@` with something on both sides, no whitespace or
/// control char. Anything else is dropped (never an error, never echoed).
pub fn valid_email(s: &str) -> bool {
    let n = s.chars().count();
    if !(3..=EMAIL_MAX).contains(&n) {
        return false;
    }
    if s.chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\u{feff}')
    {
        return false;
    }
    let mut parts = s.split('@');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(l), Some(d), None) if !l.is_empty() && !d.is_empty()
    )
}

/// A logged-in account as shown: an opaque alias, the org display and the login email (if known).
#[derive(Clone, PartialEq, Eq)]
pub struct Identity {
    pub alias: String,
    pub org: Option<String>,
    pub email: Option<String>,
}

/// `Debug` never prints the email: an `{:?}` in a log line must not leak it.
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("alias", &self.alias)
            .field("org", &self.org)
            .field("email", &self.email.as_ref().map(|_| "<hidden>"))
            .finish()
    }
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
    let email_shown = Some(email.trim().to_string()).filter(|e| valid_email(e));
    Some(Identity {
        alias: format!("acct-{hex}"),
        org: acct.org_name.as_deref().and_then(org_display),
        email: email_shown,
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
    #[serde(default)]
    email: Option<String>,
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
    let from = |c: Cached| {
        c.alias.filter(|a| valid_alias(a)).map(|alias| Identity {
            alias,
            org: c.org.filter(|o| valid_org(o)),
            email: c.email.filter(|e| valid_email(e)),
        })
    };
    if let Some(c) = read_small(cache, 4096).and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
    {
        if c.mtime_ns == mtime_ns && c.size == meta.len() {
            return from(c);
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
            email: id.as_ref().and_then(|i| i.email.clone()),
        },
    );
    id
}

/// [`login_identity_with`] reading the real file.
pub fn login_identity(login: &Path, cache: &Path) -> Option<Identity> {
    login_identity_with(login, cache, &|p| read_small(p, LOGIN_MAX_BYTES))
}

/// A session's account: the G37 alias wins; else the login identity; else none (unattributed).
/// The org and email come from the login whenever there is one (also under a pinned alias).
pub fn account_from(env_account: Option<&str>, login: Option<Identity>) -> Option<Identity> {
    match (env_account, login) {
        (Some(a), login) if valid_alias(a) => Some(Identity {
            alias: a.to_string(),
            org: login.as_ref().and_then(|i| i.org.clone()),
            email: login.and_then(|i| i.email),
        }),
        (_, login) => login,
    }
}

/// This process's config dir and account, read from the environment (the one place G36
/// reads `AUTOPILOT_ACCOUNT`; the collector never does). Never fails: any login-file
/// problem means no account.
pub fn current_claude_account(live_base: &Path) -> (PathBuf, Option<Identity>) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cfg = std::env::var("CLAUDE_CONFIG_DIR").ok();
    let acct = std::env::var(ACCOUNT_ENV).ok();
    let dir = claude_config_dir(cfg.as_deref(), &home);
    let login = login_identity(
        &login_path(cfg.as_deref(), &home),
        &identity_cache_path(live_base, &dir),
    );
    let id = account_from(acct.as_deref(), login);
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
    fn a_dir_hash_alias_is_recognized_only_against_its_own_root() {
        // /home/cookys/.claude hashes to ae88ee5d: the ghost account of PM 01M7052H79
        assert_eq!(path_hash8(Path::new("/home/cookys/.claude")), "ae88ee5d");
        assert!(is_dir_hash_alias("acct-ae88ee5d", "ae88ee5d"));
        assert!(!is_dir_hash_alias("acct-337c98de", "ae88ee5d"));
        assert!(!is_dir_hash_alias("ae88ee5d", "ae88ee5d"));
        assert!(!is_dir_hash_alias("codex-ae88ee5d", "ae88ee5d"));
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
        let a = derive(login("org-1", "me@example.com", "Acme").as_bytes()).unwrap();
        let b = derive(login("org-2", "me@example.com", "Acme").as_bytes()).unwrap();
        let a2 = derive(login("org-1", "me@example.com", "Acme").as_bytes()).unwrap();
        assert_ne!(a.alias, b.alias);
        assert_eq!(a.alias, a2.alias);
        assert!(valid_alias(&a.alias) && a.alias.len() == 13);
        // two config dirs with the same login: same alias (the hash never sees the path)
        let d1 = tempfile::tempdir().unwrap();
        let d2 = tempfile::tempdir().unwrap();
        for d in [&d1, &d2] {
            std::fs::write(
                d.path().join(".claude.json"),
                login("org-1", "me@example.com", "Acme"),
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
    fn env_alias_wins_and_no_login_is_no_account() {
        let dir = tempfile::tempdir().unwrap();
        let id = derive(login("o", "e@x.com", "Acme").as_bytes());
        let won = account_from(Some("acct-pinned"), id.clone()).unwrap();
        assert_eq!(won.alias, "acct-pinned");
        assert_eq!(won.org.as_deref(), Some("Acme"));
        // no login: no account, never a hash of the config dir
        assert_eq!(account_from(None, None), None);
        assert_eq!(account_from(Some("me@example.com"), None), None);
        assert_eq!(account_from(Some(""), None), None);
        assert_eq!(
            account_from(Some("acct-pinned"), None).map(|i| i.alias),
            Some("acct-pinned".to_string())
        );
        assert_eq!(
            account_from(None, id.clone()).unwrap().alias,
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
            "me@example.com's Organization",
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
    fn email_validation_and_derivation() {
        for ok in [
            "a@b",
            "me@example.com",
            &format!("{}@example.com", "a".repeat(240)),
        ] {
            assert!(valid_email(ok), "{ok}");
        }
        for bad in [
            "",
            "ab",
            "a@",
            "@b",
            "@",
            "a@@b",
            "a@b@c",
            "no-at-sign",
            "me @example.com",
            "me@exam\nple.com",
            "me@example.com\u{7}",
            "me@exam\u{feff}ple.com",
            &format!("{}@example.com", "a".repeat(250)),
        ] {
            assert!(!valid_email(bad), "{bad:?}");
        }
        // trimmed on the way in; a bad one is None but the alias still derives
        let id = derive(login("o", "  me@example.com ", "Acme").as_bytes()).unwrap();
        assert_eq!(id.email.as_deref(), Some("me@example.com"));
        let id = derive(login("o", "not an email", "Acme").as_bytes()).unwrap();
        assert!(id.alias.starts_with("acct-") && id.email.is_none());
        // a pinned alias keeps the login's email and org
        let acct = account_from(
            Some("work-1"),
            derive(login("o", "me@example.com", "Acme").as_bytes()),
        )
        .unwrap();
        assert_eq!(
            (
                acct.alias.as_str(),
                acct.org.as_deref(),
                acct.email.as_deref()
            ),
            ("work-1", Some("Acme"), Some("me@example.com"))
        );
        // a cache written before the email existed still loads (email None)
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".claude.json");
        std::fs::write(&f, login("o", "me@example.com", "Acme")).unwrap();
        let cache = dir.path().join("identity.json");
        let meta = std::fs::metadata(&f).unwrap();
        let mtime = meta
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        std::fs::write(
            &cache,
            format!(
                r#"{{"mtime_ns":{mtime},"size":{},"alias":"acct-1a2b3c4d","org":"Acme"}}"#,
                meta.len()
            ),
        )
        .unwrap();
        let old = login_identity(&f, &cache).unwrap();
        assert_eq!((old.alias.as_str(), old.email), ("acct-1a2b3c4d", None));
    }

    #[test]
    fn typed_parse_never_keeps_other_fields_and_the_cache_holds_no_secret() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".claude.json");
        std::fs::write(&f, login("o", "me@example.com", "Acme")).unwrap();
        let cache = dir.path().join("sub").join("identity.json");
        let id = login_identity(&f, &cache).unwrap();
        let out = format!("{id:?}");
        let cached = std::fs::read_to_string(&cache).unwrap();
        // the email is the one login field the cache keeps (0600); the Debug form hides it
        assert!(cached.contains("me@example.com"), "{cached}");
        assert_eq!(id.email.as_deref(), Some("me@example.com"));
        for text in [out, cached.clone()] {
            for secret in ["FAKE-TOKEN", "ACCT-SECRET", "ORGTYPE-SECRET", "\"o\""] {
                assert!(!text.contains(secret), "{secret} in {text}");
            }
        }
        assert!(!format!("{id:?}").contains("me@example.com"));
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
        std::fs::write(&f, login("o1", "me@example.com", "Acme")).unwrap();
        let n = Cell::new(0);
        let rd = |p: &Path| {
            n.set(n.get() + 1);
            std::fs::read(p).ok()
        };
        let a = login_identity_with(&f, &cache, &rd).unwrap();
        let b = login_identity_with(&f, &cache, &rd).unwrap();
        assert_eq!((n.get(), &a), (1, &b));
        // a different size reparses and yields the new identity
        std::fs::write(&f, login("o22", "me@example.com", "Acme")).unwrap();
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
