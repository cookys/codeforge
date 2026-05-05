#!/usr/bin/env node
/**
 * check-improvements.js — SessionStart hook
 *
 * Checks for:
 * 1. Unprocessed session digests (from PreCompact/SessionEnd) -- escalates if >=3
 * 2. Pending improvement suggestions
 *
 * Outputs reminders to Claude Code context. Silent if nothing pending.
 *
 * Pure Node.js, zero external dependencies.
 *
 * Part of: codeforge project
 */
'use strict';
const fs = require('fs');
const path = require('path');
const os = require('os');

const crypto = require('crypto');

const digestDir = path.join(os.homedir(), '.claude', 'session-digests');
const queuePath = path.join(os.homedir(), '.claude', 'improvement-queue.json');

// Derive repo root from __filename: <repo>/.claude/scripts/check-improvements.js
// → dirname(scripts) → dirname(.claude) → <repo>
const PROJECT_ROOT = path.dirname(path.dirname(path.dirname(__filename)));
const projectHash = crypto.createHash('sha1').update(PROJECT_ROOT).digest('hex').slice(0, 12);
const devFlowMarker = path.join(os.homedir(), '.claude', `.dev-flow-warned-${projectHash}`);

async function main() {
  // Drain stdin (required by Claude Code hooks — must consume before exiting)
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);

  // Reset dev-flow gate marker so check-dev-flow.js re-warns on first
  // code-touch of this new session
  try {
    if (fs.existsSync(devFlowMarker)) fs.unlinkSync(devFlowMarker);
  } catch (_) {}

  const output = [];

  // Check 1: Unprocessed session digests (escalate if >=3)
  try {
    if (fs.existsSync(digestDir)) {
      const files = fs.readdirSync(digestDir).filter(f => f.endsWith('.json'));
      let unprocessed = 0;
      for (const file of files) {
        try {
          const data = JSON.parse(fs.readFileSync(path.join(digestDir, file), 'utf8'));
          if (data.processed === false) unprocessed++;
        } catch (_) {}
      }
      if (unprocessed >= 3) {
        output.push(`WARNING: ${unprocessed} unprocessed digests -- invoke session-digest skill BEFORE starting dev task`);
      } else if (unprocessed > 0) {
        output.push(`${unprocessed} unprocessed session digest(s)`);
      }
    }
  } catch (_) {}

  // Check 2: Pending improvement suggestions
  try {
    if (fs.existsSync(queuePath)) {
      const queue = JSON.parse(fs.readFileSync(queuePath, 'utf8'));
      const pending = queue.items.filter(i => i.status === 'pending');
      if (pending.length > 0) {
        output.push(`${pending.length} pending improvement(s)`);
      }
    }
  } catch (_) {}

  if (output.length > 0) {
    console.log(output.join('\n'));
  }
}

main().catch(() => {});
