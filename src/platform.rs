//! Every OS difference lives here. Callers use these functions and stay free of `cfg`.
//!
//! - process liveness / termination / identity → `sysinfo` (one API for Linux, macOS, Windows;
//!   replaces shelling out to `kill`, reading `/proc`, or `tasklist`)
//! - detached child spawn → the only place that needs raw `std::os::{unix,windows}` extensions
//! - stop signals → SIGTERM on Unix, Ctrl-Break on Windows (Ctrl-C is handled by the caller)
//! - shell command strings → paths that survive a POSIX shell, which is what Claude Code runs
//!   hooks through on every OS (Git Bash on Windows)

use std::path::Path;
use std::process::Command;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

/// Refresh just `pid` (with its command line) and hand the snapshot to `f`.
fn with_process<T>(pid: u32, f: impl FnOnce(Option<&sysinfo::Process>) -> T) -> T {
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::everything(),
    );
    f(sys.process(pid))
}

/// Is a process with this pid currently running?
pub fn pid_alive(pid: u32) -> bool {
    with_process(pid, |p| p.is_some())
}

/// True when the process's command line contains every token in `needles`.
/// Used to tell a real daemon from a recycled pid.
pub fn cmdline_contains(pid: u32, needles: &[&str]) -> bool {
    with_process(pid, |p| {
        p.is_some_and(|p| {
            let cmd = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            needles.iter().all(|n| cmd.contains(n))
        })
    })
}

/// Ask the process to stop: SIGTERM where signals exist, otherwise (Windows) terminate it.
/// Returns false if the process is gone or the request was refused.
pub fn request_terminate(pid: u32) -> bool {
    with_process(pid, |p| {
        p.is_some_and(|p| p.kill_with(Signal::Term).unwrap_or_else(|| p.kill()))
    })
}

/// Resolves when the OS asks this process to stop other than via Ctrl-C:
/// SIGTERM on Unix, Ctrl-Break on Windows. Never resolves if the handler can't be installed.
pub async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
            return;
        }
    }
    #[cfg(windows)]
    {
        if let Ok(mut s) = tokio::signal::windows::ctrl_break() {
            s.recv().await;
            return;
        }
    }
    std::future::pending::<()>().await
}

/// Start `cmd` fully detached: own process group / no inherited console, so a signal sent to
/// the parent's group (Claude Code's statusline teardown, Ctrl-C) never reaches the child.
pub fn spawn_detached(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    cmd.spawn()
}

/// Run a bash script and wait for it. Unix executes it directly (shebang); Windows cannot, so it
/// goes through Git for Windows' `bash` (located next to `git`, since a bare `bash` on PATH may
/// be WSL's, which cannot resolve `C:\` paths).
pub fn run_shell_script(script: &Path, args: &[&str]) -> std::io::Result<std::process::ExitStatus> {
    #[cfg(windows)]
    {
        Command::new(git_bash()).arg(script).args(args).status()
    }
    #[cfg(not(windows))]
    {
        Command::new(script).args(args).status()
    }
}

/// `<git root>/bin/bash.exe`, derived from `git --exec-path` (`<root>/mingw64/libexec/git-core`).
#[cfg(windows)]
fn git_bash() -> std::path::PathBuf {
    Command::new("git")
        .arg("--exec-path")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            Path::new(s.trim())
                .ancestors()
                .nth(3)
                .map(|root| root.join("bin").join("bash.exe"))
        })
        .filter(|p| p.exists())
        .unwrap_or_else(|| "bash".into())
}

/// Build a command that runs `argv` at the lowest CPU priority, bounded by `timeout_secs`.
/// Unix wraps coreutils (`nice -n 19 timeout N argv…`), which also enforces the timeout; Windows
/// has neither, so it spawns `argv` directly at BELOW_NORMAL priority and relies on
/// [`guard_timeout`] for the bound.
pub fn low_priority_command(argv: &[&str], timeout_secs: u64) -> Command {
    #[cfg(unix)]
    {
        let mut cmd = Command::new("nice");
        cmd.args(["-n", "19", "timeout", &timeout_secs.to_string()])
            .args(argv);
        cmd
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = timeout_secs;
        let mut cmd = Command::new(argv[0]);
        cmd.args(&argv[1..])
            .creation_flags(BELOW_NORMAL_PRIORITY_CLASS | CREATE_NO_WINDOW);
        cmd
    }
}

/// Kills `pid` after `secs` unless dropped first. No-op on Unix, where `timeout` (see
/// [`low_priority_command`]) already bounds the child. Drop it once the child has been reaped so a
/// recycled pid is never signalled.
pub struct TimeoutGuard(#[allow(dead_code)] Option<std::sync::mpsc::Sender<()>>);

pub fn guard_timeout(pid: u32, secs: u64) -> TimeoutGuard {
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                rx.recv_timeout(std::time::Duration::from_secs(secs))
            {
                request_terminate(pid);
            }
        });
        TimeoutGuard(Some(tx))
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, secs);
        TimeoutGuard(None)
    }
}

/// Render a path for a hook / statusLine `command` string. Claude Code runs these through a POSIX
/// shell where `\` is an escape character, so Windows paths use `/` (accepted by bash, cmd and
/// node). Paths containing whitespace are double-quoted.
pub fn shell_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    let s = if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    };
    if s.contains(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_is_alive_and_nonexistent_is_not() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(4_000_000_000));
    }

    #[test]
    fn cmdline_contains_matches_own_exe_name() {
        assert!(cmdline_contains(std::process::id(), &["codeforge"]));
        assert!(!cmdline_contains(
            std::process::id(),
            &["definitely-not-in-argv-9f3a"]
        ));
    }

    #[test]
    fn shell_path_quotes_whitespace_only() {
        assert_eq!(shell_path(Path::new("/a/b.js")), "/a/b.js");
        assert_eq!(shell_path(Path::new("/a b/c.js")), "\"/a b/c.js\"");
    }

    #[cfg(windows)]
    #[test]
    fn shell_path_uses_forward_slashes_on_windows() {
        assert_eq!(shell_path(Path::new(r"C:\Users\x\a.js")), "C:/Users/x/a.js");
    }

    #[test]
    fn terminate_kills_a_child() {
        #[cfg(windows)]
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .spawn()
            .unwrap();
        #[cfg(unix)]
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        assert!(request_terminate(child.id()));
        assert!(child.wait().is_ok());
    }
}
