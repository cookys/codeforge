//! 知識管線段 1——`codeforge lessons candidates --json`
//!（plan `doc/plans/2026-11-23-knowledge-pipeline.md` 第 4 節段 1–2、第 10.2 節 T1；
//! T2 機器閘見 [`gate`]）。
//!
//! 對當前專案 store 的 L0 唯讀掃描：只保留 `SessionDigest`、本席（seat ==
//! `self_seat` 且在席位白名單）、白名單 model、metadata 齊全的 signal；用受限
//! 詞彙表分類 content，輸出候選 JSON——只有受限詞彙欄位與證據指標，絕不輸出
//! `content`、路徑、seat 原文、model 原文或任何 signal 文字。
//!
//! T2：每筆候選在去重前過機器閘（[`gate::check`]，source＝該筆 signal 的
//! `content`）；命中就當場丟棄、只累計規則 id 次數；次數寫
//! `$CODEFORGE_DATA_DIR/lessons-gate.json`（只有 `{"<rule-id>": <count>}`，
//! 無命中為 `{}`，絕無命中文字）。stdout 輸出不變（仍只有候選陣列）。

pub mod candidates;
pub mod gate;
pub mod vocab;

use anyhow::Result;

use crate::db;

/// 沒帶 `--json` 時印出的繁中說明（一行；exit 0）。
const NO_JSON_NOTE: &str = "codeforge lessons candidates：候選串流供知識管線段 2（distill-scan）消費，請加上 --json 取得 JSON 輸出。";

/// CLI 入口：`codeforge lessons candidates [--json]`。
pub fn run(ctx: &db::Context, json: bool) -> Result<()> {
    if !json {
        println!("{NO_JSON_NOTE}");
        return Ok(());
    }
    println!("{}", render(ctx)?);
    Ok(())
}

/// `--json` 的輸出（緊湊 JSON 陣列字串）。設定檔不存在／解析失敗／任一清單
/// 為空 → fail-closed 輸出 `[]`（T1 行為不變，且不寫 lessons-gate.json）。
/// T2：每筆候選過機器閘；命中次數寫 `$CODEFORGE_DATA_DIR/lessons-gate.json`
///（只有 `{"<rule-id>": <count>}`，無命中覆寫 `{}`）。stdout 不變。
pub(crate) fn render(ctx: &db::Context) -> Result<String> {
    let Some(cfg) = candidates::load_config(&candidates::config_path()) else {
        return Ok("[]".to_string());
    };
    let deny = gate::DenyList::load_default();
    let (cands, counts) = candidates::collect(ctx, &cfg, deny.as_ref());
    gate::write_counts(&gate::gate_path(), &counts)?;
    Ok(serde_json::to_string(&cands)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::candidates::{
        collect, config_path, lane_hash_of, load_config, Candidate, Evidence, LessonsConfig,
    };
    use crate::lessons::gate::{self, DenyList, RuleId};
    use crate::lessons::vocab::{PitfallType, SolutionType, Tool, Topic};
    use crate::memory::l0::{Signal, SignalSource};
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::path::Path;
    use tempfile::TempDir;

    /// 正例 content：命中 Topic=Test、Pitfall=Timeout、Solution=RetryBackoff、
    /// Tool=Cargo，且含帳號別名樣式與 ≥20 字元句子（供逐窗洩漏檢查）。
    const GOOD_CONTENT: &str = "cargo test timed out twice during the nightly integration run; fixed by adding retry with backoff before publish — account alias zeta99 owns this lane";

    fn good_config() -> LessonsConfig {
        LessonsConfig {
            self_seat: "alpha.local".to_string(),
            seats: vec!["alpha.local".to_string(), "bravo.local".to_string()],
            models: vec!["claude-sonnet-4-5".to_string()],
            repos: vec!["demo-repo".to_string()],
        }
    }

    /// T2：機器閘要求 deny 存在；在記憶體用明顯假的詞（不進 repo），
    /// 不讀環境、不碰真 HOME。詞不會出現在任何測試 content 裡。
    fn deny() -> DenyList {
        DenyList::new(vec!["aliasfoo".into(), "hostbar".into()])
    }

    fn signal(
        content: &str,
        source: SignalSource,
        seat: Option<&str>,
        model: Option<&str>,
        session_id: Option<&str>,
    ) -> Signal {
        let mut s = Signal::new(content.to_string(), source);
        s.session_id = session_id.map(str::to_string);
        s.time_start = Some("2026-11-26T01:00:00Z".to_string());
        s.time_end = Some("2026-11-26T01:45:00Z".to_string());
        s.seat = seat.map(str::to_string);
        s.model = model.map(str::to_string);
        s
    }

    /// 在 tempdir 建 `<repo>/.codeforge/signals/*.jsonl` 的 fake store，回傳 ctx。
    fn store_with(repo_name: &str, tmp: &Path, signals: &[Signal]) -> crate::db::Context {
        let store = tmp.join(repo_name).join(".codeforge");
        let sig_dir = store.join("signals");
        std::fs::create_dir_all(&sig_dir).unwrap();
        let body = signals
            .iter()
            .map(|s| serde_json::to_string(s).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(sig_dir.join("2026-11-26.jsonl"), body).unwrap();
        crate::db::Context {
            project_dir: store,
            brain_dir: tmp.join("brain"),
            db_path: tmp.join("state.db"),
        }
    }

    fn good_signal(session_id: &str) -> Signal {
        signal(
            GOOD_CONTENT,
            SignalSource::SessionDigest,
            Some("alpha.local"),
            Some("claude-sonnet-4-5"),
            Some(session_id),
        )
    }

    #[test]
    fn positive_single_candidate_exact_fields() {
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[good_signal("sess-0001")]);
        let (out, _counts) = collect(&ctx, &good_config(), Some(&deny()));
        assert_eq!(out.len(), 1, "恰一筆候選");

        let expect_lane: String = {
            let d = Sha256::digest(b"alpha.local");
            d.iter().take(6).map(|b| format!("{b:02x}")).collect()
        };
        assert_eq!(lane_hash_of("alpha.local"), expect_lane);
        assert_eq!(expect_lane.len(), 12);
        assert!(expect_lane.chars().all(|c| c.is_ascii_hexdigit()));

        let c = &out[0];
        assert_eq!(c.topic, Topic::Test);
        assert_eq!(c.pitfall_type, PitfallType::Timeout);
        assert_eq!(c.solution_type, SolutionType::RetryBackoff);
        assert_eq!(c.tool, Tool::Cargo);
        assert_eq!(c.repo, "demo-repo");
        assert_eq!(c.evidence.session_id, "sess-0001");
        assert_eq!(c.evidence.time_start, "2026-11-26T01:00:00Z");
        assert_eq!(c.evidence.time_end, "2026-11-26T01:45:00Z");
        assert_eq!(c.evidence.lane_hash, expect_lane);

        // 欄位集合恰為六個、evidence 恰四個鍵
        let v: serde_json::Value = serde_json::to_value(c).unwrap();
        let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "evidence",
                "pitfall_type",
                "repo",
                "solution_type",
                "tool",
                "topic"
            ]
        );
        let mut ekeys: Vec<String> = v["evidence"].as_object().unwrap().keys().cloned().collect();
        ekeys.sort();
        assert_eq!(
            ekeys,
            vec!["lane_hash", "session_id", "time_end", "time_start"]
        );
    }

    // 反向探針：每一種都必須輸出空（collect 回 []）

    #[test]
    fn negative_a_alias_style_content_without_classification_hit() {
        // 含帳號別名樣式與 ≥20 字元句子，但四個詞彙表全沒命中
        let content = "account alias zeta99 was used for the quarterly review meeting";
        let tmp = TempDir::new().unwrap();
        let sig = signal(
            content,
            SignalSource::SessionDigest,
            Some("alpha.local"),
            Some("claude-sonnet-4-5"),
            Some("sess-0001"),
        );
        let ctx = store_with("demo-repo", tmp.path(), &[sig]);
        assert!(collect(&ctx, &good_config(), Some(&deny())).0.is_empty());
    }

    #[test]
    fn negative_b_seat_is_another_seat() {
        let sig = signal(
            GOOD_CONTENT,
            SignalSource::SessionDigest,
            Some("bravo.local"),
            Some("claude-sonnet-4-5"),
            Some("sess-0001"),
        );
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[sig]);
        assert!(collect(&ctx, &good_config(), Some(&deny())).0.is_empty());
    }

    #[test]
    fn negative_c_seat_not_in_whitelist() {
        let sig = signal(
            GOOD_CONTENT,
            SignalSource::SessionDigest,
            Some("charlie.local"),
            Some("claude-sonnet-4-5"),
            Some("sess-0001"),
        );
        // self_seat 是 charlie（seat == self_seat 成立），但白名單沒有它
        let mut cfg = good_config();
        cfg.self_seat = "charlie.local".to_string();
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[sig]);
        assert!(collect(&ctx, &cfg, Some(&deny())).0.is_empty());
    }

    #[test]
    fn negative_d_seat_empty() {
        let sig = signal(
            GOOD_CONTENT,
            SignalSource::SessionDigest,
            Some(""),
            Some("claude-sonnet-4-5"),
            Some("sess-0001"),
        );
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[sig]);
        assert!(collect(&ctx, &good_config(), Some(&deny())).0.is_empty());
        // seat 完全缺席（舊格式 L0）也丟棄
        let sig2 = signal(
            GOOD_CONTENT,
            SignalSource::SessionDigest,
            None,
            Some("claude-sonnet-4-5"),
            Some("sess-0002"),
        );
        let tmp2 = TempDir::new().unwrap();
        let ctx2 = store_with("demo-repo", tmp2.path(), &[sig2]);
        assert!(collect(&ctx2, &good_config(), Some(&deny())).0.is_empty());
    }

    #[test]
    fn negative_e_non_claude_model() {
        let sig = signal(
            GOOD_CONTENT,
            SignalSource::SessionDigest,
            Some("alpha.local"),
            Some("qwen-x"),
            Some("sess-0001"),
        );
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[sig]);
        assert!(collect(&ctx, &good_config(), Some(&deny())).0.is_empty());
    }

    #[test]
    fn negative_f_source_not_session_digest() {
        let sig = signal(
            GOOD_CONTENT,
            SignalSource::Manual,
            Some("alpha.local"),
            Some("claude-sonnet-4-5"),
            Some("sess-0001"),
        );
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[sig]);
        assert!(collect(&ctx, &good_config(), Some(&deny())).0.is_empty());
    }

    /// (g) lessons.toml 不存在 → 輸出 `[]`（經 render 的完整路徑）
    #[test]
    fn negative_g_missing_lessons_toml_outputs_empty_array() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let store = tmp.path().join("demo-repo").join(".codeforge");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(store.join("signals")).unwrap();
        let sig = good_signal("sess-0001");
        std::fs::write(
            store.join("signals").join("2026-11-26.jsonl"),
            serde_json::to_string(&sig).unwrap() + "\n",
        )
        .unwrap();
        let _env = EnvGuard::set(&data, &store);
        let ctx = crate::db::Context::load().unwrap();
        assert!(render(&ctx).unwrap() == "[]");
    }

    /// 設定檔存在但任一清單為空 → 同樣 fail-closed 回 `None`
    #[test]
    fn load_config_fail_closed_on_missing_or_empty_list() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("lessons.toml");
        assert!(load_config(&missing).is_none());

        let p = tmp.path().join("lessons.toml");
        std::fs::write(
            &p,
            "self_seat = \"alpha.local\"\nseats = []\nmodels = [\"claude-sonnet-4-5\"]\nrepos = [\"demo-repo\"]\n",
        )
        .unwrap();
        assert!(load_config(&p).is_none());
        std::fs::write(
            &p,
            "self_seat = \"alpha.local\"\nseats = [\"alpha.local\"]\nmodels = [\"claude-sonnet-4-5\"]\nrepos = [\"demo-repo\"]\n",
        )
        .unwrap();
        let cfg = load_config(&p).unwrap();
        assert_eq!(cfg.self_seat, "alpha.local");
    }

    /// repo 不在 repos 清單 → 整個 store 不產候選
    #[test]
    fn repo_outside_list_produces_nothing() {
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("other-repo", tmp.path(), &[good_signal("sess-0001")]);
        assert!(collect(&ctx, &good_config(), Some(&deny())).0.is_empty());
    }

    /// 去重：同 (topic,pitfall,solution,tool,repo,session_id) 只留一筆；
    /// 不同 session 仍是不同筆。
    #[test]
    fn dedup_on_normalized_key_plus_session_id() {
        let tmp = TempDir::new().unwrap();
        let ctx = store_with(
            "demo-repo",
            tmp.path(),
            &[
                good_signal("sess-0001"),
                good_signal("sess-0001"),
                good_signal("sess-0002"),
            ],
        );
        let (out, _counts) = collect(&ctx, &good_config(), Some(&deny()));
        assert_eq!(out.len(), 2);
        // 讀序保留：重複的 sess-0001 留第一筆，sess-0002 是另一筆
        assert_eq!(out[0].evidence.session_id, "sess-0001");
        assert_eq!(out[1].evidence.session_id, "sess-0002");
    }

    /// 洩漏檢查：輸出 JSON 序列化後不含 L0 content 任一 ≥20 字元子字串（逐窗）。
    #[test]
    fn output_never_carries_ge20_content_substrings() {
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[good_signal("sess-0001")]);
        let (out, _counts) = collect(&ctx, &good_config(), Some(&deny()));
        let json = serde_json::to_string(&out).unwrap();
        let chars: Vec<char> = GOOD_CONTENT.chars().collect();
        let mut windows = 0;
        for i in 0..=chars.len() - 20 {
            let w: String = chars[i..i + 20].iter().collect();
            assert!(!json.contains(&w), "洩漏 ≥20 字元片段：{w}");
            windows += 1;
        }
        assert!(windows > 0, "正例 content 太短，逐窗檢查沒跑到");
        // 連單一字串本身也不得出現
        assert!(!json.contains(GOOD_CONTENT));
    }

    /// schema：輸出可反序列化進 `deny_unknown_fields` 的 struct，且值為
    /// kebab-case 字串。
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct CandidateSchema {
        topic: String,
        pitfall_type: String,
        solution_type: String,
        tool: String,
        repo: String,
        evidence: EvidenceSchema,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EvidenceSchema {
        session_id: String,
        time_start: String,
        time_end: String,
        lane_hash: String,
    }

    #[test]
    fn output_matches_deny_unknown_fields_schema() {
        let tmp = TempDir::new().unwrap();
        let ctx = store_with("demo-repo", tmp.path(), &[good_signal("sess-0001")]);
        let (out, _counts) = collect(&ctx, &good_config(), Some(&deny()));
        let json = serde_json::to_string(&out).unwrap();
        let parsed: Vec<CandidateSchema> = serde_json::from_str(&json).unwrap_or_else(|e| {
            panic!("schema 反序列化失敗（deny_unknown_fields 應通過）：{e}; json={json}")
        });
        assert_eq!(parsed.len(), 1);
        let parsed = &parsed[0];
        assert_eq!(parsed.topic, "test");
        assert_eq!(parsed.pitfall_type, "timeout");
        assert_eq!(parsed.solution_type, "retry-backoff");
        assert_eq!(parsed.tool, "cargo");
        assert_eq!(parsed.repo, "demo-repo");
        assert_eq!(parsed.evidence.session_id, "sess-0001");
        assert_eq!(parsed.evidence.time_start, "2026-11-26T01:00:00Z");
        assert_eq!(parsed.evidence.time_end, "2026-11-26T01:45:00Z");
        assert_eq!(parsed.evidence.lane_hash, lane_hash_of("alpha.local"));

        // Candidate／Evidence 本身也是 deny_unknown_fields 的 schema
        let cands: Vec<Candidate> = serde_json::from_str(&json).unwrap();
        assert_eq!(cands.len(), 1);
        let ev: Evidence = serde_json::from_value(serde_json::json!({
            "session_id": "s",
            "time_start": "t0",
            "time_end": "t1",
            "lane_hash": "h"
        }))
        .unwrap();
        assert!(serde_json::from_value::<Evidence>(serde_json::json!({
            "session_id": "s",
            "time_start": "t0",
            "time_end": "t1",
            "lane_hash": "h",
            "content": "leak"
        }))
        .is_err());
        assert!(
            serde_json::from_value::<Candidate>(serde_json::json!({
                "topic": "test",
                "pitfall_type": "timeout",
                "solution_type": "retry-backoff",
                "tool": "cargo",
                "repo": "demo-repo",
                "evidence": ev,
                "content": "leak"
            }))
            .is_err(),
            "未知欄位（content）必須被 deny_unknown_fields 擋掉"
        );
    }

    /// 沒帶 --json 的說明：一行繁中、exit 0（run 回 Ok）。
    #[test]
    fn non_json_note_is_one_line() {
        assert!(!NO_JSON_NOTE.contains('\n'));
        assert!(NO_JSON_NOTE.contains("--json"));
    }

    /// 環境變數行為：CODEFORGE_DATA_DIR 決定 lessons.toml 位置、
    /// CODEFORGE_DIR 決定 store 位置（tempdir 沙箱，不碰真 HOME）。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 設定兩個環境變數、離場時還原原值（set_var 是全程序的）。
    struct EnvGuard {
        data_dir: Option<std::ffi::OsString>,
        cf_dir: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(data_dir: &Path, cf_dir: &Path) -> Self {
            let prev_data = std::env::var_os("CODEFORGE_DATA_DIR");
            let prev_cf = std::env::var_os("CODEFORGE_DIR");
            std::env::set_var("CODEFORGE_DATA_DIR", data_dir);
            std::env::set_var("CODEFORGE_DIR", cf_dir);
            Self {
                data_dir: prev_data,
                cf_dir: prev_cf,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.data_dir {
                Some(v) => std::env::set_var("CODEFORGE_DATA_DIR", v),
                None => std::env::remove_var("CODEFORGE_DATA_DIR"),
            }
            match &self.cf_dir {
                Some(v) => std::env::set_var("CODEFORGE_DIR", v),
                None => std::env::remove_var("CODEFORGE_DIR"),
            }
        }
    }

    #[test]
    fn config_path_honors_codeforge_data_dir() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let store = tmp.path().join("demo-repo").join(".codeforge");
        let _env = EnvGuard::set(&data, &store);
        assert_eq!(config_path(), data.join("lessons.toml"));
    }

    #[test]
    fn end_to_end_render_via_env_dirs() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let store = tmp.path().join("demo-repo").join(".codeforge");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(store.join("signals")).unwrap();
        let sig = good_signal("sess-0001");
        std::fs::write(
            store.join("signals").join("2026-11-26.jsonl"),
            serde_json::to_string(&sig).unwrap() + "\n",
        )
        .unwrap();
        std::fs::write(
            data.join("lessons.toml"),
            "self_seat = \"alpha.local\"\nseats = [\"alpha.local\", \"bravo.local\"]\nmodels = [\"claude-sonnet-4-5\"]\nrepos = [\"demo-repo\"]\n",
        )
        .unwrap();
        // T2 機器閘 fail-closed：無 lessons-deny.txt 會把候選全丟（no-deny-list）
        std::fs::write(data.join("lessons-deny.txt"), "aliasfoo\n").unwrap();
        let _env = EnvGuard::set(&data, &store);
        let ctx = crate::db::Context::load().unwrap();
        assert_eq!(ctx.project_dir, store);
        let out = render(&ctx).unwrap();
        let v: Vec<Candidate> = serde_json::from_str(&out).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].repo, "demo-repo");
    }

    /// T2 collect 整合：一筆合格、一筆 session_id 含 `/home/`（`/home/` 後接空白，
    /// 不構成三段位址，只命中 abs-path）→ 結果只剩一筆、計數 `{"abs-path":1}`；
    /// render stdout 仍只有候選陣列；lessons-gate.json 只含規則 id、不含 `/home/`。
    #[test]
    fn gate_integration_collect_drops_and_writes_counts() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let store = tmp.path().join("demo-repo").join(".codeforge");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(store.join("signals")).unwrap();
        let good = good_signal("sess-0001");
        let mut bad = good_signal("sess-0002");
        bad.session_id = Some("sess-0002 /home/ logs".to_string());
        let body = [
            serde_json::to_string(&good).unwrap(),
            serde_json::to_string(&bad).unwrap(),
        ]
        .join("\n")
            + "\n";
        std::fs::write(store.join("signals").join("2026-11-26.jsonl"), body).unwrap();
        std::fs::write(
            data.join("lessons.toml"),
            "self_seat = \"alpha.local\"\nseats = [\"alpha.local\", \"bravo.local\"]\nmodels = [\"claude-sonnet-4-5\"]\nrepos = [\"demo-repo\"]\n",
        )
        .unwrap();
        std::fs::write(
            data.join("lessons-deny.txt"),
            "# sandbox fake terms\naliasfoo\nhostbar\n",
        )
        .unwrap();
        let _env = EnvGuard::set(&data, &store);
        let ctx = crate::db::Context::load().unwrap();
        let cfg = candidates::load_config(&candidates::config_path()).unwrap();
        let deny = gate::DenyList::load_default().unwrap();

        let (cands, counts) = candidates::collect(&ctx, &cfg, Some(&deny));
        assert_eq!(cands.len(), 1, "一筆合格、一筆被閘掉");
        assert_eq!(cands[0].evidence.session_id, "sess-0001");
        assert_eq!(counts.get(&RuleId::AbsPath), Some(&1));
        assert_eq!(counts.len(), 1, "只累計命中的規則");

        // render：stdout 輸出不變（仍只有候選陣列）
        let out = render(&ctx).unwrap();
        let v: Vec<Candidate> = serde_json::from_str(&out).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].evidence.session_id, "sess-0001");

        // 命中次數寫 lessons-gate.json：只有 {"abs-path":1}、絕不含命中文字
        let gate_file = data.join("lessons-gate.json");
        let gate_body = std::fs::read_to_string(&gate_file).unwrap();
        assert_eq!(gate_body, "{\"abs-path\":1}");
        assert!(!gate_body.contains("/home/"));
    }

    /// T2：設定檔不存在（T1 fail-closed）→ render `[]`，且不寫 lessons-gate.json。
    #[test]
    fn missing_config_writes_no_gate_file() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let store = tmp.path().join("demo-repo").join(".codeforge");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(store.join("signals")).unwrap();
        let sig = good_signal("sess-0001");
        std::fs::write(
            store.join("signals").join("2026-11-26.jsonl"),
            serde_json::to_string(&sig).unwrap() + "\n",
        )
        .unwrap();
        let _env = EnvGuard::set(&data, &store);
        let ctx = crate::db::Context::load().unwrap();
        assert_eq!(render(&ctx).unwrap(), "[]");
        assert!(
            !data.join("lessons-gate.json").exists(),
            "無設定檔 → 不寫閘檔"
        );
    }

    /// T2：全數合格（無命中）→ lessons-gate.json 覆寫成 `{}`。
    #[test]
    fn no_hits_overwrites_gate_file_with_empty_object() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new().unwrap();
        let data = tmp.path().join("data");
        let store = tmp.path().join("demo-repo").join(".codeforge");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(store.join("signals")).unwrap();
        let sig = good_signal("sess-0001");
        std::fs::write(
            store.join("signals").join("2026-11-26.jsonl"),
            serde_json::to_string(&sig).unwrap() + "\n",
        )
        .unwrap();
        std::fs::write(
            data.join("lessons.toml"),
            "self_seat = \"alpha.local\"\nseats = [\"alpha.local\", \"bravo.local\"]\nmodels = [\"claude-sonnet-4-5\"]\nrepos = [\"demo-repo\"]\n",
        )
        .unwrap();
        std::fs::write(data.join("lessons-deny.txt"), "aliasfoo\n").unwrap();
        let _env = EnvGuard::set(&data, &store);
        let ctx = crate::db::Context::load().unwrap();
        let out = render(&ctx).unwrap();
        let v: Vec<Candidate> = serde_json::from_str(&out).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(
            std::fs::read_to_string(data.join("lessons-gate.json")).unwrap(),
            "{}"
        );
    }

    /// 唯讀保證：跑完 collect 後 store 原封不動（不推進 cursor、不寫檔）。
    #[test]
    fn collect_is_read_only() {
        let tmp = TempDir::new().unwrap();
        let store = tmp.path().join("demo-repo").join(".codeforge");
        let sig = good_signal("sess-0001");
        let body = serde_json::to_string(&sig).unwrap() + "\n";
        std::fs::create_dir_all(store.join("signals")).unwrap();
        std::fs::write(store.join("signals").join("2026-11-26.jsonl"), &body).unwrap();
        let ctx = crate::db::Context {
            project_dir: store,
            brain_dir: tmp.path().join("brain"),
            db_path: tmp.path().join("state.db"),
        };
        collect(&ctx, &good_config(), Some(&deny()));
        assert_eq!(
            std::fs::read_to_string(ctx.project_dir.join("signals").join("2026-11-26.jsonl"))
                .unwrap(),
            body
        );
        // 沒建 db、沒建 log.md、沒建任何新檔
        assert!(!ctx.db_path.exists());
        assert!(!ctx.project_dir.join("log.md").exists());
        let entries: Vec<String> = std::fs::read_dir(&ctx.project_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["signals"]);
    }
}
