use crate::db;
use anyhow::{Context, Result};

pub fn run(ctx: &db::Context) -> Result<()> {
    ensure_gitignore(&ctx.project_dir)?;
    if ctx.is_initialized() {
        println!("✓ .codeforge/ 已存在於 {}", ctx.project_dir.display());
        return Ok(());
    }

    // 建立目錄結構
    let dirs = [
        ctx.project_dir.join("store").join("concepts"),
        ctx.project_dir.join("store").join("connections"),
        ctx.project_dir.join("store").join("qa"),
        ctx.project_dir.join("signals"),
        ctx.project_dir.join("projections"),
        ctx.brain_dir.join("episodes"),
        ctx.brain_dir.join("skills"),
        ctx.brain_dir.join("identity"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir).with_context(|| format!("建立目錄失敗：{}", dir.display()))?;
    }

    // 建立 log.md（人類可讀 signal 日誌）
    let log_path = ctx.project_dir.join("log.md");
    if !log_path.exists() {
        std::fs::write(&log_path, "# CodeForge Signal Log\n\n")?;
    }

    // 初始化 DB 並跑 schema
    let conn = ctx.open_db()?;
    db::migrations::run(&conn)?;

    println!("✓ CodeForge 初始化完成");
    println!("  專案記憶：{}", ctx.project_dir.display());
    println!("  個人 brain：{}", ctx.brain_dir.display());
    println!("  狀態 DB：{}", ctx.db_path.display());
    println!();
    println!("下一步：`codeforge learn \"你學到的東西\"` 開始記憶");

    Ok(())
}

/// In a git repo, keep `.codeforge/` (private L0/L1 memory) out of commits: append the ignore
/// line once. Idempotent; no-op outside git repos or when the store is not a `<repo>/.codeforge`.
fn ensure_gitignore(project_dir: &std::path::Path) -> Result<()> {
    if project_dir
        .file_name()
        .map(|n| n != ".codeforge")
        .unwrap_or(true)
    {
        return Ok(());
    }
    let Some(root) = project_dir.parent() else {
        return Ok(());
    };
    if !root.join(".git").exists() {
        return Ok(());
    }
    let gi = root.join(".gitignore");
    let existing = std::fs::read_to_string(&gi).unwrap_or_default();
    let covered = existing.lines().map(str::trim).any(|l| {
        matches!(
            l,
            ".codeforge" | ".codeforge/" | "/.codeforge" | "/.codeforge/"
        )
    });
    if covered {
        return Ok(());
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(".codeforge/\n");
    std::fs::write(&gi, out).with_context(|| format!("寫入失敗：{}", gi.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn gitignore_added_once_in_git_repo() {
        let t = TempDir::new().unwrap();
        std::fs::create_dir(t.path().join(".git")).unwrap();
        std::fs::write(t.path().join(".gitignore"), "target").unwrap();
        let pd = t.path().join(".codeforge");
        ensure_gitignore(&pd).unwrap();
        ensure_gitignore(&pd).unwrap();
        let gi = std::fs::read_to_string(t.path().join(".gitignore")).unwrap();
        assert_eq!(gi, "target\n.codeforge/\n");
    }

    #[test]
    fn gitignore_untouched_outside_git_or_when_already_ignored() {
        let t = TempDir::new().unwrap();
        ensure_gitignore(&t.path().join(".codeforge")).unwrap();
        assert!(!t.path().join(".gitignore").exists());
        std::fs::create_dir(t.path().join(".git")).unwrap();
        std::fs::write(t.path().join(".gitignore"), "/.codeforge\n").unwrap();
        ensure_gitignore(&t.path().join(".codeforge")).unwrap();
        assert_eq!(
            std::fs::read_to_string(t.path().join(".gitignore")).unwrap(),
            "/.codeforge\n"
        );
    }
}
