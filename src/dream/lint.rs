use crate::db;
use crate::memory::l1;
/// Dream Lint：偵測矛盾、孤兒、缺口（rule-based，零 LLM 成本）
use anyhow::Result;

pub struct LintResult {
    pub issues: usize,
}

pub fn run(ctx: &db::Context) -> Result<LintResult> {
    let store_dir = ctx.project_dir.join("store");
    let entries = l1::scan_all(&store_dir)?;
    let mut issues = 0;

    for entry in &entries {
        // 孤兒偵測：strength < 0.1（衰減過多，無人引用）
        if entry.frontmatter.strength < 0.1 && entry.frontmatter.status == "active" {
            issues += 1;
        }

        // 斷鏈 wikilinks 偵測
        for link in &entry.frontmatter.links {
            let slug = l1::L1Entry::slugify(link);
            let exists = ["concepts", "connections", "qa"]
                .iter()
                .any(|dir| store_dir.join(dir).join(format!("{}.md", slug)).exists());
            if !exists {
                issues += 1;
            }
        }
    }

    Ok(LintResult { issues })
}
