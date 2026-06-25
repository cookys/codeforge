#!/usr/bin/env node
'use strict';

/**
 * session-digest.js — PreCompact + SessionEnd hook
 *
 * Reads a Claude Code session transcript (JSONL) from stdin metadata,
 * extracts high-confidence learning signals (error-recovery, user-correction,
 * self-correction), and writes a compact JSON digest.
 *
 * Hook placement:
 *   - PreCompact (primary): fires before context compaction, full transcript available
 *   - SessionEnd (backup): fires on exit/clear/logout (rare — user keeps session open)
 *
 * Output (A′, 2026-06-17): <repoRoot>/.codeforge/digests/<YYYY-MM-DD>-<session_id_first8>.json
 *   repoRoot is discovered by walking up from cwd to the nearest `.codeforge` dir
 *   (like git finds `.git`). If cwd is NOT inside a codeforge project, NO digest is
 *   written — non-init'd dirs never land plaintext on disk (privacy root-cause fix).
 *
 * Pure Node.js, zero external dependencies.
 *
 * Part of: codeforge project
 */

const fs = require('fs');
const path = require('path');
const readline = require('readline');
const os = require('os');

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

const LOG_PATH = path.join(__dirname, 'session-digest.log');

function log(level, msg) {
  try {
    const ts = new Date().toISOString();
    fs.appendFileSync(LOG_PATH, `[${ts}] [${level}] ${msg}\n`);
  } catch (_) {
    // silent
  }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MIN_ASSISTANT_MESSAGES = 10;
const ERROR_WINDOW = 5; // assistant messages to look ahead for recovery
// A′ (2026-06-17): digests land per-repo under <repoRoot>/.codeforge/digests/,
// discovered by walking up from cwd to the nearest .codeforge dir (see
// findCodeforgeRoot / digestDirFor). Non-init'd dirs never write to disk.
const DIGEST_SUBDIR = path.join('.codeforge', 'digests');
const DIGEST_MAX_AGE_DAYS = 30;
const IMPROVEMENT_QUEUE_PATH = path.join(os.homedir(), '.claude', 'improvement-queue.json');
const KNOWLEDGE_OVERSIZE_LINES = 300;
const SKILL_OVERSIZE_LINES = 200;
const INDEX_RECENT_MAX = 10;
const KNOWLEDGE_STALE_DAYS = 30;

// ---------------------------------------------------------------------------
// Pattern definitions
// ---------------------------------------------------------------------------

const USER_CORRECTION_PATTERNS = [
  /不對/u, /錯了/u, /不是這個/u, /不是那個/u, /搞錯/u, /弄錯/u,
  /wrong/i, /incorrect/i, /that's not/i, /不要這樣/u, /你誤解/u, /你理解錯/u,
];

const SELF_CORRECTION_PATTERNS = [
  /我[搞弄]錯/u, /不對.*應該/u, /wait.*actually/i, /I was wrong/i,
  /let me reconsider/i, /之前.*錯/u,
];

const NAVIGATIONAL_SELF_CORRECTION = /let me try a different file/i;

const GIT_ERROR_PATTERNS = [
  /already exists/i, /nothing to commit/i, /up to date/i,
  /not a git repository/i, /branch .* already/i,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function truncate(str, max) {
  if (!str) return '';
  if (str.length <= max) return str;
  return str.slice(0, max);
}

/** Extract primary text from a message content array. */
function extractText(contentArray) {
  if (!Array.isArray(contentArray)) return '';
  const parts = [];
  for (const block of contentArray) {
    if (block.type === 'text' && typeof block.text === 'string') {
      parts.push(block.text);
    }
  }
  return parts.join('\n');
}

/** Extract thinking text from a message content array. */
function extractThinking(contentArray) {
  if (!Array.isArray(contentArray)) return '';
  const parts = [];
  for (const block of contentArray) {
    if (block.type === 'thinking' && typeof block.text === 'string') {
      parts.push(block.text);
    }
  }
  return parts.join('\n');
}

/** Extract tool_use blocks from assistant message content. */
function extractToolUses(contentArray) {
  if (!Array.isArray(contentArray)) return [];
  const uses = [];
  for (const block of contentArray) {
    if (block.type === 'tool_use') {
      uses.push(block);
    }
  }
  return uses;
}

/** Extract tool_result blocks from user message content. */
function extractToolResults(contentArray) {
  if (!Array.isArray(contentArray)) return [];
  const results = [];
  for (const block of contentArray) {
    if (block.type === 'tool_result') {
      results.push(block);
    }
  }
  return results;
}

/** Get text content from a tool_result (may be string or array of blocks). */
function toolResultText(result) {
  if (typeof result.content === 'string') return result.content;
  if (Array.isArray(result.content)) {
    return result.content
      .filter(b => b.type === 'text')
      .map(b => b.text)
      .join('\n');
  }
  return '';
}

/** Derive a file path from a tool_use input. */
function toolFilePath(toolUse) {
  if (!toolUse || !toolUse.input) return null;
  return toolUse.input.file_path || toolUse.input.path || null;
}

/** Derive a command string from a Bash tool_use. */
function toolCommand(toolUse) {
  if (!toolUse || !toolUse.input) return null;
  return toolUse.input.command || null;
}

/** Check if a Bash command is a git command. */
function isGitCommand(cmd) {
  if (!cmd) return false;
  return /^\s*git\s/m.test(cmd) || /&&\s*git\s/m.test(cmd) || /;\s*git\s/m.test(cmd);
}

/** Check if error text looks like a benign git error. */
function isGitError(cmd, errorText) {
  if (!isGitCommand(cmd)) return false;
  return GIT_ERROR_PATTERNS.some(p => p.test(errorText));
}

// ---------------------------------------------------------------------------
// Skill source map loading (config-driven)
// ---------------------------------------------------------------------------

/**
 * Load skill-to-source-path mapping from project config.
 * Looks for .claude/context-mapping.json in the project directory.
 *
 * Expected format:
 * [
 *   { "paths": ["src/api/", "src/routes/"], "skill": "api-dev" },
 *   { "paths": ["src/database/"], "skill": "db-migration" }
 * ]
 *
 * @param {string} cwd - Project working directory
 * @returns {Array<{paths: string[], skill: string}>}
 */
function loadSkillSourceMap(cwd) {
  const jsonPath = path.join(cwd, '.claude', 'context-mapping.json');
  try {
    if (fs.existsSync(jsonPath)) {
      return JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
    }
  } catch (_) {}
  return []; // No mapping = skip skill gap detection
}

// ---------------------------------------------------------------------------
// Noise and signature helpers for error-recovery extraction
// ---------------------------------------------------------------------------

const NOISE_LINE_PATTERNS = [
  /^\s*(?:\[?error\]?:?)?\s*\(?\s*exit\s+(code|status)\s*:?\s*\d+\s*\)?\s*$/i,
  /^\s*(?:\[?error\]?:?)?\s*\(?\s*(command|process|bash\s+command)\s+(failed\s+with\s+exit\s+code|exited\s+with\s+code)\s*:?\s*\d+\.?\s*\)?\s*$/i,
  /^\s*error\s*:\s*exit\s+(code|status)\s*\d+\s*$/i,
];

function isPureExitStatusNoise(errorText) {
  if (!errorText || !errorText.trim()) return true;   // 全空 = 無實質 = 噪音
  const residual = errorText
    .split('\n')
    .filter(line => line.trim() && !NOISE_LINE_PATTERNS.some(p => p.test(line)))
    .join('\n')
    .trim();
  return residual.length === 0;   // 剝樣板後無殘留 → 純噪音
}

// 精確 path detector：避免吃普通英文 word/word；涵蓋帶 line:col 尾綴
// 順序重要（alternation 從左優先）：最具體「含副檔名多段路徑」在前。
const PATH_RE = new RegExp([
  // (R3) 含副檔名的多段路徑，相對或絕對皆涵蓋（最後段須有 .副檔名 → 不吃 read/write）：
  //   src/foo.rs:1:1、lib/foo.rs:9:9、/a/foo.rs:12:3、./x.ts
  '(?:(?:~|\\.{1,2})?\\/)?(?:[\\w.\\-]+\\/)+[\\w.\\-]+\\.\\w+(?::\\d+(?::\\d+)?)?',
  '(?:~|\\.{1,2})\\/[\\w.\\-\\/]+',                            // ~/ ./ ../ 開頭路徑（無副檔名也算）
  '\\/(?:[\\w.\\-]+\\/)+[\\w.\\-]+',                           // 絕對多段無副檔名 /usr/local/bin
].join('|'), 'g');

function recoverySignature(toolName, rawErrorText) {
  const norm = (rawErrorText || '')
    .toLowerCase()
    .replace(PATH_RE, '<path>')              // 精確路徑 → 佔位（不吃 read/write）
    .replace(/:\d+:\d+\b/g, ':<lc>')          // 殘留 line:col
    .replace(/\bline\s+\d+\b/gi, 'line <n>')  // "line 42"
    .replace(/[.\-][0-9a-f]{6,}\b/gi, '<tmp>')// 臨時檔 hash / -XXXXXX
    .replace(/\s+/g, ' ')
    .trim();
  // 完整 normalized 字串即指紋（保留 E\d+ / HTTP status / diag code 等區分性 token；不 slice 前綴）
  return `${toolName}::${norm}`;
}

function withRepeatMeta(signal, count, fileCount) {
  const marker = `[repeat_count=${count} same_session=true${fileCount > 1 ? ` files=${fileCount}` : ''}]`;
  return { ...signal, context: signal.context ? `${marker} ${signal.context}` : marker };
}

// ---------------------------------------------------------------------------
// Signal extractors
// ---------------------------------------------------------------------------

/**
 * Signal A: Error-Recovery
 *
 * We track tool_use -> tool_result pairs. When a tool_result has is_error,
 * we look ahead up to ERROR_WINDOW assistant messages for a successful
 * tool_result targeting the same file/command.
 */
function extractErrorRecoveries(messages) {
  const signals = [];

  // Build a map: tool_use_id -> tool_use block (from assistant messages)
  const toolUseMap = new Map();
  for (const msg of messages) {
    if (msg._role === 'assistant') {
      for (const tu of extractToolUses(msg._content)) {
        if (tu.id) toolUseMap.set(tu.id, tu);
      }
    }
  }

  // Walk messages looking for error tool_results
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg._role !== 'user') continue;

    for (const result of extractToolResults(msg._content)) {
      if (!result.is_error) continue;

      const errorToolUseId = result.tool_use_id;
      const errorToolUse = toolUseMap.get(errorToolUseId);
      if (!errorToolUse) continue;

      const toolName = errorToolUse.name;

      // Filter out Read, Glob, Grep errors
      if (['Read', 'Glob', 'Grep'].includes(toolName)) continue;

      const errorText = toolResultText(result);

      // Filter out cancelled
      if (/cancell?ed/i.test(errorText)) continue;

      // Filter out git errors in Bash
      if (toolName === 'Bash') {
        const cmd = toolCommand(errorToolUse);
        if (isGitError(cmd, errorText)) continue;
      }

      // Look ahead for recovery: next ERROR_WINDOW assistant messages
      const errorFile = toolFilePath(errorToolUse);
      const errorCmd = toolCommand(errorToolUse);
      let assistantCount = 0;
      let recovered = false;

      for (let j = i + 1; j < messages.length && assistantCount < ERROR_WINDOW; j++) {
        const futureMsg = messages[j];
        if (futureMsg._role === 'assistant') {
          assistantCount++;
        }
        if (futureMsg._role !== 'user') continue;

        // Check tool_results in this user message for non-error targeting same file
        for (const futureResult of extractToolResults(futureMsg._content)) {
          if (futureResult.is_error) continue;

          const futureToolUse = toolUseMap.get(futureResult.tool_use_id);
          if (!futureToolUse) continue;
          if (futureToolUse.name !== toolName) continue;

          const futureFile = toolFilePath(futureToolUse);
          const futureCmd = toolCommand(futureToolUse);

          // Match by file path or by similar command prefix
          let match = false;
          if (errorFile && futureFile && errorFile === futureFile) match = true;
          if (toolName === 'Bash' && errorCmd && futureCmd) {
            // Same first token of command
            const errFirst = errorCmd.trim().split(/\s+/)[0];
            const futFirst = futureCmd.trim().split(/\s+/)[0];
            if (errFirst === futFirst) match = true;
          }

          if (match) {
            recovered = true;
            break;
          }
        }
        if (recovered) break;
      }

      if (recovered) {
        if (isPureExitStatusNoise(errorText)) continue;

        // Find context: what was the assistant doing around this error?
        let context = '';
        for (let k = i - 1; k >= 0 && k >= i - 3; k--) {
          if (messages[k]._role === 'assistant') {
            context = truncate(extractText(messages[k]._content), 200);
            break;
          }
        }

        signals.push({
          type: 'error-recovery',
          confidence: 'high',
          tool: toolName,
          error: truncate(errorText, 300),
          file: errorFile || undefined,
          context: context || undefined,
          _rawError: errorText, // R2：raw（未截斷）供 signature；下方聚合後 delete
        });
      }
    }
  }

  const grouped = new Map();   // signature -> { signal, count, files:Set }
  for (const s of signals) {
    const key = recoverySignature(s.tool, s._rawError);   // R2：用 raw、非 truncate 後 s.error
    const g = grouped.get(key);
    if (g) {
      g.count += 1;
      if (s.file) g.files.add(s.file);
    } else {
      grouped.set(key, { signal: s, count: 1, files: new Set(s.file ? [s.file] : []) });
    }
  }
  return [...grouped.values()].map(({ signal, count, files }) => {
    const out = count > 1 ? withRepeatMeta(signal, count, files.size) : signal;
    delete out._rawError;            // R2：不洩漏 raw 進 digest（避免明文/未遮罩外洩 + 體積）
    return out;
  });
}

/**
 * Signal B: User Correction
 */
function extractUserCorrections(messages) {
  const signals = [];

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg._role !== 'user') continue;

    const text = extractText(msg._content);
    if (!text || text.length < 10) continue;
    if (text.includes('<system-reminder>')) continue;
    // Skill invocations inject the skill markdown as a user message; skip those.
    // The Skill tool prepends "Base directory for this skill: <path>" as the first line.
    if (text.startsWith('Base directory for this skill:')) continue;

    const matched = USER_CORRECTION_PATTERNS.some(p => p.test(text));
    if (!matched) continue;

    // Find previous assistant message for context
    let assistantContext = '';
    for (let k = i - 1; k >= 0; k--) {
      if (messages[k]._role === 'assistant') {
        assistantContext = truncate(extractText(messages[k]._content), 200);
        break;
      }
    }

    signals.push({
      type: 'user-correction',
      confidence: 'high',
      correction: truncate(text, 200),
      assistant_context: assistantContext || undefined,
    });
  }

  return signals;
}

/**
 * Signal C: Self-Correction
 */
function extractSelfCorrections(messages) {
  const signals = [];

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg._role !== 'assistant') continue;

    const text = extractText(msg._content);
    const thinking = extractThinking(msg._content);
    const combined = text + '\n' + thinking;

    const matched = SELF_CORRECTION_PATTERNS.some(p => p.test(combined));
    if (!matched) continue;

    // Filter out navigational self-corrections
    if (NAVIGATIONAL_SELF_CORRECTION.test(combined)) continue;

    // Check precondition: follows a failed tool result or change in approach
    let preconditionMet = false;
    for (let k = i - 1; k >= Math.max(0, i - 3); k--) {
      const prev = messages[k];
      if (prev._role === 'user') {
        for (const result of extractToolResults(prev._content)) {
          if (result.is_error) {
            preconditionMet = true;
            break;
          }
        }
      }
      if (preconditionMet) break;
    }

    if (!preconditionMet) continue;

    // Extract the matching text
    const matchText = SELF_CORRECTION_PATTERNS.reduce((best, p) => {
      const m = combined.match(p);
      if (m) {
        // Get surrounding context (up to 300 chars centered on match)
        const idx = combined.indexOf(m[0]);
        const start = Math.max(0, idx - 100);
        const end = Math.min(combined.length, idx + m[0].length + 200);
        const snippet = combined.slice(start, end);
        if (!best || snippet.length > best.length) return snippet;
      }
      return best;
    }, '');

    signals.push({
      type: 'self-correction',
      confidence: 'medium',
      text: truncate(matchText, 300),
    });
  }

  return signals;
}

// ---------------------------------------------------------------------------
// Digest landing (A′): per-repo, privacy-by-default
// ---------------------------------------------------------------------------

/**
 * Walk up from startDir to the nearest ancestor containing a `.codeforge`
 * directory (like git discovering `.git`). Returns the canonical repo root, or
 * null if none found — meaning the session is NOT a codeforge project and no
 * digest is written (privacy root-cause fix: non-init'd dirs never land plaintext).
 * Canonical (realpath) resolution keeps symlinked / equivalent paths from
 * landing under two different roots.
 * @param {string} startDir
 * @returns {string|null} canonical repo root, or null
 */
function findCodeforgeRoot(startDir) {
  if (!startDir) return null;
  let dir;
  try {
    dir = fs.realpathSync(startDir);
  } catch (_) {
    dir = path.resolve(startDir);
  }
  for (;;) {
    let isRepo = false;
    try {
      isRepo = fs.statSync(path.join(dir, '.codeforge')).isDirectory();
    } catch (_) {
      isRepo = false;
    }
    if (isRepo) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null; // reached filesystem root
    dir = parent;
  }
}

/** Per-repo digest dir for a given cwd, or null if cwd is not in a codeforge repo. */
function digestDirFor(cwd) {
  const root = findCodeforgeRoot(cwd);
  return root ? path.join(root, DIGEST_SUBDIR) : null;
}

// ---------------------------------------------------------------------------
// Cleanup old digests (per-repo)
// ---------------------------------------------------------------------------

function cleanupOldDigests(digestDir) {
  try {
    if (!digestDir || !fs.existsSync(digestDir)) return;

    const cutoff = Date.now() - DIGEST_MAX_AGE_DAYS * 24 * 60 * 60 * 1000;
    const files = fs.readdirSync(digestDir);

    for (const file of files) {
      if (!file.endsWith('.json')) continue;
      const filePath = path.join(digestDir, file);
      try {
        const stat = fs.statSync(filePath);
        if (stat.mtimeMs < cutoff) {
          fs.unlinkSync(filePath);
          log('INFO', `Cleaned up old digest: ${file}`);
        }
      } catch (_) {
        // skip individual file errors
      }
    }
  } catch (err) {
    log('WARN', `Cleanup error: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Improvement suggestion generation
// ---------------------------------------------------------------------------

/**
 * Count lines in a file. Returns 0 if file doesn't exist or can't be read.
 */
function countLines(filePath) {
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    return content.split('\n').length;
  } catch (_) {
    return 0;
  }
}

/**
 * Read the existing improvement queue, or create a fresh one.
 */
function readQueue() {
  try {
    if (fs.existsSync(IMPROVEMENT_QUEUE_PATH)) {
      return JSON.parse(fs.readFileSync(IMPROVEMENT_QUEUE_PATH, 'utf8'));
    }
  } catch (_) {
    // corrupted file, start fresh
  }
  return { version: 1, last_updated: new Date().toISOString(), items: [] };
}

/**
 * Write the improvement queue atomically.
 */
function writeQueue(queue) {
  queue.last_updated = new Date().toISOString();
  const dir = path.dirname(IMPROVEMENT_QUEUE_PATH);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(IMPROVEMENT_QUEUE_PATH, JSON.stringify(queue, null, 2), 'utf8');
}

/**
 * Add an item to the queue with deduplication.
 *
 * Status model (written by any consumer — learn skill, check-improvements.js,
 * manual queue surgery):
 *   - pending   : not yet handled
 *   - done      : user acknowledged and work shipped
 *   - rejected  : user decided not to do it (wontfix)
 *   - resolved  : handled / out-of-scope / superseded (used by autopilot:learn
 *                 when the same false positive repeats; semantically equivalent
 *                 to rejected for dedup purposes)
 *
 * Dedup keys:
 *   - For `index-overflow`: same type + any terminal status
 *   - For everything else:  same type + same `file` + any terminal status
 *
 * If a pending duplicate exists, update its created timestamp and mutable
 * fields (lines/count/days_ago/title). Terminal-status items (done /
 * rejected / resolved) block re-adding the same logical issue — this
 * prevents session-digest from re-firing false positives that the user or
 * learn skill already resolved. Prior to 2026-04-22 `resolved` was missing
 * from the blocklist, so items like the i-polish skill-oversize false
 * positive re-fired every session.
 */
const TERMINAL_STATUSES = ['done', 'rejected', 'resolved'];

function addToQueue(queue, item, now) {
  const isDuplicate = (existing) => {
    if (existing.type !== item.type) return false;
    if (item.type === 'index-overflow') {
      return existing.status === 'pending';
    }
    // For file-based types, match on type + file
    return existing.file === item.file && existing.status === 'pending';
  };

  const existingIdx = queue.items.findIndex(isDuplicate);
  if (existingIdx >= 0) {
    // Update timestamp and current values
    queue.items[existingIdx].created = now;
    if (item.lines !== undefined) queue.items[existingIdx].lines = item.lines;
    if (item.count !== undefined) queue.items[existingIdx].count = item.count;
    if (item.days_ago !== undefined) queue.items[existingIdx].days_ago = item.days_ago;
    if (item.title) queue.items[existingIdx].title = item.title;
    return;
  }

  // Check if a terminal-status version exists — don't re-add.
  const terminalExists = queue.items.some(existing => {
    if (existing.type !== item.type) return false;
    if (TERMINAL_STATUSES.includes(existing.status)) {
      if (item.type === 'index-overflow') return true;
      return existing.file === item.file;
    }
    return false;
  });
  if (terminalExists) return;

  const idx = queue.items.filter(i => i.created === now).length;
  queue.items.push({
    id: `imp-${Date.now()}-${idx}`,
    created: now,
    source: 'session-digest',
    status: 'pending',
    ...item,
  });
}

/**
 * Generate improvement suggestions based on mechanical checks.
 */
function generateImprovements(cwd) {
  if (!cwd) return;

  const queue = readQueue();
  const now = new Date().toISOString();
  let added = 0;

  // Check 1: Knowledge file size
  const knowledgeDir = path.join(cwd, '.claude', 'knowledge');
  try {
    if (fs.existsSync(knowledgeDir)) {
      const files = fs.readdirSync(knowledgeDir).filter(f =>
        f.endsWith('.md') && f !== 'INDEX.md' && !f.startsWith('.')
      );
      for (const file of files) {
        const filePath = path.join(knowledgeDir, file);
        // Skip if it's a directory (e.g. archive/)
        try {
          if (fs.statSync(filePath).isDirectory()) continue;
        } catch (_) { continue; }

        const lines = countLines(filePath);
        if (lines > KNOWLEDGE_OVERSIZE_LINES) {
          addToQueue(queue, {
            type: 'knowledge-oversize',
            title: `${file} exceeds ${KNOWLEDGE_OVERSIZE_LINES} lines (currently ${lines} lines)`,
            action: 'Archive old entries or split file',
            file: `.claude/knowledge/${file}`,
            lines,
          }, now);
          added++;
        }
      }
    }
  } catch (err) {
    log('WARN', `Knowledge size check error: ${err.message}`);
  }

  // Check 2: Skill file size
  const skillsDir = path.join(cwd, '.claude', 'skills');
  try {
    if (fs.existsSync(skillsDir)) {
      const skillDirs = fs.readdirSync(skillsDir);
      for (const skillName of skillDirs) {
        const skillMdPath = path.join(skillsDir, skillName, 'SKILL.md');
        if (!fs.existsSync(skillMdPath)) continue;

        const lines = countLines(skillMdPath);
        if (lines > SKILL_OVERSIZE_LINES) {
          addToQueue(queue, {
            type: 'skill-oversize',
            title: `${skillName} exceeds ${SKILL_OVERSIZE_LINES} lines (currently ${lines} lines)`,
            action: 'Split into sub-skills or use references',
            file: `.claude/skills/${skillName}/SKILL.md`,
            lines,
          }, now);
          added++;
        }
      }
    }
  } catch (err) {
    log('WARN', `Skill size check error: ${err.message}`);
  }

  // Check 3: INDEX.md recent learning overflow
  const indexPath = path.join(knowledgeDir, 'INDEX.md');
  try {
    if (fs.existsSync(indexPath)) {
      const content = fs.readFileSync(indexPath, 'utf8');
      const lines = content.split('\n');

      // Find "Recent Learnings" section (supports both Chinese and English)
      let inRecentSection = false;
      let inTable = false;
      let headerSeen = false;
      let dataRows = 0;

      for (const line of lines) {
        // Match both Chinese and English section names
        if (/^##\s.*(最近學習|Recent Learnings)/i.test(line)) {
          inRecentSection = true;
          continue;
        }
        if (inRecentSection && /^##\s/.test(line)) {
          // Hit next section
          break;
        }
        if (!inRecentSection) continue;

        if (line.startsWith('|')) {
          if (!inTable) {
            // First | line is header
            inTable = true;
            headerSeen = false;
            continue;
          }
          if (!headerSeen) {
            // Second | line is separator (|---|---|...)
            headerSeen = true;
            continue;
          }
          // Data row
          dataRows++;
        }
      }

      if (dataRows > INDEX_RECENT_MAX) {
        addToQueue(queue, {
          type: 'index-overflow',
          title: `INDEX.md Recent Learnings has ${dataRows} entries (limit ${INDEX_RECENT_MAX})`,
          action: 'Remove oldest entries',
          count: dataRows,
        }, now);
        added++;
      }
    }
  } catch (err) {
    log('WARN', `INDEX.md overflow check error: ${err.message}`);
  }

  // Check 4: last-verified staleness
  try {
    if (fs.existsSync(knowledgeDir)) {
      const files = fs.readdirSync(knowledgeDir).filter(f =>
        f.endsWith('.md') && f !== 'INDEX.md' && !f.startsWith('.')
      );
      const now_ms = Date.now();

      for (const file of files) {
        const filePath = path.join(knowledgeDir, file);
        try {
          if (fs.statSync(filePath).isDirectory()) continue;
        } catch (_) { continue; }

        try {
          const content = fs.readFileSync(filePath, 'utf8');
          const firstLine = content.split('\n')[0] || '';
          const match = firstLine.match(/<!--\s*last-verified:\s*(\d{4}-\d{2}-\d{2})\s*-->/);
          if (match) {
            const verifiedDate = new Date(match[1] + 'T00:00:00Z');
            const daysAgo = Math.floor((now_ms - verifiedDate.getTime()) / (24 * 60 * 60 * 1000));
            if (daysAgo > KNOWLEDGE_STALE_DAYS) {
              addToQueue(queue, {
                type: 'knowledge-stale',
                title: `${file} not verified in ${daysAgo} days (last: ${match[1]})`,
                action: 'Review content accuracy, update last-verified',
                file: `.claude/knowledge/${file}`,
                last_verified: match[1],
                days_ago: daysAgo,
              }, now);
              added++;
            }
          }
        } catch (_) {
          // skip individual file errors
        }
      }
    }
  } catch (err) {
    log('WARN', `Staleness check error: ${err.message}`);
  }

  if (added > 0 || queue.items.some(i => i.status === 'pending')) {
    writeQueue(queue);
    const pending = queue.items.filter(i => i.status === 'pending').length;
    log('INFO', `Improvement queue: ${added} checks triggered, ${pending} total pending items`);
  }
}

// ---------------------------------------------------------------------------
// Transcript-aware improvement checks (Level 2)
// ---------------------------------------------------------------------------

/**
 * Analyze transcript for skill coverage gaps and repeated errors.
 * Writes findings to the improvement queue.
 *
 * Skill-to-source mapping is loaded from .claude/context-mapping.json.
 */
function generateTranscriptImprovements(cwd, messages) {
  if (!cwd || !messages || messages.length === 0) return;

  const queue = readQueue();
  const now = new Date().toISOString();
  let added = 0;

  // Load skill source map from project config
  const SKILL_SOURCE_MAP = loadSkillSourceMap(cwd);

  // Build sets: which files were edited, which skills were invoked
  const editedFiles = new Set();
  const invokedSkills = new Set();
  const errorCounts = new Map(); // error pattern -> count

  const toolUseMap = new Map();
  for (const msg of messages) {
    if (msg._role === 'assistant') {
      for (const tu of extractToolUses(msg._content)) {
        if (tu.id) toolUseMap.set(tu.id, tu);

        // Track Skill invocations
        if (tu.name === 'Skill' && tu.input && tu.input.skill) {
          invokedSkills.add(tu.input.skill);
        }

        // Track edited files
        if (['Edit', 'Write'].includes(tu.name) && tu.input) {
          const fp = tu.input.file_path;
          if (fp) editedFiles.add(fp);
        }
      }
    }

    // Track error patterns
    if (msg._role === 'user') {
      for (const result of extractToolResults(msg._content)) {
        if (!result.is_error) continue;
        const tu = toolUseMap.get(result.tool_use_id);
        if (!tu) continue;
        // Skip navigational errors
        if (['Read', 'Glob', 'Grep'].includes(tu.name)) continue;

        const errorText = toolResultText(result);
        if (/cancell?ed/i.test(errorText)) continue;
        if (/Sibling tool call errored/i.test(errorText)) continue;
        if (tu.name === 'Bash' && isGitError(toolCommand(tu), errorText)) continue;

        // Extract error signature (first line, truncated)
        const sig = `${tu.name}:${(errorText.split('\n')[0] || '').substring(0, 80)}`;
        errorCounts.set(sig, (errorCounts.get(sig) || 0) + 1);
      }
    }
  }

  // Check: Skill coverage gaps
  for (const mapping of SKILL_SOURCE_MAP) {
    const edited = [...editedFiles].some(fp =>
      mapping.paths.some(p => fp.includes(p))
    );
    if (edited && !invokedSkills.has(mapping.skill)) {
      addToQueue(queue, {
        type: 'skill-gap',
        title: `Modified ${mapping.paths[0]} but did not invoke ${mapping.skill}`,
        action: `Invoke ${mapping.skill} before modifying this area`,
        file: mapping.paths[0],
        skill: mapping.skill,
      }, now);
      added++;
    }
  }

  // Check: Repeated errors (3+ times with same signature)
  for (const [sig, count] of errorCounts) {
    if (count >= 3) {
      addToQueue(queue, {
        type: 'repeated-error',
        title: `Same error type occurred ${count} times: ${sig.substring(0, 100)}`,
        action: 'Consider recording to knowledge to prevent recurrence',
        error_signature: sig,
        count,
      }, now);
      added++;
    }
  }

  if (added > 0) {
    // Tag this run's new/updated items with their origin project root so the
    // codeforge-clone-only SessionStart hook (check-improvements.js) can scope
    // surfaced suggestions to the current project, instead of nagging about
    // other projects' items in the shared global improvement-queue.
    for (const it of queue.items) {
      if (it.created === now && it.project === undefined) it.project = cwd;
    }
    writeQueue(queue);
    log('INFO', `Transcript checks: ${added} new items (${editedFiles.size} files edited, ${invokedSkills.size} skills invoked, ${errorCounts.size} error types)`);
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  // Read hook input from stdin
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  const stdinText = Buffer.concat(chunks).toString('utf8').trim();

  if (!stdinText) {
    log('INFO', 'No stdin input, exiting.');
    return;
  }

  let input;
  try {
    input = JSON.parse(stdinText);
  } catch (err) {
    log('ERROR', `Failed to parse stdin JSON: ${err.message}`);
    return;
  }

  const { session_id, transcript_path, cwd } = input;

  // Generate improvement suggestions (independent of transcript content)
  try {
    generateImprovements(cwd);
  } catch (err) {
    log('WARN', `Improvement generation error: ${err.message}`);
  }

  if (!transcript_path) {
    log('ERROR', 'No transcript_path in input');
    return;
  }

  if (!fs.existsSync(transcript_path)) {
    log('ERROR', `Transcript not found: ${transcript_path}`);
    return;
  }

  log('INFO', `Processing session ${session_id || 'unknown'}, transcript: ${transcript_path}`);

  // Stream-parse the JSONL transcript
  const messages = [];
  const rl = readline.createInterface({
    input: fs.createReadStream(transcript_path, { encoding: 'utf8' }),
    crlfDelay: Infinity,
  });

  for await (const line of rl) {
    if (!line.trim()) continue;
    try {
      const entry = JSON.parse(line);
      const role = entry.message?.role;
      if (role === 'assistant' || role === 'user') {
        messages.push({
          _role: role,
          _content: entry.message?.content || [],
          _cwd: entry.cwd,
          _timestamp: entry.timestamp,
        });
      }
    } catch (_) {
      // skip malformed lines
    }
  }

  // Count assistant messages
  const assistantCount = messages.filter(m => m._role === 'assistant').length;
  log('INFO', `Parsed ${messages.length} messages (${assistantCount} assistant)`);

  // Run transcript-aware improvement checks (Level 2) — always, even for short sessions
  try {
    generateTranscriptImprovements(cwd, messages);
  } catch (err) {
    log('WARN', `Transcript improvement check error: ${err.message}`);
  }

  if (assistantCount < MIN_ASSISTANT_MESSAGES) {
    log('INFO', `Only ${assistantCount} assistant messages (< ${MIN_ASSISTANT_MESSAGES}), skipping digest.`);
    return;
  }

  // Extract signals
  const errorRecoveries = extractErrorRecoveries(messages);
  const userCorrections = extractUserCorrections(messages);
  const selfCorrections = extractSelfCorrections(messages);
  const signals = [...errorRecoveries, ...userCorrections, ...selfCorrections];

  log('INFO', `Extracted ${signals.length} signals: ${errorRecoveries.length} error-recovery, ${userCorrections.length} user-correction, ${selfCorrections.length} self-correction`);

  if (signals.length === 0) {
    log('INFO', 'No signals extracted, skipping digest.');
    return;
  }

  // Build digest
  const sessionIdShort = (session_id || 'unknown').slice(0, 8);
  const dateStr = new Date().toISOString().slice(0, 10);

  const digest = {
    session_id: session_id || 'unknown',
    date: dateStr,
    cwd: cwd || null,
    assistant_message_count: assistantCount,
    signals,
    processed: false,
  };

  // Write digest — A′: land per-repo, skip entirely if cwd is not a codeforge project.
  const digestDir = digestDirFor(cwd);
  if (!digestDir) {
    log('INFO', `cwd not inside a .codeforge repo (${cwd || 'unknown'}), skipping digest write.`);
    return;
  }
  fs.mkdirSync(digestDir, { recursive: true });
  const digestFileName = `${dateStr}-${sessionIdShort}.json`;
  const digestPath = path.join(digestDir, digestFileName);

  // Atomic write: write a temp file then rename into place, so a concurrent
  // `dream` ingest never reads a half-written digest (and the ingest's
  // mtime-guard can reliably detect a fresh rewrite). rename(2) is atomic on
  // the same filesystem.
  const tmpPath = `${digestPath}.${process.pid}.tmp`;
  fs.writeFileSync(tmpPath, JSON.stringify(digest, null, 2), 'utf8');
  fs.renameSync(tmpPath, digestPath);
  log('INFO', `Wrote digest: ${digestPath}`);

  // Cleanup old digests in this repo's digest dir.
  cleanupOldDigests(digestDir);
}

if (require.main === module) {
  main().catch(err => {
    log('ERROR', `Unhandled error: ${err.message}\n${err.stack}`);
  });
} else {
  module.exports = {
    isPureExitStatusNoise,
    recoverySignature,
    withRepeatMeta,
    extractErrorRecoveries,
  };
}
