//! Rule-based L0 → LLM egress summarizer (P1).
//!
//! Converts raw L0 signals into a structured summary (source, length, tools,
//! error category keywords, file extensions, controlled domain topics) while
//! stripping filesystem paths, numeric literals, and verbatim conversation text.
//! Enforces that no contiguous substring of 20+ Unicode characters matches the
//! original `signal.content`.

use crate::memory::l0::{Signal, SignalSource};
use std::collections::BTreeSet;

/// Strict upper bound (exclusive) on the longest common contiguous substring
/// (counted in Unicode scalar values) between the outbound summary and raw content.
pub const MAX_EGRESS_OVERLAP_CHARS: usize = 20;

/// Claude Code tool names matched case-sensitively (or via `tool=<name>`).
const CLAUDE_TOOLS: &[&str] = &[
    "Bash",
    "Edit",
    "Glob",
    "Grep",
    "MultiEdit",
    "NotebookEdit",
    "Read",
    "Skill",
    "Task",
    "TodoWrite",
    "WebFetch",
    "WebSearch",
    "Write",
];

/// CLI / dev tool names matched case-insensitively on token boundaries.
const CLI_TOOLS: &[(&str, &str)] = &[
    ("agy", "agy"),
    ("awk", "awk"),
    ("bash", "Bash"),
    ("cargo", "cargo"),
    ("claude", "claude"),
    ("clippy", "clippy"),
    ("cmake", "cmake"),
    ("codex", "codex"),
    ("crontab", "crontab"),
    ("curl", "curl"),
    ("docker", "docker"),
    ("git", "git"),
    ("jq", "jq"),
    ("kubectl", "kubectl"),
    ("make", "make"),
    ("node", "node"),
    ("npm", "npm"),
    ("pip", "pip"),
    ("pnpm", "pnpm"),
    ("pytest", "pytest"),
    ("python", "python"),
    ("python3", "python"),
    ("rsync", "rsync"),
    ("rustc", "rustc"),
    ("rustfmt", "rustfmt"),
    ("sed", "sed"),
    ("sqlite3", "sqlite3"),
    ("ssh", "ssh"),
    ("systemctl", "systemctl"),
    ("tmux", "tmux"),
    ("yarn", "yarn"),
    ("zsh", "zsh"),
];

/// Allowlisted code / config file extensions (emitted without path or filename stem).
fn known_extension(ext_lower: &str) -> Option<&'static str> {
    match ext_lower {
        ".bash" => Some(".bash"),
        ".c" => Some(".c"),
        ".cfg" => Some(".cfg"),
        ".conf" => Some(".conf"),
        ".cpp" => Some(".cpp"),
        ".css" => Some(".css"),
        ".csv" => Some(".csv"),
        ".env" => Some(".env"),
        ".go" => Some(".go"),
        ".h" => Some(".h"),
        ".hpp" => Some(".hpp"),
        ".html" => Some(".html"),
        ".ini" => Some(".ini"),
        ".java" => Some(".java"),
        ".js" => Some(".js"),
        ".json" => Some(".json"),
        ".jsonl" => Some(".jsonl"),
        ".jsx" => Some(".jsx"),
        ".lock" => Some(".lock"),
        ".log" => Some(".log"),
        ".md" => Some(".md"),
        ".php" => Some(".php"),
        ".py" => Some(".py"),
        ".rb" => Some(".rb"),
        ".rs" => Some(".rs"),
        ".service" => Some(".service"),
        ".sh" => Some(".sh"),
        ".sql" => Some(".sql"),
        ".timer" => Some(".timer"),
        ".toml" => Some(".toml"),
        ".ts" => Some(".ts"),
        ".tsx" => Some(".tsx"),
        ".txt" => Some(".txt"),
        ".xml" => Some(".xml"),
        ".yaml" => Some(".yaml"),
        ".yml" => Some(".yml"),
        ".zsh" => Some(".zsh"),
        _ => None,
    }
}

/// Controlled error-category taxonomy: `(canonical_tag, pipe_separated_keywords)`.
const ERROR_TAXONOMY: &[(&str, &str)] = &[
    (
        "permission",
        "permission denied|eacces|eperm|unauthorized|forbidden|權限|拒絕",
    ),
    (
        "not-found",
        "not found|enoent|no such file|missing|找不到|不存在",
    ),
    (
        "syntax-parse",
        "syntax error|parse error|invalid json|unexpected token|解析失敗|語法",
    ),
    (
        "type-compile",
        "mismatched types|cannot borrow|lifetime|trait bound|typeerror|編譯|型別",
    ),
    ("timeout", "timeout|timed out|deadline|逾時|超時"),
    (
        "network",
        "connection refused|econnrefused|dns|tls|ssl|network|連線|網路",
    ),
    (
        "encoding-utf8",
        "utf-8|utf8|char boundary|cjk|unicode|亂碼|截斷",
    ),
    (
        "git-vcs",
        "merge conflict|rebase|detached head|pre-push|worktree|分支|衝突",
    ),
    (
        "path-env",
        "command not found|not in path|environment|環境變數|路徑",
    ),
    (
        "test-lint",
        "assertion failed|test failed|clippy|rustfmt|shellcheck|測試失敗|斷言",
    ),
    ("lock-race", "busy|locked|race|deadlock|stale|競態|鎖"),
    (
        "data-loss",
        "overwrite|truncate|cleared|清空|覆寫|遺失|誤刪",
    ),
    (
        "format-style",
        "format|indent|convention|繁體中文|正體中文|格式|風格",
    ),
    (
        "config-hook",
        "hook|settings|config|crontab|systemd|設定|排程",
    ),
];

/// Controlled short domain-topic hints (canonical English tokens, <= 12 chars each).
const TOPIC_LEXICON: &[(&str, &str)] = &[
    ("async", "async|tokio|await|非同步"),
    ("atomic", "atomic|rename|暫存檔|原子"),
    ("auth", "auth|oauth|token|login|認證|憑證"),
    ("backup", "backup|snapshot|備份|快照"),
    ("build", "build|compile|cargo|建置|編譯"),
    ("cache", "cache|快取"),
    ("cli", "cli|subcommand|flag|指令"),
    ("config", "config|settings|env|設定"),
    ("crontab", "crontab|cron|排程"),
    ("daemon", "daemon|service|systemd|背景服務"),
    ("database", "sqlite|sql|migration|schema|資料庫"),
    ("deploy", "deploy|release|部署|發佈"),
    ("digest", "digest|ingest|摘要|吸入"),
    ("git", "git|commit|branch|merge|rebase|worktree"),
    ("hook", "hook|pretooluse|sessionend|sessionstart"),
    ("json", "json|jsonl|serde"),
    ("lint", "clippy|rustfmt|shellcheck|lint"),
    ("logging", "log|stderr|stdout|日誌"),
    ("memory", "memory|l0|l1|fts|記憶"),
    ("network", "http|tls|curl|socket|網路"),
    ("path", "path|symlink|dir|路徑"),
    ("plugin", "plugin|mod|外掛"),
    ("redact", "redact|mask|secret|遮罩|去識別"),
    ("rust", "rust|crate|borrow|lifetime|trait"),
    ("session", "session|transcript|對話"),
    ("test", "test|fixture|assert|測試"),
    ("unicode", "utf-8|utf8|cjk|unicode|中文|截斷"),
];

/// Convert an L0 [`Signal`] into a deterministic structured summary safe for LLM egress.
///
/// Contains only structured metadata and controlled-vocabulary labels (source,
/// char length, category, tool names, error categories, file extensions, domain
/// topics) with all paths, numbers, and verbatim conversation text stripped.
/// Guarantees `longest_common_substring_chars(&summary, &signal.content) < 20`.
pub fn summarize_for_llm(signal: &Signal) -> String {
    summarize_text(source_label(&signal.source), &signal.content)
}

/// Longest L1 title prefix sent to the LLM: one char under the overlap bar, so a
/// title can never carry a 20-char run of any source text.
const L1_TITLE_EGRESS_CHARS: usize = MAX_EGRESS_OVERLAP_CHARS - 1;

/// Summarize an L1 entry for the reconcile prompt. The body is LLM-rewritten but
/// may still echo a source sentence, so it gets the same structured summary as an
/// L0 signal; the title is cut below the overlap bar.
/// Guarantees `longest_common_substring_chars(&summary, body) < 20` and that the
/// title part is under 20 chars.
pub fn summarize_l1_for_llm(title: &str, body: &str) -> String {
    let title_cut: String = title.chars().take(L1_TITLE_EGRESS_CHARS).collect();
    format!("title: {title_cut}\n{}", summarize_text("l1", body))
}

fn summarize_text(source: &str, raw: &str) -> String {
    let category = detect_category(raw);
    let length_chars = raw.chars().count();
    let file_exts = extract_file_extensions(raw);
    let stripped = strip_paths_and_numbers(raw);
    let tools = extract_tools(raw, &stripped);
    let error_categories = extract_error_categories(raw, &stripped);
    let topics = extract_topics(&stripped);

    let tools_str = join_or_default(&tools, "none");
    let errors_str = join_or_default(&error_categories, "general");
    let exts_str = join_or_default(&file_exts, "none");
    let topics_str = join_or_default(&topics, "general");

    let summary = format!(
        "[Structured Signal Summary]\n\
         source: {source}\n\
         category: {category}\n\
         length_chars: {length_chars}\n\
         tools: {tools_str}\n\
         error_categories: {errors_str}\n\
         file_exts: {exts_str}\n\
         topics: {topics_str}"
    );

    break_long_overlaps(summary, raw, MAX_EGRESS_OVERLAP_CHARS - 1)
}

/// Compute the length (in Unicode characters) of the longest common contiguous
/// substring between `a` and `b`. CJK-safe (operates on `char`, never bytes).
#[cfg(test)]
pub fn longest_common_substring_chars(a: &str, b: &str) -> usize {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    find_longest_common_span(&ac, &bc)
        .map(|(s, e)| e - s)
        .unwrap_or(0)
}

fn find_longest_common_span(ac: &[char], bc: &[char]) -> Option<(usize, usize)> {
    if ac.is_empty() || bc.is_empty() {
        return None;
    }
    let mut prev = vec![0usize; bc.len() + 1];
    let mut curr = vec![0usize; bc.len() + 1];
    let mut best_len = 0usize;
    let mut best_end = 0usize;
    for (i, &ca) in ac.iter().enumerate() {
        for (j, &cb) in bc.iter().enumerate() {
            if ca == cb {
                let v = prev[j] + 1;
                curr[j + 1] = v;
                if v > best_len {
                    best_len = v;
                    best_end = i + 1;
                }
            } else {
                curr[j + 1] = 0;
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    if best_len > 0 {
        Some((best_end - best_len, best_end))
    } else {
        None
    }
}

/// Hard safety net: if `summary` and `raw` share any contiguous substring longer
/// than `max_allowed` chars (e.g. adversarial input echoing our template), break
/// every offending span deterministically until overlap <= `max_allowed`.
fn break_long_overlaps(summary: String, raw: &str, max_allowed: usize) -> String {
    let bc: Vec<char> = raw.chars().collect();
    if bc.len() <= max_allowed {
        return summary;
    }
    let mut ac: Vec<char> = summary.chars().collect();
    let breaker = ['\u{2423}', '¦', '·', '§', '¤', '~', '|', '#']
        .into_iter()
        .find(|c| !bc.contains(c));

    let max_iters = ac.len() + 1;
    for _ in 0..max_iters {
        let Some((s, e)) = find_longest_common_span(&ac, &bc) else {
            break;
        };
        if e - s <= max_allowed {
            break;
        }
        let mid = s + (max_allowed / 2).max(1);
        if let Some(ch) = breaker {
            if mid < ac.len() {
                ac[mid] = ch;
                continue;
            }
        }
        // Fallback when every candidate breaker appears in `raw`: shrink span.
        let cut_from = (s + (max_allowed / 2).max(1)).min(e);
        ac.drain(cut_from..e);
    }
    ac.into_iter().collect()
}

fn source_label(source: &SignalSource) -> &'static str {
    match source {
        SignalSource::Manual => "manual",
        SignalSource::ClaudeCodeSession => "claude-code",
        SignalSource::WebChatClaude => "claude.ai",
        SignalSource::WebChatChatGpt => "chatgpt",
        SignalSource::Ingest { .. } => "ingest",
        SignalSource::Hook { .. } => "hook",
        SignalSource::AbsorbedMemory => "absorbed",
        SignalSource::SessionDigest => "session-digest",
    }
}

fn detect_category(raw: &str) -> &'static str {
    let lower = raw.to_lowercase();
    if raw.contains("【錯誤修復】") || lower.contains("error-recovery") {
        return "error-recovery";
    }
    if raw.contains("【使用者糾正】") || lower.contains("user-correction") {
        return "user-correction";
    }
    if raw.contains("【自我修正】") || lower.contains("self-correction") {
        return "self-correction";
    }
    let is_question =
        raw.contains('?') || raw.contains('？') || lower.contains("如何") || lower.contains("怎麼");
    if is_question {
        return "question";
    }
    if raw.contains(" vs ") || raw.contains(" 和 ") || raw.contains(" 與 ") {
        return "comparison";
    }
    if raw.contains("不對")
        || raw.contains("錯了")
        || raw.contains("不要這樣")
        || raw.contains("請改用")
        || lower.contains("wrong")
        || lower.contains("incorrect")
        || lower.contains("instead")
    {
        return "user-correction";
    }
    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("panic")
        || lower.contains("exception")
        || raw.contains("失敗")
        || raw.contains("錯誤")
        || raw.contains("報錯")
    {
        return "error-recovery";
    }
    "note"
}

fn is_path_token(tok: &str) -> bool {
    if tok.contains("://") {
        return true;
    }
    if tok.starts_with('/')
        || tok.starts_with("~/")
        || tok.starts_with("./")
        || tok.starts_with("../")
    {
        return true;
    }
    if tok.contains('\\') {
        return true;
    }
    // Windows drive path such as C:/foo/bar
    let bytes = tok.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
    {
        return true;
    }
    if tok.matches('/').count() >= 2 {
        return true;
    }
    if tok.contains('/') && extract_ext_from_token(tok).is_some() {
        return true;
    }
    false
}

fn is_token_delim(c: char) -> bool {
    c.is_whitespace() || "\"'`()[]{},;<>|，。、（）".contains(c)
}

fn is_edge_punct(c: char) -> bool {
    "\"'`()[]{},;<>!?".contains(c)
}

fn is_ext_delim(c: char) -> bool {
    c.is_whitespace() || ",;|，。、：（）".contains(c)
}

fn is_tool_sep(c: char) -> bool {
    !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '='
}

fn is_word_sep(c: char) -> bool {
    !c.is_ascii_alphanumeric() && c != '_' && c != '-'
}

/// Remove filesystem paths, URLs, and all ASCII digit characters from `raw`.
fn strip_paths_and_numbers(raw: &str) -> String {
    let mut kept = Vec::new();
    for raw_tok in raw.split(is_token_delim) {
        if raw_tok.is_empty() {
            continue;
        }
        // Handle key=value tokens like `file=/a/b/c.rs` or `error=...`
        let candidate = match raw_tok.split_once('=') {
            Some((k, v)) if is_path_token(v) => k,
            _ if is_path_token(raw_tok) => continue,
            _ => raw_tok,
        };
        let no_digits: String = candidate.chars().filter(|c| !c.is_ascii_digit()).collect();
        let trimmed = no_digits.trim_matches([':', '.', '-', '_']);
        if !trimmed.is_empty() {
            kept.push(trimmed.to_string());
        }
    }
    kept.join(" ")
}

fn extract_ext_from_token(tok: &str) -> Option<&'static str> {
    let mut cleaned = tok.trim_matches(is_edge_punct);
    if let Some((_, rhs)) = cleaned.split_once('=') {
        cleaned = rhs;
    }
    // Strip trailing :line or :line:col segments.
    while let Some((head, tail)) = cleaned.rsplit_once(':') {
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            cleaned = head;
        } else {
            break;
        }
    }
    let base = cleaned
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(cleaned)
        .trim_end_matches('.');
    let dot = base.rfind('.')?;
    if dot == 0 {
        return None;
    }
    let ext_lower = base[dot..].to_ascii_lowercase();
    known_extension(&ext_lower)
}

fn extract_file_extensions(raw: &str) -> Vec<&'static str> {
    let mut exts = BTreeSet::new();
    for tok in raw.split(is_ext_delim) {
        if let Some(ext) = extract_ext_from_token(tok) {
            exts.insert(ext);
            if exts.len() >= 8 {
                break;
            }
        }
    }
    exts.into_iter().collect()
}

fn extract_tools(raw: &str, stripped: &str) -> Vec<&'static str> {
    let mut found = BTreeSet::new();
    // 1. Explicit `tool=<Name>` markers (from session-digest) and PascalCase Claude tools.
    for tok in raw.split(is_tool_sep) {
        let name = tok.strip_prefix("tool=").unwrap_or(tok);
        if let Some(&tool) = CLAUDE_TOOLS.iter().find(|&&t| t == name) {
            found.insert(tool);
        } else if let Some((_, canonical)) = CLI_TOOLS
            .iter()
            .find(|(pat, _)| name.eq_ignore_ascii_case(pat))
        {
            found.insert(*canonical);
        }
    }
    // 2. CLI tools in stripped text (paths & numbers already removed).
    for tok in stripped.split(is_word_sep) {
        if tok.is_empty() {
            continue;
        }
        let lower = tok.to_ascii_lowercase();
        if let Some((_, canonical)) = CLI_TOOLS.iter().find(|(pat, _)| *pat == lower) {
            found.insert(*canonical);
        }
    }
    found.into_iter().take(8).collect()
}

fn has_rust_error_code(raw: &str) -> bool {
    raw.as_bytes().windows(5).any(|w| {
        w[0] == b'E'
            && w[1].is_ascii_digit()
            && w[2].is_ascii_digit()
            && w[3].is_ascii_digit()
            && w[4].is_ascii_digit()
    })
}

fn extract_error_categories(raw: &str, stripped: &str) -> Vec<&'static str> {
    let mut cats = Vec::new();
    let lower = stripped.to_lowercase();
    for &(cat, keywords) in ERROR_TAXONOMY {
        let hit = keywords.split('|').any(|kw| lower.contains(kw))
            || (cat == "type-compile" && has_rust_error_code(raw));
        if hit && !cats.contains(&cat) {
            cats.push(cat);
        }
    }
    cats.truncate(6);
    cats
}

fn extract_topics(stripped: &str) -> Vec<&'static str> {
    let mut topics = Vec::new();
    let lower = stripped.to_lowercase();
    let words: BTreeSet<&str> = lower.split(is_word_sep).collect();
    for &(topic, patterns) in TOPIC_LEXICON {
        let hit = patterns.split('|').any(|p| {
            if p.is_ascii() {
                words.contains(p)
            } else {
                lower.contains(p)
            }
        });
        if hit && !topics.contains(&topic) {
            topics.push(topic);
        }
    }
    topics.truncate(6);
    topics
}

fn join_or_default(items: &[&str], default: &str) -> String {
    if items.is_empty() {
        default.to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_cjk_path_and_80char_user_correction_has_overlap_under_20() {
        let content = "【使用者糾正】correction=請不要再用 sed -i 直接覆寫 /home/user/projects/demo-app/config/crontab.conf 第 42 行，每次這樣操作都會把整個排程檔案清空並造成資料遺失，請改用暫存檔寫入再原子 rename 取代原檔";
        assert!(
            content.chars().count() >= 80,
            "fixture must be >= 80 chars, got {}",
            content.chars().count()
        );
        let sig = Signal::new(content.to_string(), SignalSource::SessionDigest);
        let summary = summarize_for_llm(&sig);
        let lcs = longest_common_substring_chars(&summary, &sig.content);
        assert!(
            lcs < MAX_EGRESS_OVERLAP_CHARS,
            "expected LCS < 20, got {lcs}\nsummary:\n{summary}"
        );
        assert!(summary.contains("source: session-digest"));
        assert!(summary.contains("category: user-correction"));
        assert!(summary.contains("sed"));
        assert!(summary.contains(".conf"));
        assert!(summary.contains("data-loss"));
        assert!(!summary.contains("/home/user"), "path stripped: {summary}");
        assert!(!summary.contains("demo-app"), "segment stripped: {summary}");
        assert!(!summary.contains("42"), "number stripped: {summary}");
    }

    #[test]
    fn summarize_handles_adversarial_template_echo_under_20() {
        let probe = Signal::new("probe signal".to_string(), SignalSource::Manual);
        let template_like = summarize_for_llm(&probe);
        // Feed our own summary text back as raw signal content!
        let adversarial = Signal::new(template_like, SignalSource::Manual);
        let summary = summarize_for_llm(&adversarial);
        let lcs = longest_common_substring_chars(&summary, &adversarial.content);
        assert!(
            lcs < MAX_EGRESS_OVERLAP_CHARS,
            "adversarial echo must still have LCS < 20, got {lcs}\nsummary:\n{summary}"
        );
    }

    #[test]
    fn summarize_extracts_rust_compile_error_without_leaking_path_or_code() {
        let content = "【錯誤修復】tool=Bash | error=error[E0308]: mismatched types in /workspace/demo/src/dream/compile.rs:612:19 | context=修改 compile_signal 簽章後編譯失敗";
        let sig = Signal::new(content.to_string(), SignalSource::SessionDigest);
        let summary = summarize_for_llm(&sig);
        let lcs = longest_common_substring_chars(&summary, &sig.content);
        assert!(lcs < MAX_EGRESS_OVERLAP_CHARS, "got LCS {lcs}: {summary}");
        assert!(summary.contains("Bash"));
        assert!(summary.contains("type-compile"));
        assert!(summary.contains(".rs"));
        assert!(!summary.contains("/workspace/demo"));
        assert!(!summary.contains("612"));
        assert!(!summary.contains("E0308"));
    }

    #[test]
    fn longest_common_substring_is_cjk_safe() {
        assert_eq!(longest_common_substring_chars("正體中文測試", "中文測"), 3);
        assert_eq!(longest_common_substring_chars("", "abc"), 0);
        assert_eq!(longest_common_substring_chars("abc", "xyz"), 0);
    }
}
