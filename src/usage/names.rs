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
/// Widest name, in terminal columns (the fleet table's name column).
pub const NAME_MAX: usize = 24;
/// Widest default part (org short form, email local part), in columns.
const PART_MAX: usize = 16;
/// Widest part in a disambiguated default (`org·local`): 10 + 1 + 10, room left for `-NN`.
const PAIR_PART_MAX: usize = 10;

fn cells(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// The longest prefix of `s` at most `max` columns wide (CJK-safe: whole chars only).
fn clip(s: &str, max: usize) -> String {
    let mut w = 0;
    s.chars()
        .take_while(|c| {
            w += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
            w <= max
        })
        .collect()
}

/// Operator-set names: alias → name.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Names(pub BTreeMap<String, String>);

/// A name the operator may set: 1..=24 columns wide, no control char, BOM or whitespace other
/// than a plain space (the pane's `isDisplayName` uses JS `\s`, which includes U+FEFF), no `@`
/// (never an email), and not an `acct-` key. Every default name passes it too.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && cells(s) <= NAME_MAX
        && s.trim() == s
        && !s.chars().any(|c| {
            c.is_control() || c == '@' || c == '\u{FEFF}' || (c.is_whitespace() && c != ' ')
        })
        && !key_like(s)
}

/// Providers whose account keys are `<provider>-<sha8>` (`collect::root_alias`).
pub const KEY_PROVIDERS: [&str; 1] = ["codex"];

/// An internal account key, never a name: `acct-…`, or a [`KEY_PROVIDERS`] key such as
/// `codex-4cc1b5c8` (8 lowercase hex). A chosen `ci-20261105` is a name, not a key.
pub fn key_like(s: &str) -> bool {
    s.starts_with("acct-")
        || s.split_once('-').is_some_and(|(p, h)| {
            KEY_PROVIDERS.contains(&p)
                && h.len() == 8
                && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// The name a key gives when nothing else does: `codex` for `codex-4cc1b5c8`, the key itself
/// when someone chose it (`AUTOPILOT_ACCOUNT`), `None` for an `acct-` key.
fn key_default(key: &str) -> Option<&str> {
    if key.starts_with("acct-") {
        None
    } else if key_like(key) {
        key.split_once('-').map(|(p, _)| p)
    } else {
        Some(key)
    }
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

/// The names in `path`, for drawing (statusline runs this every refresh): a missing, unreadable
/// or unparseable file is no names; an entry whose name fails [`valid_name`] is skipped (the file
/// is hand-editable).
pub fn load(path: &Path) -> Names {
    let Ok(Some(t)) = read_table(path) else {
        return Names::default();
    };
    let mut out = BTreeMap::new();
    if let Some(toml::Value::Table(names)) = t.get("names") {
        for (k, v) in names {
            if let Some(v) = v.as_str() {
                if !k.is_empty() && valid_name(v) {
                    out.insert(k.clone(), v.to_string());
                }
            }
        }
    }
    Names(out)
}

/// The file as a table; `None` when it does not exist. A read or parse error is an error.
fn read_table(path: &Path) -> Result<Option<toml::Table>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("讀不到名稱檔 {}", path.display())),
    };
    let t = text.parse::<toml::Table>().with_context(|| {
        format!(
            "名稱檔 {} 不是合法的 TOML，沒有修改；請先修好它",
            path.display()
        )
    })?;
    Ok(Some(t))
}

/// Set `key`'s name in `path`, keeping everything else in the file as it is (other names,
/// entries [`load`] skips, other tables). A file that cannot be read or parsed is left
/// untouched and is an error; so is a `names` key that is not a table.
pub fn set(path: &Path, key: &str, name: &str) -> Result<()> {
    let mut t = read_table(path)?.unwrap_or_default();
    let names = t
        .entry("names")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let toml::Value::Table(names) = names else {
        anyhow::bail!(
            "名稱檔 {} 的 names 不是表格，沒有修改；請先修好它",
            path.display()
        );
    };
    names.insert(key.to_string(), toml::Value::String(name.to_string()));
    write_table(path, &t)
}

/// The names from [`default_path`] (none when there is no path).
pub fn load_default() -> Names {
    default_path().map(|p| load(&p)).unwrap_or_default()
}

/// Write `table` to `path`: 0600, atomic, the directory created 0700.
fn write_table(path: &Path, table: &toml::Table) -> Result<()> {
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
    let body = format!(
        "# codeforge 帳號顯示名（codeforge usage alias set 寫入）。key 是內部帳號 key，只在這裡出現。\n{}",
        toml::to_string(table)?
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
    clip(s, PART_MAX)
}

/// `personal`, or the org's first word lowercased.
fn org_short(org: &str) -> String {
    let first = org.split_whitespace().next().unwrap_or(org);
    part(&first.to_lowercase())
}

fn local_part(email: &str) -> String {
    part(email.split('@').next().unwrap_or(email))
}

/// `org·local`, each part cut so the pair and a `-NN` suffix fit [`NAME_MAX`].
fn pair(org: &str, email: &str) -> String {
    format!(
        "{}·{}",
        clip(&org_short(org), PAIR_PART_MAX),
        clip(&local_part(email), PAIR_PART_MAX)
    )
}

/// The display name of each of `items`, in order: the set name, else the org short form, else
/// the email local part, else the key itself when it is a name someone chose (`AUTOPILOT_ACCOUNT`),
/// else the provider of a `<provider>-<8 hex>` key (`codex`); `None` for an `acct-` key with none
/// of these. Default names that collide get `·<email local part>`, and what still
/// collides gets the smallest free `-2`, `-3`… in key order (a set name is never renamed), so no
/// two accounts end with the same name.
pub fn assign(items: &[Who], names: &Names) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = items
        .iter()
        .map(|w| {
            names.0.get(w.key).cloned().or_else(|| {
                // the first default that is itself a valid name (an org or email local part
                // starting `acct-` is skipped, so a key-like string never shows)
                [
                    w.org.map(org_short),
                    w.email.map(local_part),
                    key_default(w.key).map(part),
                ]
                .into_iter()
                .flatten()
                .find(|s| valid_name(s))
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
                let p = pair(org, email);
                if valid_name(&p) {
                    out[i] = Some(p);
                }
            }
        }
    }
    let mut taken: std::collections::HashSet<String> = out.iter().flatten().cloned().collect();
    let mut groups = groups(&out);
    groups.sort_by_key(|g| g.iter().map(|&i| items[i].key).min());
    for mut g in groups {
        // a set name keeps its place and is never renamed; defaults are numbered after it,
        // each with the smallest suffix no other account already shows
        g.sort_by_key(|&i| (!set(i), items[i].key));
        for i in g.into_iter().skip(1) {
            if set(i) {
                continue;
            }
            if let Some(base) = out[i].clone() {
                let n = (2..)
                    .find(|n| !taken.contains(&format!("{base}-{n}")))
                    .unwrap_or(2);
                let s = format!("{base}-{n}");
                taken.insert(s.clone());
                out[i] = Some(s);
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
            "找不到「{}」：請用帳號的 email 或 `codeforge usage alias list` 顯示的名字",
            shown_who(who)
        ),
        _ => anyhow::bail!(
            "「{}」對到 {} 個帳號，請改用 email 指定",
            shown_who(who),
            hits.len()
        ),
    }
}

/// `who` for an error message: an email is never echoed (errors reach stderr and logs).
fn shown_who(who: &str) -> &str {
    if who.contains('@') {
        "該 email"
    } else {
        who
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
    fn set_writes_a_0600_file_and_keeps_everything_else_in_it() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub/accounts.toml");
        set(&p, "acct-11111111", "work").unwrap();
        assert_eq!(
            load(&p),
            Names(BTreeMap::from([("acct-11111111".into(), "work".into())]))
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(m, 0o600);
        }
        // hand-edited: entries load skips stay in the file across a set
        std::fs::write(
            &p,
            "[names]\n\"acct-1\" = \"ok\"\n\"codex-1a2b3c4d\" = \"mycodex\"\n\"acct-2\" = \"a@b.c\"\n\"acct-3\" = 5\n[other]\nk = 1\n",
        )
        .unwrap();
        assert_eq!(
            load(&p),
            Names(BTreeMap::from([
                ("acct-1".into(), "ok".into()),
                ("codex-1a2b3c4d".into(), "mycodex".into()),
            ]))
        );
        set(&p, "acct-4", "new").unwrap();
        let t: toml::Table = std::fs::read_to_string(&p).unwrap().parse().unwrap();
        assert_eq!(t["names"]["acct-2"].as_str(), Some("a@b.c"));
        assert_eq!(t["names"]["acct-3"].as_integer(), Some(5));
        assert_eq!(t["other"]["k"].as_integer(), Some(1));
        assert_eq!(load(&p).0.len(), 3);
        // a file that does not parse is drawn as no names, and set refuses to touch it
        std::fs::write(&p, "not toml [").unwrap();
        assert_eq!(load(&p), Names::default());
        assert!(set(&p, "acct-1", "x").is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "not toml [");
        std::fs::write(&p, "names = 3\n").unwrap();
        assert!(set(&p, "acct-1", "x").is_err());
        assert_eq!(load(&d.path().join("missing.toml")), Names::default());
    }

    #[test]
    fn defaults_fit_the_name_column_and_numbering_never_lands_on_a_taken_name() {
        let got = assign(
            &[
                who(
                    "acct-1",
                    Some("Internationalization Corp"),
                    Some("jonathan.smith@example.com"),
                ),
                who(
                    "acct-2",
                    Some("Internationalization Corp"),
                    Some("jonathan.smyth@example.com"),
                ),
                who(
                    "acct-3",
                    Some("研發中心研發中心研發中心"),
                    Some("a@example.com"),
                ),
            ],
            &Names::default(),
        );
        let got: Vec<String> = got.into_iter().map(Option::unwrap).collect();
        for n in &got {
            assert!(cells(n) <= NAME_MAX, "{n}");
        }
        assert_eq!(got[0], "internatio·jonathan.s");
        assert_eq!(got[1], "internatio·jonathan.s-2");
        assert_eq!(got[2], "研發中心研發中心");
        // personal ×2 beside an org that already shows as personal-2
        let got = assign(
            &[
                who("acct-1", Some("personal"), None),
                who("acct-2", Some("personal"), None),
                who("acct-3", Some("Personal-2"), None),
            ],
            &Names::default(),
        );
        let got: Vec<String> = got.into_iter().map(Option::unwrap).collect();
        let mut uniq = got.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), 3, "{got:?}");
    }

    #[test]
    fn a_default_that_looks_like_a_key_is_skipped() {
        let got = assign(
            &[
                who("acct-1", None, Some("acct-ops@example.com")),
                who("acct-2", Some("acct-labs"), Some("dev@example.com")),
                who("acct-3", Some("acct-x"), Some("acct-y@example.com")),
            ],
            &Names::default(),
        );
        assert_eq!(got, [None, Some("dev".into()), None]);
    }

    #[test]
    fn a_provider_hash_key_is_shown_by_its_provider_never_the_hash() {
        let got = assign(
            &[
                who("codex-4cc1b5c8", None, None),
                who("codex-0a1b2c3d", None, None),
                who("work-laptop", None, None),
            ],
            &Names::default(),
        );
        assert_eq!(
            got,
            [
                Some("codex-2".into()),
                Some("codex".into()),
                Some("work-laptop".into())
            ]
        );
        let set = Names([("codex-4cc1b5c8".to_string(), "公司 codex".to_string())].into());
        let got = assign(&[who("codex-4cc1b5c8", None, None)], &set);
        assert_eq!(got, [Some("公司 codex".into())]);
        assert!(!valid_name("codex-4cc1b5c8"));
        // a chosen name that only ends in 8 hex digits is kept as is
        for ok in [
            "codex-2",
            "work-laptop",
            "ci-20261105",
            "box-cafebabe",
            "codex-DEADBEEF",
        ] {
            assert!(valid_name(ok), "{ok}");
        }
        let got = assign(
            &[
                who("ci-20261105", None, None),
                who("ci-20261106", None, None),
            ],
            &Names::default(),
        );
        assert_eq!(
            got,
            [Some("ci-20261105".into()), Some("ci-20261106".into())]
        );
    }

    #[test]
    fn a_resolve_error_never_echoes_an_email() {
        let items = [who("acct-1", Some("Stranity"), Some("a@example.com"))];
        let e = resolve(&items, &Names::default(), "typo@example.com").unwrap_err();
        assert!(!format!("{e:#}").contains("typo@example.com"), "{e:#}");
        let e = resolve(&items, &Names::default(), "nobody").unwrap_err();
        assert!(format!("{e:#}").contains("nobody"));
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
            &"工".repeat(13),
            "工作\u{FEFF}",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
