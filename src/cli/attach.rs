//! `codeforge attach` — spawn a companion tmux pane running `codeforge tui`.
//!
//! When run from inside a tmux session (detected via `$TMUX`), splits a
//! new pane horizontally to the right (so the existing window becomes
//! the "main" pane) and launches the TUI in it. When not inside tmux,
//! prints a one-line hint telling the user to start tmux first and
//! exits non-zero.
//!
//! The design assumes `codeforge` is on `$PATH` — which it must be,
//! since the user just invoked `codeforge attach`. We don't resolve the
//! current binary's absolute path because shell-escaping that cleanly
//! across spaces / quotes is more failure-prone than simply trusting
//! PATH resolution inside the spawned pane.

use anyhow::{anyhow, bail, Result};
use std::process::Command;

/// Default split ratio (percent of the parent pane given to the new
/// companion pane). 30 → ~54 cols on a 1080p / 180-col terminal, right
/// on the Narrow/Standard boundary. Users can override with `--size`.
pub const DEFAULT_SIZE: u16 = 30;
/// Minimum split ratio; smaller than this the companion pane is too
/// narrow for the TUI to render anything useful (Compact breakpoint).
pub const MIN_SIZE: u16 = 20;
/// Maximum split ratio; larger than this the main pane (where Claude
/// Code is running) shrinks too much.
pub const MAX_SIZE: u16 = 70;

/// Entry point wired into `src/cli/mod.rs`.
pub fn run(size: Option<u16>) -> Result<()> {
    run_with(size, &RealEnv, &RealSpawner)
}

/// Dependency-injected core for unit testing. Production code never
/// calls this directly — use [`run`].
fn run_with(size: Option<u16>, env: &dyn Env, spawner: &dyn Spawner) -> Result<()> {
    if !detect_tmux(env) {
        bail!(
            "codeforge attach 需要在 tmux session 裡執行。\n\
             請先啟動 tmux：\n\
             \n  tmux new -s codeforge\n\
             \n然後在新 session 裡跑：\n\
             \n  codeforge attach"
        );
    }
    let size = clamp_size(size);
    let args = build_tmux_args(size);
    spawner.spawn(&args)
}

/// True iff `$TMUX` is set to a non-empty value. tmux sets this to the
/// socket path + session id the moment it spawns a shell inside a pane,
/// so its presence is the canonical "am I inside tmux?" signal.
fn detect_tmux(env: &dyn Env) -> bool {
    env.var("TMUX").is_some_and(|v| !v.is_empty())
}

/// Clamp the user-supplied ratio to `[MIN_SIZE, MAX_SIZE]`. `None` uses
/// [`DEFAULT_SIZE`].
fn clamp_size(size: Option<u16>) -> u16 {
    size.unwrap_or(DEFAULT_SIZE).clamp(MIN_SIZE, MAX_SIZE)
}

/// Build the argv for `tmux split-window`. Exported as a pure function
/// so tests can assert the exact argv without invoking tmux.
fn build_tmux_args(size: u16) -> Vec<String> {
    vec![
        "split-window".to_string(),
        "-h".to_string(),
        "-p".to_string(),
        size.to_string(),
        "codeforge tui".to_string(),
    ]
}

// ─── Test seams ─────────────────────────────────────────────────────

trait Env {
    fn var(&self, key: &str) -> Option<String>;
}

struct RealEnv;
impl Env for RealEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

trait Spawner {
    fn spawn(&self, args: &[String]) -> Result<()>;
}

struct RealSpawner;
impl Spawner for RealSpawner {
    fn spawn(&self, args: &[String]) -> Result<()> {
        let status = Command::new("tmux").args(args).status().map_err(|e| {
            anyhow!(
                "failed to invoke tmux: {e}. Is tmux installed and on $PATH?"
            )
        })?;
        if !status.success() {
            bail!("tmux split-window exited with status {status}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    // ---- stub env ----

    struct StubEnv(HashMap<String, String>);
    impl Env for StubEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }
    fn env_with_tmux() -> StubEnv {
        let mut m = HashMap::new();
        m.insert("TMUX".to_string(), "/tmp/tmux-1000/default,42,0".to_string());
        StubEnv(m)
    }
    fn env_empty() -> StubEnv {
        StubEnv(HashMap::new())
    }
    fn env_empty_tmux_var() -> StubEnv {
        let mut m = HashMap::new();
        m.insert("TMUX".to_string(), "".to_string());
        StubEnv(m)
    }

    // ---- capturing spawner ----

    struct CapturingSpawner {
        calls: RefCell<Vec<Vec<String>>>,
    }
    impl CapturingSpawner {
        fn new() -> Self {
            CapturingSpawner {
                calls: RefCell::new(Vec::new()),
            }
        }
    }
    impl Spawner for CapturingSpawner {
        fn spawn(&self, args: &[String]) -> Result<()> {
            self.calls.borrow_mut().push(args.to_vec());
            Ok(())
        }
    }

    // ---- tests ----

    #[test]
    fn not_in_tmux_returns_error() {
        let env = env_empty();
        let spawner = CapturingSpawner::new();
        let err = run_with(None, &env, &spawner).unwrap_err();
        assert!(
            err.to_string().contains("tmux session"),
            "error must mention tmux: {err}"
        );
        assert!(
            spawner.calls.borrow().is_empty(),
            "must not spawn tmux when not inside a session"
        );
    }

    #[test]
    fn empty_tmux_var_also_fails() {
        // `$TMUX=` (set but empty) shouldn't count as "inside tmux".
        let env = env_empty_tmux_var();
        let spawner = CapturingSpawner::new();
        assert!(run_with(None, &env, &spawner).is_err());
    }

    #[test]
    fn default_size_used_when_none() {
        let env = env_with_tmux();
        let spawner = CapturingSpawner::new();
        run_with(None, &env, &spawner).unwrap();
        let calls = spawner.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].contains(&DEFAULT_SIZE.to_string()),
            "default size missing from argv: {:?}",
            calls[0]
        );
    }

    #[test]
    fn explicit_size_passes_through_when_in_range() {
        let env = env_with_tmux();
        let spawner = CapturingSpawner::new();
        run_with(Some(40), &env, &spawner).unwrap();
        assert!(spawner.calls.borrow()[0].contains(&"40".to_string()));
    }

    #[test]
    fn size_below_min_clamps_up() {
        assert_eq!(clamp_size(Some(5)), MIN_SIZE);
        assert_eq!(clamp_size(Some(0)), MIN_SIZE);
    }

    #[test]
    fn size_above_max_clamps_down() {
        assert_eq!(clamp_size(Some(90)), MAX_SIZE);
        assert_eq!(clamp_size(Some(100)), MAX_SIZE);
    }

    #[test]
    fn size_none_uses_default() {
        assert_eq!(clamp_size(None), DEFAULT_SIZE);
    }

    #[test]
    fn size_in_range_returned_verbatim() {
        assert_eq!(clamp_size(Some(45)), 45);
        assert_eq!(clamp_size(Some(MIN_SIZE)), MIN_SIZE);
        assert_eq!(clamp_size(Some(MAX_SIZE)), MAX_SIZE);
    }

    #[test]
    fn build_tmux_args_has_split_horizontal() {
        let args = build_tmux_args(30);
        // Full argv shape: ["split-window", "-h", "-p", "30", "codeforge tui"].
        // Pin every index so a regression that inserts / drops an arg
        // surfaces here rather than at tmux runtime.
        assert_eq!(args.len(), 5, "argv must be exactly 5 elements: {args:?}");
        assert_eq!(args[0], "split-window");
        assert_eq!(args[1], "-h");
        assert_eq!(args[2], "-p");
        assert_eq!(args[3], "30");
        assert_eq!(args[4], "codeforge tui");
    }

    #[test]
    fn build_tmux_args_command_lands_at_final_position() {
        // The tmux man page requires the shell-command to be the final
        // positional argument after all flags. Verify it at index len-1.
        let args = build_tmux_args(45);
        assert_eq!(args.last().map(String::as_str), Some("codeforge tui"));
    }

    #[test]
    fn in_tmux_triggers_exactly_one_spawn() {
        let env = env_with_tmux();
        let spawner = CapturingSpawner::new();
        run_with(Some(35), &env, &spawner).unwrap();
        assert_eq!(spawner.calls.borrow().len(), 1);
    }
}
