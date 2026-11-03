# G36 — Usage collector v1

> PM G36 (2026-11-03). Decisions (msg 01M6FKJT6SXZ0YE0VZGSQA763W): (c) local `summary.json` +
> `codeforge usage push` → blob sha (fleet kv is option A, later); (d) quota per account on the
> statusline usage row, none in the forge-stats band, details via `codeforge usage` + a pane tab.
> Alias interface agreed with autopilot G37 (msg 01M6FKM6GMYRRV2S1269KEJWAC). Size L, branch
> `feature/g36-usage-collector`. Plan review g1 (codex + Fable): 20 findings folded (3 blocking);
> g2 (cap reached, depth-0 adjudicated): 13 folded (4 blocking).

## Goal

One machine-wide answer to "how much have I used, on which account, how close to the limit",
with no logins, credential files, OTel export, prompts or paths leaving the machine.

## Success criteria

1. Statusline runs with `rate_limits` write a quota snapshot per account; the usage row shows
   this session's bars plus each other fresh account as `alias NN% countdown`.
2. `codeforge usage` prints day × model (× project, local only) tokens and an estimated cost after
   an incremental collect.
3. `summary.json` holds per-account quota and per-day × model totals only; `usage push` uploads it.
4. No file outside the listed globs is opened; no transcript text and no path is persisted (a
   source is known by the sha256 of its path).
5. Every G36 file (`usage.db` + `-wal`/`-shm`, snapshots, `summary.json`) is 0600 and its dirs
   0700 from creation; wider existing modes are tightened (one stderr note). G36 owns its dirs:
   `<data_dir>/usage/` (`CODEFORGE_DATA_DIR`, default `~/.local/share/codeforge`, as `src/db`)
   for `usage.db`; `<live_base>/codeforge/usage/` for `quota/` and `summary.json`.

Non-goals: fleet-wide aggregation (waits for fleet kv); per-token attribution when one config
dir ran several tokens; automatic collect on this host (no codeforge hooks here).

## Phase 1 — alias, quota snapshot, usage row (done: 5bef593, e880088)

- Alias: `AUTOPILOT_ACCOUNT` if `^[a-z0-9-]{1,24}$`, else `acct-` + sha8(canonical config dir,
  default `~/.claude`); env values are parameters of tested functions. Codex: `codex-` +
  sha8(codex root). The alias is not secret; it is the only account identifier ever written.
- Snapshot keys exactly `schema, alias, root, written_at_ms, five_hour, seven_day` (windows
  `{pct, resets_at}` or null; `root` = sha8(canonical config dir), added in phase 2); only
  `rate_limits.*` is read from the statusline input (privacy test: markers in `session_id`,
  `transcript_path`, `cwd`, `workspace` never reach the snapshot bytes).
- Snapshot `<live_base>/codeforge/usage/quota/<alias>.json` (`codeforge.quota/1`), via the shared
  0600/0700 tmp+rename writer, rewritten after 30 s, on a changed percentage, or when its mode is
  wider than 0600. A window past `resets_at` is unknown; a snapshot older than 6 h is dropped.
- Read: name-sorted, first 16 `*.json`, opened `O_NOFOLLOW | O_NONBLOCK`, handle must be a
  regular file ≤ 4 KiB, ≤ 4 KiB read; the file stem must equal the body's alias and match the
  alias grammar (phase 2 adds the stem check); bad ones skipped; most constrained first.
- No new subprocess: `<live_base>` is already resolved on every statusline run before G36
  (`write_live_context`; the per-run `findmnt` is the TUI guideline §7 deviation). G36 reuses
  that process-cached value; `record_quota` takes the dir as a parameter. Failures never change
  output or exit code.
- Row (both layouts): own bars unchanged, then other accounts in the room left, most constrained
  kept first; an entry is whole, or `alias NN%` without its countdown, or omitted — never cut
  mid-token. Red ≥ 95 % is never demoted; yellow ≥ 80 % once per row counting the own bars, the
  rest bold.

## Phase 2 — collector (`src/usage/collect.rs`, `codeforge usage collect [--quiet]`)

Store `<data_dir>/usage/usage.db`. Before SQLite opens it, in this order: the dir is created 0700
or tightened; the db is created with `OpenOptions::mode(0o600)` or tightened; existing `-wal`/`-shm`
are tightened; only then open (the unix VFS gives new sidecars the db's mode). No umask change.
Test: from a 0644 db with a stale 0644 `-wal`, collect writes and all three stat 0600.

| Source | Glob (exactly) | Inspected | Persisted |
|---|---|---|---|
| Claude | `<cfg>/projects/*/*.jsonl`, `<cfg>/projects/*/*/subagents/*.jsonl` | `requestId`, `timestamp`, `cwd`, `message.model`, `message.usage.{input_tokens, output_tokens, cache_read_input_tokens, cache_creation.ephemeral_5m/1h_input_tokens, speed}` | request id, UTC day, model, counts, speed, cwd last component |
| Codex | `<codex>/sessions/YYYY/MM/DD/rollout-*.jsonl` | `timestamp`, `turn_context.{model, cwd}`, `token_count.info.total_token_usage.{input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens}` | session id (file name), UTC day, model, counts, cwd last component |

Lines are parsed as `serde_json::Value`; nothing else is copied out. The project dimension (cwd
basename) stays local, never pushed.

- **Containment.** Roots: `CLAUDE_CONFIG_DIR` and `~/.claude`; `CODEX_HOME` or `~/.codex`;
  canonicalized, deduplicated. Walk with `symlink_metadata`, never into a symlinked dir or onto a
  symlinked file; open `O_NOFOLLOW | O_NONBLOCK`; the handle must be a regular file whose canonical
  path starts with its root. Nothing outside the globs (`settings.json`, `.credentials.json`,
  `auth.json`) is opened.
- **No paths.** Per-file state is keyed by `source_id` = sha256(canonical path); the path lives in
  memory only (a project folder name is a slug of the absolute cwd, so it is never stored).
- **Requests.** Lines without `requestId` are skipped. Per request id the counters are merged by
  per-counter maximum, and the stored maximum is kept, so repeated lines, a response split across
  collects and rereads are idempotent (spike, 2026-11-03: 12 503 requests in 40 recent
  transcripts, every line of a request carried identical usage; 6 usage lines had no request id).
  No `cache_creation` breakdown → `cache_creation_input_tokens` counts as 5-minute writes.
- **Account of a root.** Collect never reads `AUTOPILOT_ACCOUNT`. A root's alias is the alias of
  the newest quota snapshot whose `root` equals sha8(canonical root), else `acct-` + that sha8;
  the mapping is stored in the db, so attribution does not depend on which process ran collect.
- **Incremental.** Per source: inode, size, `offset`, `head` = sha256 of the first
  min(4096, committed offset) bytes with that length; compared over the same length only. Only
  newline-terminated lines are consumed; the tail waits. Buckets, dedup rows and cursor commit in
  one transaction. Shrunk size, new inode or new `head` → reread from 0.
- **Replay.** Records older than 35 days are never counted. Claude request ids count once and are
  kept 40 days (> horizon), so a reread cannot recount. Codex: per session id the highest counted
  cumulative is stored; a line adds only what exceeds it (copies, replacements, decreases add 0).
- **Buckets.** `(day, source, account, model, project, speed) → input, output, cache_write_5m,
  cache_write_1h, cache_read, requests`. A model id not matching `^[A-Za-z0-9._-]{1,64}$` is
  stored as `unknown`.

## Phase 3 — price, summary, `usage`, `push`

- Price table data, `PRICES_AS_OF = "2026-10-06"`, from the bundled claude-api reference ("Current
  Models" table; `shared/prompt-caching.md` § Economics). $/MTok in/out: Fable 5.1 10/50,
  Opus 5.5 4/20, Opus 5 · 4.8–4.6 5/25, Sonnet 5.5 · 5 2/10, Sonnet 4.6 3/15, Haiku 5.5 0.10/0.50,
  Haiku 4.5 1/5. Each model is one data row of five rates (in, out, cw5m = 1.25·in, cw1h = 2·in,
  read = 0.1·in except Opus 5.5 $0.20 and Fable 5.1 $0.25, written out) and a fast multiplier
  (2 on Opus 5.5/5/4.8, else 1); cost = m × Σ tokens × rate. Tests pin the two read exceptions.
  Haiku 5.5 > 100 K prompt priced flat (stated). Unknown model (incl. all Codex) → null, shown
  `—`. Totals sum priced rows only and say so (`≥ $1.23 · 2 unpriced`); null when none is priced.
  Column labelled "API-equivalent".
- `codeforge usage [--days N] [--by model|project|account] [--json]` and `usage collect` both
  collect and write `<live_base>/codeforge/usage/summary.json` (0600, shared writer). Exact
  schema, nothing else at any depth:

```json
{"schema":"codeforge.usage-summary/1","generated_at_ms":0,"host":"host-1a2b3c4d","prices_as_of":"2026-10-06",
 "accounts":[{"alias":"acct-a","written_at_ms":0,"five_hour":{"pct":0.3,"resets_at":0},"seven_day":null}],
 "days":[{"day":"2026-11-03","source":"claude","account":"acct-a","model":"claude-opus-5-5","speed":"standard",
          "input":0,"output":0,"cache_write_5m":0,"cache_write_1h":0,"cache_read":0,"requests":0,"cost_usd":null}]}
```

  `host` = `host-` + sha8(hostname); `days` = last 7 days. Values are constrained too: aliases
  by the alias grammar, `host` `host-` + 8 hex, `model` `^[A-Za-z0-9._-]{1,64}$`, `day`
  `YYYY-MM-DD`, `source` ∈ {claude, codex}, `speed` ∈ {standard, fast}, numbers finite ≥ 0,
  `cost_usd` number or null.
- `usage push`: collect, write, read back, validate keys, types and values recursively (anything
  else → refuse; tests put a path and an email in `model` and in an alias), then
  `fleet upload <file> --name usage-summary.json --mime application/json` — a fixed name, so no
  local path is sent — and print the sha. Test with a fake `fleet` recording argv and the bytes.
  No `fleet` → clear error, exit 1.

## Phase 4–5 — pane tab, install chain, docs

- forge-stats `用量 / usage` tab: bounded read of `summary.json`, account lines and today's model
  totals, the summary's age; over 1 h it names `codeforge usage` as the refresh. Never spawns.
- `install --hooks` adds `codeforge usage collect --quiet` after `ship --no-hook` (not active
  here). Docs: README, CLAUDE.md, CHANGELOG, `doc/specs/codeforge-usage.md`.

## Limits

Transcripts do not record which OAuth token served them: with several accounts in one config
dir, transcript buckets carry that dir's alias (quota snapshots stay exact). G37 uses one config
dir per account, which removes this. Costs are list-price estimates, not subscription billing.

## Tests (each with a negative control)

Phase 1: alias; throttle, wider-mode rewrite, staleness; row widths and colours; FIFO and symlink.
Privacy: fixtures carry markers in every text field (`message.content`, tool input/output, file
contents, cwd parent dirs, the project slug, an email-shaped string); after collect, the raw bytes
of `usage.db`, `-wal` and `summary.json` contain none (control: persist one text field → red).
Containment: symlinked project dir, symlinked `*.jsonl`, FIFO `*.jsonl`, `settings.json` — none
read. Accounting: requestId dedup; subagent once; a record split across collects once; an
interrupted collect leaves state unchanged; shrink, inode and head changes reread without
recount; old records after a reread; Codex copy, replacement, decrease; mixed fast/standard day.
Price: known, unknown → null, fast eligible / not, cache rates. Summary: exact schema; push
refuses an extra key at top level, in `accounts`, in `days`. Modes: 0600/0700 by stat incl.
`-wal`/`-shm`; widened modes repaired.

## Review

Plan: hetero plan review (codex + Fable, cap 3). Implementation: autopilot four seats per group —
phase 1 / phases 2–3 / phases 4–5. Phases 2–5 start after the plan receipt passes.
