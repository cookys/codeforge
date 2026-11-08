//! Account display names. The `acct-…` alias is an internal key and never reaches a screen:
//! an account is shown by the name the operator set (`codeforge usage alias set`), else by a
//! short form of its org (`stranity`, `personal`), with the email's local part added when two
//! accounts share it. An account with neither org nor email (an identity from before the
//! login-derived alias) has no name and is not shown; a key that is not an `acct-` hash
//! (`AUTOPILOT_ACCOUNT`, a codex account) is a chosen name and shows as itself.
//!
//! The names live in one file on the host that draws them, `~/.config/codeforge/accounts.toml`
//! (0600, in no repo), keyed by the alias, which is itself the hash of org uuid + email.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Env override of the names file (tests, or a second operator profile).
pub const ENV_FILE: &str = "CODEFORGE_ACCOUNTS_FILE";
/// Longest name, in chars.
const NAME_MAX: usize = 24;
/// Longest default part (org short form, email local part), in chars.
const PART_MAX: usize = 16;

/// Operator-set names: alias → name.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Names(pub BTreeMap<String, String>);

/// A name the operator may set: 1..=24 chars, no control char or whitespace other than a
/// plain space, no `@` (never an email), and not an `acct-` key.
pub fn valid_name(s: &str) -> bool {
    let n = s.chars().count();
    (1..=NAME_MAX).contains(&n)
        && s.trim() == s
        && !s
            .chars()
            .any(|c| c.is_control() || c == '@' || (c.is_whitespace() && c != ' '))
        && !s.starts_with("acct-")
}

/// `$CODEFORGE_ACCOUNTS_FILE`, else `$XDG_CONFIG_HOME/codeforge/accounts.toml`, else
/// `~/.config/codeforge/accounts.toml`.
pub fn default_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENV_FILE).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
    Some(base.join("codeforge").join("accounts.toml"))
}

/// The names in `path`. A missing or unreadable file is no names; an entry whose key is not an
/// `acct-` alias or whose name fails [`valid_name`] is dropped (the file is hand-editable).
pub fn load(path: &Path) -> Names {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Names::default();
    };
    let Ok(t) = text.parse::<toml::Table>() else {
        return Names::default();
    };
    let mut out = BTreeMap::new();
    if let Some(toml::Value::Table(names)) = t.get("names") {
        for (k, v) in names {
            if let Some(v) = v.as_str() {
                if k.starts_with("acct-") && valid_name(v) {
                    out.insert(k.clone(), v.to_string());
                }
            }
        }
    }
    Names(out)
}

/// The names from [`default_path`] (none when there is no path).
pub fn load_default() -> Names {
    default_path().map(|p| load(&p)).unwrap_or_default()
}

/// Write `names` to `path`: 0600, atomic, the directory created 0700.
pub fn save(path: &Path, names: &Names) -> Result<()> {
    let dir = path.parent().context("名稱檔路徑沒有上層資料夾")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)?;
    let mut table = toml::Table::new();
    table.insert(
        "names".into(),
        toml::Value::Table(
            names
                .0
                .iter()
                .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                .collect(),
        ),
    );
    let body = format!(
        "# codeforge 帳號顯示名（codeforge usage alias set 寫入）。key 是內部帳號 key，只在這裡出現。\n{}",
        toml::to_string(&table)?
    );
    let mut tmp = tempfile::Builder::new()
        .prefix(".accounts.toml.")
        .tempfile_in(dir)?;
    tmp.write_all(body.as_bytes())?;
    tmp.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))?;
    }
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// One account to name.
#[derive(Debug, Clone, Copy)]
pub struct Who<'a> {
    pub key: &'a str,
    pub org: Option<&'a str>,
    pub email: Option<&'a str>,
}

fn part(s: &str) -> String {
    s.chars().take(PART_MAX).collect()
}

/// `personal`, or the org's first word lowercased.
fn org_short(org: &str) -> String {
    let first = org.split_whitespace().next().unwrap_or(org);
    part(&first.to_lowercase())
}

fn local_part(email: &str) -> String {
    part(email.split('@').next().unwrap_or(email))
}

/// The display name of each of `items`, in order: the set name, else the org short form, else
/// the email local part, else the key itself when it is a name someone chose (`AUTOPILOT_ACCOUNT`,
/// a codex account); `None` for an `acct-` key with none of these. Default names that collide get `·<email local part>`, and what still
/// collides gets `-2`, `-3`… in key order (a set name is never renamed).
pub fn assign(items: &[Who], names: &Names) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = items
        .iter()
        .map(|w| {
            names.0.get(w.key).cloned().or_else(|| {
                w.org
                    .map(org_short)
                    .or_else(|| w.email.map(local_part))
                    .or_else(|| (!w.key.starts_with("acct-")).then(|| part(w.key)))
                    .filter(|s| !s.is_empty())
            })
        })
        .collect();
    let set = |i: usize| names.0.contains_key(items[i].key);
    let groups = |out: &[Option<String>]| {
        let mut g: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, n) in out.iter().enumerate() {
            if let Some(n) = n {
                g.entry(n.clone()).or_default().push(i);
            }
        }
        g.into_values().filter(|v| v.len() > 1).collect::<Vec<_>>()
    };
    for g in groups(&out) {
        for i in g {
            if let (false, Some(org), Some(email)) = (set(i), items[i].org, items[i].email) {
                out[i] = Some(format!("{}·{}", org_short(org), local_part(email)));
            }
        }
    }
    for mut g in groups(&out) {
        // a set name keeps its place and is never renamed; defaults are numbered after it
        g.sort_by_key(|&i| (!set(i), items[i].key));
        for (n, i) in g.into_iter().enumerate().skip(1) {
            if set(i) {
                continue;
            }
            if let Some(s) = &out[i] {
                out[i] = Some(format!("{s}-{}", n + 1));
            }
        }
    }
    out
}

/// The key of the one account in `items` that `who` names: its email (any case) or the name
/// it is shown by now. Errors (in 繁體中文) when none or several match.
pub fn resolve(items: &[Who], names: &Names, who: &str) -> Result<String> {
    let shown = assign(items, names);
    let hits: Vec<&str> = items
        .iter()
        .zip(&shown)
        .filter(|(w, n)| {
            w.email.is_some_and(|e| e.eq_ignore_ascii_case(who)) || n.as_deref() == Some(who)
        })
        .map(|(w, _)| w.key)
        .collect();
    match hits.as_slice() {
        [k] => Ok(k.to_string()),
        [] => anyhow::bail!(
            "找不到「{who}」：請用帳號的 email 或 `codeforge usage alias list` 顯示的名字"
        ),
        _ => anyhow::bail!("「{who}」對到 {} 個帳號，請改用 email 指定", hits.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_finds_one_account_by_email_or_shown_name() {
        let items = [
            who("acct-11111111", Some("Stranity"), Some("Amy@example.com")),
            who("acct-22222222", Some("personal"), None),
            who("acct-33333333", None, None),
        ];
        let n = Names::default();
        assert_eq!(
            resolve(&items, &n, "amy@example.com").unwrap(),
            "acct-11111111"
        );
        assert_eq!(resolve(&items, &n, "stranity").unwrap(), "acct-11111111");
        assert_eq!(resolve(&items, &n, "personal").unwrap(), "acct-22222222");
        // the key itself and unknown names are no match
        assert!(resolve(&items, &n, "acct-33333333").is_err());
        assert!(resolve(&items, &n, "nobody").is_err());
    }

    fn who<'a>(key: &'a str, org: Option<&'a str>, email: Option<&'a str>) -> Who<'a> {
        Who { key, org, email }
    }

    #[test]
    fn default_is_the_org_short_form_then_the_email_local_part_and_never_the_key() {
        let got = assign(
            &[
                who("acct-11111111", Some("Stranity Inc"), Some("a@example.com")),
                who("acct-22222222", Some("personal"), None),
                who("acct-33333333", None, Some("bob@example.com")),
                who("acct-44444444", None, None),
                who("work", None, None),
            ],
            &Names::default(),
        );
        assert_eq!(
            got,
            [
                Some("stranity".into()),
                Some("personal".into()),
                Some("bob".into()),
                None,
                Some("work".into()),
            ]
        );
    }

    #[test]
    fn a_shared_org_adds_the_email_local_part_and_what_still_collides_is_numbered() {
        let got = assign(
            &[
                who("acct-22222222", Some("Stranity"), Some("bob@example.com")),
                who("acct-11111111", Some("Stranity"), Some("amy@example.com")),
                who("acct-44444444", Some("personal"), None),
                who("acct-33333333", Some("personal"), None),
            ],
            &Names::default(),
        );
        assert_eq!(
            got,
            [
                Some("stranity·bob".into()),
                Some("stranity·amy".into()),
                Some("personal-2".into()),
                Some("personal".into()),
            ]
        );
    }

    #[test]
    fn a_set_name_wins_and_is_kept_as_set() {
        let names = Names(BTreeMap::from([("acct-11111111".into(), "工作".into())]));
        let got = assign(
            &[
                who("acct-11111111", Some("Stranity"), Some("a@example.com")),
                who("acct-22222222", Some("Stranity"), Some("b@example.com")),
                who("acct-33333333", None, None),
            ],
            &names,
        );
        assert_eq!(got, [Some("工作".into()), Some("stranity".into()), None]);
    }

    #[test]
    fn names_round_trip_through_a_0600_file_and_bad_entries_are_dropped() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub/accounts.toml");
        let names = Names(BTreeMap::from([("acct-11111111".into(), "work".into())]));
        save(&p, &names).unwrap();
        assert_eq!(load(&p), names);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(m, 0o600);
        }
        std::fs::write(
            &p,
            "[names]\n\"acct-1\" = \"ok\"\n\"x\" = \"no-key\"\n\"acct-2\" = \"a@b.c\"\n\"acct-3\" = 5\n",
        )
        .unwrap();
        assert_eq!(
            load(&p),
            Names(BTreeMap::from([("acct-1".into(), "ok".into())]))
        );
        std::fs::write(&p, "not toml [").unwrap();
        assert_eq!(load(&p), Names::default());
        assert_eq!(load(&d.path().join("missing.toml")), Names::default());
    }

    #[test]
    fn valid_name_rejects_emails_keys_and_control_chars() {
        for ok in ["work", "工作帳號", "stranity 2"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            " x",
            "a@b",
            "acct-1234",
            "a\tb",
            "a\nb",
            &"x".repeat(25),
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
