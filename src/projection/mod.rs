// Projection layer — Phase 3d stickiness / Phase 4 AGENTS.md sync will wire
// this in. Kept compiled to prevent bit-rot.
#![allow(dead_code)]

use anyhow::Result;
use crate::db;
use crate::memory::l1;

/// 投影 top-N L1 entries 到 .claude/memory/ 目錄
pub fn project_to_claude_memory(ctx: &db::Context, top_n: usize) -> Result<usize> {
    let store_dir = ctx.project_dir.join("store");
    let mut entries = l1::scan_all(&store_dir)?;

    // 按 strength 排序，取 top-N
    entries.sort_by(|a, b| b.frontmatter.strength.partial_cmp(&a.frontmatter.strength).unwrap_or(std::cmp::Ordering::Equal));
    entries.retain(|e| e.frontmatter.status == "active");
    entries.truncate(top_n);

    let memory_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("無法取得 home 目錄"))?
        .join(".claude")
        .join("projects")
        .join(project_id(ctx))
        .join("memory");

    std::fs::create_dir_all(&memory_dir)?;

    // 清除舊的投影（只刪 codeforge-* 前綴的檔案）
    if let Ok(dir) = std::fs::read_dir(&memory_dir) {
        for entry in dir.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("codeforge-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // 寫入新投影
    for (i, entry) in entries.iter().enumerate() {
        let filename = format!("codeforge-{:02}-{}.md", i + 1, l1::L1Entry::slugify(&entry.frontmatter.topic));
        let dest = memory_dir.join(filename);
        std::fs::write(&dest, &entry.body)?;
    }

    // 更新 AGENTS.md（加入 CodeForge 記憶摘要）
    update_agents_md(ctx, &entries)?;

    Ok(entries.len())
}

fn project_id(_ctx: &db::Context) -> String {
    // 使用專案目錄路徑的 hash 作為 ID（與 Claude Code 一致）
    let cwd = std::env::current_dir().unwrap_or_default();
    let path_str = cwd.to_string_lossy();
    // 簡單替換：/ -> - 並取最後段
    path_str.replace('/', "-").trim_start_matches('-').to_string()
}

fn update_agents_md(_ctx: &db::Context, top_entries: &[l1::L1Entry]) -> Result<()> {
    let agents_path = std::env::current_dir()
        .unwrap_or_default()
        .join("AGENTS.md");

    let section_start = "<!-- codeforge:memory:start -->";
    let section_end = "<!-- codeforge:memory:end -->";

    let existing = std::fs::read_to_string(&agents_path).unwrap_or_default();

    let summary = if top_entries.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = top_entries.iter()
            .take(10)
            .map(|e| format!("- **{}** (strength: {:.2})", e.frontmatter.topic, e.frontmatter.strength))
            .collect();
        format!(
            "{}\n## CodeForge Memory (top-{} active knowledge)\n\n{}\n\n{}",
            section_start,
            top_entries.len(),
            items.join("\n"),
            section_end
        )
    };

    let new_content = if existing.contains(section_start) {
        // 替換舊的 section
        let before = existing.split(section_start).next().unwrap_or("");
        let after = existing.split(section_end).last().unwrap_or("");
        format!("{}{}{}", before, summary, after)
    } else {
        format!("{}\n\n{}", existing.trim_end(), summary)
    };

    std::fs::write(&agents_path, new_content)?;
    Ok(())
}
