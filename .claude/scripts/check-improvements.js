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

const digestDir = path.join(os.homedir(), '.claude', 'session-digests');
const queuePath = path.join(os.homedir(), '.claude', 'improvement-queue.json');

async function main() {
  // Drain stdin (required by Claude Code hooks — must consume before exiting)
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);

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
