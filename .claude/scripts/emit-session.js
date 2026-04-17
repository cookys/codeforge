#!/usr/bin/env node
/**
 * emit-session.js — SessionStart / SessionEnd hook
 *
 * Shells out to `codeforge emit <event>` with session metadata from stdin.
 * Runs silently; never fails the hook chain (codeforge binary missing or
 * daemon down → no-op).
 *
 * Usage (from settings.json):
 *   node .claude/scripts/emit-session.js session_start
 *   node .claude/scripts/emit-session.js session_end
 *
 * Part of: codeforge project (Phase 2a P7)
 */
'use strict';
const { spawnSync } = require('child_process');

async function main() {
  const event = process.argv[2];
  if (!event) {
    // Malformed invocation — exit silently (don't break hook chain)
    process.exit(0);
  }

  // Drain stdin (Claude Code hook contract)
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  let cwd = process.cwd();
  try {
    const input = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}');
    if (typeof input.cwd === 'string') cwd = input.cwd;
  } catch (_) {}

  // Fire-and-forget; swallow all errors
  try {
    spawnSync('codeforge', ['emit', event, '--field', `cwd=${cwd}`], {
      stdio: 'ignore',
      timeout: 2500,
    });
  } catch (_) {}
  process.exit(0);
}

main().catch(() => process.exit(0));
