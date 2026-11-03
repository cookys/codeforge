# G36 — Usage collector v1

> PM dispatch G36 (2026-11-03). Decisions: PM msg 01M6FKJT6SXZ0YE0VZGSQA763W — (c) **C**: local
> `summary.json` + `codeforge usage push` → blob sha (a fleet per-host kv `fleet put/get` is queued
> on fleet as option A; the fleet-wide view waits for it); (d) quota **per account on statusline
> row 2**, no quota in the forge-stats summary, details via `codeforge usage` + a forge-stats pane
> tab. Alias interface agreed with autopilot (G37): msg 01M6FKM6GMYRRV2S1269KEJWAC.
> Inventory this builds on: G35 report (sources, fields, what is and is not knowable).
> Size L. Branch `feature/g36-usage-collector` (on develop 0474820, after G33).

## Goal

One machine-wide answer to "how much have I used, on which account, and how close am I to the
limit", without logins, credential files, OTel export, prompts or paths leaving the machine.

## Success criteria

1. Every statusline run with `rate_limits` writes a quota snapshot for its account alias; the
   usage row shows this session's bars plus every other fresh account as `alias NN% countdown`.
2. `codeforge usage` prints days × model (× project, local only) with tokens and an estimated
   cost, after an incremental collect that rereads only what changed.
3. `<live_base>/codeforge/usage/summary.json` carries per-account quota and per-day × model totals
   and nothing else; `codeforge usage push` uploads it and prints the sha.
4. No file outside the listed globs is opened; no text field of a transcript is kept.

## Non-goals (v1)

- Fleet-wide aggregation (waits for fleet kv, option A).
- Exact per-token attribution when one config dir ran several OAuth tokens (see Limits).
- Running collect automatically on this host (no codeforge hooks are installed here; standing rule).

## Design

### 1. Account alias (`src/usage/alias.rs`)

`alias(env_account: Option<&str>, config_dir: &Path) -> String`:
`AUTOPILOT_ACCOUNT` when it matches `^[a-z0-9-]{1,24}$`; else `acct-` + first 8 hex of
sha256(canonicalized config dir; default `~/.claude`). The env value is a parameter, never read
inside the tested function (env-var test seam race). Codex sources use `codex-` + sha8(CODEX_HOME,
default `~/.codex`). The alias is not secret (agreed with autopilot); it is the only account
identifier written anywhere.

### 2. Quota snapshot (statusline, hot path)

`read_status_input` already parses `rate_limits.five_hour|seven_day.{used_percentage,resets_at}`.
When either is present, write `<live_base>/codeforge/usage/quota/<alias>.json`:

```json
{"schema":"codeforge.quota/1","alias":"acct-1a2b3c4d","written_at_ms":0,
 "five_hour":{"pct":0.32,"resets_at":1793680000},"seven_day":{"pct":0.75,"resets_at":1793860000}}
```

tmp file + rename (several sessions share one alias). The statusline is a one-shot process, so
throttling reads the file on disk: rewrite only when its `written_at_ms` is older than 30 s or the
percentages changed. Failure never changes rendering or exit.

**Staleness**: a window whose `resets_at` is in the past is unknown (not shown); a snapshot older
than 6 h drops out. Both tested.

### 3. Usage row (both render paths)

Row 1 of the no-pet layout (~l.902) and the boxed layout (~l.1120) is **replaced**, not added:
`5h ▮▯ 32% 2h10m  7d ▮▮ 75% 2d1h  ctx ▮▮ 54%` (this session, unchanged) followed by
`· acct-b 81% 3d2h · acct-c 12% 4h` — each other fresh account's more constrained window.
Colours per TUI guideline: yellow ≥ 80 %, red ≥ 95 %; at most one yellow per row (the others drop
to bold). Width: other accounts drop from the end first (least used first), then ctx, then
countdowns. Snapshot read is bounded: at most 16 files, each ≤ 4 KiB by metadata and by bytes read,
regular files only (no symlinks), parsed as `serde_json::Value`, bad ones skipped.

### 4. Collector (`src/usage/collect.rs`, `codeforge usage collect`)

Store: `~/.codeforge/usage.db` (machine-wide, beside `ship-failed/`), WAL, migrations inline.

| Source | Glob (exactly) | Read | Dedup |
|---|---|---|---|
| Claude | `<config>/projects/*/*.jsonl`, `<config>/projects/*/*/subagents/*.jsonl` | `requestId`, `timestamp`, `message.model`, `message.usage.{input_tokens,output_tokens,cache_read_input_tokens,cache_creation.ephemeral_5m_input_tokens,ephemeral_1h_input_tokens,speed}` | `requestId` (one response is several lines with the same usage); table pruned past 40 days |
| Codex | `<codex>/sessions/**/rollout-*.jsonl` | `turn_context.model`, `token_count.info.total_token_usage.*` | per file: diff of the cumulative total against the stored last total |

Config dirs: `CLAUDE_CONFIG_DIR` (if set) and `~/.claude`, deduplicated by canonical path.
Per file state: canonical path, inode, size, offset (+ last codex total). size < offset or inode
changed → reread from 0. Lines are parsed as `serde_json::Value`; only the fields above are read
and nothing textual is stored. Project dimension: the transcript's project folder name is a
path-derived slug → stored locally as the repo **basename** only (`cwd` last component), used by
`codeforge usage` and never pushed.

Buckets: `(day UTC, source, account, model, project) → input, output, cache_write_5m,
cache_write_1h, cache_read, requests, fast_requests`.

### 5. Cost estimate (`src/usage/price.rs`)

Data table, `PRICES_AS_OF = "2026-10-06"` (source: Anthropic pricing via the claude-api reference).
$/MTok input / output: Fable 5.1 10/50, Opus 5.5 4/20, Opus 5 5/25, Opus 4.8–4.6 5/25,
Sonnet 5.5 2/10, Sonnet 5 2/10, Sonnet 4.6 3/15, Haiku 5.5 0.10/0.50, Haiku 4.5 1/5.
Cache write 1.25× (5 m) / 2× (1 h) input; cache read 0.1× input except Opus 5.5 $0.20 and
Fable 5.1 $0.25. Fast mode (Opus 5.5 / 5 / 4.8): 2×. Haiku 5.5 above 100 K prompt tokens costs
more; v1 applies the flat rate and says so. Unknown model (incl. every Codex model) → cost
`null`, shown as `—`, never 0. Subscription use is not billed per token: the column is labelled
"API-equivalent".

### 6. `codeforge usage` / `--json` / `push`

- `codeforge usage [--days N] [--by model|project|account] [--json]`: collect, then a table.
- `summary.json` (`codeforge.usage-summary/1`): `generated_at_ms`, `host` (opaque: `host-` +
  sha8(hostname)), `accounts` (fresh quota snapshots), `days` (last 7 × source × account × model
  token totals + estimated cost). No project, path, prompt or email.
- `codeforge usage push`: collect, write summary, `fleet upload --mime application/json`, print the
  sha. Missing `fleet` → a clear error, exit 1.

### 7. forge-stats pane tab (after G33 — now on develop)

A `用量 / usage` tab in the pane reads `summary.json` (bounded read) and draws the account lines
and today's model totals. Never spawns codeforge. Band unchanged (decision d).

### 8. Install chain (code only, not active on this host)

`codeforge install --hooks` adds `codeforge usage collect --quiet` after `ship --no-hook` in the
SessionEnd chain. This host keeps no codeforge hooks, so collect runs on demand here.

## Limits (v1, written down)

- A transcript does not record which OAuth token served it: transcript buckets carry the
  config-dir alias even when G37 ran several accounts in one dir. The quota snapshot is exact
  (statusline sees `AUTOPILOT_ACCOUNT`). G37 giving each account its own config dir resolves it.
- Costs are estimates at list price; subscription plans are not billed this way.

## Tests (each with a negative control)

alias (valid env, invalid env → hash, default dir); snapshot write + throttle + staleness (expired
window, 6 h); row rendering at 144/100/56/24 columns with 0–3 other accounts, colours, one-yellow
rule; bounded snapshot read (symlink, >4 KiB, 17 files, bad JSON); collector (dedup by requestId,
incremental offset, truncation → reread, subagent glob, codex cumulative diff, no text stored —
assert the db holds no string longer than a model id); price (known, unknown → null, fast, cache);
summary schema has no project/path keys.

## Phases

1. alias + quota snapshot + usage row (statusline) — reviewable alone.
2. collector + db + price + `codeforge usage`.
3. summary + push.
4. forge-stats pane tab.
5. install chain + docs (README, CLAUDE.md, CHANGELOG, spec `doc/specs/codeforge-usage.md`).

Review: autopilot four-seat (as G33), one round per phase pair (1, 2–3, 4–5), or as PM decides.
