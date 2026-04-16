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
 * Output: ~/.claude/session-digests/<YYYY-MM-DD>-<session_id_first8>.json
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
const DIGEST_DIR = path.join(os.homedir(), '.claude', 'session-digests');
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
        });
      }
    }
  }

  return signals;
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
// Cleanup old digests
// ---------------------------------------------------------------------------

function cleanupOldDigests() {
  try {
    if (!fs.existsSync(DIGEST_DIR)) return;

    const cutoff = Date.now() - DIGEST_MAX_AGE_DAYS * 24 * 60 * 60 * 1000;
    const files = fs.readdirSync(DIGEST_DIR);

    for (const file of files) {
      if (!file.endsWith('.json')) continue;
      const filePath = path.join(DIGEST_DIR, file);
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
 * Dedup key: same type + file (or type + count for index-overflow).
 * If a pending duplicate exists, update its created timestamp.
 * Done/rejected items are not duplicated.
 */
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

  // Check if done/rejected version exists — don't re-add
  const doneExists = queue.items.some(existing => {
    if (existing.type !== item.type) return false;
    if (['done', 'rejected'].includes(existing.status)) {
      if (item.type === 'index-overflow') return true;
      return existing.file === item.file;
    }
    return false;
  });
  if (doneExists) return;

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

  // Write digest
  fs.mkdirSync(DIGEST_DIR, { recursive: true });
  const digestFileName = `${dateStr}-${sessionIdShort}.json`;
  const digestPath = path.join(DIGEST_DIR, digestFileName);

  fs.writeFileSync(digestPath, JSON.stringify(digest, null, 2), 'utf8');
  log('INFO', `Wrote digest: ${digestPath}`);

  // Cleanup old digests
  cleanupOldDigests();
}

main().catch(err => {
  log('ERROR', `Unhandled error: ${err.message}\n${err.stack}`);
});
