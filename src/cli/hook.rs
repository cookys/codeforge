//! `codeforge hook <name> [args…]` — run a hook script embedded in this binary.
//!
//! settings.json carries `codeforge hook emit-session session_start` instead of
//! `node <data dir>/codeforge/hooks/<version>/emit-session.js session_start`, so the
//! command is the same on every machine and every version (B35). The script is
//! materialized into a private per-version cache on first use and run with `node`,
//! inheriting stdin/stdout/stderr; the script's exit code is passed through. The
//! forwarder's own failures always exit 1, never 2: Claude Code treats a hook exit
//! of 2 as blocking, and that must only ever come from the script itself.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The global-safe hook scripts, embedded at build time.
pub(crate) const HOOK_SCRIPTS: &[(&str, &str)] = &[
    (
        "emit-session.js",
        include_str!("../../.claude/scripts/emit-session.js"),
    ),
    (
        "session-digest.js",
        include_str!("../../.claude/scripts/session-digest.js"),
    ),
];

/// Hook names as written in settings.json (`emit-session`, `session-digest`).
pub(crate) fn names() -> Vec<&'static str> {
    HOOK_SCRIPTS
        .iter()
        .map(|(file, _)| file.trim_end_matches(".js"))
        .collect()
}

/// Accepts `emit-session` or `emit-session.js`.
fn lookup(name: &str) -> Option<(&'static str, &'static str)> {
    let file = name.strip_suffix(".js").unwrap_or(name);
    HOOK_SCRIPTS
        .iter()
        .find(|(f, _)| f.trim_end_matches(".js") == file)
        .copied()
}

/// `$CODEFORGE_DATA_DIR`, else `<data_local_dir>/codeforge` — the same data dir
/// `db::Context` uses.
pub(crate) fn codeforge_data_dir() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CODEFORGE_DATA_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = dirs::data_local_dir() {
        return Ok(p.join("codeforge"));
    }
    let home = dirs::home_dir().context("找不到使用者 home 目錄且無 data_local_dir")?;
    Ok(home.join(".local").join("share").join("codeforge"))
}

/// Root of all script caches (`uninstall` removes it whole).
pub(crate) fn hooks_root() -> Result<PathBuf> {
    Ok(codeforge_data_dir()?.join("hooks"))
}

/// This version's script cache. Private: it never appears in settings.json.
pub(crate) fn cache_dir() -> Result<PathBuf> {
    Ok(hooks_root()?.join(env!("CARGO_PKG_VERSION")))
}

/// Write `content` to `base/file` unless it is already there byte for byte.
/// tmp + rename so a concurrent hook never reads half a file; if the rename fails
/// (Windows will not replace an existing file) and the destination now holds the
/// same bytes, another hook won the race and that is success.
pub(crate) fn materialize_in(base: &Path, file: &str, content: &str) -> Result<PathBuf> {
    let dest = base.join(file);
    if fs::read(&dest).is_ok_and(|b| b == content.as_bytes()) {
        return Ok(dest);
    }
    fs::create_dir_all(base).with_context(|| format!("建立 {} 失敗", base.display()))?;
    let tmp = base.join(format!("{file}.tmp.{}", std::process::id()));
    fs::write(&tmp, content).with_context(|| format!("寫入 {} 失敗", tmp.display()))?;
    if let Err(e) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_file(&tmp);
        if fs::read(&dest).is_ok_and(|b| b == content.as_bytes()) {
            return Ok(dest);
        }
        return Err(e).with_context(|| format!("寫入 {} 失敗", dest.display()));
    }
    Ok(dest)
}

/// Materialize every embedded script into this version's cache (install warms it
/// so an unwritable data dir fails at install time, not inside a hook).
pub(crate) fn materialize_all() -> Result<PathBuf> {
    let base = cache_dir()?;
    for (file, content) in HOOK_SCRIPTS {
        materialize_in(&base, file, content)?;
    }
    Ok(base)
}

/// Run `node <script> args…` with the given stdin (production: inherit) and
/// inherited stdout/stderr. Returns the script's exit code; a signal-terminated
/// child maps to 1.
pub(crate) fn exec_script(script: &Path, args: &[String], stdin: Stdio) -> Result<i32> {
    let status = match Command::new("node")
        .arg(script)
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!("找不到 node"),
        Err(e) => return Err(e).context("啟動 node 失敗"),
    };
    Ok(status.code().unwrap_or(1))
}

/// `codeforge hook <name> [args…]`. `Ok(code)` is the process exit code: the
/// script's own, or 1 for an unknown name. `Err` is a forwarder failure; the
/// caller prints it and exits 1.
pub fn run(name: &str, args: &[String]) -> Result<i32> {
    let Some((file, content)) = lookup(name) else {
        eprintln!(
            "codeforge hook：未知的 hook「{name}」（可用：{}）",
            names().join(", ")
        );
        return Ok(1);
    };
    let script = materialize_in(&cache_dir()?, file, content)
        .with_context(|| format!("codeforge hook {name}：無法落地腳本"))?;
    exec_script(&script, args, Stdio::inherit())
        .with_context(|| format!("codeforge hook {name} 未執行"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_available() -> bool {
        Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    #[test]
    fn names_and_lookup() {
        assert_eq!(names(), vec!["emit-session", "session-digest"]);
        assert_eq!(lookup("emit-session").unwrap().0, "emit-session.js");
        assert_eq!(lookup("session-digest.js").unwrap().0, "session-digest.js");
        assert!(lookup("check-dev-flow").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn unknown_name_exits_one_without_touching_disk() {
        assert_eq!(run("no-such-hook", &[]).unwrap(), 1);
    }

    #[test]
    fn materialize_writes_once_and_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("hooks").join("9.9.9");
        let p = materialize_in(&base, "x.js", "console.log(1)\n").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "console.log(1)\n");
        let mtime = fs::metadata(&p).unwrap().modified().unwrap();
        let p2 = materialize_in(&base, "x.js", "console.log(1)\n").unwrap();
        assert_eq!(p, p2);
        assert_eq!(fs::metadata(&p2).unwrap().modified().unwrap(), mtime);
        // changed content is rewritten
        materialize_in(&base, "x.js", "console.log(2)\n").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "console.log(2)\n");
        // no tmp left behind
        let names: Vec<_> = fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["x.js".to_string()]);
    }

    #[test]
    fn materialize_fails_when_base_unwritable() {
        let tmp = tempfile::TempDir::new().unwrap();
        let blocker = tmp.path().join("file");
        fs::write(&blocker, "").unwrap();
        // base is a path *under a regular file* → create_dir_all fails
        assert!(materialize_in(&blocker.join("sub"), "x.js", "1").is_err());
    }

    #[test]
    fn exec_script_forwards_stdin_argv_and_exit_code() {
        if !node_available() {
            eprintln!("skip: node not available");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let out = tmp.path().join("out.json");
        let script = tmp.path().join("echo.js");
        fs::write(
            &script,
            format!(
                "const fs=require('fs');\
                 const input=fs.readFileSync(0,'utf8');\
                 fs.writeFileSync({out:?},JSON.stringify({{input,argv:process.argv.slice(2)}}));\
                 process.exit(Number(process.argv[2]));",
                out = out.to_string_lossy()
            ),
        )
        .unwrap();
        let stdin_file = tmp.path().join("in.json");
        let payload = "{\"session_id\":\"s1\",\"cwd\":\"/x y\"}\n";
        fs::write(&stdin_file, payload).unwrap();

        for code in [0, 2, 7] {
            let stdin = Stdio::from(fs::File::open(&stdin_file).unwrap());
            let args = vec![code.to_string(), "two words".to_string()];
            assert_eq!(exec_script(&script, &args, stdin).unwrap(), code);
            let got: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
            assert_eq!(got["input"], payload);
            assert_eq!(
                got["argv"],
                serde_json::json!([code.to_string(), "two words"])
            );
        }
    }
}
