//! Transcript collector (G36 plan, phase 2).
//!
//! Reads only the numeric usage fields of Claude Code and Codex transcripts, incrementally,
//! into `usage.db` buckets. Containment: the walk never follows a symlink, each file is opened
//! `O_NOFOLLOW | O_NONBLOCK`, must be a regular file and lie under its canonical root. Nothing
//! textual is persisted: a source is known by sha256(canonical path), a project by the last
//! component of its cwd (local only), a model id only when it matches the id grammar.

use super::{alias, quota};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Records older than this are never counted.
pub const HORIZON_DAYS: i64 = 35;
/// Request ids and Codex session totals are kept this long (longer than the horizon, so a
/// reread from 0 cannot recount a pruned id).
pub const DEDUP_KEEP_DAYS: i64 = 40;
const DAY_MS: i64 = 86_400_000;
const HEAD_MAX: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Claude,
    Codex,
}

/// A transcript root: canonical dir and its sha8.
#[derive(Clone, Debug)]
pub struct Root {
    pub kind: Kind,
    pub dir: PathBuf,
    pub hash: String,
    /// The alias derived from this root's login file (set by the caller; `None` for Codex
    /// roots and Claude roots without a usable login).
    pub account: Option<String>,
}

/// Canonicalize and deduplicate the roots that exist.
pub fn roots(claude_dirs: &[PathBuf], codex_dir: Option<&Path>) -> Vec<Root> {
    let mut out: Vec<Root> = Vec::new();
    let mut add = |kind: Kind, d: &Path| {
        if let Ok(dir) = std::fs::canonicalize(d) {
            if dir.is_dir() && !out.iter().any(|r| r.dir == dir) {
                let hash = alias::path_hash8(&dir);
                out.push(Root {
                    kind,
                    dir,
                    hash,
                    account: None,
                });
            }
        }
    };
    for d in claude_dirs {
        add(Kind::Claude, d);
    }
    if let Some(d) = codex_dir {
        add(Kind::Codex, d);
    }
    out
}

fn real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_dir())
}

fn real_file(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn name_of(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// The transcript files of a root, exactly the plan's globs; no symlinked dir is entered and
/// no symlinked or special file is taken.
pub fn sources(root: &Root) -> Vec<PathBuf> {
    let mut out = Vec::new();
    match root.kind {
        Kind::Claude => {
            for proj in entries(&root.dir.join("projects")) {
                if !real_dir(&proj) {
                    continue;
                }
                for f in entries(&proj) {
                    if name_of(&f).ends_with(".jsonl") && real_file(&f) {
                        out.push(f);
                    } else if real_dir(&f) {
                        let sub = f.join("subagents");
                        if real_dir(&sub) {
                            out.extend(
                                entries(&sub)
                                    .into_iter()
                                    .filter(|g| name_of(g).ends_with(".jsonl") && real_file(g)),
                            );
                        }
                    }
                }
            }
        }
        Kind::Codex => {
            for y in entries(&root.dir.join("sessions"))
                .into_iter()
                .filter(|p| real_dir(p))
            {
                for m in entries(&y).into_iter().filter(|p| real_dir(p)) {
                    for d in entries(&m).into_iter().filter(|p| real_dir(p)) {
                        out.extend(entries(&d).into_iter().filter(|f| {
                            let n = name_of(f);
                            n.starts_with("rollout-") && n.ends_with(".jsonl") && real_file(f)
                        }));
                    }
                }
            }
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `^[A-Za-z0-9._-]{1,64}$`, else `unknown`.
pub fn model_id(m: Option<&str>) -> String {
    match m {
        Some(m)
            if !m.is_empty()
                && m.len() <= 64
                && m.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) =>
        {
            m.to_string()
        }
        _ => "unknown".to_string(),
    }
}

/// The last component of a cwd, at most 64 chars; nothing else of the path is kept.
fn project_of(cwd: Option<&str>) -> String {
    static HOME: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    let Some(cwd) = cwd else {
        return String::new();
    };
    // a session run in the home dir would store the user name as its project
    if HOME.get_or_init(dirs::home_dir).as_deref() == Some(Path::new(cwd)) {
        return "~".to_string();
    }
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.chars().take(64).collect())
        .unwrap_or_default()
}

/// Opaque request ids only: ASCII alphanumerics, `_` and `-`, at most 128.
fn request_id(v: &Value) -> Option<&str> {
    let r = v["requestId"].as_str()?;
    (!r.is_empty()
        && r.len() <= 128
        && r.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(r)
}

/// (record ms, UTC day) of a line's `timestamp`, when within the horizon.
fn when(v: &Value, now_ms: i64) -> Option<(i64, String)> {
    stamp(v).filter(|(ms, _)| within(*ms, now_ms))
}

fn within(ms: i64, now_ms: i64) -> bool {
    now_ms.saturating_sub(ms) <= HORIZON_DAYS * DAY_MS
}

/// (record ms, UTC day) of a line's `timestamp`, at any age.
fn stamp(v: &Value) -> Option<(i64, String)> {
    let ts = chrono::DateTime::parse_from_rfc3339(v["timestamp"].as_str()?).ok()?;
    let ms = ts.timestamp_millis();
    Some((
        ms,
        ts.with_timezone(&chrono::Utc)
            .format("%Y-%m-%d")
            .to_string(),
    ))
}

/// The largest token count one record may carry (~1.1e12); a larger or non-integer value makes
/// the record invalid (skipped), so no sum can approach i64.
pub const MAX_COUNT: u64 = 1 << 40;

/// A count field: absent → 0, a non-negative integer ≤ MAX_COUNT, else None (record skipped).
fn n(v: &Value) -> Option<i64> {
    if v.is_null() {
        return Some(0);
    }
    v.as_u64().filter(|x| *x <= MAX_COUNT).map(|x| x as i64)
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Counts {
    input: i64,
    output: i64,
    cw5m: i64,
    cw1h: i64,
    read: i64,
}

impl Counts {
    fn max(self, o: Counts) -> Counts {
        Counts {
            input: self.input.max(o.input),
            output: self.output.max(o.output),
            cw5m: self.cw5m.max(o.cw5m),
            cw1h: self.cw1h.max(o.cw1h),
            read: self.read.max(o.read),
        }
    }
    fn minus(self, o: Counts) -> Counts {
        Counts {
            input: self.input - o.input,
            output: self.output - o.output,
            cw5m: self.cw5m - o.cw5m,
            cw1h: self.cw1h - o.cw1h,
            read: self.read - o.read,
        }
    }
    fn any(self) -> bool {
        self.input != 0 || self.output != 0 || self.cw5m != 0 || self.cw1h != 0 || self.read != 0
    }
}

struct Key<'a> {
    day: &'a str,
    source: &'a str,
    account: &'a str,
    model: &'a str,
    project: &'a str,
    speed: &'a str,
}

/// Add to a bucket with checked arithmetic: an overflow is an error, which rolls the whole
/// file's transaction back (the file is skipped, nothing half-counted).
fn add_bucket(tx: &Transaction, k: &Key, c: Counts, requests: i64) -> Result<()> {
    let have: Option<[i64; 6]> = tx
        .query_row(
            "SELECT input, output, cw5m, cw1h, read, requests FROM buckets
             WHERE day = ?1 AND source = ?2 AND account = ?3 AND model = ?4 AND project = ?5
               AND speed = ?6",
            params![k.day, k.source, k.account, k.model, k.project, k.speed],
            |r| {
                Ok([
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ])
            },
        )
        .optional()?;
    if let Some(h) = have {
        let add = [c.input, c.output, c.cw5m, c.cw1h, c.read, requests];
        if h.iter().zip(add).any(|(a, b)| a.checked_add(b).is_none()) {
            anyhow::bail!("用量計數溢位，這個檔案略過");
        }
    }
    tx.execute(
        "INSERT INTO buckets (day, source, account, model, project, speed,
                              input, output, cw5m, cw1h, read, requests)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT (day, source, account, model, project, speed) DO UPDATE SET
             input = input + excluded.input, output = output + excluded.output,
             cw5m = cw5m + excluded.cw5m, cw1h = cw1h + excluded.cw1h,
             read = read + excluded.read, requests = requests + excluded.requests",
        params![
            k.day, k.source, k.account, k.model, k.project, k.speed, c.input, c.output, c.cw5m,
            c.cw1h, c.read, requests
        ],
    )?;
    Ok(())
}

/// One Claude line: per request id the counters merge by per-counter maximum and only the
/// increase over what is stored is added, so repeated lines, split responses and rereads
/// count once. Lines without a request id or usage are skipped.
fn claude_line(tx: &Transaction, v: &Value, account: &str, now_ms: i64) -> Result<()> {
    let Some(rid) = request_id(v) else {
        return Ok(());
    };
    let u = &v["message"]["usage"];
    if !u.is_object() {
        return Ok(());
    }
    let Some((ms, day)) = when(v, now_ms) else {
        return Ok(());
    };
    let cc = &u["cache_creation"];
    let (cw5m, cw1h) = if cc.is_object() {
        (
            n(&cc["ephemeral_5m_input_tokens"]),
            n(&cc["ephemeral_1h_input_tokens"]),
        )
    } else {
        (n(&u["cache_creation_input_tokens"]), Some(0))
    };
    // any count off the bound makes the record invalid: skipped, never clamped
    let (Some(input), Some(output), Some(cw5m), Some(cw1h), Some(read)) = (
        n(&u["input_tokens"]),
        n(&u["output_tokens"]),
        cw5m,
        cw1h,
        n(&u["cache_read_input_tokens"]),
    ) else {
        return Ok(());
    };
    let line = Counts {
        input,
        output,
        cw5m,
        cw1h,
        read,
    };
    type Stored = (String, String, String, String, String, Counts);
    let stored: Option<Stored> = tx
        .query_row(
            "SELECT day, account, model, project, speed, input, output, cw5m, cw1h, read
             FROM requests WHERE request_id = ?1",
            [rid],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    Counts {
                        input: r.get(5)?,
                        output: r.get(6)?,
                        cw5m: r.get(7)?,
                        cw1h: r.get(8)?,
                        read: r.get(9)?,
                    },
                ))
            },
        )
        .optional()?;
    match stored {
        None => {
            let model = model_id(v["message"]["model"].as_str());
            let project = project_of(v["cwd"].as_str());
            let speed = if u["speed"] == "fast" {
                "fast"
            } else {
                "standard"
            };
            tx.execute(
                "INSERT INTO requests VALUES (?1, ?2, 'claude', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    rid, day, account, model, project, speed, line.input, line.output, line.cw5m,
                    line.cw1h, line.read, ms
                ],
            )?;
            let k = Key {
                day: &day,
                source: "claude",
                account,
                model: &model,
                project: &project,
                speed,
            };
            add_bucket(tx, &k, line, 1)?;
        }
        Some((sday, sacct, smodel, sproject, sspeed, old)) => {
            let new = old.max(line);
            let delta = new.minus(old);
            if delta.any() {
                tx.execute(
                    "UPDATE requests SET input = ?2, output = ?3, cw5m = ?4, cw1h = ?5, read = ?6
                     WHERE request_id = ?1",
                    params![rid, new.input, new.output, new.cw5m, new.cw1h, new.read],
                )?;
                let k = Key {
                    day: &sday,
                    source: "claude",
                    account: &sacct,
                    model: &smodel,
                    project: &sproject,
                    speed: &sspeed,
                };
                add_bucket(tx, &k, delta, 0)?;
            }
        }
    }
    Ok(())
}

/// The session id of a Codex rollout: the uuid ending its file name.
fn codex_session(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let id = stem.get(stem.len().checked_sub(36)?..)?;
    (id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
        .then(|| id.to_ascii_lowercase())
}

/// Codex `rate_limits.primary`/`secondary` minutes of the windows kept; any other is ignored.
const FIVE_HOUR_MIN: u64 = 300;
const WEEK_MIN: u64 = 10_080;

/// One rate-limit window: (fraction used, resets_at unix seconds).
pub type Limit = (f64, i64);

/// The 5h and weekly windows of a `rate_limits` object: only `used_percent` (kept as a fraction,
/// in the summary's range), `window_minutes` and `resets_at` are read; credits, plan and the rest never are.
/// A null or malformed window, or one of another length, is no window. `None` when `rl` is not
/// an object (no observation).
fn rate_limits(rl: &Value) -> Option<(Option<Limit>, Option<Limit>)> {
    rl.as_object()?;
    let (mut five, mut week) = (None, None);
    for w in [&rl["primary"], &rl["secondary"]] {
        let pct = w["used_percent"]
            .as_f64()
            .map(|p| p / 100.0)
            .filter(|p| p.is_finite() && (0.0..=super::summary::PCT_MAX).contains(p));
        let resets = w["resets_at"]
            .as_u64()
            .filter(|r| *r <= super::summary::UINT_MAX);
        let (Some(pct), Some(resets)) = (pct, resets) else {
            continue;
        };
        let lim = Some((pct, resets as i64));
        match w["window_minutes"].as_u64() {
            Some(FIVE_HOUR_MIN) => five = lim,
            Some(WEEK_MIN) => week = lim,
            _ => {}
        }
    }
    Some((five, week))
}

/// Keep the observation when it is at least as new as the stored one for `account` (a reread of
/// the same line writes the same values).
fn store_limits(
    tx: &Transaction,
    account: &str,
    at_ms: i64,
    (five, week): (Option<Limit>, Option<Limit>),
) -> Result<()> {
    tx.execute(
        "INSERT INTO codex_limits VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account) DO UPDATE SET observed_ms = excluded.observed_ms,
             five_pct = excluded.five_pct, five_resets = excluded.five_resets,
             week_pct = excluded.week_pct, week_resets = excluded.week_resets
         WHERE excluded.observed_ms >= codex_limits.observed_ms",
        params![
            account,
            at_ms,
            five.map(|w| w.0),
            five.map(|w| w.1),
            week.map(|w| w.0),
            week.map(|w| w.1)
        ],
    )?;
    Ok(())
}

/// The newest rate-limit observation of every codex account, as quota snapshots for the summary
/// (`seen_at_ms` = the observation time). An observation with no window, or whose windows have
/// all reset by `now_ms`, says nothing current and is left out: that account is `no_window`.
pub fn codex_snapshots(conn: &Connection, now_ms: i64) -> Result<Vec<quota::Snapshot>> {
    let mut st = conn.prepare(
        "SELECT account, observed_ms, five_pct, five_resets, week_pct, week_resets
         FROM codex_limits ORDER BY account",
    )?;
    let now_s = now_ms.div_euclid(1000);
    let rows = st.query_map([], |r| {
        let at: i64 = r.get(1)?;
        let win = |p: Option<f64>, s: Option<i64>| {
            p.zip(s).map(|(pct, resets_at)| quota::Window {
                pct,
                resets_at,
                seen: Some(quota::Seen {
                    at_ms: at,
                    session: String::new(),
                }),
            })
        };
        Ok(quota::Snapshot {
            alias: r.get(0)?,
            org: None,
            email: None,
            root: None,
            written_at_ms: at,
            five_hour: win(r.get(2)?, r.get(3)?),
            seven_day: win(r.get(4)?, r.get(5)?),
        })
    })?;
    let mut out = Vec::new();
    for s in rows {
        let s = s?;
        if [&s.five_hour, &s.seven_day]
            .into_iter()
            .flatten()
            .any(|w| w.resets_at > now_s)
        {
            out.push(s);
        }
    }
    Ok(out)
}

/// One Codex line. `turn_context` sets the session's model and project; a `token_count`
/// event adds only what its cumulative totals exceed the highest already counted for the
/// session (copies, replacements and decreasing totals add nothing), and its `rate_limits` are
/// kept when newer than the account's stored observation (at the line's timestamp, else
/// `mtime_ms`, the file's).
fn codex_line(
    tx: &Transaction,
    v: &Value,
    session: &str,
    account: &str,
    now_ms: i64,
    mtime_ms: Option<i64>,
) -> Result<()> {
    let p = &v["payload"];
    let stored = tx
        .query_row(
            "SELECT input, cached, cache_write, output, model, project FROM codex_sessions
             WHERE session_id = ?1",
            [session],
            |r| {
                Ok((
                    [r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?],
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let (old, mut model, mut project) =
        stored.unwrap_or(([0; 4], "unknown".to_string(), String::new()));
    let seen = |tx: &Transaction, totals: [i64; 4], model: &str, project: &str, ms: i64| {
        tx.execute(
            "INSERT INTO codex_sessions VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (session_id) DO UPDATE SET input = excluded.input,
                 cached = excluded.cached, cache_write = excluded.cache_write,
                 output = excluded.output, seen_ms = max(seen_ms, excluded.seen_ms),
                 model = excluded.model, project = excluded.project",
            params![session, totals[0], totals[1], totals[2], totals[3], ms, model, project],
        )
    };
    if v["type"] == "turn_context" {
        model = model_id(p["model"].as_str());
        project = project_of(p["cwd"].as_str());
        seen(tx, old, &model, &project, 0)?;
        return Ok(());
    }
    if p["type"] != "token_count" {
        return Ok(());
    }
    if let Some(lim) = rate_limits(&p["rate_limits"]) {
        if let Some(at) = stamp(v).map(|(ms, _)| ms).or(mtime_ms) {
            store_limits(tx, account, at, lim)?;
        }
    }
    let t = &p["info"]["total_token_usage"];
    if !t.is_object() {
        return Ok(());
    }
    let Some((ms, day)) = stamp(v) else {
        return Ok(());
    };
    let (Some(i), Some(c), Some(w), Some(o)) = (
        n(&t["input_tokens"]),
        n(&t["cached_input_tokens"]),
        n(&t["cache_write_input_tokens"]),
        n(&t["output_tokens"]),
    ) else {
        return Ok(());
    };
    let new = [i, c, w, o];
    let d: Vec<i64> = new.iter().zip(old).map(|(a, b)| (a - b).max(0)).collect();
    if d.iter().all(|x| *x == 0) {
        return Ok(());
    }
    let top = [
        new[0].max(old[0]),
        new[1].max(old[1]),
        new[2].max(old[2]),
        new[3].max(old[3]),
    ];
    if !within(ms, now_ms) {
        // past the horizon: not counted, but it is the base the next cumulative is measured from
        // (stamped now, so the dedup prune keeps it while the file is still being read)
        seen(tx, top, &model, &project, now_ms)?;
        return Ok(());
    }
    seen(tx, top, &model, &project, ms)?;
    let c = Counts {
        // Codex input_tokens include the cached ones; they are priced as reads
        input: (d[0] - d[1]).max(0),
        output: d[3],
        cw5m: d[2],
        cw1h: 0,
        read: d[1],
    };
    let k = Key {
        day: &day,
        source: "codex",
        account,
        model: &model,
        project: &project,
        speed: "standard",
    };
    add_bucket(tx, &k, c, 1)?;
    Ok(())
}

/// sha256 of the first `len` bytes, read from the open handle (no second open by path).
fn head_hash(f: &std::fs::File, len: u64) -> std::io::Result<String> {
    let mut buf = vec![0u8; len.min(HEAD_MAX) as usize];
    #[cfg(unix)]
    std::os::unix::fs::FileExt::read_exact_at(f, &mut buf, 0)?;
    #[cfg(not(unix))]
    {
        let mut g = f;
        g.seek(SeekFrom::Start(0))?;
        g.read_exact(&mut buf)?;
    }
    Ok(hex(&Sha256::digest(&buf)))
}

/// The longest line parsed (real transcripts carry usage on lines up to ~8 MB). A longer line
/// is read through in BufReader-sized pieces and dropped: never kept, never counted.
pub const MAX_LINE: usize = 16 << 20;

/// The next newline-terminated line into `buf`: `Some((bytes consumed, kept))`, `kept` false
/// when it was longer than `max` (then `buf` is empty). `None` at the end or before an
/// unterminated tail, which waits for the next collect.
fn next_line<R: BufRead>(
    r: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<Option<(u64, bool)>> {
    buf.clear();
    let (mut n, mut keep) = (0u64, true);
    loop {
        let avail = r.fill_buf()?;
        if avail.is_empty() {
            return Ok(None);
        }
        let (take, done) = match avail.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (avail.len(), false),
        };
        if keep && buf.len() + take <= max {
            buf.extend_from_slice(&avail[..take]);
        } else if keep {
            keep = false;
            buf.clear();
            buf.shrink_to(0);
        }
        r.consume(take);
        n += take as u64;
        if done {
            return Ok(Some((n, keep)));
        }
    }
}

#[cfg(unix)]
fn inode(m: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    m.ino() as i64
}

#[cfg(not(unix))]
fn inode(_m: &std::fs::Metadata) -> i64 {
    0
}

/// Collect one file in one transaction (buckets, dedup rows and cursor commit together).
/// Returns whether the file was read; a file outside its root or not a regular file is not.
pub fn collect_file(
    conn: &mut Connection,
    path: &Path,
    root: &Root,
    account: &str,
    now_ms: i64,
) -> Result<bool> {
    let canon = std::fs::canonicalize(path)?;
    if !canon.starts_with(&root.dir) {
        return Ok(false);
    }
    let f = quota::open_no_follow(path)?;
    let meta = f.metadata()?;
    if !meta.file_type().is_file() {
        return Ok(false);
    }
    // not written since before the horizon: it holds nothing countable
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);
    if mtime_ms.is_some_and(|m| now_ms - m > HORIZON_DAYS * DAY_MS) {
        return Ok(true);
    }
    let (ino, size) = (inode(&meta), meta.len());
    let source_id = hex(&Sha256::digest(canon.as_os_str().as_encoded_bytes()));
    let session = match root.kind {
        Kind::Codex => match codex_session(path) {
            Some(s) => Some(s),
            None => return Ok(false),
        },
        Kind::Claude => None,
    };
    let stored: Option<(i64, i64, i64, String)> = conn
        .query_row(
            "SELECT inode, offset, head_len, head FROM sources WHERE source_id = ?1",
            [&source_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let mut offset: u64 = 0;
    if let Some((sino, soff, slen, shead)) = &stored {
        let same = *sino == ino && (*soff as u64) <= size && head_hash(&f, *slen as u64)? == *shead;
        if same {
            offset = *soff as u64;
        }
    }
    if stored.is_some() && offset == size {
        return Ok(true);
    }
    // a Codex session whose base was pruned (silent past DEDUP_KEEP_DAYS) and that grew again:
    // read from the start, so its old cumulative, now past the horizon, is the base once more
    if let (Some(sid), true) = (&session, offset > 0) {
        let has_base = conn
            .query_row(
                "SELECT 1 FROM codex_sessions WHERE session_id = ?1",
                [sid],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !has_base {
            offset = 0;
        }
    }
    (&f).seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::new((&f).take(size - offset));
    let tx = conn.transaction()?;
    let mut consumed = offset;
    let mut buf = Vec::new();
    while let Some((got, kept)) = next_line(&mut reader, &mut buf, MAX_LINE)? {
        consumed += got;
        if !kept {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        match &session {
            None => claude_line(&tx, &v, account, now_ms)?,
            Some(s) => codex_line(&tx, &v, s, account, now_ms, mtime_ms)?,
        }
    }
    let head_len = consumed.min(HEAD_MAX);
    let head = head_hash(&f, head_len)?;
    tx.execute(
        "INSERT INTO sources VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (source_id) DO UPDATE SET inode = excluded.inode, size = excluded.size,
             offset = excluded.offset, head_len = excluded.head_len, head = excluded.head",
        params![
            source_id,
            ino,
            size as i64,
            consumed as i64,
            head_len as i64,
            head
        ],
    )?;
    tx.commit()?;
    Ok(true)
}

/// A root's account: Codex roots are `codex-<sha8>`. A Claude root takes the alias of the
/// newest quota snapshot naming its sha8 as `root` (stored, so attribution does not depend on
/// which process runs collect), else the alias derived from the root's login file
/// (`root.account`), else the stored one, else `acct-<sha8>`. Known limitation: transcripts
/// written before a re-login are attributed to the account logged in now.
/// `AUTOPILOT_ACCOUNT` is never read here.
pub fn root_alias(
    conn: &Connection,
    root: &Root,
    snapshots: &[quota::Snapshot],
    now_ms: i64,
) -> Result<String> {
    if root.kind == Kind::Codex {
        return Ok(format!("codex-{}", root.hash));
    }
    let newest = snapshots
        .iter()
        .filter(|s| s.root.as_deref() == Some(root.hash.as_str()))
        .max_by_key(|s| s.written_at_ms);
    if let Some(alias) = newest.map(|s| &s.alias).or(root.account.as_ref()) {
        conn.execute(
            "INSERT INTO roots VALUES (?1, ?2, ?3)
             ON CONFLICT (root_hash) DO UPDATE SET alias = excluded.alias, seen_ms = excluded.seen_ms",
            params![root.hash, alias, now_ms],
        )?;
        return Ok(alias.clone());
    }
    let stored: Option<String> = conn
        .query_row(
            "SELECT alias FROM roots WHERE root_hash = ?1",
            [&root.hash],
            |r| r.get(0),
        )
        .optional()?;
    Ok(stored.unwrap_or_else(|| format!("acct-{}", root.hash)))
}

#[derive(Debug, Default)]
pub struct Report {
    pub files: usize,
    pub skipped: usize,
}

/// Collect every root. A file that fails is skipped (its transaction rolls back) and the
/// rest go on.
pub fn collect(
    conn: &mut Connection,
    roots: &[Root],
    snapshots: &[quota::Snapshot],
    now_ms: i64,
) -> Result<Report> {
    let mut rep = Report::default();
    for root in roots {
        let account = root_alias(conn, root, snapshots, now_ms)?;
        for path in sources(root) {
            match collect_file(conn, &path, root, &account, now_ms) {
                Ok(true) => rep.files += 1,
                _ => rep.skipped += 1,
            }
        }
    }
    let keep = now_ms - DEDUP_KEEP_DAYS * DAY_MS;
    conn.execute("DELETE FROM requests WHERE seen_ms < ?1", [keep])?;
    conn.execute(
        "DELETE FROM codex_sessions WHERE seen_ms > 0 AND seen_ms < ?1",
        [keep],
    )?;
    Ok(rep)
}

/// One bucket row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub day: String,
    pub source: String,
    pub account: String,
    pub model: String,
    pub project: String,
    pub speed: String,
    pub tokens: super::price::Tokens,
    pub requests: u64,
}

/// Bucket rows from `since_day` (YYYY-MM-DD) on, oldest first.
pub fn rows_since(conn: &Connection, since_day: &str) -> Result<Vec<Row>> {
    let mut st = conn.prepare(
        "SELECT day, source, account, model, project, speed, input, output, cw5m, cw1h, read, requests
         FROM buckets WHERE day >= ?1 ORDER BY day, source, account, model, project, speed",
    )?;
    let u = |x: i64| x.max(0) as u64;
    let rows = st
        .query_map([since_day], |r| {
            Ok(Row {
                day: r.get(0)?,
                source: r.get(1)?,
                account: r.get(2)?,
                model: r.get(3)?,
                project: r.get(4)?,
                speed: r.get(5)?,
                tokens: super::price::Tokens {
                    input: u(r.get(6)?),
                    output: u(r.get(7)?),
                    cache_write_5m: u(r.get(8)?),
                    cache_write_1h: u(r.get(9)?),
                    cache_read: u(r.get(10)?),
                },
                requests: u(r.get(11)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
#[path = "collect_tests.rs"]
mod tests;
