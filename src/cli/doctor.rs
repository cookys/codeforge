//! `codeforge doctor` — 全腦健康診斷面板。
//!
//! 主動跑一次前景 probe（~2s），讀其餘維度，以正體中文人話列出：
//! - 本地腦：L1 active count + store 歷史
//! - 央腦：opt-in 狀態、即時 probe 結果（outcome/latency/status）、
//!   上次 probe 快取、上次 ship、queue 深度與最舊一筆
//! - 連線設定：base_url
//!
//! 黃/灰態附 next-step 操作建議。

use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db;
use crate::memory::l1;
use crate::mnemos::config::MnemosConfig;
use crate::mnemos::health::{
    central_light, fresh_enough, local_light, probe_now, queue_degraded, queue_info, read_liveness,
    read_ship, CentralLight, LocalLight, ProbeOutcome,
};

/// doctor 輸入維度（純資料結構，方便 render_doctor 純函式測試）。
pub struct DoctorInput {
    /// L1 active 概念數量
    pub l1_active: usize,
    /// store/concepts/ 目錄是否存在（曾 dream 過）
    pub has_store_history: bool,
    /// Mnemos opt-in 狀態
    pub opted_in: bool,
    /// 即時前景 probe 結果：(outcome, latency_ms, http_status)
    pub live_probe: (ProbeOutcome, Option<u32>, Option<u16>),
    /// 上次 probe 快取（含 last_probe_at）
    pub last_probe_at: Option<i64>,
    /// 上次 ship 時間戳（unix sec）
    pub last_ship_at: Option<i64>,
    /// 上次 ship 是否成功
    pub last_ship_ok: Option<bool>,
    /// queue 深度
    pub queue_count: usize,
    /// queue 最舊一筆 age（秒）
    pub queue_oldest_age: Option<u64>,
    /// 央腦燈號（已由 run() 計算，render_doctor 不再讀磁碟）
    pub central: CentralLight,
    /// Mnemos base_url
    pub base_url: String,
    /// 目前 unix 時間（供相對時間計算）
    pub now: i64,
}

/// 純組裝函式：吃已算好的 DoctorInput，回傳格式化字串（供測試直接呼叫）。
///
/// 正體中文人話，逐行列出各維度。黃/灰態附 next-step 建議。
pub fn render_doctor(input: &DoctorInput) -> String {
    let mut lines: Vec<String> = Vec::new();

    // ── 本地腦 ──────────────────────────────────────────────────────────────
    lines.push("─── 本地腦（local memory）".to_string());

    let local = local_light(input.l1_active, input.has_store_history);
    let local_status = match local {
        LocalLight::Active => format!("● 活躍（{} 筆 active L1 concept）", input.l1_active),
        LocalLight::Empty => "◌ 空白（store 目錄存在但無 active L1）".to_string(),
        LocalLight::Hidden => "— 未初始化（從未 dream，無 store 歷史）".to_string(),
    };
    lines.push(format!("  狀態：{}", local_status));

    if matches!(local, LocalLight::Empty) {
        lines.push(
            "  建議：執行 `codeforge dream` 重新編譯 L0 → L1，或 `codeforge learn` 新增知識。"
                .to_string(),
        );
    }

    lines.push(String::new()); // blank line

    // ── 央腦（Mnemos）──────────────────────────────────────────────────────
    lines.push("─── 央腦（Mnemos central memory）".to_string());

    if !input.opted_in {
        lines.push("  Opt-in：否（未設定 ~/.config/mnemos.env，跳過中央同步）".to_string());
        lines.push(
            "  建議：若要啟用中央腦，建立 ~/.config/mnemos.env 並設定 MNEMOS_INGEST_URL。"
                .to_string(),
        );
    } else {
        let central_label = match input.central {
            CentralLight::Ok => "● OK",
            CentralLight::Degraded => "◐ 降級",
            CentralLight::Offline => "○ 離線",
            CentralLight::Pending => "◌ 待定",
            CentralLight::Hidden => "—",
        };
        lines.push(format!("  Opt-in：是  央腦狀態：{}", central_label));

        // 即時 probe（標「即時量測」）
        let (outcome, latency, http_status) = &input.live_probe;
        let probe_str = match outcome {
            ProbeOutcome::Ok => {
                let lat = latency
                    .map(|l| format!("{}ms", l))
                    .unwrap_or_else(|| "—".to_string());
                format!("● OK  延遲 {}  HTTP {}", lat, http_status.unwrap_or(200))
            }
            ProbeOutcome::Unreachable => "○ 無法連線（連線被拒或逾時）".to_string(),
            ProbeOutcome::HttpError => {
                let st = http_status
                    .map(|s| format!("HTTP {}", s))
                    .unwrap_or_else(|| "HTTP ?".to_string());
                format!("◐ HTTP 錯誤（{}）", st)
            }
            ProbeOutcome::Never => "◌ 從未成功（Never）".to_string(),
        };
        lines.push(format!("  即時 probe（~2s）：{}", probe_str));

        // next-step for offline/error/never
        match outcome {
            ProbeOutcome::Unreachable => {
                lines.push(
                    "  建議：Mnemos server 沒在跑。可執行：cd ~/projects/mnemos && cargo run -p mnemos -- serve"
                        .to_string(),
                );
            }
            ProbeOutcome::HttpError => {
                lines.push(
                    "  建議：server 有回應但返回錯誤。確認 mnemos 版本與 MNEMOS_INGEST_URL 設定。"
                        .to_string(),
                );
            }
            ProbeOutcome::Never => {
                lines.push(
                    "  建議：從未成功連線 Mnemos，請確認 base_url 設定，再執行 `codeforge mnemos-cli probe --verbose` 診斷。"
                        .to_string(),
                );
            }
            ProbeOutcome::Ok => {}
        }

        // 上次 probe 快取（顯示相對時間）
        if let Some(probe_at) = input.last_probe_at {
            let age_secs = (input.now - probe_at).max(0) as u64;
            lines.push(format!(
                "  上次快取 probe：{}前（{} 秒前）",
                fmt_age(age_secs),
                age_secs
            ));
        } else {
            lines.push("  上次快取 probe：無記錄".to_string());
        }

        // 上次 ship
        match (input.last_ship_at, input.last_ship_ok) {
            (Some(at), Some(ok)) => {
                let age_secs = (input.now - at).max(0) as u64;
                let ok_str = if ok { "成功 ✓" } else { "失敗 ✗" };
                lines.push(format!(
                    "  上次 ship：{}前（{}）",
                    fmt_age(age_secs),
                    ok_str
                ));
                if !ok {
                    lines.push(
                        "  建議：ship 失敗，可執行 `codeforge ship --resend` 補送。".to_string(),
                    );
                }
            }
            (Some(at), None) => {
                let age_secs = (input.now - at).max(0) as u64;
                lines.push(format!("  上次 ship：{}前", fmt_age(age_secs)));
            }
            _ => {
                lines.push("  上次 ship：無記錄".to_string());
            }
        }

        // queue
        let queue_count = input.queue_count;
        let oldest_str = match input.queue_oldest_age {
            Some(age) => format!("，最舊 {}", fmt_age(age)),
            None => String::new(),
        };
        lines.push(format!("  待重送 queue：{} 筆{}", queue_count, oldest_str));

        if queue_count > 0 {
            // 有待重送就提示（不依賴 central light 合成）
            lines.push(
                "  建議：有未送達的 ledger，可執行 `codeforge ship --resend` 補送。".to_string(),
            );
        }
    }

    lines.push(String::new()); // blank line

    // ── 連線設定 ─────────────────────────────────────────────────────────────
    lines.push("─── 連線設定".to_string());
    lines.push(format!("  base_url：{}", input.base_url));

    lines.join("\n")
}

/// 格式化秒數為人話（中文）。
fn fmt_age(secs: u64) -> String {
    if secs < 60 {
        format!("{}秒", secs)
    } else if secs < 3600 {
        format!("{}分{}秒", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}小時{}分", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}天{}小時", secs / 86400, (secs % 86400) / 3600)
    }
}

/// `codeforge doctor` 主入口。
///
/// 流程：
/// 1. 讀本地維度（L1 count、store 歷史）
/// 2. 讀 Mnemos config（opted_in、base_url）
/// 3. 前景 probe（僅 opted-in 時，~2s，即時量測）
/// 4. 讀快取維度（last_probe_at、last_ship、queue 深度）
/// 5. 算 CentralLight（pure，不讀磁碟）
/// 6. 組 DoctorInput → render_doctor → 印出
pub fn run(ctx: &db::Context) -> Result<()> {
    let store_dir = ctx.project_dir.join("store");
    let l1_active = l1::count_active(&store_dir);
    let has_store_history = store_dir.join("concepts").exists();

    let opted_in = MnemosConfig::opted_in();
    let base_url = MnemosConfig::load()
        .map(|c| c.base_url)
        .unwrap_or_else(|_| "（讀取失敗）".to_string());

    // 前景即時 probe（未 opt-in 不打網路）
    let live_probe = if opted_in {
        probe_now()
    } else {
        (ProbeOutcome::Never, None, None)
    };

    // 快取維度
    let last_liveness = read_liveness();
    let last_probe_at = last_liveness.as_ref().map(|l| l.last_probe_at);
    let last_ship = read_ship();
    let last_ship_at = last_ship.as_ref().map(|s| s.last_ship_at);
    let last_ship_ok = last_ship.as_ref().map(|s| s.last_ship_ok);
    let (queue_count, queue_oldest_age) = queue_info();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // 算 CentralLight（用快取，不依賴 live_probe）
    let qd = queue_degraded();
    let lv_fresh = last_liveness.as_ref().filter(|l| fresh_enough(l, now));
    let central = central_light(opted_in, lv_fresh, last_ship.as_ref(), qd, now);

    let input = DoctorInput {
        l1_active,
        has_store_history,
        opted_in,
        live_probe,
        last_probe_at,
        last_ship_at,
        last_ship_ok,
        queue_count,
        queue_oldest_age,
        central,
        base_url,
        now,
    };

    println!("codeforge doctor — 腦部健康診斷");
    println!();
    println!("{}", render_doctor(&input));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemos::health::ProbeOutcome;

    // Test constructor mirrors DoctorInput's many fields; grouping adds no clarity.
    #[allow(clippy::too_many_arguments)]
    fn make_input(
        l1_active: usize,
        has_store: bool,
        opted_in: bool,
        outcome: ProbeOutcome,
        latency: Option<u32>,
        http_status: Option<u16>,
        last_ship_ok: Option<bool>,
        queue_count: usize,
    ) -> DoctorInput {
        DoctorInput {
            l1_active,
            has_store_history: has_store,
            opted_in,
            live_probe: (outcome, latency, http_status),
            last_probe_at: Some(1_000_000 - 300),
            last_ship_at: if last_ship_ok.is_some() {
                Some(1_000_000 - 3600)
            } else {
                None
            },
            last_ship_ok,
            queue_count,
            queue_oldest_age: if queue_count > 0 { Some(90_000) } else { None },
            central: CentralLight::Ok,
            base_url: "http://127.0.0.1:8845".to_string(),
            now: 1_000_000,
        }
    }

    #[test]
    fn doctor_lists_key_labels() {
        let input = make_input(
            3,
            true,
            true,
            ProbeOutcome::Ok,
            Some(42),
            Some(200),
            Some(true),
            0,
        );
        let out = render_doctor(&input);
        let preview: String = out.chars().take(200).collect();
        assert!(
            out.contains("本地腦"),
            "應含本地腦 label，got: {:?}",
            preview
        );
        assert!(out.contains("央腦"), "應含央腦 label，got: {:?}", preview);
        assert!(
            out.contains("待重送"),
            "應含待重送 label，got: {:?}",
            preview
        );
        assert!(
            out.contains("127.0.0.1"),
            "應含 base_url 地址，got: {:?}",
            preview
        );
    }

    #[test]
    fn offline_state_contains_next_step() {
        let input = make_input(
            5,
            true,
            true,
            ProbeOutcome::Unreachable,
            None,
            None,
            None,
            0,
        );
        let out = render_doctor(&input);
        let preview: String = out.chars().take(400).collect();
        assert!(
            out.contains("cargo run -p mnemos -- serve"),
            "Offline 態應含 serve 指令建議，got: {:?}",
            preview
        );
    }

    #[test]
    fn pending_state_contains_next_step() {
        let input = make_input(5, true, true, ProbeOutcome::Never, None, None, None, 0);
        let out = render_doctor(&input);
        let preview: String = out.chars().take(400).collect();
        assert!(
            out.contains("mnemos-cli probe --verbose"),
            "Never 態應含 probe --verbose 建議，got: {:?}",
            preview
        );
        assert!(
            out.contains("base_url") || out.contains("確認"),
            "Never 態應含 base_url 或確認字樣，got: {:?}",
            preview
        );
    }

    #[test]
    fn ok_state_has_no_serve_hint() {
        let input = make_input(
            5,
            true,
            true,
            ProbeOutcome::Ok,
            Some(25),
            Some(200),
            Some(true),
            0,
        );
        let out = render_doctor(&input);
        let preview: String = out.chars().take(400).collect();
        // OK 態不應出現 "serve" 建議（重啟 server 的指令）
        assert!(
            !out.contains("cargo run -p mnemos -- serve"),
            "Ok 態不應含 serve 建議，got: {:?}",
            preview
        );
    }

    #[test]
    fn no_optin_shows_optin_hint() {
        let input = make_input(
            0,
            false,
            false,
            ProbeOutcome::Unreachable,
            None,
            None,
            None,
            0,
        );
        let out = render_doctor(&input);
        let preview: String = out.chars().take(400).collect();
        assert!(
            out.contains("Opt-in：否"),
            "未 opt-in 應顯示 Opt-in：否，got: {:?}",
            preview
        );
        assert!(
            out.contains("mnemos.env"),
            "未 opt-in 應提示 mnemos.env，got: {:?}",
            preview
        );
    }

    #[test]
    fn queue_depth_shown() {
        let input = make_input(
            2,
            true,
            true,
            ProbeOutcome::Ok,
            Some(10),
            Some(200),
            Some(false),
            5,
        );
        let out = render_doctor(&input);
        let preview: String = out.chars().take(400).collect();
        assert!(
            out.contains("5") && out.contains("待重送"),
            "應顯示 queue 深度（5 + 待重送），got: {:?}",
            preview
        );
        assert!(
            out.contains("ship --resend"),
            "有 queue 應提示 --resend，got: {:?}",
            preview
        );
    }
}
