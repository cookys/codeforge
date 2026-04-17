//! `codeforge daemon` — start/stop/status for the tick-loop process.
//!
//! `start` runs foreground. Use systemd (P6) to background-manage it.
//! `stop` sends SIGTERM to the pid recorded in daemon.pid.
//! `status` inspects pidfile + last_tick_at to give a health summary.

use crate::daemon::{self, lifecycle};
use crate::db::Context;
use anyhow::{Context as _, Result};
use std::time::Duration;

pub fn run(ctx: &Context, subcmd: DaemonCmd) -> Result<()> {
    match subcmd {
        DaemonCmd::Start => start(ctx),
        DaemonCmd::Stop => stop(ctx),
        DaemonCmd::Status => status(ctx),
        DaemonCmd::Install => install(),
    }
}

pub enum DaemonCmd {
    Start,
    Stop,
    Status,
    Install,
}

fn start(ctx: &Context) -> Result<()> {
    let pidfile = lifecycle::pidfile_path(ctx);
    lifecycle::acquire_pidfile(&pidfile)?;

    // Ensure DB open + migrations applied
    let mut conn = ctx.open_db()?;
    crate::db::migrations::run(&conn)?;

    eprintln!(
        "codeforge daemon starting (pid={}, tick={}s, db={})",
        std::process::id(),
        daemon::TICK_INTERVAL_SECS,
        ctx.db_path.display()
    );

    // Drive the async runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("建立 tokio runtime 失敗")?;

    let result = rt.block_on(async {
        let shutdown = lifecycle::install_signal_handlers();
        daemon::run_tick_loop(&mut conn, Duration::from_secs(daemon::TICK_INTERVAL_SECS), shutdown)
            .await
    });

    lifecycle::release_pidfile(&pidfile);

    if let Err(e) = &result {
        eprintln!("codeforge daemon exited with error: {e:#}");
    } else {
        eprintln!("codeforge daemon stopped cleanly");
    }
    result
}

fn stop(ctx: &Context) -> Result<()> {
    let pidfile = lifecycle::pidfile_path(ctx);
    if !pidfile.exists() {
        return Err(anyhow::anyhow!(
            "daemon 未執行（pidfile 不存在：{}）",
            pidfile.display()
        ));
    }
    let pid = lifecycle::read_pidfile(&pidfile)?;
    if !lifecycle::pid_alive(pid) {
        // Stale pidfile — just clean up
        lifecycle::release_pidfile(&pidfile);
        return Err(anyhow::anyhow!(
            "pidfile 指向 pid {} 但該 process 已不存在。清理並視為已停止。",
            pid
        ));
    }

    lifecycle::send_sigterm(pid, Duration::from_secs(10))?;
    lifecycle::release_pidfile(&pidfile);
    eprintln!("codeforge daemon stopped (pid={pid})");
    Ok(())
}

fn status(ctx: &Context) -> Result<()> {
    let pidfile = lifecycle::pidfile_path(ctx);

    let (running, pid) = if pidfile.exists() {
        match lifecycle::read_pidfile(&pidfile) {
            Ok(p) => (lifecycle::pid_alive(p), Some(p)),
            Err(_) => (false, None),
        }
    } else {
        (false, None)
    };

    println!("codeforge daemon status");
    println!("  running:   {}", if running { "yes" } else { "no" });
    if let Some(p) = pid {
        println!("  pid:       {p} {}", if running { "" } else { "(stale)" });
    }
    println!("  pidfile:   {}", pidfile.display());
    println!("  db:        {}", ctx.db_path.display());

    // Query DB for last tick info (works even when daemon is down)
    if ctx.db_path.exists() {
        match ctx.open_db() {
            Ok(conn) => {
                let row: rusqlite::Result<(i64, i64)> = conn.query_row(
                    "SELECT tick_at, tick_count FROM last_tick_at WHERE id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                );
                match row {
                    Ok((tick_at, count)) => {
                        let ago = current_unix_secs().saturating_sub(tick_at as u64);
                        println!("  last tick: {tick_at} ({ago}s ago)");
                        println!("  ticks:     {count}");
                    }
                    Err(_) => println!("  last tick: (never)"),
                }
                let unseen: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM event_inbox WHERE seen_at IS NULL",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                println!("  pending events (unseen): {unseen}");
            }
            Err(_) => println!("  db: (unavailable)"),
        }
    } else {
        println!("  db: (not yet initialized)");
    }

    Ok(())
}

fn current_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write the systemd user unit file to ~/.config/systemd/user/,
/// run daemon-reload, and print activation instructions.
fn install() -> Result<()> {
    let exe = std::env::current_exe().context("無法取得當前執行檔路徑")?;
    let unit_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("無法定位 config dir（$XDG_CONFIG_HOME 或 ~/.config）"))?
        .join("systemd")
        .join("user");
    let unit_path = unit_dir.join("codeforge-daemon.service");

    std::fs::create_dir_all(&unit_dir)
        .with_context(|| format!("建立 {} 失敗", unit_dir.display()))?;

    let content = render_systemd_unit(&exe.display().to_string());
    std::fs::write(&unit_path, &content)
        .with_context(|| format!("寫入 {} 失敗", unit_path.display()))?;

    eprintln!("✓ 寫入 systemd user unit：{}", unit_path.display());

    // Reload systemd daemon
    match std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
    {
        Ok(st) if st.success() => eprintln!("✓ systemctl --user daemon-reload"),
        Ok(st) => eprintln!("⚠ systemctl daemon-reload 非零退出（{}）", st),
        Err(e) => eprintln!("⚠ 無法執行 systemctl（非 systemd 系統？）：{e}"),
    }

    eprintln!();
    eprintln!("下一步：");
    eprintln!("  systemctl --user enable --now codeforge-daemon");
    eprintln!("  systemctl --user status codeforge-daemon");
    eprintln!("  journalctl --user -u codeforge-daemon -f    # 看 log");
    Ok(())
}

pub fn render_systemd_unit(exe_path: &str) -> String {
    format!(
        "\
[Unit]
Description=CodeForge daemon — tick loop + game state
Documentation=https://github.com/cookys/codeforge
After=default.target

[Service]
Type=simple
ExecStart={exe_path} daemon start
Restart=on-failure
RestartSec=5s
# Graceful shutdown: SIGTERM → flush current tick → exit
KillSignal=SIGTERM
TimeoutStopSec=15s

[Install]
WantedBy=default.target
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_unit_has_required_sections() {
        let unit = render_systemd_unit("/usr/local/bin/codeforge");
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("ExecStart=/usr/local/bin/codeforge daemon start"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn systemd_unit_includes_sigterm_handling() {
        // Spec contract: SIGTERM → flush → exit; timeout to protect against hang
        let unit = render_systemd_unit("/bin/x");
        assert!(unit.contains("KillSignal=SIGTERM"));
        assert!(unit.contains("TimeoutStopSec"));
    }
}
