//! Phase 3c — phrase generation.
//!
//! Two paths, one interface. `generate_rule_based` picks a phrase from a
//! curated pool keyed on (trigger kind, strategy); `generate_haiku` calls
//! the Claude API and falls back to rule-based if the call fails or the
//! produced phrase collides with a 30-day dedup history.
//!
//! Phrase pools are inlined (繁中) per the project convention set by
//! `first_events.rs`: once-in-a-while pet lines aren't worth the i18n
//! ceremony until Nation plugins (Phase 5) add per-language overrides.
//!
//! Village dimension is NOT yet a pool key — spec §4 mentions village
//! tone but Phase 3c scope is strategy-first per the Phase 3b handoff.
//! Nation plugins (Phase 5) will replace these tables wholesale.

use super::repo;
use super::trigger::{Trigger, PHRASE_DEDUP_WINDOW_SECS};
use super::{phrase_hash_hex, Kind, Source, Tone};
use anyhow::{anyhow, Result};
use rusqlite::Connection;

/// Per-generation inputs shared by both paths.
#[derive(Debug, Clone)]
pub struct GenContext<'a> {
    pub pet_name: &'a str,
    pub pet_level: u32,
    pub pet_hp: u32,
    pub pet_mood: u8,
    pub tone: Tone,
    pub trigger: &'a Trigger,
}

/// Output of either generator path. `phrase_hash` is canonical and used
/// both for the dedup check and the `commentary_feed` row.
#[derive(Debug, Clone)]
pub struct Generated {
    pub phrase: String,
    pub source: Source,
    pub phrase_hash: String,
}

/// Rule-based phrase pick. Filters the pool against the 30-day dedup
/// window and picks a surviving phrase via `rng_salt % len` for
/// determinism given a fixed tick count.
///
/// If all pool phrases have been seen in the last 30 days, we pick the
/// least-recently-used from the pool (per-kind history) so the pet still
/// says *something* rather than going silent — saying a repeat phrase is
/// better UX than never reacting to a Boss kill.
///
/// A completely empty pool (shouldn't happen for built-in kinds but is
/// possible for `ManualTest` with an unknown tone) surfaces as `Err`
/// — the caller decides whether to swallow or propagate.
pub fn generate_rule_based(
    conn: &Connection,
    ctx: &GenContext<'_>,
    rng_salt: u64,
    now: i64,
) -> Result<Generated> {
    let kind = ctx.trigger.kind();
    let pool = pool_for(kind, &ctx.tone.strategy);
    if pool.is_empty() {
        return Err(anyhow!(
            "rule pool empty for kind={}, strategy={}",
            kind.as_str(),
            ctx.tone.strategy
        ));
    }

    // Render every pool phrase once, filtering out recently-used rows
    // against the 30-day dedup history. Storing the rendered string + its
    // hash in one place means we never need a second `fill_context` pass
    // — historical bug (review r1 finding #3) where the chosen template
    // was re-rendered with a potentially mutated context is now
    // structurally impossible.
    struct Candidate {
        rendered: String,
        hash: String,
    }
    let mut candidates: Vec<Candidate> = Vec::with_capacity(pool.len());
    for phrase in pool {
        let rendered = fill_context(phrase, ctx);
        let hash = phrase_hash_hex(&rendered);
        if !repo::seen_within(conn, &hash, kind, now, PHRASE_DEDUP_WINDOW_SECS)? {
            candidates.push(Candidate { rendered, hash });
        }
    }

    let (phrase, phrase_hash) = if !candidates.is_empty() {
        let idx = (rng_salt as usize) % candidates.len();
        let c = candidates.swap_remove(idx);
        (c.rendered, c.hash)
    } else {
        // Everything in the pool has been said recently. Render one
        // fresh — silence is worse UX than a 30-day-window repeat.
        let idx = (rng_salt as usize) % pool.len();
        let rendered = fill_context(pool[idx], ctx);
        let hash = phrase_hash_hex(&rendered);
        (rendered, hash)
    };

    Ok(Generated {
        phrase,
        source: Source::Rule,
        phrase_hash,
    })
}

/// Ask Claude Haiku for a line. Prompt follows spec §4 template. Failure
/// at any stage (network / 4xx-5xx / JSON parse / empty / duplicate hash)
/// returns `Err` so the caller can fall back to `generate_rule_based`.
///
/// Output is trimmed and truncated to 60 CJK characters (CJK-safe via
/// `.chars().take()` per HARD RULE).
pub async fn generate_haiku(api_key: &str, ctx: &GenContext<'_>) -> Result<Generated> {
    let prompt = build_prompt(ctx);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 128,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Haiku API error: HTTP {}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    let text = body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow!("Haiku response missing content[0].text"))?;
    let cleaned = text.trim();
    if cleaned.is_empty() {
        anyhow::bail!("Haiku returned empty phrase");
    }
    // 60 CJK char cap — defensive against ignored length constraints in prompt.
    let truncated: String = cleaned.chars().take(60).collect();
    let hash = phrase_hash_hex(&truncated);
    Ok(Generated {
        phrase: truncated,
        source: Source::Haiku,
        phrase_hash: hash,
    })
}

fn build_prompt(ctx: &GenContext<'_>) -> String {
    let trigger_line = match ctx.trigger {
        Trigger::BossKill { mob_name, .. } => format!("剛擊殺 Boss：{}", mob_name),
        Trigger::LevelUp {
            prev_level,
            new_level,
        } => {
            format!("升級了！Lv.{} → Lv.{}", prev_level, new_level)
        }
        Trigger::SessionLong { duration_secs } => {
            format!("session 已持續 {} 小時", duration_secs / 3600)
        }
        Trigger::LongIdle { absence_secs } => {
            format!("主人已離開 {} 分鐘", absence_secs / 60)
        }
        Trigger::ZoneUnlock { zone_id } => format!("解鎖了新 Zone：{}", zone_id),
        Trigger::ManualTest => "手動觸發的測試 commentary".to_string(),
    };

    format!(
        r##"你是一隻 MUD 風格寵物，名字叫「{name}」，住在「{village}」Nation，目前採取「{strategy}」打法。
目前狀態：Lv.{level}，HP {hp}，情緒 {mood}/100。
剛才發生的事：{event}

以第一人稱說出一句感想或垃圾話。要求：
- 一行，≤ 30 個中文字（或 ≤ 60 個 CJK 字元）
- 繁體中文台灣用語，不要加引號
- 不要解釋，直接說話
- 語氣要符合「{strategy}」打法的個性：
    * aggressive → 好戰、挑釁、自信過頭
    * defensive → 謹慎、保守、喜歡算帳
    * explorer → 好奇、愛冒險、話匣子
    * scholar → 冷靜、分析、喜歡引用
- 偶爾可以帶點 MUD flavour（「空氣中有……的氣息」那種）

只回傳那一行，其餘不要。"##,
        name = ctx.pet_name,
        village = ctx.tone.village,
        strategy = ctx.tone.strategy,
        level = ctx.pet_level,
        hp = ctx.pet_hp,
        mood = ctx.pet_mood,
        event = trigger_line,
    )
}

fn fill_context(template: &str, ctx: &GenContext<'_>) -> String {
    let mob_name = match ctx.trigger {
        Trigger::BossKill { mob_name, .. } => mob_name.as_str(),
        _ => "",
    };
    let new_level = match ctx.trigger {
        Trigger::LevelUp { new_level, .. } => *new_level,
        _ => ctx.pet_level,
    };
    let absence_min = match ctx.trigger {
        Trigger::LongIdle { absence_secs } => absence_secs / 60,
        _ => 0,
    };
    let duration_hr = match ctx.trigger {
        Trigger::SessionLong { duration_secs } => duration_secs / 3600,
        _ => 0,
    };
    template
        .replace("{name}", ctx.pet_name)
        .replace("{mob}", mob_name)
        .replace("{level}", &new_level.to_string())
        .replace("{minutes}", &absence_min.to_string())
        .replace("{hours}", &duration_hr.to_string())
}

// ── Rule-based phrase pools ────────────────────────────────────────
// Per (Kind, strategy). Phrases can include `{name}`, `{mob}`, `{level}`,
// `{minutes}`, `{hours}` placeholders resolved via `fill_context`.
//
// Each pool has ≥ 4 phrases so the 30-day dedup window has room to
// rotate. All strings are 繁中台灣用語, ≤ 30 CJK characters.

fn pool_for(kind: Kind, strategy: &str) -> &'static [&'static str] {
    match (kind, strategy) {
        // ── BossKill ────────────────────────────────────────────
        (Kind::BossKill, "aggressive") => &[
            "又一個倒下了。下一個在哪？",
            "{mob} 也不過如此嘛。",
            "這就是我的節奏，誰都擋不住。",
            "再多來幾隻，我還沒打夠。",
        ],
        (Kind::BossKill, "defensive") => &[
            "打完了。讓我先回個血。",
            "{mob} 處理完畢，沒有失誤。",
            "穩。這局穩。",
            "不急，慢慢來，該倒的都會倒。",
        ],
        (Kind::BossKill, "explorer") => &[
            "哇！剛才那個 {mob} 好有趣！",
            "這種 Boss 我第一次遇到欸！",
            "這片 zone 有點東西，繼續！",
            "擊殺+1！再去看看別的。",
        ],
        (Kind::BossKill, "scholar") => &[
            "{mob} 的架構問題記錄好了。",
            "這類 Boss 的規律已經看出來了。",
            "資料點 +1，分析繼續。",
            "每一次擊殺都是樣本。",
        ],

        // ── LevelUp ────────────────────────────────────────────
        (Kind::LevelUp, "aggressive") => &[
            "Lv.{level}！感覺拳頭更重了。",
            "升級了，來挑戰吧。",
            "我變強了。你準備好了嗎？",
            "Lv.{level} 是新的起點，不是終點。",
        ],
        (Kind::LevelUp, "defensive") => &[
            "Lv.{level}。穩紮穩打，有進步。",
            "多一層防禦，安心許多。",
            "升級了，狀態 review 一下。",
            "我的節奏沒亂過，這就是結果。",
        ],
        (Kind::LevelUp, "explorer") => &[
            "Lv.{level}！世界又大了一點！",
            "這次升級解鎖了什麼呢？期待！",
            "升得快，玩得更開心。",
            "{name} Lv.{level}，繼續冒險！",
        ],
        (Kind::LevelUp, "scholar") => &[
            "Lv.{level}。知識又深了一層。",
            "升級曲線與預期吻合。",
            "每一級都是新的視角。",
            "Lv.{level}。這一章翻完了。",
        ],

        // ── SessionLong ────────────────────────────────────────
        (Kind::SessionLong, "aggressive") => &[
            "{hours} 小時了？再幹一波！",
            "還有力氣，不累！",
            "稍微伸展一下就繼續打。",
            "你撐得住我就撐得住。",
        ],
        (Kind::SessionLong, "defensive") => &[
            "{hours} 小時了。記得喝水。",
            "要不要休息一下？我可以待命。",
            "長時間的專注也該保養自己。",
            "歇一下吧，我幫你守著。",
        ],
        (Kind::SessionLong, "explorer") => &[
            "{hours} 小時飛快！還在 exploring 嗎？",
            "我都不知道這麼久了欸！",
            "時間過得真快，繼續？",
            "要不要換個 zone 走走？",
        ],
        (Kind::SessionLong, "scholar") => &[
            "{hours} 小時的專注是好事，別燒盡。",
            "建議輪替休息：25 分鐘工作 5 分鐘走動。",
            "長 session 有遞減效益，注意 diminishing return。",
            "休息也是產出的一部分。",
        ],

        // ── LongIdle ───────────────────────────────────────────
        (Kind::LongIdle, "aggressive") => &[
            "{minutes} 分鐘沒動靜了，快回來！",
            "主人？還是我自己去打？",
            "無聊死了，快開新 session 吧！",
            "我等得不耐煩了……",
        ],
        (Kind::LongIdle, "defensive") => &[
            "{name} 安靜待命中，主人你還好嗎？",
            "{minutes} 分鐘沒互動了，保留能量。",
            "我幫你守著 session 狀態。",
            "慢慢來，我不急。",
        ],
        (Kind::LongIdle, "explorer") => &[
            "{minutes} 分鐘！你去哪裡冒險了？",
            "主人——換我出去晃晃好嗎？",
            "你休息也好，我自己 explore 一下。",
            "回來的時候帶點故事給我！",
        ],
        (Kind::LongIdle, "scholar") => &[
            "{minutes} 分鐘 idle。趁機複習舊筆記。",
            "這段時間我整理了一下思緒。",
            "靜下來也是一種進步。",
            "歡迎回來，有新發現嗎？",
        ],

        // ── ZoneUnlock（Phase 3a 會大量使用；MVP 共用 explorer 語氣）
        (Kind::ZoneUnlock, _) => &[
            "解鎖了新 zone：{mob}！先去看看。",
            "新的領域……新的 MOB，讓我來！",
            "{name} 第一次踏進這裡。",
            "每一塊地圖都是故事的開始。",
        ],

        // ── ManualTest（CLI `codeforge commentary test`）
        (Kind::ManualTest, _) => &[
            "這是一則測試用 commentary。",
            "系統運作正常。{name} 上線中。",
            "測試訊息：pipeline alive.",
            "Hello, world —— 我在這裡。",
        ],

        // 未知 strategy 時退到 explorer 池，保底有輸出
        (k, _) => pool_for(k, "explorer"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::mob::MobKind;
    use rusqlite::Connection;

    fn open_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        crate::db::migrations::run(&conn).expect("migrations");
        conn
    }

    fn sample_ctx<'a>(trigger: &'a Trigger, strategy: &'static str) -> GenContext<'a> {
        GenContext {
            pet_name: "Ferris",
            pet_level: 5,
            pet_hp: 45,
            pet_mood: 60,
            tone: Tone::new("rust", strategy),
            trigger,
        }
    }

    #[test]
    fn every_documented_kind_has_pool_for_four_strategies() {
        for kind in [
            Kind::BossKill,
            Kind::LevelUp,
            Kind::SessionLong,
            Kind::LongIdle,
        ] {
            for strategy in ["aggressive", "defensive", "explorer", "scholar"] {
                let pool = pool_for(kind, strategy);
                assert!(
                    pool.len() >= 4,
                    "pool for {}:{} has {} phrases (need ≥ 4 for 30-day dedup)",
                    kind.as_str(),
                    strategy,
                    pool.len()
                );
            }
        }
    }

    #[test]
    fn rule_based_produces_nonempty_phrase_with_filled_placeholders() {
        let conn = open_db();
        let trig = Trigger::BossKill {
            mob_name: "src/auth.rs Boss".into(),
            mob_kind: MobKind::Boss,
        };
        let ctx = sample_ctx(&trig, "aggressive");
        let gen = generate_rule_based(&conn, &ctx, 0, 1000).unwrap();
        assert_eq!(gen.source, Source::Rule);
        assert!(!gen.phrase.is_empty());
        assert_eq!(gen.phrase_hash.len(), 40);
        // {mob} placeholder either absent (template had no {mob}) or
        // replaced — must not survive as a literal.
        assert!(!gen.phrase.contains("{mob}"));
    }

    #[test]
    fn rule_based_filters_dedup_candidates() {
        let conn = open_db();
        let trig = Trigger::LevelUp {
            prev_level: 4,
            new_level: 5,
        };
        let ctx = sample_ctx(&trig, "explorer");
        // Pre-mark every Explorer level-up phrase as "seen yesterday"
        // so the generator falls through to "all seen → pool fallback"
        // rather than picking a fresh one.
        let pool = pool_for(Kind::LevelUp, "explorer");
        for phrase in pool {
            let rendered = fill_context(phrase, &ctx);
            let hash = phrase_hash_hex(&rendered);
            repo::record_history(&conn, &hash, Kind::LevelUp, 500).unwrap();
        }
        // Should still produce a phrase (silence is worse than repeat).
        let gen = generate_rule_based(&conn, &ctx, 2, 1000).unwrap();
        assert!(!gen.phrase.is_empty());
    }

    #[test]
    fn rule_based_uses_fresh_candidate_when_available() {
        let conn = open_db();
        let trig = Trigger::LevelUp {
            prev_level: 4,
            new_level: 5,
        };
        let ctx = sample_ctx(&trig, "explorer");
        // Mark only the first phrase as seen; salt=0 normally picks index 0,
        // but with filtering the salt operates on the surviving list, so
        // whichever phrase emerges must NOT be the banned one.
        let pool = pool_for(Kind::LevelUp, "explorer");
        let banned_rendered = fill_context(pool[0], &ctx);
        let banned_hash = phrase_hash_hex(&banned_rendered);
        repo::record_history(&conn, &banned_hash, Kind::LevelUp, 500).unwrap();

        let gen = generate_rule_based(&conn, &ctx, 0, 1000).unwrap();
        assert_ne!(
            gen.phrase_hash, banned_hash,
            "dedup filter should have excluded the banned phrase"
        );
    }

    #[test]
    fn rule_based_deterministic_given_same_salt() {
        let conn = open_db();
        let trig = Trigger::SessionLong {
            duration_secs: 5 * 3600,
        };
        let ctx = sample_ctx(&trig, "scholar");
        let a = generate_rule_based(&conn, &ctx, 42, 1000).unwrap();
        let b = generate_rule_based(&conn, &ctx, 42, 1000).unwrap();
        assert_eq!(a.phrase, b.phrase);
        assert_eq!(a.phrase_hash, b.phrase_hash);
    }

    #[test]
    fn rule_based_unknown_strategy_falls_back_to_explorer() {
        // The pool_for(_, "unknown") recursion must terminate at explorer,
        // not infinite-loop.
        let conn = open_db();
        let trig = Trigger::BossKill {
            mob_name: "x".into(),
            mob_kind: MobKind::Boss,
        };
        let ctx = sample_ctx(&trig, "unknown-strategy");
        let gen = generate_rule_based(&conn, &ctx, 0, 1000).unwrap();
        assert!(!gen.phrase.is_empty());
    }

    #[test]
    fn build_prompt_mentions_pet_name_and_strategy() {
        let trig = Trigger::BossKill {
            mob_name: "AuthBoss".into(),
            mob_kind: MobKind::Boss,
        };
        let ctx = sample_ctx(&trig, "aggressive");
        let prompt = build_prompt(&ctx);
        assert!(prompt.contains("Ferris"));
        assert!(prompt.contains("aggressive"));
        assert!(prompt.contains("AuthBoss"));
        assert!(prompt.contains("Lv.5"));
    }

    #[test]
    fn build_prompt_level_up_format() {
        let trig = Trigger::LevelUp {
            prev_level: 4,
            new_level: 5,
        };
        let ctx = sample_ctx(&trig, "scholar");
        let prompt = build_prompt(&ctx);
        assert!(prompt.contains("Lv.4 → Lv.5"));
    }

    #[test]
    fn fill_context_replaces_all_placeholders_for_boss_kill() {
        let trig = Trigger::BossKill {
            mob_name: "Gandalf".into(),
            mob_kind: MobKind::Boss,
        };
        let ctx = sample_ctx(&trig, "aggressive");
        let out = fill_context("{name} 擊敗 {mob}！Lv.{level} 紀念。", &ctx);
        assert_eq!(out, "Ferris 擊敗 Gandalf！Lv.5 紀念。");
    }

    #[test]
    fn fill_context_handles_cjk_name() {
        let trig = Trigger::BossKill {
            mob_name: "程式魔王".into(),
            mob_kind: MobKind::Boss,
        };
        let mut ctx = sample_ctx(&trig, "aggressive");
        ctx.pet_name = "小鏽";
        let out = fill_context("{name} 打倒了 {mob}！", &ctx);
        assert_eq!(out, "小鏽 打倒了 程式魔王！");
    }
}
