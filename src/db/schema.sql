-- CodeForge game state schema（version 1）
-- 版本化管理，不用外部 migration 框架

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- version 需 UNIQUE，INSERT OR IGNORE 才有依據（修正 Phase 1 疏失）
-- 先清除 Phase 1 疏失期間留下的重複 row，再建 UNIQUE index；
-- UNIQUE index 若看到既存重複資料會直接 fail
DELETE FROM schema_version
WHERE rowid NOT IN (
    SELECT MIN(rowid) FROM schema_version GROUP BY version
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_schema_version_version
    ON schema_version(version);

-- 插入初始版本（若不存在）
INSERT OR IGNORE INTO schema_version (version) VALUES (1);

-- ─── Pet 系統 ────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS pet (
    id          INTEGER PRIMARY KEY CHECK (id = 1), -- 單寵物（Phase 1）
    village     TEXT NOT NULL,                       -- rust/go/python/typescript/javascript
    name        TEXT NOT NULL,
    level       INTEGER NOT NULL DEFAULT 1,
    xp          INTEGER NOT NULL DEFAULT 0,
    xp_to_next  INTEGER NOT NULL DEFAULT 100,
    -- 五維屬性（來自 PowerProvider + memory activity）
    atk         INTEGER NOT NULL DEFAULT 10,  -- Fluency
    hp          INTEGER NOT NULL DEFAULT 10,  -- Activity
    def         INTEGER NOT NULL DEFAULT 10,  -- Integrity
    sup         INTEGER NOT NULL DEFAULT 10,  -- Reach
    ver         INTEGER NOT NULL DEFAULT 10,  -- Breadth
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── XP 事件日誌 ─────────────────────────────────────────

CREATE TABLE IF NOT EXISTS xp_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT NOT NULL,  -- 'learn' | 'dream_compile' | 'search' | 'ingest' | 'session_end'
    xp_delta    INTEGER NOT NULL,
    detail      TEXT,           -- 觸發細節（如 learn 的 topic）
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── 徽章 ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS badges (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    badge_id    TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    earned_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── Dream Cycle 執行記錄 ────────────────────────────────

CREATE TABLE IF NOT EXISTS dream_runs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    operations      TEXT NOT NULL,  -- JSON array: ["compile","lint","dedup","absorb","decay","track"]
    signals_compiled INTEGER NOT NULL DEFAULT 0,
    l1_created      INTEGER NOT NULL DEFAULT 0,
    l1_updated      INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER,
    status          TEXT NOT NULL DEFAULT 'completed', -- 'completed' | 'failed'
    error           TEXT,
    ran_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── FTS5 搜尋索引（L1 知識庫）────────────────────────────

CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    file_path,   -- .codeforge/store/ 相對路徑
    title,
    body,
    tags,
    tokenize = 'porter unicode61'
);

-- ─── Signal 編譯狀態追蹤 ────────────────────────────────

CREATE TABLE IF NOT EXISTS signal_cursors (
    source_file TEXT PRIMARY KEY,   -- signals/*.jsonl 檔案名
    last_offset INTEGER NOT NULL DEFAULT 0,  -- 已編譯到的 byte offset
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── Settings ────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 預設設定
INSERT OR IGNORE INTO settings (key, value) VALUES
    ('theme', 'amber'),
    ('statusline_width', '100'),
    -- Phase 3c: AI Commentary opt-in flag. '0' = off, '1' = on. Env var
    -- CODEFORGE_COMMENTARY=1 also enables; the daemon ORs the two sources.
    ('commentary_opt_in', '0');

-- ═══════════════════════════════════════════════════════════════
-- Phase 2a — Daemon schema (version 2)
-- 新增 tables：daemon 的 ECS serialize 目的地、zone state、combat log、
-- hook→daemon event 通道（Option D）、tick anchor
-- ═══════════════════════════════════════════════════════════════

INSERT OR IGNORE INTO schema_version (version) VALUES (2);

-- ─── Pet Snapshot（daemon tick 序列化目的地）────────────────
-- 由 daemon 獨占寫入；CLI statusline 唯讀。
-- Phase 1 的 `pet` 表保留作 legacy；Phase 2+ 逐步遷移 statusline 讀這張表

CREATE TABLE IF NOT EXISTS pet_snapshot (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    village         TEXT NOT NULL,
    level           INTEGER NOT NULL,
    hp              INTEGER NOT NULL,
    hp_max          INTEGER NOT NULL,
    xp              INTEGER NOT NULL,
    xp_to_next      INTEGER NOT NULL,
    atk             INTEGER NOT NULL,
    def             INTEGER NOT NULL,
    sup             INTEGER NOT NULL,
    ver             INTEGER NOT NULL,
    last_message    TEXT,
    -- Phase 3d: Mood（0..=100），預設 60 落在「正常」區間
    mood            INTEGER NOT NULL DEFAULT 60
                      CHECK (mood BETWEEN 0 AND 100),
    mood_tick_stamp INTEGER NOT NULL DEFAULT 0,
    -- Phase 3b: Strategy（aggressive / defensive / explorer / scholar）
    -- 預設 explorer：Phase 2b 既有戰鬥行為的 1.0x/1.0x 中性設定，
    -- 讓既有 DB 升級後的戰鬥結果不變。
    strategy        TEXT NOT NULL DEFAULT 'explorer',
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── Game World（zone 狀態、Zone Mastery 聲望）─────────────
-- zone_id: rust / python / typescript / go / javascript

CREATE TABLE IF NOT EXISTS game_world (
    zone_id       TEXT PRIMARY KEY,
    unlocked      INTEGER NOT NULL DEFAULT 0,   -- 0 = locked, 1 = unlocked
    kill_count    INTEGER NOT NULL DEFAULT 0,   -- 累積擊殺（Zone Mastery 依據）
    last_visit_at TEXT
);

-- ─── Combat Log（MOB 擊殺紀錄，Phase 2b 會填）──────────────

CREATE TABLE IF NOT EXISTS combat_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    zone_id     TEXT NOT NULL,
    mob_name    TEXT NOT NULL,
    mob_kind    TEXT NOT NULL,                  -- weak / medium / boss
    xp_gained   INTEGER NOT NULL DEFAULT 0,
    loot        TEXT,                           -- JSON blob
    occurred_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─── Event Inbox（hook → daemon 通道，Option D）────────────
-- 兩寫者縮窄版：hook INSERT (id/payload/created_at)，
-- daemon UPDATE (seen_at)。欄位不重疊，SQLite WAL 直接處理。

CREATE TABLE IF NOT EXISTS event_inbox (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    payload    TEXT NOT NULL,                   -- JSON blob
    created_at INTEGER NOT NULL,                -- unix ts (seconds)
    seen_at    INTEGER                          -- unix ts；NULL = 未處理
);

CREATE INDEX IF NOT EXISTS idx_event_inbox_unseen
    ON event_inbox(id) WHERE seen_at IS NULL;

-- ─── Last Tick Anchor（daemon crash recovery）──────────────
-- 由 daemon 每 tick 完成後更新；catch-up 邏輯從這裡讀起點

CREATE TABLE IF NOT EXISTS last_tick_at (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    tick_at    INTEGER NOT NULL,                -- unix ts 最後成功 tick
    tick_count INTEGER NOT NULL DEFAULT 0       -- 累積 tick 次數
);

-- ═══════════════════════════════════════════════════════════════
-- Phase 2b — Combat schema (version 3)
-- MOB 生成、戰鬥記錄（用 Phase 2a 既有 combat_log）、loot inventory
-- ═══════════════════════════════════════════════════════════════

INSERT OR IGNORE INTO schema_version (version) VALUES (3);

-- ─── MOBs（daemon scanner 生成，daemon combat 擊殺）────────
-- 來源：原始碼 heuristic（TODO count / function length / dead import）
-- 六類 kind 對應 spec §2：boss / elite / zombie / ghost / doppelganger / void
-- defeated_at NULL = alive；UPDATE seen_at 模式類似 event_inbox

CREATE TABLE IF NOT EXISTS mobs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    zone_id     TEXT NOT NULL,                   -- pet village id (rust/python/...)
    kind        TEXT NOT NULL,                   -- boss | elite | zombie | ghost | doppelganger | void
    name        TEXT NOT NULL,                   -- e.g. "src/auth.rs" or "TODO cluster @ handlers/"
    hp          INTEGER NOT NULL,
    hp_max      INTEGER NOT NULL,
    atk         INTEGER NOT NULL,
    def         INTEGER NOT NULL,
    difficulty  INTEGER NOT NULL DEFAULT 1,
    spawned_at  INTEGER NOT NULL,                -- unix ts
    defeated_at INTEGER,                         -- NULL = alive
    origin_path TEXT                             -- Phase 2c: repo-relative path (nullable for P2b legacy rows)
);

-- Partial index speeds up the per-tick "alive mobs in zone" scan
CREATE INDEX IF NOT EXISTS idx_mobs_alive
    ON mobs(zone_id) WHERE defeated_at IS NULL;

-- Avoid duplicate MOBs for the same (zone, kind, name) — scanner reruns
-- should upsert HP, not spawn new rows
CREATE UNIQUE INDEX IF NOT EXISTS idx_mobs_unique_alive
    ON mobs(zone_id, kind, name) WHERE defeated_at IS NULL;

-- ─── Loot Inventory（擊殺後落地的非 XP loot）────────────
-- XP 直接落到 pet_snapshot.xp；非 XP loot（Item / Tome / Crystal / ...）
-- 進這張表，kind+name UNIQUE 聚合 quantity

CREATE TABLE IF NOT EXISTS loot_inventory (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    kind              TEXT NOT NULL,             -- item | tome | scroll | crystal | fragment | gem | skill_point
    name              TEXT NOT NULL,             -- "Rare Item" / "TODO Cleaner" / "Pattern Fragment" / ...
    quantity          INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    first_acquired_at INTEGER NOT NULL,          -- unix ts
    last_acquired_at  INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_loot_dedupe
    ON loot_inventory(kind, name);

-- ═══════════════════════════════════════════════════════════════
-- Phase 2c — TUI + Local Map (version 4)
-- mobs.origin_path (nullable TEXT) added inline in the CREATE TABLE
-- above for greenfield installs. Upgrade path for existing Phase 2b
-- DBs is handled in src/db/mod.rs::migrations::run via PRAGMA
-- table_info(mobs) + conditional ALTER — SQLite has no
-- IF NOT EXISTS for ADD COLUMN.
-- ═══════════════════════════════════════════════════════════════

INSERT OR IGNORE INTO schema_version (version) VALUES (4);

-- ═══════════════════════════════════════════════════════════════
-- Phase 3d — Stickiness (version 5)
-- pet_snapshot.mood + pet_snapshot.mood_tick_stamp added inline in
-- the CREATE TABLE above for greenfield installs. Upgrade path for
-- existing DBs is handled in src/db/mod.rs::migrations::run (SQLite
-- has no ALTER TABLE ADD COLUMN IF NOT EXISTS).
-- first_events is a new table — CREATE TABLE IF NOT EXISTS is enough.
-- ═══════════════════════════════════════════════════════════════

-- ─── First-Time Events（spec §3.8 — 一次性情感里程碑）──────────
-- event_id PRIMARY KEY 天然 idempotent：daemon 用 INSERT OR IGNORE，
-- 就算分身重啟也不會重複觸發。

CREATE TABLE IF NOT EXISTS first_events (
    event_id     TEXT PRIMARY KEY,   -- e.g. "first_boss_kill", "first_level_10"
    triggered_at TEXT NOT NULL,      -- ISO timestamp
    tick_count   INTEGER NOT NULL,   -- daemon tick when triggered
    payload      TEXT                -- JSON blob, optional context (zone_id/mob_name)
);

INSERT OR IGNORE INTO schema_version (version) VALUES (5);

-- ═══════════════════════════════════════════════════════════════
-- Phase 3b — Strategy Mode (version 6)
-- pet_snapshot.strategy added inline in the CREATE TABLE above for
-- greenfield installs. Upgrade path for existing DBs is handled in
-- src/db/mod.rs::migrations::run (SQLite has no ALTER TABLE ADD
-- COLUMN IF NOT EXISTS — same pattern as mood/origin_path).
-- ═══════════════════════════════════════════════════════════════

INSERT OR IGNORE INTO schema_version (version) VALUES (6);

-- ═══════════════════════════════════════════════════════════════
-- Phase 3c — AI Commentary (version 7)
-- Three new tables; no ALTERs needed for existing installs because
-- CREATE TABLE IF NOT EXISTS covers both greenfield and upgrade.
-- settings.commentary_opt_in is seeded via INSERT OR IGNORE above.
-- ═══════════════════════════════════════════════════════════════

-- ─── Commentary Feed (display + history view) ───────────────────
-- Append-only log of phrases emitted to the player. Statusline and
-- TUI read the latest N rows; ordering by id DESC (autoinc) is
-- stable and matches emission time without requiring a created_at
-- index separately from the PK.

CREATE TABLE IF NOT EXISTS commentary_feed (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at      INTEGER NOT NULL,               -- unix seconds
    tick_count      INTEGER NOT NULL,               -- daemon tick_count snapshot
    kind            TEXT NOT NULL,                  -- boss_kill | level_up | session_long | long_idle | zone_unlock | manual_test
    tone            TEXT NOT NULL,                  -- "{village}:{strategy}" composite key
    phrase          TEXT NOT NULL,
    phrase_hash     TEXT NOT NULL,                  -- SHA-1 of normalized phrase (dedup key)
    source          TEXT NOT NULL,                  -- 'haiku' | 'rule'
    displayed_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_commentary_feed_created
    ON commentary_feed(created_at DESC);

-- ─── Commentary History (30-day phrase dedup) ──────────────────
-- One row per (kind, phrase_hash). SHA-1 over normalized text keeps
-- punctuation/whitespace variants from bloating the pool. last_used_at
-- drives the 30-day window filter.

CREATE TABLE IF NOT EXISTS commentary_history (
    phrase_hash   TEXT NOT NULL,
    kind          TEXT NOT NULL,
    first_used_at INTEGER NOT NULL,
    last_used_at  INTEGER NOT NULL,
    use_count     INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (phrase_hash, kind)
);

CREATE INDEX IF NOT EXISTS idx_commentary_history_recent
    ON commentary_history(last_used_at DESC);

-- ─── Commentary Budget (global 1/hour rate limit) ──────────────
-- Single-row state: last_emit_at is unix seconds of last successful
-- emission. A reservation model: daemon writes last_emit_at before
-- dispatching a Haiku call; on failure, unreserve() restores the
-- prior value so a transient HTTP error doesn't burn the 1h window.

CREATE TABLE IF NOT EXISTS commentary_budget (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    last_emit_at     INTEGER,                       -- nullable: never emitted yet
    consecutive_skip INTEGER NOT NULL DEFAULT 0     -- telemetry: rate-limited triggers
);

-- Seed the single row so upsert paths can UPDATE unconditionally.
INSERT OR IGNORE INTO commentary_budget (id, last_emit_at, consecutive_skip)
    VALUES (1, NULL, 0);

INSERT OR IGNORE INTO schema_version (version) VALUES (7);
