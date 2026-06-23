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
