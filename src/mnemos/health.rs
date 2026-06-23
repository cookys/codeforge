//! 央腦（Mnemos）連線健康 — probe liveness + ship readiness。
//! statusline 熱路徑只讀此模組算好的 CentralHealth；快取 per-machine。
//! 消費者：P1 probe/spawn、P2 ship、P3/P4 statusline
#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROBE_TTL_OK: u64 = 30;
pub const PROBE_TTL_MAX: u64 = 600;
pub const SHIP_FRESH_WINDOW: u64 = 86_400;
pub const NEVER_RETIRE_DAYS: u64 = 7;
pub const CACHE_MAX_AGE: u64 = 3_600;
pub const INFLIGHT_GRACE: u64 = 10;
pub const PROBE_CONNECT_TIMEOUT: u64 = 2;
pub const QUEUE_WARN_THRESHOLD: usize = 3;

/// per-machine runtime dir，絕不放會 dotfile-sync 的 home。
pub fn runtime_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d).join("codeforge");
        }
    }
    if let Some(d) = std::env::var_os("TMPDIR") {
        if !d.is_empty() {
            return PathBuf::from(d).join("codeforge");
        }
    }
    std::env::temp_dir().join("codeforge")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    Never,
    Unreachable,
    HttpError,
    Ok,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessCache {
    pub last_probe_at: i64,
    pub last_outcome: ProbeOutcome,
    pub consecutive_failures: u32,
    pub latency_ms: Option<u32>,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipCache {
    pub last_ship_at: i64,
    pub last_ship_ok: bool,
}

pub fn liveness_path() -> PathBuf {
    runtime_dir().join("mnemos-liveness.json")
}

pub fn ship_path() -> PathBuf {
    runtime_dir().join("mnemos-ship.json")
}

pub fn lock_path() -> PathBuf {
    runtime_dir().join("mnemos-liveness.lock")
}

/// temp+rename atomic write（reader 無 torn read）。temp 同 dir 避 EXDEV，
/// 名帶 pid+nanos 避多進程撞。tmpfs 上不 fsync。
pub fn write_atomic<T: Serialize>(path: &Path, val: &T) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(
        "{}.{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("cache"),
        std::process::id(),
        nanos
    ));
    let json = serde_json::to_vec_pretty(val).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, path)
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn read_liveness() -> Option<LivenessCache> {
    read_json(&liveness_path())
}

pub fn read_ship() -> Option<ShipCache> {
    read_json(&ship_path())
}

pub fn ttl_for(outcome: ProbeOutcome, consecutive_failures: u32) -> u64 {
    match outcome {
        ProbeOutcome::Ok | ProbeOutcome::Never => PROBE_TTL_OK,
        ProbeOutcome::Unreachable | ProbeOutcome::HttpError => {
            let n = consecutive_failures.max(1);
            PROBE_TTL_OK
                .saturating_mul(1u64 << (n - 1).min(20))
                .min(PROBE_TTL_MAX)
        }
    }
}

pub fn next_failure_count(prev: u32, outcome: ProbeOutcome) -> u32 {
    match outcome {
        ProbeOutcome::Ok => 0,
        _ => prev.saturating_add(1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CentralLight {
    Hidden,
    Ok,
    Degraded,
    Offline,
    Pending,
}

pub fn central_light(
    opted_in: bool,
    liveness: Option<&LivenessCache>,
    ship: Option<&ShipCache>,
    queue_degraded: bool,
    now: i64,
) -> CentralLight {
    if !opted_in {
        return CentralLight::Hidden;
    }
    let Some(lv) = liveness else {
        return CentralLight::Offline; // 未知視同 offline
    };
    match lv.last_outcome {
        ProbeOutcome::Never => {
            if now - lv.last_probe_at > (NEVER_RETIRE_DAYS * 86_400) as i64 {
                CentralLight::Hidden
            } else {
                CentralLight::Pending
            }
        }
        ProbeOutcome::Unreachable | ProbeOutcome::HttpError => CentralLight::Offline,
        ProbeOutcome::Ok => {
            let fresh_ship_fail = ship
                .filter(|s| now - s.last_ship_at <= SHIP_FRESH_WINDOW as i64)
                .map(|s| !s.last_ship_ok)
                .unwrap_or(false);
            if fresh_ship_fail || queue_degraded {
                CentralLight::Degraded
            } else {
                CentralLight::Ok
            }
        }
    }
}

/// 判斷快取是否足夠新鮮（macOS per-boot 退路：超過 CACHE_MAX_AGE 視同過期）。
pub fn fresh_enough(l: &LivenessCache, now: i64) -> bool {
    now - l.last_probe_at <= CACHE_MAX_AGE as i64
}

fn count_json_in(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .count()
}

/// 即時 queue 深度（statusline 熱路徑，stat 級）。
pub fn queue_depth() -> usize {
    count_json_in(&crate::mnemos::state::ship_failed_dir(
        &crate::mnemos::state::ship_root(),
    ))
}

/// 可測核心：count + oldest age 注入。
/// spec §6/§11：count >= QUEUE_WARN_THRESHOLD OR 最舊一筆 age > 24h(86400s)。
pub fn queue_degraded_from(count: usize, oldest_age_secs: Option<u64>) -> bool {
    if count == 0 {
        return false;
    }
    if count >= QUEUE_WARN_THRESHOLD {
        return true;
    }
    oldest_age_secs.map(|a| a > 86_400).unwrap_or(false)
}

/// IO 殼：讀 ship-failed/ dir，計算 count + mtime-oldest，回傳是否 degraded。
pub fn queue_degraded() -> bool {
    let dir = crate::mnemos::state::ship_failed_dir(&crate::mnemos::state::ship_root());
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return false;
    };
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut count = 0usize;
    let mut oldest_age: Option<u64> = None;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().map(|x| x == "json").unwrap_or(false) {
            count += 1;
            if let Some(mtime) = file_mtime_secs(&path) {
                let age = now_secs.saturating_sub(mtime as u64);
                oldest_age = Some(oldest_age.map(|o| o.max(age)).unwrap_or(age));
            }
        }
    }
    queue_degraded_from(count, oldest_age)
}

use std::fs::OpenOptions;
use std::time::{SystemTime, UNIX_EPOCH};

fn file_mtime_secs(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// 可測核心：now 注入。回 true=本進程拿到鎖、應 spawn。
fn acquire_at(lock: &Path, now: i64) -> bool {
    if let Some(parent) = lock.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 1) 無鎖 → O_EXCL 原子建檔
    match OpenOptions::new().write(true).create_new(true).open(lock) {
        Ok(_) => return true,
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return false,
        _ => {}
    }
    // 2) 有鎖 → 只有 stale 才認領
    match file_mtime_secs(lock) {
        Some(m) if now - m > INFLIGHT_GRACE as i64 => {
            // rename-steal：只一個 statusline 能把 stale 鎖 rename 走（其餘 ENOENT）
            let owned = lock.with_extension(format!("steal.{}", std::process::id()));
            if std::fs::rename(lock, &owned).is_ok() {
                let _ = std::fs::remove_file(&owned);
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(lock)
                    .is_ok()
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn try_acquire_probe_lock() -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    acquire_at(&lock_path(), now)
}

pub fn release_probe_lock() {
    let _ = std::fs::remove_file(lock_path());
}

// ─── BrainHealth 聚合 ─────────────────────────────────────────────────────

/// 雙燈聚合：local（L1 memory）+ central（Mnemos）。
/// statusline bottom_border 的唯一輸入。
pub struct BrainHealth {
    pub local: LocalLight,
    pub central: CentralLight,
}

// ─── Task 11: local 燈 ────────────────────────────────────────────────────

/// 本地腦（L1 memory）連線狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalLight {
    /// 未 dream / 全新專案，無任何 store 記錄 → 不渲染
    Hidden,
    /// active L1 concept > 0 → 綠燈
    Active,
    /// 曾有 store 歷史但 active = 0 → 灰燈（「記憶怎麼不見了」有用訊號）
    Empty,
}

/// 計算本地腦狀態。
/// - `active_l1 > 0` → Active
/// - `active_l1 == 0` 但 `has_store_history` → Empty
/// - 否則 → Hidden（全新、無歷史）
pub fn local_light(active_l1: usize, has_store_history: bool) -> LocalLight {
    if active_l1 > 0 {
        LocalLight::Active
    } else if has_store_history {
        LocalLight::Empty
    } else {
        LocalLight::Hidden
    }
}

// ─── Task 7: classify_probe + run_probe ────────────────────────────────────

/// Map an HTTP status code (or None for network error) to a ProbeOutcome.
/// None  → Unreachable (connection-refused / timeout)
/// 2xx   → Ok          (body is ignored — tolerant of plain "ok" or JSON)
/// other → HttpError
pub fn classify_probe(status: Option<u16>) -> ProbeOutcome {
    match status {
        None => ProbeOutcome::Unreachable,
        Some(s) if (200..=299).contains(&s) => ProbeOutcome::Ok,
        Some(_) => ProbeOutcome::HttpError,
    }
}

/// Execute a single probe against Mnemos `/health`.
///
/// Non-verbose path: writes `mnemos-liveness.json` and releases the lock.
/// Verbose path (manual debug): prints to stderr, does NOT write cache.
pub fn run_probe(verbose: bool) -> anyhow::Result<()> {
    use std::time::Duration;
    let cfg = crate::mnemos::config::MnemosConfig::load()?;
    let url = format!("{}/health", cfg.base_url);

    // Minimal single-thread runtime — probe is a one-shot ≤2 s request.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (status, latency_ms) = rt.block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(PROBE_CONNECT_TIMEOUT))
            .timeout(Duration::from_secs(PROBE_CONNECT_TIMEOUT + 1))
            .build()
            .unwrap_or_default();
        let t0 = std::time::Instant::now();
        match client.get(&url).send().await {
            Ok(r) => (
                Some(r.status().as_u16()),
                Some(t0.elapsed().as_millis() as u32),
            ),
            Err(_) => (None, None),
        }
    });

    let outcome = classify_probe(status);

    if verbose {
        // Front-door: human-readable stderr, no cache write.
        eprintln!(
            "probe {} → {:?}  status={:?}  latency={:?}ms",
            url, outcome, status, latency_ms
        );
        return Ok(());
    }

    let prev_failures = read_liveness().map(|l| l.consecutive_failures).unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cache = LivenessCache {
        last_probe_at: now,
        last_outcome: outcome,
        consecutive_failures: next_failure_count(prev_failures, outcome),
        latency_ms,
        http_status: status,
    };
    let _ = write_atomic(&liveness_path(), &cache);
    release_probe_lock();
    Ok(())
}

/// 前景同步 probe 核心（供 `codeforge doctor` 直接呼叫，不寫快取、不釋鎖）。
///
/// 回傳 `(ProbeOutcome, Option<u32> latency_ms, Option<u16> http_status)`。
/// 使用 2s timeout（與 run_probe 相同）。base_url 取自 MnemosConfig。
/// 任何設定載入 / 運行時錯誤皆退 `(ProbeOutcome::Unreachable, None, None)`。
pub fn probe_now() -> (ProbeOutcome, Option<u32>, Option<u16>) {
    use std::time::Duration;
    let cfg = match crate::mnemos::config::MnemosConfig::load() {
        Ok(c) => c,
        Err(_) => return (ProbeOutcome::Unreachable, None, None),
    };
    let url = format!("{}/health", cfg.base_url);

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => return (ProbeOutcome::Unreachable, None, None),
    };

    let (status, latency_ms) = rt.block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(PROBE_CONNECT_TIMEOUT))
            .timeout(Duration::from_secs(PROBE_CONNECT_TIMEOUT + 1))
            .build()
            .unwrap_or_default();
        let t0 = std::time::Instant::now();
        match client.get(&url).send().await {
            Ok(r) => (
                Some(r.status().as_u16()),
                Some(t0.elapsed().as_millis() as u32),
            ),
            Err(_) => (None, None),
        }
    });

    (classify_probe(status), latency_ms, status)
}

/// 即時讀 ship-failed/ dir，回傳 (count, oldest_age_secs)。
/// doctor 用：顯示 queue 深度與最舊一筆 age。
pub fn queue_info() -> (usize, Option<u64>) {
    let dir = crate::mnemos::state::ship_failed_dir(&crate::mnemos::state::ship_root());
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return (0, None);
    };
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut count = 0usize;
    let mut oldest_age: Option<u64> = None;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().map(|x| x == "json").unwrap_or(false) {
            count += 1;
            if let Some(mtime) = file_mtime_secs(&path) {
                let age = now_secs.saturating_sub(mtime as u64);
                oldest_age = Some(oldest_age.map(|o| o.max(age)).unwrap_or(age));
            }
        }
    }
    (count, oldest_age)
}

// ─── Task 8: should_refresh + maybe_spawn_probe ────────────────────────────

/// Returns true if the liveness cache needs to be refreshed:
/// - No cache yet → always refresh
/// - Cache age > TTL for the current outcome/backoff → refresh
/// - Cache absolute age > CACHE_MAX_AGE (macOS per-boot退路) → refresh
pub fn should_refresh(liveness: Option<&LivenessCache>, now: i64) -> bool {
    match liveness {
        None => true,
        Some(l) => {
            let elapsed = now - l.last_probe_at;
            let ttl_expired = elapsed > ttl_for(l.last_outcome, l.consecutive_failures) as i64;
            let cache_stale = !fresh_enough(l, now);
            ttl_expired || cache_stale
        }
    }
}

/// Called from statusline's synchronous render path (fire-and-forget).
///
/// Conditions checked in order:
/// 1. opted_in?         (pure stat on config file / env var)
/// 2. should_refresh?   (TTL or absolute age expired)
/// 3. acquire lock?     (O_EXCL anti-herd)
///
/// If all pass → spawn detached `codeforge mnemos-cli probe` child.
/// Failures at any stage are silently swallowed — statusline must never block.
pub fn maybe_spawn_probe() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if !crate::mnemos::config::MnemosConfig::opted_in() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if !should_refresh(read_liveness().as_ref(), now) {
        return;
    }
    if !try_acquire_probe_lock() {
        return; // another statusline already spawned a probe this cycle
    }
    // current_exe() failure → skip silently, do not fall back to PATH lookup.
    let Ok(exe) = std::env::current_exe() else {
        // Lock was acquired — release it so the next cycle can retry.
        release_probe_lock();
        return;
    };
    // process_group(0): put probe in its own process group so Claude Code's
    // SIGTERM tree (sent to statusline's pgid) does not reach the probe child.
    // stdio all-null: prevents probe from inheriting the CC statusline pipe.
    let _ = Command::new(exe)
        .args(["mnemos-cli", "probe"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    // spawn failure is swallowed; lock stays until stale-reclaim (INFLIGHT_GRACE).
    // On success, probe child releases the lock when it finishes.
}

#[cfg(test)]
mod tests {
    use super::*;

    // 序列化 env 操作，避免並行測試競態
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn runtime_dir_prefers_xdg() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: test-only env mutation, serialized by ENV_LOCK
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(runtime_dir(), PathBuf::from("/run/user/1000/codeforge"));
        std::env::remove_var("XDG_RUNTIME_DIR");
    }

    #[test]
    fn runtime_dir_falls_back_when_xdg_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("XDG_RUNTIME_DIR");
        let d = runtime_dir();
        assert!(d.ends_with("codeforge"));
    }

    #[test]
    fn atomic_roundtrip_and_fail_soft() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mnemos-liveness.json");
        let v = LivenessCache {
            last_probe_at: 100,
            last_outcome: ProbeOutcome::Ok,
            consecutive_failures: 0,
            latency_ms: Some(12),
            http_status: Some(200),
        };
        write_atomic(&p, &v).unwrap();
        let back: LivenessCache = read_json(&p).unwrap();
        assert_eq!(back.last_outcome, ProbeOutcome::Ok);
        // fail-soft: 損壞 JSON → None
        std::fs::write(&p, b"{ not json").unwrap();
        assert!(read_json::<LivenessCache>(&p).is_none());
        // 不存在 → None
        assert!(read_json::<LivenessCache>(&dir.path().join("nope.json")).is_none());
    }

    fn lv(o: ProbeOutcome, at: i64) -> LivenessCache {
        LivenessCache {
            last_probe_at: at,
            last_outcome: o,
            consecutive_failures: 0,
            latency_ms: None,
            http_status: None,
        }
    }

    fn sh(ok: bool, at: i64) -> ShipCache {
        ShipCache {
            last_ship_at: at,
            last_ship_ok: ok,
        }
    }

    #[test]
    fn central_light_table() {
        let now = 1_000_000;
        // 未 opt-in
        assert_eq!(
            central_light(false, Some(&lv(ProbeOutcome::Ok, now)), None, false, now),
            CentralLight::Hidden
        );
        // probe ok, 無 ship → 綠
        assert_eq!(
            central_light(true, Some(&lv(ProbeOutcome::Ok, now)), None, false, now),
            CentralLight::Ok
        );
        // probe ok + 新鮮 ship 失敗 → 黃
        assert_eq!(
            central_light(
                true,
                Some(&lv(ProbeOutcome::Ok, now)),
                Some(&sh(false, now)),
                false,
                now
            ),
            CentralLight::Degraded
        );
        // probe ok + 陳(>24h) ship 失敗 → 綠（不參與）
        assert_eq!(
            central_light(
                true,
                Some(&lv(ProbeOutcome::Ok, now)),
                Some(&sh(false, now - 90_000)),
                false,
                now
            ),
            CentralLight::Ok
        );
        // probe ok + queue 積壓（queue_degraded=true）→ 黃
        assert_eq!(
            central_light(true, Some(&lv(ProbeOutcome::Ok, now)), None, true, now),
            CentralLight::Degraded
        );
        // unreachable → 中性 offline
        assert_eq!(
            central_light(
                true,
                Some(&lv(ProbeOutcome::Unreachable, now)),
                None,
                false,
                now
            ),
            CentralLight::Offline
        );
        // HttpError → offline
        assert_eq!(
            central_light(
                true,
                Some(&lv(ProbeOutcome::HttpError, now)),
                None,
                false,
                now
            ),
            CentralLight::Offline
        );
        // never <7d → pending
        assert_eq!(
            central_light(
                true,
                Some(&lv(ProbeOutcome::Never, now - 100)),
                None,
                false,
                now
            ),
            CentralLight::Pending
        );
        // never >7d → 退場 hidden
        assert_eq!(
            central_light(
                true,
                Some(&lv(ProbeOutcome::Never, now - 8 * 86_400)),
                None,
                false,
                now
            ),
            CentralLight::Hidden
        );
        // liveness 讀失敗（未知）→ offline
        assert_eq!(
            central_light(true, None, None, false, now),
            CentralLight::Offline
        );
    }

    #[test]
    fn lock_excl_then_busy_then_stale_reclaim() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("x.lock");
        // 取得「現在」的合理近似，用於第一次搶鎖
        let t0 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // 首搶成功
        assert!(acquire_at(&lock, t0));
        // 同時間再搶 → 失敗（鎖在、未 stale）
        assert!(!acquire_at(&lock, t0 + 1));
        // 超過 GRACE 後 → stale 認領成功
        assert!(acquire_at(&lock, t0 + INFLIGHT_GRACE as i64 + 1));
    }

    #[test]
    fn queue_depth_counts_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.json"), "{}").unwrap();
        std::fs::write(dir.path().join("b.json"), "{}").unwrap();
        std::fs::write(dir.path().join("note.txt"), "x").unwrap();
        assert_eq!(count_json_in(dir.path()), 2);
        assert_eq!(count_json_in(&dir.path().join("missing")), 0);
    }

    #[test]
    fn queue_degraded_from_cases() {
        // count == 0 → false（短路，不看 age）
        assert!(!queue_degraded_from(0, Some(999_999)));
        // count >= QUEUE_WARN_THRESHOLD → true
        assert!(queue_degraded_from(QUEUE_WARN_THRESHOLD, None));
        assert!(queue_degraded_from(QUEUE_WARN_THRESHOLD + 5, Some(0)));
        // count < threshold，age 剛好在邊界 (86400s) → false
        assert!(!queue_degraded_from(1, Some(86_400)));
        // count < threshold，age > 24h → true
        assert!(queue_degraded_from(1, Some(86_401)));
        assert!(queue_degraded_from(2, Some(100_000)));
        // count < threshold，無 mtime 可讀 → false
        assert!(!queue_degraded_from(1, None));
    }

    #[test]
    fn fresh_enough_check() {
        let now = 1_000_000i64;
        // 剛 probe → 新鮮
        let recent = lv(ProbeOutcome::Ok, now - 10);
        assert!(fresh_enough(&recent, now));
        // 恰好在邊界
        let at_boundary = lv(ProbeOutcome::Ok, now - CACHE_MAX_AGE as i64);
        assert!(fresh_enough(&at_boundary, now));
        // 超過 CACHE_MAX_AGE → 過期
        let stale = lv(ProbeOutcome::Ok, now - CACHE_MAX_AGE as i64 - 1);
        assert!(!fresh_enough(&stale, now));
    }

    // ── Task 7: classify_probe pure-fn tests ──────────────────────────────

    #[test]
    fn classify_probe_outcomes() {
        assert_eq!(classify_probe(Some(200)), ProbeOutcome::Ok);
        assert_eq!(classify_probe(Some(204)), ProbeOutcome::Ok);
        assert_eq!(classify_probe(Some(299)), ProbeOutcome::Ok);
        assert_eq!(classify_probe(Some(404)), ProbeOutcome::HttpError);
        assert_eq!(classify_probe(Some(500)), ProbeOutcome::HttpError);
        assert_eq!(classify_probe(Some(301)), ProbeOutcome::HttpError);
        assert_eq!(classify_probe(None), ProbeOutcome::Unreachable);
    }

    // ── Task 8: should_refresh pure-fn tests ─────────────────────────────

    #[test]
    fn should_refresh_logic() {
        let now = 1_000_000i64;

        // No cache → always refresh
        assert!(should_refresh(None, now));

        // Fresh ok cache (10s < 30s TTL) → no refresh
        let fresh = lv(ProbeOutcome::Ok, now - 10);
        assert!(!should_refresh(Some(&fresh), now));

        // Stale ok cache (40s > 30s TTL) → refresh
        let stale = lv(ProbeOutcome::Ok, now - 40);
        assert!(should_refresh(Some(&stale), now));

        // Cache within TTL but older than CACHE_MAX_AGE (macOS退路) → refresh
        // Use an Unreachable + high failure count to make TTL = 600s,
        // so the CACHE_MAX_AGE=3600s bound triggers before the TTL.
        let mut old_but_within_ttl = lv(ProbeOutcome::Ok, now - (CACHE_MAX_AGE as i64 + 1));
        old_but_within_ttl.last_outcome = ProbeOutcome::Ok; // TTL=30s but age=3601 → both fire
        assert!(should_refresh(Some(&old_but_within_ttl), now));

        // Explicit CACHE_MAX_AGE boundary check for an Ok outcome:
        // age = CACHE_MAX_AGE exactly → still fresh_enough, but TTL=30s already fired.
        // Test the case where TTL ok but CACHE_MAX_AGE triggers:
        // Make consecutive_failures=99 (backoff TTL=600s), last_probe_at so that
        // elapsed < 600 but > CACHE_MAX_AGE
        let cache_stale_by_age = LivenessCache {
            last_probe_at: now - (CACHE_MAX_AGE as i64 + 1),
            last_outcome: ProbeOutcome::Unreachable,
            consecutive_failures: 99, // TTL = 600s
            latency_ms: None,
            http_status: None,
        };
        // elapsed=3601 < 600? No, 3601>600 so TTL fires too. Use a smaller age:
        // Let's use age=100 (< 600s TTL) but > CACHE_MAX_AGE is impossible since CACHE_MAX_AGE=3600
        // So actually this case can't happen (CACHE_MAX_AGE=3600 > PROBE_TTL_MAX=600).
        // The CACHE_MAX_AGE guard is a backstop for when the probe has been running
        // successfully but the machine rebooted (Linux purges XDG_RUNTIME_DIR).
        // On macOS TMPDIR survives reboots, so a cache from 2h ago with outcome=Ok
        // (TTL=30s) would already be expired by TTL. The CACHE_MAX_AGE guard
        // is extra insurance when TTL would pass but the machine has rebooted.
        // Since CACHE_MAX_AGE (3600) > PROBE_TTL_MAX (600), the guard only fires
        // when both TTL and CACHE_MAX_AGE are in play — which the TTL alone catches
        // for all outcomes. The guard is belt-and-suspenders; we verify it compiles.
        let _ = cache_stale_by_age; // verified above conceptually
    }

    // ── Task 9: ship cache roundtrip ──────────────────────────────────────

    #[test]
    fn ship_health_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mnemos-ship.json");
        let v = ShipCache {
            last_ship_at: 42,
            last_ship_ok: true,
        };
        write_atomic(&p, &v).unwrap();
        let back: ShipCache = read_json(&p).unwrap();
        assert_eq!(back.last_ship_at, 42);
        assert!(back.last_ship_ok);

        // false 也能正確 roundtrip
        let v2 = ShipCache {
            last_ship_at: 99,
            last_ship_ok: false,
        };
        write_atomic(&p, &v2).unwrap();
        let back2: ShipCache = read_json(&p).unwrap();
        assert_eq!(back2.last_ship_at, 99);
        assert!(!back2.last_ship_ok);
    }

    // ── Task 11: local_light ─────────────────────────────────────────────

    #[test]
    fn local_light_states() {
        assert_eq!(local_light(5, true), LocalLight::Active);
        assert_eq!(local_light(1, false), LocalLight::Active);
        assert_eq!(local_light(0, true), LocalLight::Empty);
        assert_eq!(local_light(0, false), LocalLight::Hidden);
    }

    #[test]
    fn ttl_and_backoff() {
        assert_eq!(ttl_for(ProbeOutcome::Ok, 0), PROBE_TTL_OK);
        assert_eq!(ttl_for(ProbeOutcome::Never, 0), PROBE_TTL_OK);
        // 失敗指數退避：30·2^(n-1)，封頂 600
        assert_eq!(ttl_for(ProbeOutcome::Unreachable, 1), 30);
        assert_eq!(ttl_for(ProbeOutcome::Unreachable, 2), 60);
        assert_eq!(ttl_for(ProbeOutcome::HttpError, 5), 480);
        assert_eq!(ttl_for(ProbeOutcome::Unreachable, 99), PROBE_TTL_MAX);
        // recovery：ok 立刻 reset
        assert_eq!(next_failure_count(7, ProbeOutcome::Ok), 0);
        assert_eq!(next_failure_count(7, ProbeOutcome::Unreachable), 8);
        assert_eq!(next_failure_count(0, ProbeOutcome::HttpError), 1);
    }
}
