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
pub(crate) fn clip(s: &str, max: usize) -> String {
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

/// Providers whose account keys are `<provider>-<sha8>` (codex from `collect::root_alias`,
/// agy/agyclaude from `super::agy`, grok from `super::grok` — the sha8 of the host's home dir).
pub const KEY_PROVIDERS: [&str; 4] = ["agy", "agyclaude", "codex", "grok"];

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

/// The name a key gives when nothing else does: `codex` for `codex-4cc1b5c8`, `grok-cli` for
/// `grok-<sha8>` (the local grok CLI), the key itself when someone chose it
/// (`AUTOPILOT_ACCOUNT`), `None` for an `acct-` key.
fn key_default(key: &str) -> Option<&str> {
    if key.starts_with("acct-") {
        None
    } else if key_like(key) {
        match key.split_once('-').map(|(p, _)| p) {
            Some("grok") => Some("grok-cli"),
            other => other,
        }
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

/// Collector sources share accounts.toml with names/plans. Unlike display hints, invalid
/// source configuration is an error so a typo cannot silently omit usage.
pub fn load_claude_dirs(path: &Path) -> Result<Vec<PathBuf>> {
    let Some(t) = read_table(path)? else {
        return Ok(Vec::new());
    };
    let Some(collect) = t.get("collect") else {
        return Ok(Vec::new());
    };
    let collect = collect
        .as_table()
        .context("accounts.toml 的 collect 必須是表格")?;
    let Some(dirs) = collect.get("claude_dirs") else {
        return Ok(Vec::new());
    };
    dirs.as_array()
        .context("accounts.toml 的 collect.claude_dirs 必須是陣列")?
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let value = value.as_str().with_context(|| {
                format!("accounts.toml 的 collect.claude_dirs[{i}] 必須是路徑字串")
            })?;
            let dir = PathBuf::from(value);
            if !dir.is_absolute() {
                anyhow::bail!(
                    "accounts.toml 的 collect.claude_dirs[{i}] 必須是絕對路徑：「{value}」"
                );
            }
            Ok(dir)
        })
        .collect()
}

/// Set `key`'s name in `path`, keeping everything else in the file as it is (other names,
/// entries [`load`] skips, other tables). A file that cannot be read or parsed is left
/// untouched and is an error; so is a `names` key that is not a table.
pub fn set(path: &Path, key: &str, name: &str) -> Result<()> {
    edit(path, "names", key, Some(name)).map(|_| ())
}

/// Set (`Some`) or remove (`None`) `key` in the `[table]` of `path`, keeping the rest of the file.
/// Returns whether the key was there before.
fn edit(path: &Path, table: &str, key: &str, value: Option<&str>) -> Result<bool> {
    edit_many(path, &[(table, key, value)]).map(|v| v[0])
}

/// [`edit`] for several `(table, key, value)` at once: one atomic write. Whether each key was there.
fn edit_many(path: &Path, ops: &[(&str, &str, Option<&str>)]) -> Result<Vec<bool>> {
    let mut t = read_table(path)?.unwrap_or_default();
    let mut had = Vec::new();
    for (table, key, value) in ops {
        if value.is_none() && !t.contains_key(*table) {
            had.push(false);
            continue;
        }
        let entry = t
            .entry(*table)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(entries) = entry else {
            anyhow::bail!(
                "名稱檔 {} 的 {table} 不是表格，沒有修改；請先修好它",
                path.display()
            );
        };
        had.push(match value {
            Some(v) => entries
                .insert(key.to_string(), toml::Value::String(v.to_string()))
                .is_some(),
            None => entries.remove(*key).is_some(),
        });
    }
    write_table(path, &t)?;
    Ok(had)
}

/// Widest operator note, in terminal columns.
pub const NOTE_MAX: usize = 40;

/// A note the operator may attach: 1..=40 columns, trimmed, no `@`, no control char, BOM or
/// whitespace other than a plain space (the display-name rule, wider and without the key rule).
pub fn valid_note(s: &str) -> bool {
    !s.is_empty()
        && cells(s) <= NOTE_MAX
        && s.trim() == s
        && !s.chars().any(|c| {
            c.is_control() || c == '@' || c == '\u{FEFF}' || (c.is_whitespace() && c != ' ')
        })
}

/// Operator notes: a provider label (`kimi`) or an account key → short text. Local only: they go
/// into `fleet-quota.json` and the screens, never into a summary or `usage push`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Notes(pub BTreeMap<String, String>);

impl Notes {
    /// The note for an account: its key's note wins over its provider's.
    pub fn for_row(&self, key: &str, provider: &str) -> Option<&str> {
        self.0
            .get(key)
            .or_else(|| self.0.get(provider))
            .map(String::as_str)
    }
}

/// The `[notes]` of `path`, for drawing: like [`load`], any error is no notes and an entry that
/// fails [`valid_note`] is skipped.
pub fn load_notes(path: &Path) -> Notes {
    let Ok(Some(t)) = read_table(path) else {
        return Notes::default();
    };
    let mut out = BTreeMap::new();
    if let Some(toml::Value::Table(notes)) = t.get("notes") {
        for (k, v) in notes {
            if let Some(v) = v.as_str() {
                if !k.is_empty() && valid_note(v) {
                    out.insert(k.clone(), v.to_string());
                }
            }
        }
    }
    Notes(out)
}

/// The notes from [`default_path`] (none when there is no path).
pub fn load_notes_default() -> Notes {
    default_path().map(|p| load_notes(&p)).unwrap_or_default()
}

/// Operator reset rules (weekly, monthly or one-shot): the same keys as [`Notes`] → a [`super::reset_rule`] string.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Resets(pub BTreeMap<String, String>);

impl Resets {
    /// The rule for an account: its key's rule wins over its provider's.
    pub fn for_row(&self, key: &str, provider: &str) -> Option<&str> {
        self.0
            .get(key)
            .or_else(|| self.0.get(provider))
            .map(String::as_str)
    }
}

/// The `[resets]` of `path`, for drawing: any error is no rules and an invalid entry is skipped.
pub fn load_resets(path: &Path) -> Resets {
    let Ok(Some(t)) = read_table(path) else {
        return Resets::default();
    };
    let mut out = BTreeMap::new();
    if let Some(toml::Value::Table(rules)) = t.get("resets") {
        for (k, v) in rules {
            if let Some(v) = v.as_str() {
                if !k.is_empty() && super::reset_rule::valid(v) {
                    out.insert(k.clone(), v.to_string());
                }
            }
        }
    }
    Resets(out)
}

/// The rules from [`default_path`] (none when there is no path).
pub fn load_resets_default() -> Resets {
    default_path().map(|p| load_resets(&p)).unwrap_or_default()
}

/// Set `key`'s note and, when given, its rule, in one write.
pub fn set_note_and_reset(path: &Path, key: &str, note: &str, rule: Option<&str>) -> Result<()> {
    let mut ops = vec![("notes", key, Some(note))];
    if let Some(r) = rule {
        ops.push(("resets", key, Some(r)));
    }
    edit_many(path, &ops).map(|_| ())
}

/// Set only `key`'s rule.
pub fn set_reset(path: &Path, key: &str, rule: &str) -> Result<()> {
    edit(path, "resets", key, Some(rule)).map(|_| ())
}

/// Remove `key`'s note and rule; whether either was there. A missing file has neither.
pub fn clear_note_and_reset(path: &Path, key: &str) -> Result<bool> {
    if read_table(path)?.is_none() {
        return Ok(false);
    }
    let had = edit_many(path, &[("notes", key, None), ("resets", key, None)])?;
    Ok(had.contains(&true))
}

/// The closed set of subscription plan ids (fleet-quota/1 `plan`), in display order.
pub const PLANS: [&str; 6] = ["pro", "max5x", "max20x", "team", "enterprise", "api"];

/// The canonical plan id for `s` (case-insensitive; `5x`/`20x` are input aliases), else `None`.
pub fn parse_plan(s: &str) -> Option<&'static str> {
    let l = s.to_ascii_lowercase();
    let l = match l.as_str() {
        "5x" => "max5x",
        "20x" => "max20x",
        o => o,
    };
    PLANS.iter().find(|p| **p == l).copied()
}

/// The name a plan id is shown by: `Pro`, `Max 5x`, `Max 20x`, `Team`, `Enterprise`, `API`.
pub fn plan_display(id: &str) -> &'static str {
    match id {
        "pro" => "Pro",
        "max5x" => "Max 5x",
        "max20x" => "Max 20x",
        "team" => "Team",
        "enterprise" => "Enterprise",
        _ => "API",
    }
}

/// Operator-set subscription plans: the same keys as [`Notes`] → a canonical plan id. Local
/// only: they go into `fleet-quota.json` and the screens, never into a summary or `usage push`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plans(pub BTreeMap<String, String>);

impl Plans {
    /// The plan for an account: its key's plan wins over its provider's.
    pub fn for_row(&self, key: &str, provider: &str) -> Option<&'static str> {
        self.0
            .get(key)
            .or_else(|| self.0.get(provider))
            .and_then(|p| parse_plan(p))
    }
}

/// The `[plans]` of `path`, for drawing: any error is no plans and an entry that is not a plan
/// id is skipped (so is a value that is not a string).
pub fn load_plans(path: &Path) -> Plans {
    let Ok(Some(t)) = read_table(path) else {
        return Plans::default();
    };
    let mut out = BTreeMap::new();
    if let Some(toml::Value::Table(plans)) = t.get("plans") {
        for (k, v) in plans {
            if let Some(id) = v.as_str().and_then(parse_plan) {
                if !k.is_empty() {
                    out.insert(k.clone(), id.to_string());
                }
            }
        }
    }
    Plans(out)
}

/// The plans from [`default_path`] (none when there is no path).
pub fn load_plans_default() -> Plans {
    default_path().map(|p| load_plans(&p)).unwrap_or_default()
}

/// Set `key`'s plan (`plan` is a name [`parse_plan`] accepts; stored as the canonical id).
pub fn set_plan(path: &Path, key: &str, plan: &str) -> Result<()> {
    let id = parse_plan(plan).with_context(|| format!("不認得方案「{plan}」"))?;
    edit(path, "plans", key, Some(id)).map(|_| ())
}

/// Remove `key`'s plan; whether it was there. A missing file has none.
pub fn clear_plan(path: &Path, key: &str) -> Result<bool> {
    if read_table(path)?.is_none() {
        return Ok(false);
    }
    edit(path, "plans", key, None)
}

/// The provider of a `<provider>-<8 hex>` key (`codex` for `codex-4cc1b5c8`), else `None`.
pub fn key_provider(key: &str) -> Option<&str> {
    if key.starts_with("acct-") || !key_like(key) {
        return None;
    }
    key.split_once('-').map(|(p, _)| p)
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
        "# codeforge 帳號顯示名與備註（codeforge usage alias set／note set 寫入）。key 是內部帳號 key，只在這裡出現。\n{}",
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
/// else the provider of a `<provider>-<8 hex>` key (`codex`, or `grok-cli` for grok); `None` for
/// an `acct-` key with none of these. Default names that collide get `·<email local part>`, and what still
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
                who("grok-1a2b3c4d", None, None),
            ],
            &Names::default(),
        );
        assert_eq!(
            got,
            [
                Some("codex-2".into()),
                Some("codex".into()),
                Some("work-laptop".into()),
                Some("grok-cli".into()),
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

    #[test]
    fn notes_load_validate_and_set_clear_keeping_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("accounts.toml");
        assert_eq!(load_notes(&p), Notes::default());
        assert!(
            !clear_note_and_reset(&p, "kimi").unwrap(),
            "no file, no note"
        );
        std::fs::write(
            &p,
            "[names]\n\"acct-1\" = \"work\"\n[notes]\nkimi = \"週配額用完\"\nglm = \"a@b\"\nqoder = 5\nx = \" pad\"\n[other]\nk = 1\n",
        )
        .unwrap();
        // invalid entries are skipped, never fatal
        assert_eq!(
            load_notes(&p),
            Notes(BTreeMap::from([("kimi".into(), "週配額用完".into())]))
        );
        set_note_and_reset(&p, "codex-1a2b3c4d", "plus plan", None).unwrap();
        let t: toml::Table = std::fs::read_to_string(&p).unwrap().parse().unwrap();
        assert_eq!(t["names"]["acct-1"].as_str(), Some("work"));
        assert_eq!(t["notes"]["glm"].as_str(), Some("a@b"));
        assert_eq!(t["other"]["k"].as_integer(), Some(1));
        assert_eq!(load(&p).0.len(), 1, "names untouched");
        assert_eq!(load_notes(&p).0.len(), 2);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(m, 0o600);
        }
        assert!(clear_note_and_reset(&p, "kimi").unwrap());
        assert!(!clear_note_and_reset(&p, "kimi").unwrap());
        assert_eq!(
            load_notes(&p),
            Notes(BTreeMap::from([(
                "codex-1a2b3c4d".into(),
                "plus plan".into()
            )]))
        );
        // a broken file is no notes and is never rewritten
        std::fs::write(&p, "not toml [").unwrap();
        assert_eq!(load_notes(&p), Notes::default());
        assert!(set_note_and_reset(&p, "kimi", "x", None).is_err());
        assert!(clear_note_and_reset(&p, "kimi").is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "not toml [");
    }

    #[test]
    fn resets_load_validate_set_and_clear_with_the_note_keeping_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("accounts.toml");
        assert_eq!(load_resets(&p), Resets::default());
        assert!(!clear_note_and_reset(&p, "kimi").unwrap());
        std::fs::write(
            &p,
            "[resets]\nkimi = \"fri@17:00+08:00\"\nbad = \"friday\"\nnum = 5\n[other]\nk = 1\n",
        )
        .unwrap();
        assert_eq!(
            load_resets(&p),
            Resets(BTreeMap::from([("kimi".into(), "fri@17:00+08:00".into())]))
        );
        set_note_and_reset(&p, "twgame", "週四重置", Some("thu@00:00+08:00")).unwrap();
        set_note_and_reset(&p, "glm", "x", None).unwrap();
        set_reset(&p, "kimi", "sat@01:00+00:00").unwrap();
        let t: toml::Table = std::fs::read_to_string(&p).unwrap().parse().unwrap();
        assert_eq!(
            t["resets"]["bad"].as_str(),
            Some("friday"),
            "skipped entry kept"
        );
        assert_eq!(t["other"]["k"].as_integer(), Some(1));
        assert_eq!(t["notes"]["glm"].as_str(), Some("x"));
        assert!(t["resets"].get("glm").is_none());
        assert_eq!(load_resets(&p).0.len(), 2);
        assert!(clear_note_and_reset(&p, "twgame").unwrap());
        assert!(!clear_note_and_reset(&p, "twgame").unwrap());
        assert!(clear_note_and_reset(&p, "kimi").unwrap(), "rule only");
        assert_eq!(load_notes(&p).0.len(), 1);
        assert!(load_resets(&p).0.is_empty());
        let r = Resets(BTreeMap::from([
            ("acct-1".into(), "mon@01:00+00:00".into()),
            ("kimi".into(), "tue@01:00+00:00".into()),
        ]));
        assert_eq!(r.for_row("acct-1", "kimi"), Some("mon@01:00+00:00"));
        assert_eq!(r.for_row("acct-2", "kimi"), Some("tue@01:00+00:00"));
        assert_eq!(r.for_row("acct-2", "glm"), None);
    }

    #[test]
    fn an_account_note_wins_over_its_provider_note() {
        let n = Notes(BTreeMap::from([
            ("codex".into(), "team plan".into()),
            ("codex-1a2b3c4d".into(), "mine".into()),
        ]));
        assert_eq!(n.for_row("codex-1a2b3c4d", "codex"), Some("mine"));
        assert_eq!(n.for_row("codex-99999999", "codex"), Some("team plan"));
        assert_eq!(n.for_row("acct-1", "claude"), None);
    }

    #[test]
    fn valid_note_allows_40_cells_and_refuses_emails_and_control_chars() {
        for ok in [
            "kimi weekly quota used up",
            &"x".repeat(40),
            &"工".repeat(20),
        ] {
            assert!(valid_note(ok), "{ok}");
        }
        for bad in [
            "",
            " x",
            "x ",
            "a@b",
            "a\tb",
            "a\nb",
            &"x".repeat(41),
            &"工".repeat(21),
            "工作\u{FEFF}",
        ] {
            assert!(!valid_note(bad), "{bad:?}");
        }
    }

    #[test]
    fn plan_ids_aliases_and_display_forms() {
        assert_eq!(parse_plan("pro"), Some("pro"));
        assert_eq!(parse_plan("Max5x"), Some("max5x"));
        assert_eq!(parse_plan("5x"), Some("max5x"));
        assert_eq!(parse_plan("20x"), Some("max20x"));
        for bad in ["", "free", "max", "max 5x", "10x", "pro "] {
            assert_eq!(parse_plan(bad), None, "{bad:?}");
        }
        let shown: Vec<&str> = PLANS.iter().map(|p| plan_display(p)).collect();
        assert_eq!(
            shown,
            ["Pro", "Max 5x", "Max 20x", "Team", "Enterprise", "API"]
        );
    }

    #[test]
    fn plans_set_clear_and_load_keep_the_rest_and_skip_invalid_entries() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("accounts.toml");
        assert_eq!(load_plans(&p), Plans::default());
        assert!(!clear_plan(&p, "kimi").unwrap());
        std::fs::write(
            &p,
            "[plans]\ncodex = \"team\"\nbad = \"gold\"\nnum = 5\n[notes]\nkimi = \"x\"\n[other]\nk = 1\n",
        )
        .unwrap();
        assert_eq!(
            load_plans(&p),
            Plans(BTreeMap::from([("codex".into(), "team".into())]))
        );
        set_plan(&p, "acct-11111111", "max5x").unwrap();
        let t: toml::Table = std::fs::read_to_string(&p).unwrap().parse().unwrap();
        assert_eq!(
            t["plans"]["bad"].as_str(),
            Some("gold"),
            "skipped entry kept"
        );
        assert_eq!(t["notes"]["kimi"].as_str(), Some("x"));
        assert_eq!(t["other"]["k"].as_integer(), Some(1));
        assert_eq!(load_plans(&p).0.len(), 2);
        assert!(clear_plan(&p, "acct-11111111").unwrap());
        assert!(!clear_plan(&p, "acct-11111111").unwrap());
        assert_eq!(load_plans(&p).0.len(), 1);
        assert_eq!(load_notes(&p).0.len(), 1);
        let pl = Plans(BTreeMap::from([
            ("acct-1".into(), "pro".into()),
            ("codex".into(), "team".into()),
        ]));
        assert_eq!(pl.for_row("acct-1", "codex"), Some("pro"));
        assert_eq!(pl.for_row("codex-2", "codex"), Some("team"));
        assert_eq!(pl.for_row("acct-2", "claude"), None);
    }

    #[test]
    fn an_invalid_plan_is_refused_on_set_and_leaves_the_file_alone() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("accounts.toml");
        assert!(set_plan(&p, "acct-1", "gold").is_err());
        assert!(!p.exists());
    }

    #[test]
    fn key_provider_names_the_provider_of_a_provider_key_only() {
        assert_eq!(key_provider("codex-1a2b3c4d"), Some("codex"));
        assert_eq!(key_provider("agy-1a2b3c4d"), Some("agy"));
        assert_eq!(key_provider("agyclaude-1a2b3c4d"), Some("agyclaude"));
        assert_eq!(key_provider("grok-1a2b3c4d"), Some("grok"));
        assert_eq!(key_provider("acct-1a2b3c4d"), None);
        assert_eq!(key_provider("ci-20261105"), None);
        assert_eq!(key_provider("codex-XYZ"), None);
        // the agy pool keys are internal keys like the codex ones: never a display name
        assert!(!valid_name("agy-1a2b3c4d"));
        assert!(!valid_name("agyclaude-1a2b3c4d"));
        assert!(!valid_name("grok-1a2b3c4d"));
        assert!(valid_name("grok-cli"));
    }
}
