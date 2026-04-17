# CodeForge MUD Engine — Phase 2 設計規格

> 建立：2026-04-14
> 狀態：Draft
> 前置：[Phase 1 — Memory CLI + Common Pet](../projects/2026-04-14-codeforge-phase1/README.md)

## Vision

把 MUD 搬上來。

CodeForge 不只是記憶工具——它是一個以 codebase 為世界地圖的 MUD，以程式工作為遊戲引擎，以 pet 為玩家角色的長期陪伴系統。User 在放置工作（idle coding）的時候，pet 自動打怪抓寶；偶爾 pet 會對著 user 的工作說幾句垃圾話；大小地圖追蹤 user 探索過的程式世界。

---

## 核心概念對照

| CodeForge 概念 | MUD 術語 | 說明 |
|--------------|---------|------|
| Project directory | Zone / Area | 一個 repo = 一個 Zone |
| 目前工作目錄 | Current Room | cd 到哪裡就在哪個 Room |
| 程式語言村落 | Guild / Hometown | Rust Village、Python Peaks 等 |
| 複雜函數、技術債 | MOB / Monster | 自動生成，等待被擊殺 |
| Refactored code | Treasure / Loot | 擊殺 MOB 後掉落 |
| Pet | Player Character | ATK/DEF/SUP/VER 決定戰力 |
| Strategy Mode | Pet Behavior AI | 影響 idle tick 的行為優先序 |
| L0/L1 Memory | Quest Log / Tome | 知識就是 loot |
| `codeforge daemon` | MUD server process | 持續運行的 game loop |

---

## 系統架構

```
┌─────────────────────────────────────────────────────────────────┐
│  codeforge daemon                                               │
│                                                                 │
│  ┌──────────────┐   ┌──────────────┐   ┌───────────────────┐  │
│  │  Event Bus   │──▶│  Game Engine │──▶│   TUI Renderer    │  │
│  │              │   │              │   │  (crossterm)      │  │
│  │  git hooks   │   │  World Map   │   │                   │  │
│  │  file watch  │   │  Zone System │   │  ┌─────┬───────┐  │  │
│  │  inotify     │   │  Combat Tick │   │  │ Map │ Pet   │  │  │
│  │  session hook│   │  Loot System │   │  │     │ Stats │  │  │
│  │  idle timer  │   │  AI Comment  │   │  └─────┴───────┘  │  │
│  └──────────────┘   └──────────────┘   └───────────────────┘  │
│                             │                                   │
│                      ┌──────▼──────┐                           │
│                      │   SQLite    │                           │
│                      │  game_state │                           │
│                      └─────────────┘                           │
└─────────────────────────────────────────────────────────────────┘
```

### 啟動方式

```bash
# 在 tmux split pane 中啟動 companion
codeforge daemon --attach tmux

# 或直接佔用整個 terminal
codeforge daemon --fullscreen

# Phase 1 fallback（現有）：無 daemon，session hook 靜態輸出
codeforge statusline  # ← 已完成
```

---

## 1. 大小地圖系統

### 世界地圖（World Map）

```
  ╔══════════════════════════════════════╗
  ║  The Known World                     ║
  ║                                      ║
  ║   [Rust Forge-Ruins] ── [Go Glacier] ║
  ║         │                    │       ║
  ║   [PY Scriptorium]    [TS Garrison]  ║
  ║         │                            ║
  ║   [JS Bazaar]      ??? (unexplored)  ║
  ║                                      ║
  ╚══════════════════════════════════════╝
```

- 每個語言村落 = 一個 Zone，有名稱、顏色、特殊 MOB 類型
- 初始只有 user 用過的語言 Zone 解鎖
- 未探索地帶顯示 `???`（fog of war）
- 村落間距離 = L1 memory 中語言間的 link 強度

### 小地圖（Local Map / Minimap）

```
  ┌─────────────────────┐
  │  ~/projects/codepower│
  │                     │
  │  backend/ [🐉 x2]   │
  │  frontend/ [🧟 x5]  │
  │  docker/   [✓]      │
  │▶ doc/      [safe]   │
  └─────────────────────┘
```

- 顯示目前 repo 的頂層目錄
- 每個目錄旁標示 MOB 數量（未解決的問題）
- User 目前所在 Room 用 `▶` 標示
- 從 `git status`、`cargo check`、`eslint` 輸出生成 MOB

### 地圖資料來源

| 地圖層級 | 資料來源 | 更新時機 |
|---------|---------|---------|
| World Map | L1 memory 的 village 分佈 | dream compile 後 |
| Zone Map | repo 頂層目錄 + git activity | git hook / tick |
| Room detail | file-level complexity scan | 進入目錄時 |

---

## 2. 戰鬥系統（Auto-Combat）

### MOB 生成規則

```
每個 tick（60s）掃描 current Zone：
  
  高 cyclomatic complexity (>10)  → Elite Mob ⚔️  (高 HP，多 EXP)
  TODO / FIXME count > 5         → Zombie Horde 🧟 (弱，數量多)
  Function > 100 lines           → Boss Mob 🐉    (需要多 tick 才打完)
  Dead code (unused imports)     → Ghost 👻       (容易一擊必殺)
  Duplicate code block           → Doppelganger 🪞 (分裂，清不完)
  Missing test coverage          → Void Creature 🕳️ (drain DEF)
```

### 戰鬥計算

```
一個 tick 的戰鬥結果：

  hit_chance = (pet.ATK + pet.VER) / (mob.DEF + difficulty)
  damage     = pet.ATK * strategy_multiplier * rng(0.8..1.2)
  
  mob 死亡 → loot roll
  mob 逃脫 → 留在 Zone，下個 tick 再打
  pet 受傷 → HP 下降（需要 idle 時間回復）
```

### Strategy Mode（行為策略）

| 模式 | 優先序 | ATK倍率 | DEF倍率 | 特性 |
|-----|-------|---------|---------|------|
| **Aggressive** | Boss > Elite > Zombie | 1.3x | 0.8x | 快速清場，HP 消耗快 |
| **Defensive** | Ghost > Zombie > Elite | 0.9x | 1.4x | 保守打法，不冒險 |
| **Explorer** | 優先未探索 Zone 的 MOB | 1.0x | 1.0x | 解鎖新地圖 |
| **Scholar** | 優先掉 Tome 的 MOB | 0.8x | 1.0x | 最大化 L1 memory 產出 |

```bash
codeforge strategy aggressive
codeforge strategy scholar
```

### Loot 系統

| MOB 類型 | 主要 Loot | 次要 Loot |
|---------|---------|---------|
| Boss 🐉 | Rare Item + 大量 EXP | L1 Connection memory |
| Elite ⚔️ | EXP + skill point | Refactor Scroll |
| Zombie 🧟 | 少量 EXP | TODO Cleaner item |
| Ghost 👻 | Dead Code Crystal | — |
| Doppelganger 🪞 | Pattern Fragment | Abstract Gem |

**Loot 落地後的效果：**
- **EXP** → pet level up
- **Skill Point** → 解鎖 pet 技能（如 `Auto-Doc`、`Type Wizard`）
- **Tome** → 直接寫入 L1 memory store
- **Item** → 存入 inventory，user 可以手動使用（`codeforge use <item>`）

---

## 2.5 Pet Ability 系統

### 設計原則

- Ability 依**等級**解鎖，升等本身有實質意義
- 分 **Passive**（永遠生效）和 **Active**（有 cooldown）
- **Village 專屬 ability**（Lv 50）讓不同 Nation 的寵物有戰力個性差異
- 主寵才有 ability 生效；待機寵物不觸發 ability

### 通用 Ability 解鎖表

| 等級 | Ability | 類型 | 效果 |
|------|---------|------|------|
| Lv 5 | **Quick Eye** | Passive | 自動識別 Ghost MOB（dead code），命中率 +30% |
| Lv 10 | **Focus Strike** | Active（3 tick CD） | 對 Boss MOB 暴擊 2x damage |
| Lv 15 | **Tome Sense** | Passive | Scholar 策略下 Loot 掉率 +20% |
| Lv 20 | **Village Aura** | Passive | 在 home Village Zone 時，所有 stat +15% |
| Lv 30 | **Memory Recall** | Passive | 戰鬥 loot 有機率直接寫入 L1 memory（跳過 dream compile） |

### Village 專屬 Ability（Lv 50，限定）

每個 Village 有唯一的 Legendary ability，只有從該 Nation 獲得且升到 Lv 50 的寵物才能解鎖：

| Village | Ability | 效果 |
|---------|---------|------|
| Rust 🦀 | **Iron Skin** | DEF 翻倍，Boss 攻擊有 20% 機率反傷 |
| Python 🐍 | **Scripted** | 每 tick 額外掃描一次 Ghost MOB |
| Go 🐹 | **Concurrent** | 同時攻擊最多 3 個 Zombie MOB |
| TypeScript 🔷 | **Type Guard** | 完全免疫 Void Creature（missing test）的 DEF drain |
| ML 🧠 | **Gradient** | 每次升等後隨機強化一個現有 ability（+10% 效果） |
| 開源基金會 🐙 | **Community** | 蒐集到的 Tome 品質提升，L1 memory 寫入成功率 +50% |

### 戰鬥計算更新（含 Ability）

```
一個 tick 的戰鬥結果：

  # 基礎（原有）
  hit_chance = (pet.ATK + pet.VER) / (mob.DEF + difficulty)
  damage     = pet.ATK * strategy_multiplier * rng(0.8..1.2)

  # Ability 疊加
  if pet.has_ability("quick_eye") && mob.type == Ghost:
      hit_chance *= 1.3

  if pet.has_ability("focus_strike") && mob.type == Boss && cooldown_ready:
      damage *= 2.0
      set_cooldown("focus_strike", 3)

  if pet.has_ability("village_aura") && current_zone == pet.home_village:
      all_stats *= 1.15
```

### 主寵 vs 待機寵物

```
主寵（active slot）：
  - 在 tmux split 裡顯示、有動畫
  - 參與戰鬥、獲得 XP
  - Ability 生效
  - 說話（AI commentary 或 口音系統）

待機寵物（collection）：
  - 存在 Codeforge SQLite
  - 不參與戰鬥、不成長（Phase 2）
  - 可隨時切換為主寵（codeforge pet switch <name>）
  - 未來考慮：極慢速放牧 XP（Phase 3+）
```

---

## 3. 黏著度機制系統

> 來源：Survey（寵物成長模式）+ Think Tank 六角色審查（2026-04-16）
> 設計原則：所有機制必須在「玩家不主動打開遊戲」的前提下有意義。

---

### 3.1 歸來摘要（Welcome Back Report）

每次玩家開啟新 session 或 `cd` 進入已知 repo，companion pane 先顯示 2-3 行摘要：

```
╔══ Ferris 回報 ════════════════════════════════╗
║  你不在的 8 小時：                             ║
║  → 擊殺 Zombie ×7（backend/）                 ║
║  → Boss「auth_middleware.rs」HP 剩 34%         ║
║  → 撿到 Pattern Fragment ×2                   ║
║  → HP 消耗至 61%（建議休息一下）               ║
╚═══════════════════════════════════════════════╝
```

- 即使玩家不在期間沒有任何 coding，也顯示：「Ferris 在 backend/ 巡邏，沒有發現新威脅。」
- 解決 idle 遊戲最常見的流失原因：「打開看沒有東西」
- 資料來源：daemon 的 combat_log + loot_log（已有）

---

### 3.2 Pet 情緒衰減（Mood Decay）

Pet 的情緒狀態影響 commentary 語氣，製造「想回去陪它」的情感依附：

```
mood 狀態（pet_stats.mood: 0-100）：

  最近 24h 有 coding activity  → mood +10（上限 100）
  每 6h 無 activity            → mood -8
  打倒 Boss                    → mood +20
  HP < 30%                     → mood -15

mood 對應語氣：
  80-100  →  精神飽滿、活潑
  50-79   →  正常
  20-49   →  疲憊、少話、語氣低沉
  0-19    →  沮喪、問「你還在嗎？」

```

- 實作成本低：`pet_stats` 加 `mood` 欄位，tick loop 計算
- Commentary 生成時把 mood context 傳入 prompt

---

### 3.3 Zone Mastery 聲望條

累積在特定語言 Zone 的 MOB 擊殺，解鎖稱號和 cosmetic（已有 combat_log，純 aggregation）：

```
Rust Zone 聲望：
  0-49 kills    →  Traveler
  50-199 kills  →  Forger
  200-499 kills →  Iron Crafter  ✦（稱號顯示在 statusline）
  500+ kills    →  Rust Veteran  ✦✦（特殊 pet frame）

顯示位置：World Map 各 Zone 旁的聲望圖示
資料結構：zone_reputation(zone_id, kill_count, rank)
```

---

### 3.4 進展錨點（Next Unlock Target）

XP 條旁邊標示下一個具名目標，讓每次 coding 都有方向感：

```
TUI 顯示：
  XP ████░░ 420/1000   next: Focus Strike (Lv 10)

Statusline 模式（簡化）：
  [Ferris Lv.5 | ▓▓▓░ | → Lv10: Focus Strike]
```

- 不只是數字，而是有名字的倒數
- 對「不主動玩遊戲」的開發者特別重要

---

### 3.5 Loot Crafting 合成系統

讓日常打怪有「集齊材料」的目標感，取代交易系統：

```
合成配方（`codeforge craft`）：

  3× Pattern Fragment   → 1× Refactor Blueprint
                           （使用後：對應目錄的 MOB difficulty -20%，持續 7 天）

  5× Dead Code Crystal  → 1× Ghost Repellent
                           （使用後：Ghost MOB 不再在此 Zone 生成，持續 3 天）

  2× Abstract Gem       → 1× Doppelganger Ward
                           （使用後：Doppelganger 不分裂，持續 2 天）
```

- 所有材料已在現有 Loot 系統中（不需新增 MOB 類型）
- 純本地 SQLite 操作，無需網路
- `codeforge inventory` 顯示當前材料數量

---

### 3.6 `codeforge snapshot`：可分享 ASCII 輸出

零摩擦的社交成長飛輪：

```bash
codeforge snapshot
```

```
╔══════════════════════════════════════════════════════╗
║  Ferris の 冒險報告  ·  2026-04-16                   ║
╠══════════════════════════════════════════════════════╣
║  [Rust Forge-Ruins] ── [Go Glacier]                  ║
║       Veteran ✦✦         Traveler                    ║
║  [PY Scriptorium]    [TS Garrison]                   ║
║       Forger ✦           ？？？（未解鎖）             ║
╠══════════════════════════════════════════════════════╣
║  本月戰績                                            ║
║  → Boss 擊殺：12    Elite 擊殺：87    Zombie：341    ║
║  → Loot 合成：5 次   Legendary 進度：23/50 commits   ║
║  → 最長連勝：Rust 8 個 Boss 連殺                     ║
╠══════════════════════════════════════════════════════╣
║  Ferris（Rust Nation · Gold）Lv.23                   ║
║  「還有多少 Boss 在等著我？」                        ║
╚══════════════════════════════════════════════════════╝
```

- 輸出到 stdout（可複製貼到 Slack/Discord）
- 無需帳號、無需 OAuth，純文字
- `codeforge snapshot --clipboard` 直接複製到剪貼板

---

### 3.7 主動 Item 使用（防旁觀者症候群）

保留 1-2 個「玩家主動介入才能觸發」的時刻，避免純旁觀者感：

```bash
codeforge use refactor-scroll    # 對目前 Zone 的 Boss 施放，下個 tick 雙倍傷害
codeforge use ghost-repellent    # 立即清除目前 Zone 所有 Ghost MOB
```

- 不強制使用：idle 玩家完全不需要，但偶爾使用有「我決定了這件事」的能動感
- Item 從 Loot 或 Crafting 取得，稀缺但不稀缺到讓人焦慮

---

### 3.8 首次里程碑特殊事件（First-Time Moments）

以下事件只觸發一次，commentary 切換為感性模式而非平常的垃圾話：

```
觸發條件                    特殊 commentary 例：
─────────────────────────────────────────────────────────
首次進入新語言 Zone         「這是我第一次踏進 Go Glacier……空氣比 Rust 冷。」
首次擊殺 Boss               「剛才我感覺到了什麼……原來這就是『完成』的感覺。」
首次升到 Lv 10              「我……比上週的自己強了一點。謝謝你。」
首次解鎖 Legendary Ability  「Iron Skin。我把這個名字記下來了。」
首次蒐集到第 2 隻寵物       「你去了別的地方……帶回了新的夥伴。」
```

- 觸發記錄存 `first_events(event_id)` table，確保只發一次
- 這是 Day 7 留存的關鍵設計：「第一次」記憶製造最強的情感鉤

---

### 3.9 Commentary 頻率修正

> ⚠️ 覆蓋 Section 4（AI Commentary）的頻率設計

Think Tank 審查後，Commentary 設計調整：

```
修正前：每 30 分鐘至多一次（高頻觸發）
修正後：
  Global budget：每小時至多 1 條（所有觸發條件共用上限）
  預設：opt-in（需要 CODEFORGE_COMMENTARY=1 啟用）
  Pet 情緒記憶：已說過的 phrase 至少 30 天不重複
  Contextual 優先：
    「你上週還在修這個 TODO，現在它進化成 Boss 了」
    優先於 generic template
```

---

### 3.10 Nation Statusline Theme 系統

每個 Nation 可以定義自己的 statusline 視覺主題，玩家蒐集到對應的寵物後解鎖。主題反映 Nation 的技術文化個性。

#### 解鎖條件（三階）

```
Tier 1 — 擁有該 Nation 任意寵物（credential 存在即可）
  → 解鎖 Nation 色彩盤 + 基礎符號組

Tier 2 — 同 Nation 寵物達到 Lv 20+
  → 解鎖 Nation 專屬版面配置 + 特殊分隔符

Tier 3 — 同 Nation 寵物 Legendary 解鎖（Lv 50）
  → 解鎖動態效果（HP 條顏色週期、Boss 戰閃爍警告等）
```

#### 切換指令

```bash
codeforge theme rust        # 切換到 Rust Nation 主題
codeforge theme ml          # 切換到 ML Nation 主題
codeforge theme default     # 回到預設 CodeForge 主題
codeforge theme list        # 列出已解鎖主題 + 各 Tier 狀態
```

#### Nation 主題設計範例

**Rust Nation 🦀（Tier 1）**
```
[🦀 Ferris ▓▓▓░ 82hp] forge://backend/main.rs ⚔ Boss×1 [Lv.12]
```
色調：鐵鏽琥珀（amber），`[▸ ]` 方括號，`▓░` 鋼鐵感 HP 條，路徑用 `forge://` 前綴

**Rust Nation 🦀（Tier 3 Legendary）**
```
[🦀 Ferris ▓▓▓░ 82hp] forge://backend/main.rs ⚔ BOSS FIGHT ← 閃爍警告
```

---

**ML Nation 🧠（Tier 1）**
```
⟨🧠 Neurix ████░ 82hp⟩ loss:0.024 ∇ backend/ · Elite×3
```
色調：深紫漸層，`⟨∇ ⟩` 梯度符號，HP 條顯示為「收斂進度」，Zone 顯示 loss metric 風格

**ML Nation 🧠（Tier 2）**
```
⟨🧠 Neurix ████░ 82hp | atk:47 def:31⟩  ∇ backend/ [epoch 3/∞]
```

---

**Python Nation 🐍（Tier 1）**
```
(🐍 Pytho ══════░ 82hp) ~/projects/backend ◈ Zombie×5
```
色調：暖綠，`(◈ )` 括號，`═══` 平滑 HP 條，路徑用 `~/` 風格

---

**Go Nation 🐹（Tier 1）**
```
→ 🐹 Gopher  ─────░  82hp  backend/  goroutine×3 mobs
```
色調：青色，極簡箭頭風格，no-decoration，反映 Go 的「less is more」哲學

---

**TypeScript Nation 🔷（Tier 1）**
```
<🔷 Typus :: hp=82/100 :: zone=backend/ :: mobs=[Elite×2]>
```
色調：藍紫，`<:: >` 嚴格型別風格，所有值都有 key=value 標注

---

**Security Nation 🔐（Tier 1）**
```
[⛧ Cipher ████ 82hp] /etc/shadows ⚠ vulns:2 · Elite×1
```
色調：深紅黑，警告符號，路徑用絕對路徑風格，MOB 顯示為「漏洞數」

---

**開源基金會 🐙（Tier 1）**
```
{🐙 Octo ░░░░ 82hp} contrib/backend ★ PRs:3 · issues:7
```
色調：多色（依 Zone 語言變色），`{★ }` 貢獻者徽章風格，MOB 顯示為 PR/issue 數

---

#### Theme 定義格式（Nation Plugin 的一部分）

Nation 在 plugin 定義中宣告主題規格，由 Codeforge 本地渲染：

```rust
pub struct NationTheme {
    pub tier: u8,                    // 1, 2, or 3
    pub colors: ThemeColors,
    pub symbols: ThemeSymbols,
    pub layout: StatuslineLayout,    // 決定欄位順序和格式
    pub animations: Option<Vec<ThemeAnimation>>,  // Tier 3 only
}

pub struct ThemeColors {
    pub primary: AnsiColor,
    pub accent: AnsiColor,
    pub hp_high: AnsiColor,          // HP > 70%
    pub hp_mid: AnsiColor,           // HP 30-70%
    pub hp_low: AnsiColor,           // HP < 30%
    pub alert: AnsiColor,            // Boss 戰、警告
}

pub struct ThemeSymbols {
    pub bracket_open: &'static str,  // "[", "(", "⟨", "{"
    pub bracket_close: &'static str,
    pub hp_fill: &'static str,       // "▓", "═", "█", "─"
    pub hp_empty: &'static str,      // "░", " ", "·"
    pub separator: &'static str,     // "|", "::", "·", "→"
    pub mob_prefix: &'static str,    // "⚔", "∇", "◈", "⚠"
}
```

#### 解鎖狀態計算（純本地）

```
codeforge theme list 輸出：

  ✦✦✦ Rust Nation    [Tier 3 — Legendary]  ← 當前使用
  ✦✦░ ML Nation      [Tier 2 — Lv 23/50]
  ✦░░ Python Nation  [Tier 1 — 已蒐集]
  ░░░ Go Nation      [未解鎖 — 尚未加入 Go Nation]
  ░░░ TypeScript     [未解鎖]
```

解鎖判斷邏輯：
```rust
fn theme_tier(pet: &PetCredential) -> u8 {
    if pet.legendary_unlocked { return 3; }
    if pet.level >= 20 { return 2; }
    if pet.credential.is_valid() { return 1; }
    0
}
```

#### 架構說明

- Theme 定義隨 Nation manifest 下載（`codeforge nations join <url>` 時）
- 渲染完全在本地，不需要 Nation 在線
- 切換 theme 即時生效，下個 tick 開始渲染
- 預設 theme 是標準 CodeForge 主題（amber，不需任何 Nation pet）

---

### 3.11 Week Streak（Phase 3+，Nation 驗證版）

**不在 Phase 2 實作**，原因：本地 commit timestamp 可被篡改。

Phase 3+ 設計方向：
- 條件：「連續 4 週的週掃描都有活動」
- 驗證：Nation re-scan 簽發「週活躍憑證」
- 獎勵：Week Streak badge（Nation 簽名，防偽）

---

### 黏著度機制優先序

| 機制 | Phase | 實作成本 | 黏著效果 |
|------|-------|---------|---------|
| 3.1 歸來摘要 | 2b | 極低 | 解決「打開沒東西」 |
| 3.4 進展錨點 | 2b | 極低 | 目標感 |
| 3.2 Pet 情緒衰減 | 2c | 低 | 情感依附 |
| 3.8 首次里程碑 | 2c | 低 | Day 7 留存 |
| 3.3 Zone Mastery | 3a | 低 | 長期目標 |
| 3.5 Loot Crafting | 3a | 中 | 日常目標感 |
| 3.7 主動 Item 使用 | 3a | 低 | 能動感 |
| 3.6 Snapshot 分享 | 3b | 中 | 社交飛輪 |
| 3.10 Week Streak | 3+ | 中（需 Nation） | 習慣化 |

**明確 Defer（不在 Backlog，結論已定）：**
- 排行榜（P2P 無法防偽）
- 公會 / Raid Boss（需要同步上線）
- 交易系統（P2P double-spend 問題）
- 每日 Quest 特定 MOB 類型（太容易被 exploit）

---

## 4. AI Commentary（垃圾話系統）

### 觸發條件

```
高頻（每 30 分鐘至多一次）：
  - commit message 太簡短（< 10 字）→ 嘲諷
  - 同一個檔案改了超過 3 次      → 關心
  - TODO 數量增加                 → 嘆氣
  - session 超過 4 小時           → 催休息

低頻（每天至多 3 次）：
  - 打出 Boss mob                → 慶祝
  - Level up                     → 感性
  - 新 Zone 解鎖                  → 探險感
  - 長時間 idle（user 離開）       → 自言自語
```

### 生成方式

```
輸入給 Claude Haiku：
  - pet.name, pet.personality (village 決定)
  - trigger_event（如 "short_commit_message"）
  - context（git log -1, current_zone, pet.hp）
  - tone：village 語氣（Rust = 嚴肅直率, Python = 隨興, JS = 混亂）

輸出：
  一行文字，< 60 字，繁體中文，第一人稱
  不解釋，直接說話
  偶爾帶點 MUD flavor（"你感覺到空氣中有危險氣息..."）
```

### 顯示位置

```
TUI 模式：在 combat log 區域顯示，保留最近 5 條
Statusline 模式：footer 行偶爾替換為 pet 說的話（5% 機率）
```

---

## 5. TUI 渲染架構

### 版面配置（tmux split）

```
┌──────────────────────────────┬──────────────────────┐
│  Claude Code（主 pane）       │  CodeForge Companion  │
│                              │                      │
│  $ your normal workflow      │  ┌──World Map──┐     │
│  ...                         │  │  [Rust] [Go]│     │
│                              │  │  [PY]  [TS] │     │
│                              │  └─────────────┘     │
│                              │                      │
│                              │  ┌──Local Map──┐     │
│                              │  │ backend/ 🐉 │     │
│                              │  │▶doc/   safe │     │
│                              │  └─────────────┘     │
│                              │                      │
│                              │  Ferris Lv.5         │
│                              │  HP ████░░ 82        │
│                              │  XP ██░░░░ 420/1000  │
│                              │                      │
│                              │  ── Combat Log ────  │
│                              │  → 擊殺 Zombie x3    │
│                              │  → 獲得 EXP +45      │
│                              │  Ferris: 又是 TODO？  │
└──────────────────────────────┴──────────────────────┘
```

### 渲染技術

```rust
// 核心：crossterm 絕對座標定位
// 每個 tick 只重繪變化的區域（diff-based）

struct TuiRegion {
    x: u16, y: u16,
    width: u16, height: u16,
}

enum Region {
    WorldMap(TuiRegion),
    LocalMap(TuiRegion),
    PetStatus(TuiRegion),
    CombatLog(TuiRegion),
}

// 寫入時：
execute!(stdout, cursor::MoveTo(region.x, region.y))?;
write!(stdout, "{}", colored_content)?;
// 清除舊內容：
execute!(stdout, cursor::MoveTo(region.x, row), terminal::Clear(ClearType::UntilNewLine))?;
```

### Scroll Region（MUD 風格）

```
如果不用 tmux split，而是佔用單一 terminal：

  上半部：scrolling zone（Claude output / game text）
  下半部：fixed status region（pet + map）
  
  ANSI: \e[1;{scroll_bottom}r  設定捲動區域
  下半部不受 scroll 影響，永遠顯示在固定位置
```

---

## 6. Daemon 架構

### 執行模式

```
codeforge daemon
    │
    ├── EventLoop (tokio)
    │     ├── FileWatcher (inotify/kqueue)
    │     ├── GitHookReceiver (unix socket)
    │     ├── SessionHookReceiver
    │     └── TickTimer (60s interval)
    │
    ├── GameEngine
    │     ├── WorldState (zones, mobs, loot)
    │     ├── CombatProcessor
    │     ├── LootResolver
    │     └── CommentaryGenerator (Haiku API)
    │
    ├── TuiRenderer (crossterm)
    │     ├── LayoutManager (region definitions)
    │     ├── DiffRenderer (只重繪變化)
    │     └── AnimationQueue (pet movement frames)
    │
    └── StateStore (SQLite)
          game_world, mobs, inventory, combat_log
```

### IPC（與 Claude Code hooks 溝通）

> **Decision (2026-04-17)**：Phase 2a 採 **SQLite event_inbox + 500ms poll** 方案（代號 Option D）。
> 完整研究在 `doc/projects/2026-04-17-phase2a-daemon/ipc-research.md`。
> 研究結論：盤點 Phase 2a 所有事件皆無 <500ms 即時性需求；socket 方案的 daemon-down durability 成本等於重建一套 disk buffer。

```
Claude Code hook → codeforge emit <event> → INSERT INTO event_inbox
                                                     │
                           daemon poll (500ms) ───────┘
                                  │
                                  ▼
                           drain unseen rows
                                  │
                                  ▼
                           update game state (ECS + pet_snapshot)
                                  │
                                  ▼
                           UPDATE event_inbox SET seen_at = ? WHERE id IN (...)
```

**Schema**:

```sql
CREATE TABLE event_inbox (
    id         INTEGER PRIMARY KEY,
    payload    TEXT NOT NULL,              -- JSON blob
    created_at INTEGER NOT NULL,           -- unix ts
    seen_at    INTEGER                     -- NULL = 未處理；非 NULL = drain 時間
);
CREATE INDEX idx_event_inbox_unseen ON event_inbox(id) WHERE seen_at IS NULL;
```

**Payload examples**:

```json
{"event": "session_start", "cwd": "/path", "model": "sonnet-4.6"}
{"event": "session_end",   "duration_ms": 3600000}
{"event": "file_saved",    "path": "src/main.rs"}
{"event": "git_commit",    "message": "fix: stuff", "files_changed": 7}
```

**Two-writer rule（縮窄版）**：
- Daemon 獨占寫入：`pet_snapshot`、`combat_log`、`game_world` 等 derived/game state tables
- Hook 可 INSERT：`event_inbox`（append-only；寫入欄位 `id/payload/created_at`）
- Daemon 可 UPDATE：`event_inbox.seen_at`（與 hook 寫入欄位不重疊）
- CLI read-only：所有 game state tables

SQLite WAL + `busy_timeout=5000` 足以處理 hook/daemon 併發 INSERT/UPDATE（寫入集不重疊）。

**Retention**: `seen_at IS NOT NULL AND created_at < now - 7 days` 的 row 在 daemon tick 時清理。

**Daemon-down durability**: daemon 沒跑時，hook INSERT 照常成功；event 累積在 inbox table，daemon 重啟後自動 drain。零事件遺失。

**不採 Unix socket 的理由（Phase 2a）**：

| 考量 | socket | D（event_inbox） |
|------|--------|------------------|
| Daemon-down 時事件遺失 | 是（需自建 disk buffer fallback） | 否（SQLite 本身就是 buffer） |
| 失敗域 | 新增 socket lifecycle / EPIPE / stale socket / NFS 路徑 / 跨 user 權限 | 沿用 SQLite（已調校） |
| 生產除錯 | `strace`, socket inspection | `sqlite3 .codeforge/codeforge.db "SELECT …"` |
| 測試 | 需 mock socket 或真實 socket | `:memory:` SQLite 確定性測試 |
| 延遲（實測） | 0.45ms | ≤500ms |
| Latency-sensitive 事件？ | — | Phase 2a 無 |

**未來升級路徑**：若 Phase 3b+ 出現真正 <50ms 敏感事件（e.g. TUI 互動鍵盤），可**加開**第二通道（socket for realtime、SQLite 仍為 durability layer），不需拆掉 D。

---

## Phase 分解

| Phase | 內容 | 前置 |
|-------|------|------|
| **Phase 1** ✅ | Statusline + Pet + Memory CLI | done |
| **Phase 2a** | Daemon 框架 + tick loop + SQLite event_inbox（Option D） | P1 |
| **Phase 2b** | MOB 生成 + 自動戰鬥 + Loot + 歸來摘要 + 進展錨點 | P2a |
| **Phase 2c** | TUI 渲染 + Local Map + Pet 情緒衰減 + 首次里程碑事件 | P2a |
| **Phase 3a** | World Map + Zone unlock + Zone Mastery + Loot Crafting + 主動 Item | P2b+P2c |
| **Phase 3b** | Strategy Mode + AI Commentary（Haiku，opt-in） + Snapshot 分享 | P2b |
| **Phase 3c** | Week Streak（Nation 驗證版） | Nation P2P |
| **Phase 4** | Zoa 3D pet animation（tmux split） | P2c |
| **Phase 5** | Pet Breeding + 公開 Profile | P3 |
| **Defer ∞** | 公會 / Raid / 排行榜 / 交易系統 | 架構上不適合 P2P | 

---

## 設計約束

1. **Daemon 可選**：沒有 daemon 時，Phase 1 statusline 繼續正常工作。Daemon 只是 enhancement。
2. **無網路依賴**：基礎戰鬥 / 地圖不需要 API call。AI commentary 需要 ANTHROPIC_API_KEY，無 key 時改用 rule-based 垃圾話。
3. **SQLite only**：所有 game state 在本地，不需要 server。
4. **Terminal agnostic**：crossterm 支援 xterm, iTerm2, Alacritty, tmux。Zoa 3D 是 opt-in（需要支援 sixel 或 kitty graphics protocol 的 terminal）。
5. **Performance**：Tick 計算必須 < 10ms（不能拖慢 terminal response）。LLM call 是 async，不阻塞 tick loop。

---

## 未來可擴充的 MUD 功能（Backlog）

- **公會系統**：同 team 的 user 可以組隊（共用 L1 memory pool）
- **PvE Raid**：特大型技術債 → 需要多人合力擊殺的 Raid Boss
- **Auction House**：把 code pattern loot 分享給 team
- **World Events**：大型重構 = server-wide event，所有人都影響
- **Reputation System**：在特定語言 Zone 的貢獻 → 聲望 → 特殊稱號

---

*"最好的 IDE 是 MUD client"* — 某個老玩家
