//! 候選擷取（plan §10.2 T1）：`lessons.toml` 設定＋唯讀 L0 掃描＋分類。
//! T2 機器閘：每筆候選產生後、去重前跑 `gate::check`（source＝該筆 signal
//! 的 `content`）；命中就丟棄、累計規則 id 次數（見 [`super::gate`]）。
//!
//! 只讀 `<project_dir>/signals/*.jsonl`：不推進 cursor、不 mark_compiled、
//! 不寫任何檔。輸出只有受限詞彙欄位與證據指標，絕不含 content、路徑、
//! seat 原文、model 原文或任何 signal 文字。

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db;
use crate::lessons::gate::{self, GateCounts};
use crate::lessons::vocab::{PitfallType, SolutionType, Tool, Topic};
use crate::memory::l0::{Signal, SignalSource};

/// `lessons.toml`——本機設定。位置：`$CODEFORGE_DATA_DIR/lessons.toml`，
/// 未設環境變數時 `~/.codeforge/lessons.toml`。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LessonsConfig {
    /// 本席址：只有 `seat` 精確等於它（且在 `seats` 內）的 L0 才算候選。
    pub self_seat: String,
    /// 席位白名單。
    pub seats: Vec<String>,
    /// 模型白名單（精確字串比對；只列 Claude 模型）。
    pub models: Vec<String>,
    /// repo 名清單（store 所屬 repo 目錄名）。
    pub repos: Vec<String>,
}

/// 由環境變數解析設定檔路徑（資料目錄規則與 `gate::data_dir` 相同）。
pub fn config_path() -> PathBuf {
    gate::data_dir().join("lessons.toml")
}

/// 讀取並驗證設定。檔案不存在、解析失敗、或任一清單為空 → `None`
///（fail-closed：呼叫方輸出 `[]`、exit 0）。
pub fn load_config(path: &Path) -> Option<LessonsConfig> {
    let text = std::fs::read_to_string(path).ok()?;
    let cfg: LessonsConfig = toml::from_str(&text).ok()?;
    if cfg.self_seat.trim().is_empty()
        || cfg.seats.is_empty()
        || cfg.models.is_empty()
        || cfg.repos.is_empty()
    {
        return None;
    }
    Some(cfg)
}

/// 證據指標——session 識別與時間範圍、去識別 lane id。絕無 signal 文字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub session_id: String,
    pub time_start: String,
    pub time_end: String,
    /// sha256(self_seat) 十六進位前 12 碼——去識別後的本席 lane id。
    pub lane_hash: String,
}

/// 一筆候選：恰好六個欄位（受限詞彙＋repo＋證據）。無 content、無路徑、
/// 無 seat／model 原文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub topic: Topic,
    pub pitfall_type: PitfallType,
    pub solution_type: SolutionType,
    pub tool: Tool,
    pub repo: String,
    pub evidence: Evidence,
}

/// `lane_hash`＝sha256(self_seat) 十六進位前 12 碼。
pub fn lane_hash_of(self_seat: &str) -> String {
    let digest = Sha256::digest(self_seat.as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// 唯讀 L0 掃描：`<project_dir>/signals/*.jsonl` 每一行可解析的 `Signal`。
/// 不推進 cursor、不標記 compiled、不寫入。
pub fn read_signals(project_dir: &Path) -> Vec<Signal> {
    let dir = project_dir.join("signals");
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
        Err(_) => return Vec::new(),
    };
    paths.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl"));
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(sig) = serde_json::from_str::<Signal>(line) {
                out.push(sig);
            }
        }
    }
    out
}

/// 去重鍵＝正規化鍵五欄加 session_id（repo 在單一 store 內是常數，仍列入以
/// 符合 spec 的六元組）。
type DedupKey = (Topic, PitfallType, SolutionType, Tool, String, String);

/// 依設定從當前 store 抽候選（唯讀）。
///
/// T2 機器閘：每筆候選產生後、去重前跑 [`gate::check`]（source＝該筆
/// signal 的 `content`）；命中就丟棄並累計該 `RuleId` 次數，絕不存命中
/// 文字。回傳（存活的候選，命中次數）；呼叫方把次數寫入
/// [`gate::gate_path`]（無設定檔 → 不會呼叫 collect → 不寫該檔）。
pub fn collect(
    ctx: &db::Context,
    cfg: &LessonsConfig,
    deny: Option<&gate::DenyList>,
) -> (Vec<Candidate>, GateCounts) {
    // repo＝store 所屬 repo 目錄名（project_dir 的父目錄名，如 …/codeforge/.codeforge → codeforge）
    let repo = ctx
        .project_dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(str::to_string);
    let Some(repo) = repo else {
        return (Vec::new(), BTreeMap::new());
    };
    // repo 不在清單 → 該 store 不產候選（fail-closed）
    if !cfg.repos.contains(&repo) {
        return (Vec::new(), BTreeMap::new());
    }
    let lane_hash = lane_hash_of(&cfg.self_seat);
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<DedupKey> = HashSet::new();
    let mut counts: GateCounts = BTreeMap::new();
    for sig in read_signals(&ctx.project_dir) {
        let Some((key, cand)) = candidate_from(&sig, &repo, &lane_hash, cfg) else {
            continue;
        };
        // 機器閘（去重前）：命中 → 丟棄，只累計規則 id 與次數
        let Ok(json) = serde_json::to_string(&cand) else {
            continue;
        };
        if let Err(rule) = gate::check(&json, Some(&sig.content), deny) {
            *counts.entry(rule).or_insert(0) += 1;
            continue;
        }
        if seen.insert(key) {
            out.push(cand);
        }
    }
    (out, counts)
}

/// 單一 signal 的過濾＋分類；任一條件不成立回 `None`（丟棄）。
fn candidate_from(
    sig: &Signal,
    repo: &str,
    lane_hash: &str,
    cfg: &LessonsConfig,
) -> Option<(DedupKey, Candidate)> {
    // source 必須是 session-digest 擷取器（段 1 唯一擷取器）
    if !matches!(sig.source, SignalSource::SessionDigest) {
        return None;
    }
    // seat：非空、== self_seat、且在席位白名單
    let seat = sig.seat.as_deref().filter(|s| !s.trim().is_empty())?;
    if seat != cfg.self_seat || !cfg.seats.iter().any(|s| s == seat) {
        return None;
    }
    // model：非空、精確比對在模型白名單
    let model = sig.model.as_deref().filter(|s| !s.trim().is_empty())?;
    if !cfg.models.iter().any(|m| m == model) {
        return None;
    }
    let session_id = sig
        .session_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())?
        .to_string();
    let time_start = sig
        .time_start
        .as_deref()
        .filter(|s| !s.trim().is_empty())?
        .to_string();
    let time_end = sig
        .time_end
        .as_deref()
        .filter(|s| !s.trim().is_empty())?
        .to_string();
    // 分類：四個受限詞彙全要命中，任一沒命中就丟棄
    let lower: String = sig
        .content
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let topic = Topic::classify(&lower)?;
    let pitfall_type = PitfallType::classify(&lower)?;
    let solution_type = SolutionType::classify(&lower)?;
    let tool = Tool::classify(&lower)?;
    let key = (
        topic,
        pitfall_type,
        solution_type,
        tool,
        repo.to_string(),
        session_id.clone(),
    );
    let cand = Candidate {
        topic,
        pitfall_type,
        solution_type,
        tool,
        repo: repo.to_string(),
        evidence: Evidence {
            session_id,
            time_start,
            time_end,
            lane_hash: lane_hash.to_string(),
        },
    };
    Some((key, cand))
}
