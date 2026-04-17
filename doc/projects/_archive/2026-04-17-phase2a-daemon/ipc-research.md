# Phase 2a IPC — CEO Research Report

> 建立：2026-04-17
> 研究方法：平行派三個 agent（prior art / Rust impl / 批評審查）→ 彙整
> 觸發：使用者回饋「不要迴避決策、最佳解要多方研究彙整」

---

## Executive Summary

三個 agent 分別從不同角度研究後，產生**出乎意料的收斂 + 一個關鍵質疑**：

1. **Prior art + Rust 實作都指向同一答案**：tokio `UnixListener` + 檔案路徑 socket + newline JSON。這是 tmux/mpd/mpv/emacsclient/dockerd 共同的選擇，tokio 已在 Cargo.toml，零新依賴，測得 0.45ms 往返。
2. **批評審查挑戰了需求本身**：使用者要「即時」，但盤點 Phase 2a 的所有事件（session_start/file_saved/git_commit/session_end），**沒有一個實際上需要 <100ms**。Statusline 本來就是 poll（Claude Code 每次 prompt 才 render），tick 是 60s，寵物是 idle RPG。「即時」是自我強加的約束，沒有使用者可見價值。
3. **新選項浮現**：Option D — SQLite `event_inbox` table + daemon 500ms poll。批評審查發現「兩寫者規則」是部分 cargo-cult，append-only inbox table（hook INSERT、daemon UPDATE `seen_at`）寫入集不重疊，不違反原則。

## Key Decision

使用者需要先回答一個**前置問題**再挑方案：

> **Phase 2a 有任何事件在延遲 60s 內被使用者察覺「卡」嗎？**

- **是** → 接受「即時」需求，走 Option B（Unix socket）
- **否** → 需求不成立，走 Option A2 或 D（延遲 ≤500ms 或 ≤60s）

---

## Findings by Angle

### 1. 業界 prior art（Agent 1: research-analyst）

**Pattern Inventory**（核心發現）

| 系統 | Transport | Protocol | Daemon-down 行為 |
|------|-----------|----------|-----------------|
| tmux | Unix socket (XDG_RUNTIME_DIR) | 長度前綴 binary | 自動 spawn daemon |
| dockerd | Unix socket (/var/run/docker.sock) | HTTP/1.1 + JSON | 立即 fail |
| mpd | Unix socket | newline text | 立即 fail |
| mpv | Unix socket | newline JSON-RPC | EPIPE |
| emacs | Unix socket (XDG_RUNTIME_DIR) | 長度前綴 + FD 傳遞 | 自動 spawn |
| systemd | systemd-managed socket | 透明 pass fd | **零連線遺失**（systemd 緩衝） |

**Emergent patterns**
- **Unix socket + newline JSON 是壓倒性贏家**（tmux/mpd/mpv/emacsclient 各自獨立收斂）
- **自動 spawn daemon 是 tmux/emacs 風格；多數系統拒絕**（因會讓 daemon 不明就裡重啟）
- **File-watching IPC 已被業界淘汰**（Watchman 存在就是因為這條路線失敗）

**值得偷的招**
- **從 tmux 偷**：`$XDG_RUNTIME_DIR/codeforge/daemon.sock`、unlink-before-bind、hook 100ms timeout + fallback 到 `pending_events.jsonl`
- **從 mpd 偷**：`idle` 模式（daemon 推送變化通知給 TUI，eliminate polling）
- **從 systemd 偷**：socket activation（daemon 沒起時，systemd 幫你緩衝連線。重啟零遺失）

**Unknown unknown**：**Linux abstract namespace + `SO_PEERCRED`**。`\0codeforge-daemon` 由 kernel 自動清理（沒有殘留 socket file），`SO_PEERCRED` 驗證對端 UID 省掉檔案權限管理。唯一缺點：Linux only。

### 2. Rust 實作比較（Agent 2: rust-engineer）

**9 個選項的實測矩陣**（摘要）

| # | 選項 | 新 deps | LOC（daemon）| Latency | 裁定 |
|---|------|---------|-------------|---------|------|
| 1 | tokio UnixListener（path socket） | 0 | ~60 | **0.45ms** | ✅ **首推** |
| 2 | tokio UnixListener（abstract ns） | 0 | ~60 | 0.41ms | Linux-only，阻礙 macOS 測試 |
| 3 | `interprocess` crate | +1 | ~70 | ~0.45ms | ✅ 次推（若要 Windows） |
| 4 | mkfifo（named pipe） | 0 | ~80 | 0.2-1ms **但 open 會 block** | ❌ 違反 100ms budget |
| 5 | TCP loopback | 0 | ~65 | 0.50ms | ❌ 任何 local process 可連、多 port file |
| 6 | nng（nanomsg） | +2（含 C lib） | ~50 | ~0.5ms + build | ❌ Node.js 端破壞 zero-dep |
| 7 | zbus（D-Bus） | +4 | ~120 | 1-3ms | ❌ overkill，需 session bus |
| 8 | iceoryx2（shared mem） | +5 | ~100 | <0.1ms | ❌ 60-byte payload 不值得 |
| 9 | `std::os::unix::net` + async-channel | 0 | ~55 | 同 #1 | 等同 #1，但手刻較多 |

**首推：#1（tokio UnixListener + path socket）**
- `tokio` 已在 stack
- Node.js 內建 `net` module 支援 Unix socket，**零 npm deps**（測得 Node.js 端 6 行）
- macOS + Linux 都 OK，不需 `#[cfg]`
- 100ms budget 有 200× margin

**Pitfalls checklist**（對 #1 的實作風險）
- Startup `fs::remove_file` 清 stale socket（先 ignore error，clean first start 不會失敗）
- bind 後立刻 `set_permissions(0o600)`（否則同機其他 user 可寫）
- Daemon 端忽略 EPIPE（fire-and-forget 下對端先斷）
- Node.js 端用 `socket.destroy()` 不是 `socket.end()`（後者等 FIN-ACK +0.3ms）
- 每個 connection 開 `tokio::spawn`，不阻塞 accept loop
- NDJSON framing（BufReader + `lines()`）——不需要 length-prefix
- `ENOENT` vs `ECONNREFUSED` 都要處理（socket file 不存在 vs 存在但沒人聽）

### 3. 批評審查 + 替代方案（Agent 3: architect-reviewer）

**核心挑戰**：A/B/C 三選一是錯的 frame。

Agent 3 盤點 Phase 2a 的事件：
- `session_start`：觸發 welcome-back report。**但 report 是在下次 statusline render 時才顯示**，而 statusline 本來就是 poll（每次 Claude Code prompt）。40ms 到 vs 40s 到對使用者無感。
- `git_commit`：加 XP、可能觸發 boss kill 評論。評論有 1/hr rate limit（spec §3.9）。晚 60s 加 XP = 晚 60s 顯示升級——仍在「很快」範圍。
- `file_saved`：觸發 mob encounter（背景非同步，非使用者互動）。
- `session_end`：flush tick、dream。已經透過 CLI 直接叫。

**結論**：沒有任何事件實際需要 <100ms。

**Option B 的隱藏成本**（失敗模式表，摘錄）

| 觸發 | 事件下場 | 使用者可見 | 恢復 |
|------|---------|-----------|------|
| Daemon 沒跑 | connect ENOENT | 視 hook 是否 swallow | **需 disk buffer → 等於蓋了 A2** |
| Socket file 殘留 | ECONNREFUSED | 同上 | unlink before bind；有 race window |
| Hook 寫到一半被 kill | partial JSON | 事件遺失 | length-prefix 或 discard |
| Daemon 處理慢 → buffer 滿 | write block | **hook 卡住，Claude Code prompt 停** | timeout + drop → 事件遺失 |
| Daemon 寫入時掛掉 | EPIPE/SIGPIPE | hook crash 除非 mask SIGPIPE | 必須 mask |
| NFS / read-only FS | bind 失敗 | daemon 不起 | fallback 到 XDG_RUNTIME_DIR |
| 路徑 >108 bytes | bind 失敗 | 長 `$HOME` 會中（macOS 特別易） | 縮短路徑或 abstract ns |

**「若不接受事件遺失 → 必須做 disk buffer fallback → B 其實是 A2+B。」**

**兩寫者規則稽核**：
- 原規則「只有 daemon 寫 game state」是部分 cargo-cult
- **真正 load-bearing 的部分**：ECS serialization 必須由 daemon 獨占（`pet_snapshot` 表）
- **不 load-bearing 的部分**：append-only `event_inbox` table（hook INSERT、daemon UPDATE `seen_at`）寫入欄位不重疊，SQLite WAL + `busy_timeout=5000` 完美處理。沒有問題。

**Option D（新提案）：SQLite event_inbox + 500ms poll**
```sql
CREATE TABLE event_inbox (
  id INTEGER PRIMARY KEY,
  payload TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  seen_at INTEGER  -- null 表尚未處理
);
CREATE INDEX idx_unseen ON event_inbox(id) WHERE seen_at IS NULL;
```
- Hook：`codeforge emit <event>` 一行 CLI，`INSERT` 進 inbox
- Daemon：500ms poll `MAX(id)`，drain 所有 unseen
- Durable：daemon crash 不遺失事件
- Debugable：`sqlite3 .codeforge/codeforge.db "SELECT * FROM event_inbox"`
- 零新 IPC surface、零 socket 生命週期、零 SIGPIPE、零 NFS 問題

**Agent 3 的 CEO 建議**：先做 D，除非能證明某個事件真的需要 <500ms。

---

## CEO 彙整 + Decision Framework

### Option 重整理

| 方案 | Latency | Daemon-down 行為 | Deps | 失敗面 | CEO 評 |
|------|---------|-----------------|------|--------|-------|
| **A2** JSONL + inotify | ≤60s | 事件累積 | 0 | 延遲高 | 使用者已拒 |
| **B-raw** Unix socket (Agent 2 首推) | 0.45ms | ECONNREFUSED，事件遺失 | 0 | 需 fallback 才 durable | 真即時，但得蓋 disk buffer |
| **B-systemd** Unix socket + systemd activation（Agent 1 提案） | 0.45ms | systemd 緩衝，零遺失 | 0 | 需 systemd（macOS 用 launchd 類似機制） | **即時 + durable，Linux 最佳解** |
| **D** SQLite event_inbox + 500ms poll（Agent 3 提案） | ≤500ms | 事件累積在 inbox table，daemon 起來就 drain | 0 | 兩寫者規則需縮窄 | **簡單 + durable + 足夠即時** |

### 關鍵 insight（以 CEO 視角）

**Agent 3 的 framing 挑戰值得認真看**。盤點事件表後確實找不到需要 <100ms 的場景。但使用者說「及時當然比較好」不完全是效能要求——也可能是「不想感覺有延遲」「不想思考 staleness」。500ms 對任何人都是「即時」，60s 則是「等一下」。

**真正的切分線在 60s vs 500ms，不是 500ms vs 0.5ms。**

- D（500ms）比 A2（60s）快 **120×**，對「即時感」而言已足
- B（0.5ms）比 D 快 1000×，但 Phase 2a 無事件能感受到這差距
- D 的代價是承認兩寫者規則需要縮窄——但稽核後這本來就是對的

### CEO Recommendation

**首推 Option D**，信心 ~70%（Agent 3 的分析很紮實，但需要同意縮窄兩寫者規則）。

**次推 B-systemd**，信心 ~25%（若你堅持即時性有獨立價值，Linux 上 systemd activation 是最乾淨解；macOS 用 launchd 同等機制）。

**不推 B-raw**，因為最終要做 fallback，等於蓋了一份 A2 的邏輯——雙重複雜度。

### 不確定性與可能翻盤

- 若 **Phase 3b TUI** 會接鍵盤事件並要 <50ms 回應，那需要 daemon→TUI 的真即時 channel——但這是**另一條通道**（不是 hook→daemon），應分開設計。
- 若 **Nation P2P (Phase 5)** 有跨 host 即時事件需求，TCP/socket 會進來——但那是 Phase 5，不是 2a。
- 若使用者對「寵物即時反應」有情感偏好（不是工程正確性考量），D 的 500ms 延遲可能心理上不舒服。

---

## Board Decision（請使用者拍板）

請挑一個並回我編號：

1. **D**（SQLite event_inbox + 500ms poll）← CEO 首推
2. **B-systemd**（Unix socket + systemd socket activation for durability）← CEO 次推
3. **B-raw**（純 Unix socket，接受事件遺失 or 自建 disk buffer）
4. **重新盤點需求**（我忽略了某個需要 <100ms 的場景，要補 spec）

選完我會：
- 更新 `doc/specs/codeforge-mud-engine.md` §6（消除與 rpg-engine-spec 的衝突）
- 更新 `.claude/rpg-engine-spec.md` Decision Log（記錄兩寫者規則縮窄 or 維持）
- 鎖定本專案的 KR + Phase 分解

---

## Appendix：完整 Agent Output

三份完整報告另存於 research/ 子目錄（如未建：這份 report 已摘要 95% 內容）。
