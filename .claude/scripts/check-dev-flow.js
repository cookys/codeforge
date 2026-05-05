#!/usr/bin/env node
/**
 * check-dev-flow.js — PreToolUse hook
 *
 * Enforces MEMORY.md HARD RULE:
 *   Invoke autopilot:dev-flow BEFORE any code work.
 *
 * Gate behavior:
 *   - Fires on tools that touch code: Edit/Write on Rust sources under
 *     src/, or Bash running cargo / git checkout -b / git branch new.
 *   - Once per session: emits a loud reminder if marker is missing, then
 *     creates the marker so subsequent code-touches don't re-warn.
 *   - Does NOT block tool execution — reminder-only (reduces false-positive
 *     friction when the user genuinely wants a one-off action).
 *
 * Reset mechanism:
 *   check-improvements.js (SessionStart hook) wipes the marker at session
 *   start, ensuring each session re-gates.
 *
 * Marker path: ~/.claude/.dev-flow-warned-<project-hash>
 * Hashed so multiple projects don't collide.
 *
 * Part of: codeforge project
 */
'use strict';

const fs = require('fs');
const path = require('path');
const os = require('os');
const crypto = require('crypto');

// Derive repo root from __filename: <repo>/.claude/scripts/check-dev-flow.js
// → dirname(scripts) → dirname(.claude) → <repo>
const PROJECT_ROOT = path.dirname(path.dirname(path.dirname(__filename)));
const projectHash = crypto.createHash('sha1').update(PROJECT_ROOT).digest('hex').slice(0, 12);
const markerPath = path.join(os.homedir(), '.claude', `.dev-flow-warned-${projectHash}`);

function isCodeTouch(input) {
  const toolName = input.tool_name || '';
  const toolInput = input.tool_input || {};

  // Edit / Write on src/**/*.rs
  if (toolName === 'Edit' || toolName === 'Write') {
    const p = toolInput.file_path || '';
    if (p.includes('/codeforge/src/') && p.endsWith('.rs')) return true;
  }

  // Bash commands that mark code-work boundary
  if (toolName === 'Bash') {
    const cmd = toolInput.command || '';
    if (/\bcargo\s+(check|build|test|run|clippy|fmt)\b/.test(cmd)) return true;
    if (/\bgit\s+checkout\s+-b\b/.test(cmd)) return true;
    if (/\bgit\s+branch\s+[^-]\S*\s+\S/.test(cmd) && /\bnew\b/.test(cmd)) return true;
  }

  return false;
}

async function main() {
  // Drain stdin
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  let input = {};
  try {
    input = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}');
  } catch (_) {}

  if (!isCodeTouch(input)) {
    process.exit(0);
  }

  // Already warned this session? Stay silent.
  if (fs.existsSync(markerPath)) {
    process.exit(0);
  }

  // First code-touch of this session — remind loudly via additionalContext.
  // Exit code 0 = proceed; we inject a reminder via JSON output.
  const reason = [
    '═══════════════════════════════════════════════════════════════',
    'DEV-FLOW GATE: first code-touch detected in this session',
    '═══════════════════════════════════════════════════════════════',
    'MEMORY.md HARD RULE requires autopilot:dev-flow BEFORE any code work.',
    'If you have NOT invoked it this session, STOP and invoke it now.',
    '',
    'Why this matters (from feedback memory 2026-04-17):',
    '  - L-size tasks need project doc + plan doc + task checklist',
    '  - Phase 2+ requires spec read (codeforge-mud-engine.md)',
    '  - Skipping this burns the branch on partial setup',
    '',
    'Marker written → this warning won\'t repeat this session.',
    '═══════════════════════════════════════════════════════════════',
  ].join('\n');

  // Create marker
  try {
    fs.mkdirSync(path.dirname(markerPath), { recursive: true });
    fs.writeFileSync(markerPath, new Date().toISOString() + '\n');
  } catch (_) {}

  // PreToolUse hook: output on stderr (shown to Claude as extra context)
  process.stderr.write(reason + '\n');
  process.exit(0);
}

main().catch((e) => {
  process.stderr.write(`check-dev-flow hook error: ${e && e.message}\n`);
  process.exit(0); // Never block on hook error
});
