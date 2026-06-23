# Brain Connection Indicators Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** statusline bottom border 的寫死 `memory ● active` 升級成兩顆真實狀態燈 —— 本地腦（L1）+ 央腦 Mnemos（probe liveness + ship readiness），加 `codeforge doctor` 漸進揭露。

**Architecture:** 雙軸健康模型（probe=liveness、ship=readiness，防 OR 假綠）。statusline 熱路徑只讀 per-machine 雙快取（liveness/ship JSON）即時 render，永不阻塞；快取由 detached `mnemos-cli probe` 子進程（process_group 隔離、O_EXCL rename-steal 鎖防 herd）與 ship 順風車寫入。純 std、零新 dep。

**Tech Stack:** Rust 2021、clap 4、rusqlite、reqwest(rustls)+tokio、owo-colors(`if_supports_color`)、serde_json、rust-i18n。

## Global Constraints

- statusline 熱路徑（每 ~5s × 每專案 × 每 session）**禁同步網路 / 阻塞 I/O**。
- CJK 截斷 `.chars().take(N).collect::<String>()`，禁 `&s[..N]`。
- `anyhow::Result`；user-facing 正體中文。
- **零新 dep**：detach / lock / boot-age 全用 std。fmt 走 `./scripts/fmt.sh`。
- probe target = `MnemosConfig::load().base_url` + `/health`（非 `/v1/health`）。HTTP 200 = ok（不解 body）。
- 央腦健康快取放 **per-machine** `$XDG_RUNTIME_DIR/codeforge/`（macOS fallback `$TMPDIR`），**禁** home dotfile。
- 可調常數集中 `src/mnemos/health.rs`：`PROBE_TTL_OK=30s`、`PROBE_TTL_MAX=10m`、`SHIP_FRESH_WINDOW=24h`、`NEVER_RETIRE_DAYS=7d`、`CACHE_MAX_AGE=1h`、`INFLIGHT_GRACE=10s`、`PROBE_CONNECT_TIMEOUT=2s`、`QUEUE_WARN_THRESHOLD=3`。
- Spec: `doc/specs/codeforge-brain-indicators.md`（單一真實來源）。

---

## File Structure

- **Create** `src/mnemos/health.rs` — 央腦健康全部：常數、runtime dir 解析、雙快取 schema + atomic I/O、TTL/backoff、雙軸狀態計算（`CentralLight`）、O_EXCL rename-steal 鎖、probe 執行邏輯。
- **Create** `src/cli/doctor.rs` — `codeforge doctor` 命令。
- **Modify** `src/mnemos/mod.rs` — `pub mod health;`。
- **Modify** `src/cli/mnemos_cli.rs` — 加 `probe` 子命令（+ `--verbose`）。
- **Modify** `src/cli/ship.rs` — 真 POST / flush 成功時寫 `mnemos-ship.json`（opt-in gated）。
- **Modify** `src/cli/statusline.rs` — `BrainHealth` 聚合、local 燈、`should_spawn` + detached probe spawn、`bottom_border` 改「量測再印」+ 雙燈 + NO_COLOR + 降級階梯。
- **Modify** `src/main.rs` — dispatch `doctor`、`mnemos-cli probe`。
- **Modify** `locales/en.yaml` + `locales/zh-TW.yaml` — 燈 word keys。

執行順序 bottom-up：常數/path → 快取 schema → TTL → 狀態計算 → 鎖 → probe 子命令 → ship 順風車 → queue 即時讀 → spawn → i18n → local 燈 → NO_COLOR → bottom_border → doctor → wire。

---

### Task 1: health.rs 骨架 — 常數 + per-machine runtime dir 解析

**Files:**
- Create: `src/mnemos/health.rs`
- Modify: `src/mnemos/mod.rs`（加 `pub mod health;`）
- Test: 同檔 `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `pub const PROBE_TTL_OK: u64 = 30;` 等全部常數（秒）。
  - `pub fn runtime_dir() -> std::path::PathBuf` — `$XDG_RUNTIME_DIR/codeforge`，無則 `$TMPDIR/codeforge` 或 `std::env::temp_dir().join("codeforge")`。

- [ ] **Step 1: 寫失敗測試**

```rust
// src/mnemos/health.rs (整檔開頭)
//! 央腦（Mnemos）連線健康 — probe liveness + ship readiness。
//! statusline 熱路徑只讀此模組算好的 CentralHealth；快取 per-machine。
use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_dir_prefers_xdg() {
        // SAFETY: test-only env mutation
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(runtime_dir(), PathBuf::from("/run/user/1000/codeforge"));
        std::env::remove_var("XDG_RUNTIME_DIR");
    }

    #[test]
    fn runtime_dir_falls_back_when_xdg_unset() {
        std::env::remove_var("XDG_RUNTIME_DIR");
        let d = runtime_dir();
        assert!(d.ends_with("codeforge"));
    }
}
```

- [ ] **Step 2: 跑測試確認失敗** — Run: `cargo test -p codeforge mnemos::health::tests::runtime_dir`（先在 `src/mnemos/mod.rs` 加 `pub mod health;` 否則 module 不存在）。Expected: 編譯後 PASS（此 task 程式即實作，測試應直接綠）。
- [ ] **Step 3:** 在 `src/mnemos/mod.rs` 加一行 `pub mod health;`（放既有 `pub mod` 群組）。
- [ ] **Step 4: 跑測試確認通過** — Run: `cargo test mnemos::health`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs src/mnemos/mod.rs
git commit -m "feat(health): scaffold mnemos health module — constants + per-machine runtime dir"
```

---

### Task 2: 雙快取 schema + atomic 讀寫（temp+rename, fail-soft）

**Files:**
- Modify: `src/mnemos/health.rs`
- Test: 同檔

**Interfaces:**
- Consumes: `runtime_dir()`（Task 1）。
- Produces:
  - `pub enum ProbeOutcome { Never, Unreachable, HttpError, Ok }`（serde rename snake_case）。
  - `pub struct LivenessCache { pub last_probe_at: i64, pub last_outcome: ProbeOutcome, pub consecutive_failures: u32, pub latency_ms: Option<u32>, pub http_status: Option<u16> }`
  - `pub struct ShipCache { pub last_ship_at: i64, pub last_ship_ok: bool }`
  - `pub fn liveness_path() -> PathBuf` / `ship_path() -> PathBuf` / `lock_path() -> PathBuf`
  - `pub fn read_liveness() -> Option<LivenessCache>` / `read_ship() -> Option<ShipCache>`（讀/解析失敗回 None = fail-soft）。
  - `pub fn write_atomic<T: Serialize>(path: &Path, val: &T) -> std::io::Result<()>`（temp `<name>.<pid>.<rand>.tmp` 同 dir + rename；mkdir -p；無 fsync）。

- [ ] **Step 1: 寫失敗測試**

```rust
// 追加到 health.rs
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome { Never, Unreachable, HttpError, Ok }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessCache {
    pub last_probe_at: i64,
    pub last_outcome: ProbeOutcome,
    pub consecutive_failures: u32,
    pub latency_ms: Option<u32>,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipCache { pub last_ship_at: i64, pub last_ship_ok: bool }

pub fn liveness_path() -> PathBuf { runtime_dir().join("mnemos-liveness.json") }
pub fn ship_path() -> PathBuf { runtime_dir().join("mnemos-ship.json") }
pub fn lock_path() -> PathBuf { runtime_dir().join("mnemos-liveness.lock") }
```

```rust
#[test]
fn atomic_roundtrip_and_fail_soft() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("mnemos-liveness.json");
    let v = LivenessCache { last_probe_at: 100, last_outcome: ProbeOutcome::Ok,
        consecutive_failures: 0, latency_ms: Some(12), http_status: Some(200) };
    write_atomic(&p, &v).unwrap();
    let back: LivenessCache = read_json(&p).unwrap();
    assert_eq!(back.last_outcome, ProbeOutcome::Ok);
    // fail-soft: 損壞 JSON → None
    std::fs::write(&p, b"{ not json").unwrap();
    assert!(read_json::<LivenessCache>(&p).is_none());
    // 不存在 → None
    assert!(read_json::<LivenessCache>(&dir.path().join("nope.json")).is_none());
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::atomic_roundtrip`. Expected: FAIL（`write_atomic`/`read_json` 未定義）。
- [ ] **Step 3: 實作**

```rust
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

pub fn read_liveness() -> Option<LivenessCache> { read_json(&liveness_path()) }
pub fn read_ship() -> Option<ShipCache> { read_json(&ship_path()) }
```

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs
git commit -m "feat(health): dual cache schema + atomic write + fail-soft read"
```

---

### Task 3: TTL / backoff 純函式（含 recovery reset）

**Files:** Modify `src/mnemos/health.rs` · Test 同檔

**Interfaces:**
- Consumes: 常數、`ProbeOutcome`。
- Produces: `pub fn ttl_for(outcome: ProbeOutcome, consecutive_failures: u32) -> u64`；`pub fn next_failure_count(prev: u32, outcome: ProbeOutcome) -> u32`。

- [ ] **Step 1: 測試**

```rust
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
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::ttl_and_backoff`. Expected: FAIL。
- [ ] **Step 3: 實作**

```rust
pub fn ttl_for(outcome: ProbeOutcome, consecutive_failures: u32) -> u64 {
    match outcome {
        ProbeOutcome::Ok | ProbeOutcome::Never => PROBE_TTL_OK,
        ProbeOutcome::Unreachable | ProbeOutcome::HttpError => {
            let n = consecutive_failures.max(1);
            PROBE_TTL_OK.saturating_mul(1u64 << (n - 1).min(20)).min(PROBE_TTL_MAX)
        }
    }
}

pub fn next_failure_count(prev: u32, outcome: ProbeOutcome) -> u32 {
    match outcome {
        ProbeOutcome::Ok => 0,
        _ => prev.saturating_add(1),
    }
}
```

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs
git commit -m "feat(health): TTL backoff + recovery reset pure fns"
```

---

### Task 4: 雙軸狀態計算 `CentralLight`（spec §2.1 每格）

**Files:** Modify `src/mnemos/health.rs` · Test 同檔

**Interfaces:**
- Consumes: `LivenessCache`/`ShipCache`/`ProbeOutcome`、常數。
- Produces:
  - `pub enum CentralLight { Hidden, Ok, Degraded, Offline, Pending }`
  - `pub fn central_light(opted_in: bool, liveness: Option<&LivenessCache>, ship: Option<&ShipCache>, queue_depth: usize, now: i64) -> CentralLight`
  - 規則：未 opt-in → Hidden。liveness None（讀失敗/未知）→ Offline。outcome=Never 且 `now-last_probe_at > NEVER_RETIRE_DAYS*86400` → Hidden，否則 Pending。Unreachable/HttpError → Offline。Ok → 看 ship：ship 新鮮(`now-last_ship_at<=SHIP_FRESH_WINDOW`)且 `!last_ship_ok` → Degraded；queue_depth>=QUEUE_WARN_THRESHOLD → Degraded；否則 Ok。

- [ ] **Step 1: 測試（涵蓋狀態表每格）**

```rust
fn lv(o: ProbeOutcome, at: i64) -> LivenessCache {
    LivenessCache { last_probe_at: at, last_outcome: o, consecutive_failures: 0,
        latency_ms: None, http_status: None }
}
fn sh(ok: bool, at: i64) -> ShipCache { ShipCache { last_ship_at: at, last_ship_ok: ok } }

#[test]
fn central_light_table() {
    let now = 1_000_000;
    // 未 opt-in
    assert_eq!(central_light(false, Some(&lv(ProbeOutcome::Ok, now)), None, 0, now), CentralLight::Hidden);
    // probe ok, 無 ship → 綠
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Ok, now)), None, 0, now), CentralLight::Ok);
    // probe ok + 新鮮 ship 失敗 → 黃
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Ok, now)), Some(&sh(false, now)), 0, now), CentralLight::Degraded);
    // probe ok + 陳(>24h) ship 失敗 → 綠（不參與）
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Ok, now)), Some(&sh(false, now - 90_000)), 0, now), CentralLight::Ok);
    // probe ok + queue 積壓 → 黃
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Ok, now)), None, 3, now), CentralLight::Degraded);
    // unreachable → 中性 offline
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Unreachable, now)), None, 0, now), CentralLight::Offline);
    // never <7d → pending
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Never, now - 100)), None, 0, now), CentralLight::Pending);
    // never >7d → 退場 hidden
    assert_eq!(central_light(true, Some(&lv(ProbeOutcome::Never, now - 8 * 86_400)), None, 0, now), CentralLight::Hidden);
    // liveness 讀失敗（未知）→ offline
    assert_eq!(central_light(true, None, None, 0, now), CentralLight::Offline);
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::central_light_table`. Expected: FAIL。
- [ ] **Step 3: 實作**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CentralLight { Hidden, Ok, Degraded, Offline, Pending }

pub fn central_light(
    opted_in: bool,
    liveness: Option<&LivenessCache>,
    ship: Option<&ShipCache>,
    queue_depth: usize,
    now: i64,
) -> CentralLight {
    if !opted_in {
        return CentralLight::Hidden;
    }
    let Some(lv) = liveness else { return CentralLight::Offline }; // 未知視同 offline
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
            if fresh_ship_fail || queue_depth >= QUEUE_WARN_THRESHOLD {
                CentralLight::Degraded
            } else {
                CentralLight::Ok
            }
        }
    }
}
```

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs
git commit -m "feat(health): dual-axis central_light state computation (spec table)"
```

---

### Task 5: queue 深度即時讀 + 閾值

**Files:** Modify `src/mnemos/health.rs` · Test 同檔

**Interfaces:**
- Consumes: `crate::mnemos::state::ship_failed_dir`、`QUEUE_WARN_THRESHOLD`。
- Produces: `pub fn queue_depth() -> usize`（`read_dir` 數 `.json`；count==0 短路；錯誤→0）。

- [ ] **Step 1: 測試**

```rust
#[test]
fn queue_depth_counts_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.json"), "{}").unwrap();
    std::fs::write(dir.path().join("b.json"), "{}").unwrap();
    std::fs::write(dir.path().join("note.txt"), "x").unwrap();
    assert_eq!(count_json_in(dir.path()), 2);
    assert_eq!(count_json_in(&dir.path().join("missing")), 0);
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::queue_depth_counts_json`. Expected: FAIL。
- [ ] **Step 3: 實作**

```rust
fn count_json_in(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
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
```

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs
git commit -m "feat(health): live queue depth from ship-failed/ dir"
```

---

### Task 6: O_EXCL rename-steal 鎖

**Files:** Modify `src/mnemos/health.rs` · Test 同檔

**Interfaces:**
- Consumes: `lock_path()`、`INFLIGHT_GRACE`。
- Produces: `pub fn try_acquire_probe_lock() -> bool`（無 stale → `create_new`；stale（mtime 老於 GRACE）→ `rename(stale→owned_tmp)` 原子認領後 `create_new`；拿到回 true）；`pub fn release_probe_lock()`（unlink，best-effort）。

- [ ] **Step 1: 測試**

```rust
#[test]
fn lock_excl_then_busy_then_stale_reclaim() {
    let dir = tempfile::tempdir().unwrap();
    let lock = dir.path().join("x.lock");
    // 首搶成功
    assert!(acquire_at(&lock, 1000));
    // 同時間再搶 → 失敗（鎖在、未 stale）
    assert!(!acquire_at(&lock, 1001));
    // 超過 GRACE 後 → stale 認領成功
    assert!(acquire_at(&lock, 1000 + INFLIGHT_GRACE as i64 + 1));
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::lock_excl`. Expected: FAIL。
- [ ] **Step 3: 實作**（`acquire_at` 為注入 now/path 的可測核心；`try_acquire_probe_lock` 為薄包裝）

```rust
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
                OpenOptions::new().write(true).create_new(true).open(lock).is_ok()
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn try_acquire_probe_lock() -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    acquire_at(&lock_path(), now)
}

pub fn release_probe_lock() {
    let _ = std::fs::remove_file(lock_path());
}
```

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs
git commit -m "feat(health): O_EXCL rename-steal probe lock (anti-herd)"
```

---

### Task 7: probe 子進程本體 + `mnemos-cli probe`（+ --verbose）

**Files:**
- Modify: `src/mnemos/health.rs`（probe 執行）、`src/cli/mnemos_cli.rs`（子命令）、`src/main.rs`（dispatch）
- Test: health.rs（outcome 分類純函式）

**Interfaces:**
- Consumes: `MnemosConfig::load()`、`LivenessCache`、`try_acquire_probe_lock`/`release_probe_lock`、`write_atomic`、`ttl/backoff`。
- Produces:
  - `pub fn classify_probe(status: Option<u16>) -> ProbeOutcome`（None=網路錯→Unreachable；200..=299→Ok；其餘→HttpError）。
  - `pub fn run_probe(verbose: bool) -> anyhow::Result<()>`（current_thread runtime + 2s client GET /health；算 outcome + latency；read 舊 liveness 取 prev failures；寫新 liveness；release lock；verbose 印 stderr 不寫快取）。
  - mnemos_cli：`probe` 子命令呼叫 `run_probe`。

- [ ] **Step 1: 測試（純分類）**

```rust
#[test]
fn classify_probe_outcomes() {
    assert_eq!(classify_probe(Some(200)), ProbeOutcome::Ok);
    assert_eq!(classify_probe(Some(204)), ProbeOutcome::Ok);
    assert_eq!(classify_probe(Some(404)), ProbeOutcome::HttpError);
    assert_eq!(classify_probe(Some(500)), ProbeOutcome::HttpError);
    assert_eq!(classify_probe(None), ProbeOutcome::Unreachable);
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::classify_probe_outcomes`. Expected: FAIL。
- [ ] **Step 3: 實作**

```rust
pub fn classify_probe(status: Option<u16>) -> ProbeOutcome {
    match status {
        None => ProbeOutcome::Unreachable,
        Some(s) if (200..=299).contains(&s) => ProbeOutcome::Ok,
        Some(_) => ProbeOutcome::HttpError,
    }
}

pub fn run_probe(verbose: bool) -> anyhow::Result<()> {
    use std::time::Duration;
    let cfg = crate::mnemos::config::MnemosConfig::load()?;
    let url = format!("{}/health", cfg.base_url);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let (status, latency_ms) = rt.block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(PROBE_CONNECT_TIMEOUT))
            .timeout(Duration::from_secs(PROBE_CONNECT_TIMEOUT + 1))
            .build()
            .unwrap_or_default();
        let t0 = std::time::Instant::now();
        match client.get(&url).send().await {
            Ok(r) => (Some(r.status().as_u16()), Some(t0.elapsed().as_millis() as u32)),
            Err(_) => (None, None),
        }
    });
    let outcome = classify_probe(status);
    if verbose {
        eprintln!("probe {url} → {outcome:?} status={status:?} latency={latency_ms:?}ms");
        return Ok(()); // verbose 不寫快取
    }
    let prev = read_liveness().map(|l| l.consecutive_failures).unwrap_or(0);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let cache = LivenessCache {
        last_probe_at: now,
        last_outcome: outcome,
        consecutive_failures: next_failure_count(prev, outcome),
        latency_ms,
        http_status: status,
    };
    let _ = write_atomic(&liveness_path(), &cache);
    release_probe_lock();
    Ok(())
}
```

mnemos_cli.rs：在既有 subcommand enum 加 `Probe { #[arg(long)] verbose: bool }`，match arm 呼叫 `crate::mnemos::health::run_probe(verbose)`。main.rs dispatch 既有 mnemos-cli 路徑已涵蓋（跟現有 cite/context 同層）。

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health` 然後 `cargo build`。手動 smoke：`cargo run -- mnemos-cli probe --verbose`（印 outcome）。Expected: 測試 PASS、build OK。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs src/cli/mnemos_cli.rs src/main.rs
git commit -m "feat(health): mnemos-cli probe — GET /health, classify, write liveness cache"
```

---

### Task 8: statusline detached spawn（process_group + stdio null + should_spawn）

**Files:** Modify `src/mnemos/health.rs`（spawn helper）+ `src/cli/statusline.rs`（呼叫點）· Test: spawn 決策純函式

**Interfaces:**
- Consumes: `read_liveness`、`ttl_for`、`try_acquire_probe_lock`、`MnemosConfig::opted_in`。
- Produces:
  - `pub fn should_refresh(liveness: Option<&LivenessCache>, now: i64) -> bool`（None→true；`now-last_probe_at > ttl_for(outcome, failures)`）。
  - `pub fn maybe_spawn_probe()`（opt-in？should_refresh？try_acquire_probe_lock？→ spawn detached `current_exe mnemos-cli probe`）。

- [ ] **Step 1: 測試**

```rust
#[test]
fn should_refresh_logic() {
    let now = 1_000_000;
    assert!(should_refresh(None, now)); // 無快取 → 刷新
    let fresh = lv(ProbeOutcome::Ok, now - 10);
    assert!(!should_refresh(Some(&fresh), now)); // 10s < 30s TTL
    let stale = lv(ProbeOutcome::Ok, now - 40);
    assert!(should_refresh(Some(&stale), now)); // 40s > 30s
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test mnemos::health::tests::should_refresh_logic`. Expected: FAIL。
- [ ] **Step 3: 實作**

```rust
pub fn should_refresh(liveness: Option<&LivenessCache>, now: i64) -> bool {
    match liveness {
        None => true,
        Some(l) => now - l.last_probe_at > ttl_for(l.last_outcome, l.consecutive_failures) as i64,
    }
}

/// statusline 呼叫：條件滿足才 fire-and-forget spawn detached probe。永不阻塞。
pub fn maybe_spawn_probe() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    if !crate::mnemos::config::MnemosConfig::opted_in() {
        return;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    if !should_refresh(read_liveness().as_ref(), now) {
        return;
    }
    if !try_acquire_probe_lock() {
        return; // 別人正在 probe
    }
    let Ok(exe) = std::env::current_exe() else { return }; // 失敗：不 spawn、不報錯
    // process_group(0)：隔離 CC 對其 pgid 的 SIGTERM 樹狀 cleanup；stdio 全 null 脫 pipe。
    let _ = Command::new(exe)
        .args(["mnemos-cli", "probe"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    // 注意：spawn 失敗也吞掉。鎖由 probe 結束時 release；crash 殘留靠 mtime stale 回收。
}
```

statusline.rs `run()`：在算完 render 資料、寫 stdout **之前或之後**呼叫 `crate::mnemos::health::maybe_spawn_probe();`（同步路徑、非 tokio context）。

- [ ] **Step 4: 跑確認通過** — Run: `cargo test mnemos::health` + `cargo build`。Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/mnemos/health.rs src/cli/statusline.rs
git commit -m "feat(health): detached probe spawn from statusline (process_group, anti-herd)"
```

---

### Task 9: ship 順風車寫 mnemos-ship.json

**Files:** Modify `src/cli/ship.rs` · Test: ship.rs 既有測試風格（或 health.rs helper 測）

**Interfaces:**
- Consumes: `health::{write_atomic, ship_path, ShipCache}`、`MnemosConfig::opted_in`。
- Produces: ship.rs 內 `fn record_ship_health(ok: bool)`（opt-in gated；寫 `ShipCache { last_ship_at: now, last_ship_ok: ok }`）。在真 POST 三分支（`Ok`/`Exhausted`/`BadRequest`）+ flush 成功處呼叫。

- [ ] **Step 1: 測試**（helper 純行為）

```rust
// 在 ship.rs #[cfg(test)] 或 health.rs：驗 record 寫出正確 ok 值
#[test]
fn ship_health_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("mnemos-ship.json");
    let v = crate::mnemos::health::ShipCache { last_ship_at: 42, last_ship_ok: true };
    crate::mnemos::health::write_atomic(&p, &v).unwrap();
    let back: crate::mnemos::health::ShipCache = crate::mnemos::health::read_json(&p).unwrap();
    assert!(back.last_ship_ok);
    assert_eq!(back.last_ship_at, 42);
}
```

- [ ] **Step 2: 跑確認失敗/通過** — Run: `cargo test ship_health_roundtrip`. Expected: PASS（沿用 Task 2 API）。
- [ ] **Step 3: 實作**（ship.rs）

```rust
// ship.rs 內新增
fn record_ship_health(ok: bool) {
    use crate::mnemos::config::MnemosConfig;
    if !MnemosConfig::opted_in() {
        return; // 寫快取 gate 與渲染同源；互動 ship 雖 POST 但不寫健康快取
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let _ = crate::mnemos::health::write_atomic(
        &crate::mnemos::health::ship_path(),
        &crate::mnemos::health::ShipCache { last_ship_at: now, last_ship_ok: ok },
    );
}
```

接點（對照 `ship.rs` 行號，實作時以實際分支為準）：
- fresh-digest POST 結果：`SendResult::Ok` 分支 → `record_ship_health(true)`；`Exhausted` / `BadRequest` 分支 → `record_ship_health(false)`。
- `flush_failed_queue` 回傳成功（至少清掉一筆）→ `record_ship_health(true)`。
- **早 return 路徑不呼叫**（no-ship opt-out、未 opt-in no-hook、--resend、--dry-run、already_shipped 純早退、empty lessons）。

- [ ] **Step 4: 跑確認通過** — Run: `cargo test` + `cargo build`。Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/cli/ship.rs src/mnemos/health.rs
git commit -m "feat(ship): record ship readiness to per-machine cache on real POST/flush"
```

---

### Task 10: i18n 燈 word keys

**Files:** Modify `locales/en.yaml`、`locales/zh-TW.yaml` · Test: 編譯期（rust-i18n）

**Interfaces:**
- Produces: keys `ui.brain.local_active`、`ui.brain.local_empty`、`ui.brain.ok`、`ui.brain.degraded`、`ui.brain.offline`、`ui.brain.pending`、`ui.brain.local_label`(=memory)、`ui.brain.central_label`(=mnemos)。

- [ ] **Step 1:** 在兩個 yaml 加 keys（en：active/ok/degraded/offline/pending/empty；zh-TW：相同英文短詞或維持英文 —— 與既有 `active` 一致用英文短詞，label 用 `memory`/`mnemos`）。
- [ ] **Step 2: 跑確認** — Run: `cargo build`（rust-i18n compile-time 檢查 key 存在）。Expected: OK。
- [ ] **Step 3:** （無 code 實作，純 yaml）
- [ ] **Step 4:** `./scripts/fmt.sh`（yaml 不受影響，跑 build 確認）。
- [ ] **Step 5: Commit**

```bash
git add locales/en.yaml locales/zh-TW.yaml
git commit -m "feat(i18n): brain indicator word keys"
```

---

### Task 11: local 燈計算

**Files:** Modify `src/cli/statusline.rs`（或 health.rs 一個 pure fn）· Test 同檔

**Interfaces:**
- Consumes: L1 active count（statusline 已開 conn 可查）、`store/` 是否存在。
- Produces: `enum LocalLight { Hidden, Active, Empty }` + `fn local_light(active_l1: usize, has_store_history: bool) -> LocalLight`（active>0→Active；==0 且有歷史→Empty；否則 Hidden）。

- [ ] **Step 1: 測試**

```rust
#[test]
fn local_light_states() {
    assert_eq!(local_light(5, true), LocalLight::Active);
    assert_eq!(local_light(0, true), LocalLight::Empty);
    assert_eq!(local_light(0, false), LocalLight::Hidden);
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test local_light_states`. Expected: FAIL。
- [ ] **Step 3: 實作** `fn local_light(...)`（如上邏輯）。L1 active count 來源：用既有 L1/FTS 查詢（實作時找 `src/memory/l1.rs` 的 count API；若無則加一個 `count_active() -> usize`）。`has_store_history` = `.codeforge/store/concepts/` 存在且非空。
- [ ] **Step 4: 跑確認通過** — Run: `cargo test`. Expected: PASS。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/cli/statusline.rs
git commit -m "feat(statusline): local brain light state computation"
```

---

### Task 12: NO_COLOR 重構（裸 truecolor → if_supports_color）

**Files:** Modify `src/cli/statusline.rs`（色彩 helper `:205-214`）· Test: NO_COLOR 行為

**Interfaces:**
- Modify: `tc`/`tc_bold`/`tcs` 改走 `owo_colors` 的 `if_supports_color(Stream::Stdout, ...)`，使 `NO_COLOR`/非-tty 自動退無色。
- Produces: 無色時不含 ANSI escape。

- [ ] **Step 1: 測試**

```rust
#[test]
fn no_color_strips_ansi() {
    std::env::set_var("NO_COLOR", "1");
    let s = tc("hi", (0xFF, 0x00, 0x00));
    assert!(!s.contains('\x1b'), "NO_COLOR 下不該有 ANSI");
    std::env::remove_var("NO_COLOR");
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test no_color_strips_ansi`. Expected: FAIL（現況裸 truecolor 永遠有 ANSI）。
- [ ] **Step 3: 實作** — `tc` 等改用 `owo_colors::OwoColorize::if_supports_color(Stream::Stdout, |t| t.truecolor(r,g,b))`（features `supports-colors` 已開）。注意：`if_supports_color` 偵測 stdout TTY/NO_COLOR/FORCE_COLOR。確保 format 後字串行為一致。
- [ ] **Step 4: 跑確認通過** — Run: `cargo test`. Expected: PASS。手動：`NO_COLOR=1 cargo run -- statusline < sample.json` 看無 ANSI。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/cli/statusline.rs
git commit -m "refactor(statusline): route colors through owo if_supports_color (NO_COLOR aware)"
```

---

### Task 13: bottom_border 改「量測再印」+ 雙燈 + 降級階梯

**Files:** Modify `src/cli/statusline.rs`（`bottom_border` `:1050-1069` 重寫，新簽名帶 BrainHealth）· Test: 降級/CJK 寬度/無色

**Interfaces:**
- Consumes: `BrainHealth { local: LocalLight, central: CentralLight }`（statusline 聚合）、`CentralLight`/`LocalLight`、i18n words、`vis()`/`ansi_vis()`/`pad`。
- Produces: `fn bottom_border(panel_w, delim_c, ver_str, ver_vis, brain: &BrainHealth, no_color: bool) -> String`。
- 渲染順序（spec §4.3）：local 段 → central 段（非 Hidden 時）→ fill → version → `──╯`；逐級量測：fixed_full > panel_w 時依序砍 `→ doctor` hint → central word→短碼（**無色時跳過短碼直接砍 chip**）→ version chip → central 純 glyph → 只留 local。

- [ ] **Step 1: 測試**（行為斷言：寬版含兩燈詞、窄版降級不溢出）

```rust
fn bh(l: LocalLight, c: CentralLight) -> BrainHealth { BrainHealth { local: l, central: c } }

#[test]
fn bottom_border_wide_shows_both() {
    let s = bottom_border(80, DELIM, String::new(), 0, &bh(LocalLight::Active, CentralLight::Ok), true);
    assert!(s.contains("memory"));
    assert!(s.contains("mnemos"));
    assert!(ansi_vis(&s) <= 80, "不可溢出 panel_w");
}

#[test]
fn bottom_border_hidden_central_omits_mnemos() {
    let s = bottom_border(80, DELIM, String::new(), 0, &bh(LocalLight::Active, CentralLight::Hidden), true);
    assert!(s.contains("memory"));
    assert!(!s.contains("mnemos"));
}

#[test]
fn bottom_border_narrow_no_overflow() {
    for w in [20usize, 30, 40, 60] {
        let s = bottom_border(w, DELIM, "v0.0.5".into(), 6, &bh(LocalLight::Active, CentralLight::Degraded), true);
        assert!(ansi_vis(&s) <= w, "w={w} 溢出");
    }
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test bottom_border`. Expected: FAIL（簽名變更、未實作降級）。
- [ ] **Step 3: 實作** — 重寫 `bottom_border`：先組各段候選字串 + 量測 vis，套降級階梯（仿既有 `:620-627` hint 的 avail 量測選擇）。glyph：local Active=`●`綠/Empty=`◌`灰；central Ok=`●`綠/Degraded=`◐`黃/Offline=`○`灰/Pending=`◌`灰。word 由 i18n。無色路徑用 `<label>:<word>`。所有寬度用 `vis()`（CJK-safe）。更新 `render_full` 的呼叫點傳入 `BrainHealth` + `no_color`（由 `if_supports_color` 或 env 判定）。
- [ ] **Step 4: 跑確認通過** — Run: `cargo test`. 手動：多寬度 `CODEFORGE_WIDTH=70 cargo run -- statusline < sample.json`。Expected: PASS、不破版。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/cli/statusline.rs
git commit -m "feat(statusline): dual brain lights in bottom border with degradation ladder"
```

---

### Task 14: 串接 statusline run() — 聚合 BrainHealth + spawn

**Files:** Modify `src/cli/statusline.rs`（`run()` + `render_full`/`render_no_pet` 呼叫鏈）· Test: 編譯 + 手動 smoke

**Interfaces:**
- Consumes: `health::{read_liveness, read_ship, queue_depth, central_light, maybe_spawn_probe}`、`MnemosConfig::opted_in`、`local_light`、L1 count。
- Produces: `run()` 內組 `BrainHealth`，傳給 render；呼叫 `maybe_spawn_probe()`。

- [ ] **Step 1:** 在 `run()` 算出 `local = local_light(l1_count, has_store)`；`central = central_light(opted_in(), read_liveness().as_ref(), read_ship().as_ref(), queue_depth(), now)`；組 `BrainHealth`。`render_no_pet` 與 `render_full` 兩條路徑都改用新 `bottom_border` 簽名（render_no_pet 目前無 bottom_border —— 確認它是否要燈；依 spec 只 render_full 有框，no_pet 維持不變或只在有 opt-in 時於 row 末顯示，**保守：先只動 render_full**）。
- [ ] **Step 2:** `run()` 結尾（render 後）呼叫 `crate::mnemos::health::maybe_spawn_probe();`。
- [ ] **Step 3:** `cargo build`；手動 smoke 多情境（無 opt-in / opt-in 無 server / 有 server）。
- [ ] **Step 4: 跑全測** — Run: `cargo test && cargo clippy && ./scripts/fmt.sh --check`. Expected: 綠。
- [ ] **Step 5: Commit**

```bash
git add src/cli/statusline.rs
git commit -m "feat(statusline): wire BrainHealth aggregation + probe spawn into run()"
```

---

### Task 15: `codeforge doctor` 命令

**Files:** Create `src/cli/doctor.rs` · Modify `src/main.rs`（clap subcommand `Doctor` + dispatch）· Test: 輸出含關鍵欄位

**Interfaces:**
- Consumes: `health::{read_liveness, read_ship, queue_depth, run_probe}`、`MnemosConfig`、L1 count。
- Produces: `pub fn run(ctx: &db::Context) -> anyhow::Result<()>`：列 local L1 count、central opt-in、即時前景 probe（~2s）、上次 ship、queue 深度與最舊一筆、base_url；黃/灰態附 next-step 中文建議。

- [ ] **Step 1: 測試**（doctor 純文字組裝抽成 `fn render_doctor(...)->String`，斷言含關鍵 label）

```rust
#[test]
fn doctor_lists_dimensions() {
    let out = render_doctor(/* fixture: l1=3, opted_in=true, outcome=Ok, ... */);
    assert!(out.contains("local"));
    assert!(out.contains("mnemos"));
    assert!(out.contains("queue"));
}
```

- [ ] **Step 2: 跑確認失敗** — Run: `cargo test doctor_lists_dimensions`. Expected: FAIL。
- [ ] **Step 3: 實作** `render_doctor(...)` 純組裝 + `run()` 先跑前景 probe（`run_probe(true)` 風格但回 outcome 供印）再呼叫 render。main.rs 加 `Doctor` subcommand → `cli::doctor::run(&ctx)`。黃/灰附建議（如 offline→「Mnemos server 沒在跑，`cd ~/projects/mnemos && cargo run -p mnemos -- serve`」）。
- [ ] **Step 4: 跑確認通過** — Run: `cargo test && cargo build`. 手動 `cargo run -- doctor`. Expected: PASS、輸出合理。`./scripts/fmt.sh`。
- [ ] **Step 5: Commit**

```bash
git add src/cli/doctor.rs src/main.rs
git commit -m "feat(doctor): codeforge doctor — full brain health diagnostics + next-steps"
```

---

### Task 16: 收尾 — 全綠 gate + spec 對照 + docs

**Files:** 可能 Modify `README`/`doc/concepts.md`（新命令 + 燈說明）

- [ ] **Step 1:** Run: `cargo test && cargo clippy -- -D warnings && ./scripts/fmt.sh --check && cargo deny check licenses`. Expected: 全綠。
- [ ] **Step 2:** 對照 `doc/specs/codeforge-brain-indicators.md` 每節 → 有對應 task。CJK-safe 渲染確認（含 CJK village 名情境）。
- [ ] **Step 3:** README / concepts 補一段：兩顆燈語意 + `codeforge doctor` + probe 機制（per-machine 快取、零阻塞）。
- [ ] **Step 4:** 手動端對端：(a) 無 opt-in → 只 local 燈、無 probe spawn；(b) opt-in 無 server → central offline 灰、probe backoff；(c) 啟 mnemos server → central 轉綠。
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "docs: brain indicators user-facing notes + final green gate"
```

---

## Self-Review

**Spec coverage**：§2 雙軸→Task4；§2.1 狀態表→Task4 測試每格；§2.2 never 退場→Task4；§3 local→Task11；§4.1 NO_COLOR→Task12；§4.2-4.3 量測再印+降級→Task13；§5 doctor→Task15；§6 快取/位置/macOS age→Task1-2（CACHE_MAX_AGE 常數已置；macOS boot-age 退路 = 讀取時 age>CACHE_MAX_AGE 丟棄，**補進 Task2/Task4 讀取路徑**：`read_liveness` 後若 `now-last_probe_at>CACHE_MAX_AGE` 視為 None）；§7 probe 契約→Task6-8；§8 ship→Task9；§9 模組邊界→聚合在 statusline(Task14)、central 在 health.rs；§10 命名→全程；§11 常數→Task1。

**補洞**：§6 macOS CACHE_MAX_AGE 丟棄邏輯需在讀取端落實 —— 於 Task 8 `should_refresh` 與 Task 4 取用前，加 `fn fresh_enough(l:&LivenessCache, now)->bool { now-l.last_probe_at <= CACHE_MAX_AGE as i64 }`，過期則 central 視同 None（offline）+ 觸發刷新。實作 Task8 時一併加，測試補一格。

**Placeholder scan**：無 TBD；col 斷點為刻意「實作量測」（spec 已決策）。

**Type consistency**：`ProbeOutcome`/`LivenessCache`/`ShipCache`/`CentralLight`/`LocalLight`/`BrainHealth` 跨 task 一致；`write_atomic`/`read_json`/`central_light`/`maybe_spawn_probe`/`run_probe`/`record_ship_health` 簽名前後一致。
