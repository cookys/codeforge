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

## 3. AI Commentary（垃圾話系統）

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

## 4. TUI 渲染架構

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

## 5. Daemon 架構

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

```
Claude Code hook → write to unix socket → daemon receives → update game state

Socket path: ~/.codeforge/daemon.sock
Protocol: newline-delimited JSON

{"event": "session_start", "cwd": "/path", "model": "sonnet-4.6"}
{"event": "session_end",   "duration_ms": 3600000}
{"event": "file_saved",    "path": "src/main.rs"}
{"event": "git_commit",    "message": "fix: stuff", "files_changed": 7}
```

---

## Phase 分解

| Phase | 內容 | 前置 |
|-------|------|------|
| **Phase 1** ✅ | Statusline + Pet + Memory CLI | done |
| **Phase 2a** | Daemon 框架 + IPC socket + tick loop | P1 |
| **Phase 2b** | MOB 生成 + 自動戰鬥 + Loot | P2a |
| **Phase 2c** | TUI 渲染 + Local Map | P2a |
| **Phase 3a** | World Map + Zone unlock | P2b+P2c |
| **Phase 3b** | Strategy Mode | P2b |
| **Phase 3c** | AI Commentary（Haiku） | P2a |
| **Phase 4** | Zoa 3D pet animation（tmux split） | P2c |
| **Phase 5** | 多人 / 公會 / leaderboard（可選） | P3 |

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
