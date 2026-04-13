use anyhow::Result;
use crate::db;
use crate::memory::fts::FtsIndex;

pub fn run(ctx: &db::Context, query: &str, limit: usize) -> Result<()> {
    ctx.ensure_initialized()?;

    let conn = ctx.open_db()?;
    let index = FtsIndex::new(&conn);
    let results = index.search(query, limit)?;

    if results.is_empty() {
        println!("找不到符合 \"{}\" 的知識條目", query);
        println!("提示：先執行 `codeforge learn` 新增內容，再執行 `codeforge dream` 編譯");
        return Ok(());
    }

    println!("搜尋 \"{}\" — {} 筆結果\n", query, results.len());
    for (i, r) in results.iter().enumerate() {
        println!("  {}. {}", i + 1, r.title);
        println!("     {}", r.file_path);
        if let Some(snippet) = &r.snippet {
            let s = snippet.replace('\n', " ");
            let s = if s.len() > 120 { format!("{}…", &s[..120]) } else { s };
            println!("     {}", s);
        }
        println!();
    }

    // 搜尋行為本身也是記憶活動，加微量 XP
    crate::pet::xp::award(ctx, "search", 1, Some(query))?;

    Ok(())
}

pub fn status(ctx: &db::Context) -> Result<()> {
    ctx.ensure_initialized()?;

    let l1_dir = ctx.project_dir.join("store");
    let count_md = |subdir: &str| -> usize {
        let path = l1_dir.join(subdir);
        std::fs::read_dir(path)
            .map(|d| d.filter_map(|e| e.ok()).filter(|e| {
                e.path().extension().map(|x| x == "md").unwrap_or(false)
            }).count())
            .unwrap_or(0)
    };

    let concepts = count_md("concepts");
    let connections = count_md("connections");
    let qa = count_md("qa");

    let signal_dir = ctx.project_dir.join("signals");
    let signal_count = std::fs::read_dir(&signal_dir)
        .map(|d| d.filter_map(|e| e.ok()).count())
        .unwrap_or(0);

    println!("CodeForge 記憶系統狀態");
    println!("━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  L0 signals：{} 個日誌檔", signal_count);
    println!("  L1 概念：{} 條", concepts);
    println!("  L1 連結：{} 條", connections);
    println!("  L1 問答：{} 條", qa);
    println!("  總計：{} 條 L1 知識", concepts + connections + qa);
    println!();
    println!("  專案：{}", ctx.project_dir.display());
    println!("  Brain：{}", ctx.brain_dir.display());

    Ok(())
}
