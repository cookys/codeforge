# CodeForge ↔ Coding-Agent Coupling（agent-coupling surface）

> **Status**: Accepted — *A-with-seam*（2026-06-26）。這是 **decision record（ADR）**，不是 feature spec。
> **TL;DR**: codeforge 是 coding agent 的「記憶/養成器官」，目前宿主 = Claude Code。長期想支援多 agent（決策 B），但現在只實作 CC（決策 A）。策略：**不抽象、但命名並守住耦合接縫**，讓未來的 B 是「加一個 adapter」而非「重寫核心」。

---

## 1. Context — 為什麼有這份文件

codeforge 的自動價值 **100% 寄生在一個 coding agent 上**：absorb（transcript → L0）、`dream`/`ship`、statusline、pet XP，全靠宿主 agent 的 transcript + hook 機制。手動 CLI（`learn`/`dream`/`pet`）是殘餘 —— 拿掉「自動從 coding session 累積」，它就退化成普通 note CLI，與其靈魂（邊 coding 邊長知識/養寵）無關。

**定位**：codeforge = coding agent 的記憶/養成器官，**沒有宿主就不跳動**。currently 宿主 = Claude Code。

願景 B（多 agent：Cursor / Cline / …）成立，但目前唯一實際用戶（作者）只用 CC → 現實上先做 A。**現在就做完整 `AgentAdapter` 抽象 = YAGNI / 過早投資**（只有一個實作，無第二個 case 驗證會抽錯）。

**決策：A-with-seam** —— 現在只實作 CC，但把 agent-specific 的耦合收斂到下列**明確命名的接縫**，並立紀律不讓耦合擴散進核心。B 化 = 對每個接縫實作一個 adapter，核心不動。

## 2. Agent-coupling surface（4 個接縫 = B 化的全部工作量）

| # | 接縫 | 位置 | 綁宿主 agent 的什麼（CC 現況） | B 化要做的 |
|---|---|---|---|---|
| 1 | **transcript → signal**（最深） | `.claude/scripts/session-digest.js` | 從 CC hook stdin 拿 transcript metadata；讀 `~/.claude/projects/*.jsonl`；解析 CC message shape（`_role`/`_content`、`tool_use`/`tool_result`、`input.file_path`/`input.command`） | 每個 agent 一個 transcript reader → 吐統一的 internal signal shape |
| 2 | **hook wiring** | `src/cli/install.rs` | 寫 CC `~/.claude/settings.json` 的 hook（PreCompact / SessionEnd / SessionStart）+ statusLine 區塊；hook 事件名與 settings 格式都是 CC 的 | 每個 agent 一個 installer：知道該 agent 的 hook 設定格式與事件名 |
| 3 | **session 邊界事件** | `.claude/scripts/emit-session.js` | drain CC hook stdin → `codeforge emit <event> --field cwd=…`（SessionStart/End） | 每個 agent 把自己的 session start/end 映射到 codeforge 的 `emit` 入口 |
| 4 | **statusline 顯示** | `src/cli/statusline.rs`（input parse）+ install 的 statusLine 區塊 | 從 CC statusLine stdin 一行 JSON 取 `workspace.current_dir` 等欄位 | 每個 agent 一個 input adapter（或共用，若協定夠像） |

**關鍵觀察**：4 個接縫**全在最外圈**（JS hooks + install + statusline I/O）。**沒有一個滲進核心 Rust**（`memory` / `dream` / `compile` / `pet` / `daemon` / `mnemos` / `power`）。codeforge 的架構天然分層（extraction 在 JS hook、核心在 Rust 處理已正規化的 signal）→ **B 化沒想像中可怕**。

## 3. 核心是 agent-agnostic（守則：這些絕不碰宿主）

`src/memory`、`src/dream`、`src/pet`、`src/daemon`、`src/mnemos`、`src/power`，以及 L0/L1/L2 schema —— 處理的是 signal 與 concept，**不該知道宿主 agent 是誰**。判準：

- 核心吃的是**已正規化的 internal signal**（`SignalSource::SessionDigest` 等），不是 CC 的原始 message。
- 任何 `~/.claude/` 路徑、CC hook 事件名、CC message field —— **只准出現在 §2 的 4 個接縫**。在核心看到 = code smell。

## 4. 紀律（防耦合擴散）

1. 新功能若要碰宿主 agent 的東西 → 先問「這屬於哪個接縫？」收進去，別在核心開新洞。
2. 宿主專屬假設（路徑 / 格式 / 事件名）只能落在 §2 表列的那幾個檔。
3. Review checklist：核心 crate 裡出現 `.claude/` 字串或 CC message field = block，請收回接縫。

## 5. 文案 / onboarding 紀律（零成本留 B 路）

user-facing 文字用 **agent-neutral 語言**：「你的 coding agent」而非到處寫死「Claude Code」。允許在「currently / 目前宿主」這類明確處點名 CC。

好處：B 落地時文案不用大改，只換 adapter 名。**這也決定 onboarding 形態** —— 先做 CC-native 體驗（plugin 方向），但用詞留 B 的路（呼應作者「先 A、心向 B」的取捨）。

## 6. 非目標（現在明確不做）

- ❌ 完整 `AgentAdapter` trait / plugin 系統 —— YAGNI，等真要接第二個 agent 才做。
- ❌ 把 §2 接縫重構成統一介面 —— 只有一個實作（CC），抽象沒有第二個 case 驗證會抽錯。
- ✅ 現在只做：**命名接縫（本文件）+ 守紀律（§4）+ 文案中立（§5）**。三者皆近零成本。

## 7. B 化觸發條件（什麼時候才動手實作 adapter）

當下列任一成立，才把 §2 接縫實作成 adapter：

- 作者或早期用戶**實際**要在非-CC agent（Cursor / Cline / …）上跑 codeforge。
- 出現第二個**真實**宿主需求（非假想）。

屆時順序：先實作**接縫 1（transcript reader）** —— 它最深、價值最高、是「自動累積」的源頭；其餘接縫按需跟上。

## 相關

- [`doc/concepts.md`](../concepts.md) §2.5 absorb —— transcript → L0 的 CC 現況（接縫 1 的白話版）
- [`doc/specs/codeforge-install-subcommand.md`](codeforge-install-subcommand.md) —— 接縫 2（hook wiring）
- [`CLAUDE.md`](../../CLAUDE.md) —— 生態系定位（CodePower ↔ CodeForge ↔ Mnemos）
