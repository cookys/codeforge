//! 央腦（Mnemos）連線健康 — probe liveness + ship readiness。
//! statusline 熱路徑只讀此模組算好的 CentralHealth；快取 per-machine。
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
