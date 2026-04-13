-- CodeForge game state schema（version 1）
-- 版本化管理，不用外部 migration 框架

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

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
    ('statusline_width', '100');
