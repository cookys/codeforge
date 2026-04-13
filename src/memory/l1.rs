use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// L1 知識條目類型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum L1Type {
    Concept,
    Connection,
    Qa,
}

impl L1Type {
    pub fn dir_name(&self) -> &str {
        match self {
            L1Type::Concept => "concepts",
            L1Type::Connection => "connections",
            L1Type::Qa => "qa",
        }
    }
}

/// L1 Markdown 檔案的 frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Frontmatter {
    #[serde(rename = "type")]
    pub kind: L1Type,
    pub topic: String,
    pub created: String,
    pub updated: String,
    pub sources: Vec<String>,       // L0 signal ids
    pub links: Vec<String>,         // wikilinks [[topic]]（至少 2 個）
    pub refs: usize,                // 被引用次數
    pub last_ref: Option<String>,   // 最後被引用時間
    pub strength: f32,              // ACT-R activation（0.0–1.0）
    pub status: String,             // active | superseded | archived
}

impl Default for L1Frontmatter {
    fn default() -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
        Self {
            kind: L1Type::Concept,
            topic: String::new(),
            created: now.clone(),
            updated: now,
            sources: vec![],
            links: vec![],
            refs: 0,
            last_ref: None,
            strength: 1.0,
            status: "active".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct L1Entry {
    pub frontmatter: L1Frontmatter,
    pub title: String,
    pub body: String,
    pub file_path: PathBuf,
}

impl L1Entry {
    /// 從 Markdown 檔案讀取
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content, path.to_path_buf())
    }

    fn parse(content: &str, path: PathBuf) -> Result<Self> {
        // 解析 YAML frontmatter（---\n...\n---）
        let (frontmatter, body) = if content.starts_with("---\n") {
            let end = content[4..].find("\n---\n").map(|i| i + 4);
            if let Some(end) = end {
                let yaml = &content[4..end];
                let body = &content[end + 5..];
                let fm: L1Frontmatter = serde_yaml::from_str(yaml)
                    .with_context(|| format!("frontmatter 解析失敗：{}", path.display()))?;
                (fm, body.to_string())
            } else {
                (L1Frontmatter::default(), content.to_string())
            }
        } else {
            (L1Frontmatter::default(), content.to_string())
        };

        // 從 body 第一行 # 標題提取 title
        let title = body.lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l[2..].trim().to_string())
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });

        Ok(Self { frontmatter, title, body, file_path: path })
    }

    /// 寫入 Markdown 檔案
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(&self.frontmatter)?;
        let content = format!("---\n{}---\n\n{}", yaml, self.body);
        std::fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// 產生檔案路徑（store/{type}/{slug}.md）
    pub fn path_for(store_dir: &Path, kind: &L1Type, slug: &str) -> PathBuf {
        store_dir.join(kind.dir_name()).join(format!("{}.md", slug))
    }

    /// 從 topic 生成 slug（去除特殊字元）
    pub fn slugify(topic: &str) -> String {
        topic.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// 掃描 store/ 目錄，回傳所有 L1 entries
pub fn scan_all(store_dir: &Path) -> Result<Vec<L1Entry>> {
    let mut entries = Vec::new();
    for subdir in &["concepts", "connections", "qa"] {
        let dir = store_dir.join(subdir);
        if !dir.exists() { continue; }
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                match L1Entry::from_file(&path) {
                    Ok(e) => entries.push(e),
                    Err(err) => eprintln!("警告：解析 {} 失敗：{}", path.display(), err),
                }
            }
        }
    }
    Ok(entries)
}
