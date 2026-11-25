//! 機器閘（plan `doc/plans/2026-11-23-knowledge-pipeline.md` 第 10.2 節 T2）。
//!
//! 命中規則的候選當場丟棄：只記規則 id 與次數，絕不存命中文字、不送審、
//! 不離開席位與主機。規則依 `RuleId` 宣告順序對候選**序列化後的 JSON 字串**
//! 整體檢查，命中第一條就回 `Err(rule)`。`lessons-gate.json` 只有
//! `{"<rule-id>": <count>}`（無命中為 `{}`）；禁詞檔不存在 → fail-closed
//! （`no-deny-list`）。
//!
//! Cargo.toml 沒有 `regex` 相依，模式皆手寫比對（不新增相依）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 規則 id（serde kebab-case；宣告順序＝檢查順序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleId {
    /// 禁詞清單不存在（fail-closed）。
    NoDenyList,
    /// 含任一禁詞。
    DenyTerm,
    /// 三段位址 `[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+`。
    SeatAddress,
    /// 本機絕對路徑（/home/、/Users/、/tmp/、/run/、~/、X:\、X:/）。
    AbsPath,
    /// 組織名（Inc./Ltd/LLC/Corp、股份有限公司、有限公司）。
    Org,
    /// 金額（$/NT$/USD/TWD 後接數字、數字後接 元）。
    Money,
    /// source 缺席。
    NoSource,
    /// 與 source 有長度 ≥20 字元的共同子字串。
    SourceOverlap,
}

/// 命中次數：規則 id → 次數。0 次命中不進 map；JSON 序列化恰為
/// `{"<rule-id>": <count>}`（全零為 `{}`），絕不含命中文字。
pub type GateCounts = BTreeMap<RuleId, u32>;

/// 本機禁詞清單：帳號別名、裸主機名、組織名——每行一個詞。`#` 開頭與空行
/// 略過；比對不分大小寫、子字串。這個檔不進 repo（測試用 tempdir 產生）。
#[derive(Debug, Clone)]
pub struct DenyList {
    /// 小寫後的詞（空詞丟掉）。
    terms: Vec<String>,
}

impl DenyList {
    /// 由詞表建立（trim、小寫、丟空詞）。
    pub fn new(terms: Vec<String>) -> Self {
        Self {
            terms: terms
                .into_iter()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect(),
        }
    }

    /// 由檔案讀取（每行一個詞；`#` 與空行略過）；檔案不存在或不可讀 →
    /// `None`（呼叫方以 `no-deny-list` fail-closed）。
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let terms = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect();
        Some(Self::new(terms))
    }

    /// 由預設位置（[`deny_path`]）讀取；不存在 → `None`。
    pub fn load_default() -> Option<Self> {
        Self::load(&deny_path())
    }

    /// 任一禁詞是否出現在 `text`（不分大小寫的子字串）。
    pub fn matches(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.terms.iter().any(|t| lower.contains(t.as_str()))
    }
}

/// 資料目錄：`$CODEFORGE_DATA_DIR`，未設環境變數時 `~/.codeforge`
///（與 `candidates::config_path` 同一規則）。
pub fn data_dir() -> PathBuf {
    match std::env::var("CODEFORGE_DATA_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".codeforge"),
    }
}

/// 禁詞清單位置：`$CODEFORGE_DATA_DIR/lessons-deny.txt`（未設時 `~/.codeforge/`）。
pub fn deny_path() -> PathBuf {
    data_dir().join("lessons-deny.txt")
}

/// 閘命中次數檔位置：`$CODEFORGE_DATA_DIR/lessons-gate.json`（同上基底）。
pub fn gate_path() -> PathBuf {
    data_dir().join("lessons-gate.json")
}

/// 把命中次數寫到 `path`（緊湊 JSON；只有 `{"<rule-id>": <count>}`，無命中
/// 覆寫成 `{}`；絕不寫命中文字）。
pub fn write_counts(path: &Path, counts: &GateCounts) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(counts)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// 機器閘：對候選序列化後的 JSON 字串整體檢查。
///
/// 順序（命中第一條就回 Err）：`no-deny-list`（deny 為 None）→ `deny-term`
/// → `seat-address` → `abs-path` → `org` → `money` → `no-source`（source 為
/// None）→ `source-overlap`（與 source 有 ≥20 字元共同子字串，用 chars 計、
/// CJK 安全）。
pub fn check(
    candidate_json: &str,
    source: Option<&str>,
    deny: Option<&DenyList>,
) -> Result<(), RuleId> {
    let Some(deny) = deny else {
        return Err(RuleId::NoDenyList);
    };
    if deny.matches(candidate_json) {
        return Err(RuleId::DenyTerm);
    }
    if has_seat_address(candidate_json) {
        return Err(RuleId::SeatAddress);
    }
    if has_abs_path(candidate_json) {
        return Err(RuleId::AbsPath);
    }
    if has_org(candidate_json) {
        return Err(RuleId::Org);
    }
    if has_money(candidate_json) {
        return Err(RuleId::Money);
    }
    let Some(source) = source else {
        return Err(RuleId::NoSource);
    };
    if longest_common_substring_len(candidate_json, source) >= 20 {
        return Err(RuleId::SourceOverlap);
    }
    Ok(())
}

/// 位址字元集 `[A-Za-z0-9_.-]`（純 ASCII，位元級比對安全）。
fn is_addr_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')
}

/// 三段位址 `[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+`（非錨定子串；
/// regex 等价手寫——位址字元純 ASCII，位元級掃描安全）。
fn has_seat_address(s: &str) -> bool {
    let b = s.as_bytes();
    let mut prev_slash: Option<usize> = None;
    for (i, &c) in b.iter().enumerate() {
        if c != b'/' {
            continue;
        }
        if let Some(p) = prev_slash {
            // p 與 i 是相鄰的兩個斜線：第一段在 p 前至少一個位址字元、
            // 中段 (p, i) 非空且全為位址字元、第三段在 i 後至少一個位址字元
            if i >= p + 2
                && b[p + 1..i].iter().all(|&x| is_addr_byte(x))
                && p > 0
                && is_addr_byte(b[p - 1])
                && i + 1 < b.len()
                && is_addr_byte(b[i + 1])
            {
                return true;
            }
        }
        prev_slash = Some(i);
    }
    false
}

/// 本機絕對路徑：`/home/`、`/Users/`、`/tmp/`、`/run/`、`~/`，或磁碟代號
/// `X:\`／`X:/`（X 為英文字母）。
fn has_abs_path(s: &str) -> bool {
    const PREFIXES: &[&str] = &["/home/", "/Users/", "/tmp/", "/run/", "~/"];
    if PREFIXES.iter().any(|p| s.contains(p)) {
        return true;
    }
    let b = s.as_bytes();
    for i in 0..b.len().saturating_sub(2) {
        if b[i].is_ascii_alphabetic() && b[i + 1] == b':' && (b[i + 2] == b'\\' || b[i + 2] == b'/')
        {
            return true;
        }
    }
    false
}

/// 組織名：`Inc.`／`Ltd`／`LLC`／`Corp`（英文不分大小寫）或
/// `股份有限公司`／`有限公司`。
fn has_org(s: &str) -> bool {
    const EN: &[&str] = &["inc.", "ltd", "llc", "corp"];
    let lower = s.to_lowercase();
    EN.iter().any(|k| lower.contains(k)) || s.contains("股份有限公司") || s.contains("有限公司")
}

/// 金額：`$`／`NT$`／`USD`／`TWD` 後接（可有空白）數字，或數字後接 `元`。
/// （`nt$` 由 `$` 分支涵蓋；`usd`／`twd` 不分大小寫。）
fn has_money(s: &str) -> bool {
    let lower = s.to_lowercase();
    let c: Vec<char> = lower.chars().collect();
    let n = c.len();
    let digit_after_ws = |start: usize| -> bool {
        let mut j = start;
        while j < n && c[j].is_whitespace() {
            j += 1;
        }
        j < n && c[j].is_ascii_digit()
    };
    for i in 0..n {
        if c[i] == '$' && digit_after_ws(i + 1) {
            return true;
        }
        if c[i] == 'u' && i + 3 <= n && c[i + 1] == 's' && c[i + 2] == 'd' && digit_after_ws(i + 3)
        {
            return true;
        }
        if c[i] == 't' && i + 3 <= n && c[i + 1] == 'w' && c[i + 2] == 'd' && digit_after_ws(i + 3)
        {
            return true;
        }
        if c[i].is_ascii_digit() && i + 1 < n && c[i + 1] == '元' {
            return true;
        }
    }
    false
}

/// 最長共同子字串長度（用 chars 計、CJK 安全；O(n·m) 滾動陣列，≥20 提前結束）。
fn longest_common_substring_len(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 0;
    }
    let mut prev = vec![0usize; m + 1];
    let mut cur = vec![0usize; m + 1];
    let mut best = 0;
    for a_ch in a.iter() {
        for j in 0..m {
            cur[j + 1] = if *a_ch == b[j] { prev[j] + 1 } else { 0 };
            if cur[j + 1] > best {
                best = cur[j + 1];
                if best >= 20 {
                    return best;
                }
            }
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::candidates::{Candidate, Evidence};
    use crate::lessons::vocab::{PitfallType, SolutionType, Tool, Topic};
    use std::path::Path;
    use tempfile::TempDir;

    /// 與候選 JSON 沒有 ≥20 字元重疊的乾淨 source（緊湊 JSON 無空白，
    /// 此 source 的任何無空白片段都短於 20 字元）。
    const CLEAN_SOURCE: &str = "clean source xyzwvut";

    /// 沙箱禁詞檔：三個假別名＋三個假主機名（明顯假的詞，絕不進 repo）。
    const DENY_BODY: &str = "# sandbox fake aliases\naliasfoo\naliasbar\n\naliasbaz\n# sandbox fake hostnames\nhostalpha\nhostbeta\nhostgamma\n";

    fn candidate(session_id: &str) -> Candidate {
        Candidate {
            topic: Topic::Test,
            pitfall_type: PitfallType::Timeout,
            solution_type: SolutionType::RetryBackoff,
            tool: Tool::Cargo,
            repo: "demo-repo".to_string(),
            evidence: Evidence {
                session_id: session_id.to_string(),
                time_start: "2026-11-26T01:00:00Z".to_string(),
                time_end: "2026-11-26T01:45:00Z".to_string(),
                lane_hash: "0123456789ab".to_string(),
            },
        }
    }

    fn cand_json(session_id: &str) -> String {
        serde_json::to_string(&candidate(session_id)).unwrap()
    }

    /// tempdir 產生禁詞檔並載入。
    fn deny_from_body(tmp: &Path, body: &str) -> DenyList {
        let p = tmp.join("lessons-deny.txt");
        std::fs::write(&p, body).unwrap();
        DenyList::load(&p).unwrap()
    }

    /// 存在但無詞的禁詞檔（通過 no-deny-list；無詞可命中）。
    fn present_deny(tmp: &Path) -> DenyList {
        deny_from_body(tmp, "# empty sandbox deny list\n")
    }

    // 15 個反向探針：每個都必須 Err 且 rule id 正確。

    /// 路徑 ×3：`/home/x/a`、`/Users/x/a` 含三段位址（`home/x/a`、`Users/x/a`），
    /// seat-address（規則 2）先於 abs-path（規則 3）命中；`C:\x\a` 沒有 `/`，
    /// 由磁碟代號模式命中 abs-path。
    #[test]
    fn probe_paths_hit_seat_address_or_abs_path() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        assert_eq!(
            check(&cand_json("/home/x/a"), Some(CLEAN_SOURCE), Some(&deny)),
            Err(RuleId::SeatAddress)
        );
        assert_eq!(
            check(&cand_json("/Users/x/a"), Some(CLEAN_SOURCE), Some(&deny)),
            Err(RuleId::SeatAddress)
        );
        assert_eq!(
            check(&cand_json("C:\\x\\a"), Some(CLEAN_SOURCE), Some(&deny)),
            Err(RuleId::AbsPath)
        );
    }

    /// 帳號別名 ×3（禁詞檔內的三個假別名；第三个大写，验证不分大小写）。
    #[test]
    fn probe_account_aliases_hit_deny_term() {
        let tmp = TempDir::new().unwrap();
        let deny = deny_from_body(tmp.path(), DENY_BODY);
        for alias in ["aliasfoo", "aliasbar", "ALIASBAZ"] {
            assert_eq!(
                check(&cand_json(alias), Some(CLEAN_SOURCE), Some(&deny)),
                Err(RuleId::DenyTerm),
                "別名 {alias} 必須命中 deny-term"
            );
        }
    }

    /// 主機名 ×3（禁詞檔內的三個假主機名）。
    #[test]
    fn probe_hostnames_hit_deny_term() {
        let tmp = TempDir::new().unwrap();
        let deny = deny_from_body(tmp.path(), DENY_BODY);
        for host in ["hostalpha", "hostbeta", "hostgamma"] {
            assert_eq!(
                check(&cand_json(host), Some(CLEAN_SOURCE), Some(&deny)),
                Err(RuleId::DenyTerm),
                "主機名 {host} 必須命中 deny-term"
            );
        }
    }

    /// 組織 ×3。
    #[test]
    fn probe_orgs_hit_org() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        for org in ["Acme Inc.", "Foo Ltd", "某某股份有限公司"] {
            assert_eq!(
                check(&cand_json(org), Some(CLEAN_SOURCE), Some(&deny)),
                Err(RuleId::Org),
                "組織 {org} 必須命中 org"
            );
        }
    }

    /// 金額 ×3。
    #[test]
    fn probe_money_hits_money() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        for m in ["$120", "NT$ 300", "500元"] {
            assert_eq!(
                check(&cand_json(m), Some(CLEAN_SOURCE), Some(&deny)),
                Err(RuleId::Money),
                "金額 {m} 必須命中 money"
            );
        }
    }

    /// 正例：乾淨候選＋不重疊 source → Ok。
    #[test]
    fn clean_candidate_with_disjoint_source_passes() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        assert_eq!(
            check(&cand_json("sess-0001"), Some(CLEAN_SOURCE), Some(&deny)),
            Ok(())
        );
    }

    /// source 含候選 JSON 裡一段 20 字元字串 → Err(source-overlap)。
    #[test]
    fn source_overlap_ge20_chars_hits() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        let json = cand_json("sess-0001");
        // JSON 裡完整時間字串恰好 20 字元
        let src20 = "before 2026-11-26T01:00:00Z after";
        assert_eq!(
            check(&json, Some(src20), Some(&deny)),
            Err(RuleId::SourceOverlap)
        );
    }

    /// 只有 19 字元重疊（少結尾 Z；緊湊 JSON 無空白，無法延伸）→ Ok。
    #[test]
    fn source_overlap_only_19_chars_passes() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        let json = cand_json("sess-0001");
        let src19 = "before 2026-11-26T01:00:00 after";
        assert_eq!(check(&json, Some(src19), Some(&deny)), Ok(()));
    }

    /// no-source：source 為 None（其餘規則先全通過）→ Err。
    #[test]
    fn no_source_hits() {
        let tmp = TempDir::new().unwrap();
        let deny = present_deny(tmp.path());
        assert_eq!(
            check(&cand_json("sess-0001"), None, Some(&deny)),
            Err(RuleId::NoSource)
        );
    }

    /// no-deny-list：deny 為 None → fail-closed Err（第一條規則）。
    #[test]
    fn no_deny_list_hits_fail_closed() {
        assert_eq!(
            check(&cand_json("sess-0001"), Some(CLEAN_SOURCE), None),
            Err(RuleId::NoDenyList)
        );
    }

    /// 禁詞檔：`#` 與空行略過、比對不分大小寫、子字串。
    #[test]
    fn deny_list_loads_terms_skips_comments_and_blanks() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("lessons-deny.txt");
        std::fs::write(&p, DENY_BODY).unwrap();
        let deny = DenyList::load(&p).unwrap();
        assert!(deny.matches("using ALIASFOO here"));
        assert!(deny.matches("connect via hostALPHA"));
        assert!(!deny.matches("nothing to see"));
        // 註解與空行不是詞
        assert!(!deny.matches("# sandbox fake aliases"));
        assert!(!deny.matches("\n"));
    }

    /// 檔案不存在 → None（呼叫方 fail-closed）。
    #[test]
    fn deny_list_missing_file_is_none() {
        let tmp = TempDir::new().unwrap();
        assert!(DenyList::load(&tmp.path().join("lessons-deny.txt")).is_none());
    }

    /// 三段位址邊界。
    #[test]
    fn seat_address_pattern_edges() {
        assert!(has_seat_address("a/b/c"));
        assert!(has_seat_address("org/team/seat-x"));
        assert!(!has_seat_address("/a/b")); // 開頭斜線前沒有第一段
        assert!(!has_seat_address("a//b")); // 中段為空
        assert!(!has_seat_address("a/b")); // 只有兩段
    }

    /// 絕對路徑邊界。
    #[test]
    fn abs_path_edges() {
        assert!(has_abs_path("/tmp/x"));
        assert!(has_abs_path("/run/x"));
        assert!(has_abs_path("~/x"));
        assert!(has_abs_path("C:/x"));
        assert!(has_abs_path("z:\\x"));
        assert!(!has_abs_path("home/relative"));
    }

    /// 金額邊界。
    #[test]
    fn money_edges() {
        assert!(has_money("costs $ 5 today"));
        assert!(has_money("100USD 50"));
        assert!(has_money("TWD200"));
        assert!(has_money("10元"));
        assert!(!has_money("no digits here $"));
        assert!(!has_money("usd only no number"));
    }

    /// 組織名邊界。
    #[test]
    fn org_edges() {
        assert!(has_org("ACME INC."));
        assert!(has_org("foo ltd bar"));
        assert!(has_org("某某有限公司"));
        assert!(!has_org("plain name"));
    }

    /// lessons-gate.json 內容：只有 `{"<rule-id>": <count>}`；無命中覆寫 `{}`。
    #[test]
    fn write_counts_only_rule_ids_and_empty_overwrite() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("lessons-gate.json");
        let mut counts = GateCounts::new();
        counts.insert(RuleId::AbsPath, 1);
        write_counts(&p, &counts).unwrap();
        let body = std::fs::read_to_string(&p).unwrap();
        assert_eq!(body, "{\"abs-path\":1}");
        assert!(!body.contains("/home/"), "絕不寫命中文字");
        // 無命中也覆寫成 {}
        write_counts(&p, &GateCounts::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{}");
    }
}
