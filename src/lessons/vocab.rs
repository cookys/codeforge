//! 受限詞彙表（知識管線段 1→2，plan `doc/plans/2026-11-23-knowledge-pipeline.md`
//! 第 4 節段 1–2、第 10.2 節 T1）。
//!
//! 候選教訓的每個欄位都是這些固定 enum 之一，serde 輸出 kebab-case 字串。
//! 刻意沒有自由文字欄位，所以候選不可能攜逐字訊號文字。分類是「對 signal
//! content 小寫後」的 ASCII 小寫子字串比對；取**第一個**（宣告順序）命中的
//! 值；任一 enum 沒命中，該筆 signal 就不是候選。

use serde::{Deserialize, Serialize};

/// 主題——教訓在講什麼。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Topic {
    Dispatch,
    Build,
    Test,
    Git,
    Install,
    Hook,
    Statusline,
    Quota,
    Fleet,
    OtherTooling,
}

impl Topic {
    /// 關鍵字表：(值, ASCII 小寫子字串)，順序＝enum 宣告順序（先命中先取）。
    const TABLE: &[(Self, &[&str])] = &[
        (Self::Dispatch, &["dispatch"]),
        (Self::Build, &["build", "compile", "linker", "link error"]),
        (Self::Test, &["test"]),
        (Self::Git, &["git"]),
        (Self::Install, &["install"]),
        (Self::Hook, &["hook"]),
        (
            Self::Statusline,
            &["statusline", "status-line", "status line"],
        ),
        (Self::Quota, &["quota", "rate limit"]),
        (Self::Fleet, &["fleet"]),
        (
            Self::OtherTooling,
            &["tool", "tooling", "script", "util", "cli"],
        ),
    ];

    /// `content_lower`（已小寫）裡第一個命中的值，沒命中回 `None`。
    pub fn classify(content_lower: &str) -> Option<Self> {
        Self::TABLE
            .iter()
            .find(|(_, kws)| kws.iter().any(|k| content_lower.contains(k)))
            .map(|(v, _)| *v)
    }
}

/// 坑類型——踩到的是哪種坑。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PitfallType {
    Path,
    Permission,
    Dependency,
    Race,
    Format,
    Config,
    QuotaLimit,
    Timeout,
}

impl PitfallType {
    const TABLE: &[(Self, &[&str])] = &[
        (
            Self::Path,
            &["path", "no such file", "file not found", "enotfound"],
        ),
        (
            Self::Permission,
            &["permission", "eacces", "denied", "forbidden"],
        ),
        (
            Self::Dependency,
            &["dependenc", "version conflict", "unsatisfied"],
        ),
        (Self::Race, &["race", "concurr", "deadlock"]),
        (
            Self::Format,
            &[
                "format",
                "parse",
                "json",
                "yaml",
                "encoding",
                "utf-8",
                "utf8",
                "serializ",
                "deserializ",
            ],
        ),
        (
            Self::Config,
            &[
                "config",
                "settings",
                "toml",
                "env var",
                "environment variable",
            ],
        ),
        (
            Self::QuotaLimit,
            &[
                "quota",
                "rate limit",
                "limit exceeded",
                "too many requests",
                "429",
            ],
        ),
        (
            Self::Timeout,
            &["timeout", "timed out", "deadline", "etimedout"],
        ),
    ];

    pub fn classify(content_lower: &str) -> Option<Self> {
        Self::TABLE
            .iter()
            .find(|(_, kws)| kws.iter().any(|k| content_lower.contains(k)))
            .map(|(v, _)| *v)
    }
}

/// 解法類型——用哪種解法繞過。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SolutionType {
    SetEnv,
    FixPath,
    AddFlag,
    RetryBackoff,
    PinVersion,
    ChangeOrder,
    EscapeQuote,
    UseSandbox,
}

impl SolutionType {
    const TABLE: &[(Self, &[&str])] = &[
        (
            Self::SetEnv,
            &[
                "set-env",
                "set env",
                "environment variable",
                "env var",
                "export ",
            ],
        ),
        (
            Self::FixPath,
            &[
                "fix the path",
                "fix-path",
                "fix path",
                "absolute path",
                "relative path",
                "correct path",
            ],
        ),
        (
            Self::AddFlag,
            &["add the flag", "add a flag", "add --", "use --", "flag"],
        ),
        (
            Self::RetryBackoff,
            &[
                "retry",
                "retries",
                "backoff",
                "back-off",
                "back off",
                "exponential",
                "re-attempt",
            ],
        ),
        (
            Self::PinVersion,
            &[
                "pin the version",
                "pin-version",
                "pinned",
                "lock the version",
                "lockfile",
                "lock file",
            ],
        ),
        (
            Self::ChangeOrder,
            &[
                "change the order",
                "change-order",
                "reorder",
                "swap the order",
                "run it first",
                "run first",
                "do it before",
            ],
        ),
        (
            Self::EscapeQuote,
            &[
                "escape",
                "escaped",
                "quoting",
                "quote",
                "single-quote",
                "double-quote",
            ],
        ),
        (Self::UseSandbox, &["sandbox"]),
    ];

    pub fn classify(content_lower: &str) -> Option<Self> {
        Self::TABLE
            .iter()
            .find(|(_, kws)| kws.iter().any(|k| content_lower.contains(k)))
            .map(|(v, _)| *v)
    }
}

/// 適用工具——教訓綁定哪個工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    Cargo,
    Git,
    Node,
    Bash,
    Zsh,
    Tmux,
    Claude,
    Codex,
    Agy,
    Fleet,
    Systemd,
}

impl Tool {
    const TABLE: &[(Self, &[&str])] = &[
        (Self::Cargo, &["cargo"]),
        (Self::Git, &["git"]),
        (Self::Node, &["node"]),
        (Self::Bash, &["bash"]),
        (Self::Zsh, &["zsh"]),
        (Self::Tmux, &["tmux"]),
        (Self::Claude, &["claude"]),
        (Self::Codex, &["codex"]),
        (Self::Agy, &["agy"]),
        (Self::Fleet, &["fleet"]),
        (Self::Systemd, &["systemd"]),
    ];

    pub fn classify(content_lower: &str) -> Option<Self> {
        Self::TABLE
            .iter()
            .find(|(_, kws)| kws.iter().any(|k| content_lower.contains(k)))
            .map(|(v, _)| *v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_takes_first_hit_in_declared_order() {
        // "cargo build test with git" — Topic 先命中 Build（build 在 test 前）。
        assert_eq!(
            Topic::classify("cargo build test with git"),
            Some(Topic::Build)
        );
        assert_eq!(
            Topic::classify("the dispatch queue is broken"),
            Some(Topic::Dispatch)
        );
        assert_eq!(
            Topic::classify("a tmux script for the tool"),
            Some(Topic::OtherTooling)
        );
        assert_eq!(Topic::classify("nothing matched here at all"), None);
    }

    #[test]
    fn classify_returns_none_when_no_keyword_hits() {
        assert_eq!(PitfallType::classify("account alias zeta99"), None);
        assert_eq!(SolutionType::classify("account alias zeta99"), None);
        assert_eq!(Tool::classify("account alias zeta99"), None);
        assert_eq!(PitfallType::classify(""), None);
    }

    #[test]
    fn kebab_case_serde_and_roundtrip() {
        assert_eq!(
            serde_json::to_string(&Topic::OtherTooling).unwrap(),
            "\"other-tooling\""
        );
        assert_eq!(
            serde_json::to_string(&PitfallType::QuotaLimit).unwrap(),
            "\"quota-limit\""
        );
        assert_eq!(
            serde_json::to_string(&SolutionType::RetryBackoff).unwrap(),
            "\"retry-backoff\""
        );
        assert_eq!(
            serde_json::to_string(&SolutionType::EscapeQuote).unwrap(),
            "\"escape-quote\""
        );
        assert_eq!(
            serde_json::to_string(&Tool::Systemd).unwrap(),
            "\"systemd\""
        );

        let t: Topic = serde_json::from_str("\"other-tooling\"").unwrap();
        assert_eq!(t, Topic::OtherTooling);
        let p: PitfallType = serde_json::from_str("\"quota-limit\"").unwrap();
        assert_eq!(p, PitfallType::QuotaLimit);
        let s: SolutionType = serde_json::from_str("\"retry-backoff\"").unwrap();
        assert_eq!(s, SolutionType::RetryBackoff);
        let tl: Tool = serde_json::from_str("\"cargo\"").unwrap();
        assert_eq!(tl, Tool::Cargo);
        assert!(serde_json::from_str::<Topic>("\"not-a-topic\"").is_err());
    }
}
