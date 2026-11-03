# G36 — Usage collector v1

> PM dispatch G36 (2026-11-03). Decisions: PM msg 01M6FKJT6SXZ0YE0VZGSQA763W — (c) **C**: local
> `summary.json` + `codeforge usage push` → blob sha (a fleet per-host kv `fleet put/get` is queued
> on fleet as option A; the fleet-wide view waits for it); (d) quota **per account on statusline
> row 2**, no quota in the forge-stats summary, details via `codeforge usage` + a forge-stats pane
> tab. Alias interface agreed with autopilot (G37): msg 01M6FKM6GMYRRV2S1269KEJWAC.
> Inventory this builds on: G35 report (sources, fields, what is and is not knowable).
> Size L. Branch `feature/g36-usage-collector` (on develop 0474820, after G33).
> Plan review g1 (codex + Fable, 2026-11-03): 20 findings folded (3 blocking).

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
4. No file outside the listed globs is opened; no text field of a transcript is kept, and no
   path (a source is known by the sha256 of its path).
5. Every file G36 creates — `usage.db` (+ `-wal`/`-shm`), quota snapshots, `summary.json` — is
   mode 0600, and the directories it creates are 0700, set at creation (not chmod after).
   `usage.db` is pre-created with `OpenOptions::mode(0o600)` before SQLite opens it (SQLite's unix
   VFS gives `-wal`/`-shm` the db file's mode); after open, the db and any existing `-wal`/`-shm`
   or directory with wider bits are tightened and the fix is reported once on stderr. No umask
   change (process-global).

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

tmp file (created 0600) + rename (several sessions share one alias). The statusline is a one-shot process, so
throttling reads the file on disk: rewrite only when its `written_at_ms` is older than 30 s or the
percentages changed. Failure never changes rendering or exit.

**Staleness**: a window whose `resets_at` is in the past is unknown (not shown); a snapshot older
than 6 h drops out. Both tested.

**No new subprocess.** The statusline already resolves `<live_base>` on every run before G36
(`write_live_context` → `crate::live::resolve_live_base`, whose first probe runs `findmnt`; that
per-run spawn is the deviation listed in TUI guideline §7, with its own fix direction). G36 reuses
the resolved, process-cached base from the same run and adds no spawn of its own. A test pins the
call shape: `record_quota` takes the quota dir as a parameter and resolves nothing.

**Existing files.** A snapshot whose mode is wider than 0600 is rewritten regardless of the
throttle; the writer (`crate::live::write_live_json`, reused) creates the tmp file 0600 and the
directories 0700 and tightens an existing directory to 0700.

### 3. Usage row (both render paths)

Row 1 of the no-pet layout (~l.902) and the boxed layout (~l.1120) is **replaced**, not added:
`5h ▮▯ 32% 2h10m  7d ▮▮ 75% 2d1h  ctx ▮▮ 54%` (this session, unchanged) followed by
`· acct-b 81% 3d2h · acct-c 12% 4h` — each other fresh account's more constrained window.
Colours per TUI guideline: yellow ≥ 80 %, red ≥ 95 %; at most one yellow per row (the others drop
to bold). Width: other accounts drop from the end first (least used first), then ctx, then
countdowns. Snapshot read is bounded: entries sorted by file name, the first 16 `*.json` taken, each opened
with `O_NOFOLLOW | O_NONBLOCK`, then the handle must be a regular file ≤ 4 KiB, at most 4 KiB
read, parsed as `serde_json::Value`; bad ones skipped (phase 1 review g1). Display order: most
constrained first.

### 4. Collector (`src/usage/collect.rs`, `codeforge usage collect`)

Store: `~/.codeforge/usage.db` (machine-wide, beside `ship-failed/`), WAL, migrations inline.

**Fields.** Each transcript line is parsed as `serde_json::Value` and only these are inspected;
everything else is never copied out of the parsed value:

| Source | Glob (exactly) | Inspected | Persisted |
|---|---|---|---|
| Claude | `<config>/projects/*/*.jsonl`, `<config>/projects/*/*/subagents/*.jsonl` | `requestId`, `timestamp`, `cwd`, `message.model`, `message.usage.{input_tokens, output_tokens, cache_read_input_tokens, cache_creation.ephemeral_5m_input_tokens, cache_creation.ephemeral_1h_input_tokens, speed}` | request id (dedup), UTC day, model id, token counts, speed; `cwd` → its last component only |
| Codex | `<codex>/sessions/YYYY/MM/DD/rollout-*.jsonl` (fixed depth 4) | `timestamp`, `turn_context.model`, `turn_context.cwd`, `token_count.info.total_token_usage.{input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens}` | session id (from the file name), UTC day, model id, token counts; `cwd` → last component only |

The project dimension (cwd basename) is local only and never pushed.

**Containment (a53b6048, a8e444ca).** Roots: `CLAUDE_CONFIG_DIR` (if set) and `~/.claude`, and
`CODEX_HOME` or `~/.codex`, each canonicalized once and deduplicated. The walk uses
`symlink_metadata` at every level and never descends into a symlinked directory or takes a
symlinked file. Each file is opened `O_NOFOLLOW | O_NONBLOCK`; the handle must be a regular file;
its canonical path must start with its canonical root, else it is skipped. Nothing outside the
globs above is opened (no `settings.json`, `.credentials.json`, `auth.json`, …). Tests: a
symlinked project dir and a symlinked `*.jsonl` pointing outside the root are not read; a FIFO
named `*.jsonl` does not block.

**No paths persisted (89d5d9cf, c81050d7).** Per-file state is keyed by `source_id` =
sha256(canonical path) (64 hex); the path itself lives only in memory during a collect. A Claude
project folder name is a slug of the absolute cwd, so it is never stored either.

**Incremental reads (066f5e7e, aebcffeb).** Per `source_id`: inode, size, `offset`, `head` =
sha256 of the first 4 KiB. A collect reads from `offset` and consumes only newline-terminated
lines; an unterminated tail is left for the next collect. The bucket increments, the dedup rows
and the new cursor are committed in one transaction, so an interrupted collect counts nothing
twice and loses nothing. Size < offset, a changed inode or a changed `head` → reread from 0.

**Replay and dedup (66270fbf, c3e2dd33).** Aggregation horizon: records older than 35 days are
never counted. Claude: a request id is counted once (one response appears as several lines with
the same usage); dedup rows are kept 40 days, longer than the horizon, so a reread from 0 cannot
recount a pruned id. Codex: per rollout session id (from the file name, not the path) the highest
cumulative total already counted is stored; a line adds only the part above it, so a copy, a
replaced file or a reread adds nothing, and a cumulative value that goes down adds nothing.

**Buckets (e11b68d6).** `(day UTC, source, account, model, project, speed) → input, output,
cache_write_5m, cache_write_1h, cache_read, requests` — fast and standard tokens are kept apart so
the fast multiplier applies only to fast tokens.

**Account of a transcript**: the config-dir alias (`acct-` + hash8 of the root); Codex:
`codex-` + hash8(codex root). See Limits.

### 5. Cost estimate (`src/usage/price.rs`)

Data table, `PRICES_AS_OF = "2026-10-06"`. Source: the bundled claude-api reference, "Current
Models (cached: 2026-10-06)" table and `shared/prompt-caching.md` § Economics.
Cost of a bucket = m × (input·r_in + output·r_out + cw5m·1.25·r_in + cw1h·2·r_in + cache_read·r_cr),
where r_cr is the table's cache-read rate (0.1·r_in unless listed) and m = 2 for `speed = fast` on
a model with a fast tier (Opus 5.5 / 5 / 4.8), else 1 — `fast` on another model gets no multiplier
(tested).
$/MTok input / output: Fable 5.1 10/50, Opus 5.5 4/20, Opus 5 5/25, Opus 4.8–4.6 5/25,
Sonnet 5.5 2/10, Sonnet 5 2/10, Sonnet 4.6 3/15, Haiku 5.5 0.10/0.50, Haiku 4.5 1/5.
Cache write 1.25× (5 m) / 2× (1 h) input; cache read 0.1× input except Opus 5.5 $0.20 and
Fable 5.1 $0.25. Fast mode (Opus 5.5 / 5 / 4.8): 2×. Haiku 5.5 above 100 K prompt tokens costs
more; v1 applies the flat rate and says so. Unknown model (incl. every Codex model) → cost
`null`, shown as `—`, never 0. Subscription use is not billed per token: the column is labelled
"API-equivalent".

### 6. `codeforge usage` / `--json` / `push`

- `codeforge usage [--days N] [--by model|project|account] [--json]`: collect, write
  `summary.json`, then a table. `codeforge usage collect [--quiet]` also writes it.
- `summary.json`, exact schema (every key listed; nothing else at any depth):

```json
{"schema":"codeforge.usage-summary/1","generated_at_ms":0,"host":"host-1a2b3c4d",
 "prices_as_of":"2026-10-06",
 "accounts":[{"alias":"acct-a","written_at_ms":0,
              "five_hour":{"pct":0.32,"resets_at":0},"seven_day":{"pct":0.75,"resets_at":0}}],
 "days":[{"day":"2026-11-03","source":"claude","account":"acct-a","model":"claude-opus-5-5",
          "speed":"standard","input":0,"output":0,"cache_write_5m":0,"cache_write_1h":0,
          "cache_read":0,"requests":0,"cost_usd":1.23}]}
```

  `host` = `host-` + sha8(hostname); `five_hour`/`seven_day` may be null; `cost_usd` may be null;
  `days` covers the last 7 days. No project, path, prompt, email or hostname.
- `codeforge usage push`: collect, write the summary, read it back, validate it against the schema
  above **recursively** (an unknown key or a wrong type anywhere → refuse), then
  `fleet upload --mime application/json` of exactly the validated bytes and print the sha. Missing
  `fleet` → a clear error, exit 1. Tests: an extra key inside an `accounts` row and inside a `days`
  row are both refused.
- `summary.json` is written 0600 via tmp + rename (the shared writer).

### 7. forge-stats pane tab (after G33 — now on develop)

A `用量 / usage` tab in the pane reads `summary.json` (bounded read) and draws the account lines
and today's model totals, with the summary's age; older than 1 h it says so and names
`codeforge usage` as the refresh. Never spawns codeforge. Band unchanged (decision d).

### 8. Install chain (code only, not active on this host)

`codeforge install --hooks` adds `codeforge usage collect --quiet` after `ship --no-hook` in the
SessionEnd chain. This host keeps no codeforge hooks, so collect runs on demand here.

## Limits (v1, written down)

- A transcript does not record which OAuth token served it: transcript buckets carry the
  config-dir alias even when G37 ran several accounts in one dir. The quota snapshot is exact
  (statusline sees `AUTOPILOT_ACCOUNT`). G37 giving each account its own config dir resolves it.
- Costs are estimates at list price; subscription plans are not billed this way.

## Tests (each with a negative control)

- alias: valid env, invalid env → hash, default dir.
- quota: write + throttle + staleness (expired window, 6 h); a wider-mode snapshot is rewritten;
  row rendering with 0–3 other accounts, colours, one yellow per row counting the own bars; FIFO
  and symlink in the snapshot dir.
- privacy: fixture transcripts carry distinctive markers in every text field (`message.content`,
  tool input and output, file contents, the parent dirs of `cwd`, the project folder slug, an
  email-shaped string); after collect, the raw bytes of `usage.db`, `-wal` and `summary.json` hold
  none of them. Negative control: persisting one text field turns it red.
- containment: symlinked project dir, symlinked `*.jsonl`, FIFO `*.jsonl`, a file outside the
  globs (`settings.json`) — none read.
- accounting: dedup by requestId; subagent transcript counted once; a record split across two
  collects counted once; an interrupted collect (error before commit) leaves state unchanged;
  truncation, inode change and head change → reread without recount; a Claude record older than
  the horizon after a reread; Codex copy, replacement and decreasing cumulative.
- price: known, unknown → null, fast on a fast-tier model, fast on another model (no multiplier),
  cache write 5 m / 1 h, cache read; mixed fast and standard in one day.
- summary: schema exact; push refuses an unknown key at the top level, in `accounts` and in
  `days`; uploads the validated bytes.
- modes: 0600 files, 0700 dirs, including the db's `-wal`/`-shm`, by stat; widened modes repaired.

## Phases

1. alias + quota snapshot + usage row (statusline) — reviewable alone.
2. collector + db + price + `codeforge usage`.
3. summary + push.
4. forge-stats pane tab.
5. install chain + docs (README, CLAUDE.md, CHANGELOG, spec `doc/specs/codeforge-usage.md`).

Review: autopilot four-seat (as G33), one round per phase pair (1, 2–3, 4–5), or as PM decides.
