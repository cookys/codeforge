# src/mnemos/ — Mnemos source role (detail split from the root CLAUDE.md)

Loaded by Claude Code when a file under `src/mnemos/` is touched. The entry points
`src/cli/ship.rs` and `src/cli/mnemos_cli.rs` live in `src/cli/`, whose CLAUDE.md points here.

## Mnemos source role (pillar, from the root overview)

- **Mnemos source role** — `codeforge ship` 把 L1 + git log 再 digest 成 L2 daily ledger、POST 到 Mnemos（中央 brain）。（讀 session jsonl 為設計目標、code 未實作，見 ship spec 修正 (c)；`codeforge mnemos-cli cite-detect` 可手動回填用過的 atom；**auto-cite-on-ship（B18）已把偵測+回寫接進 `ship` 路徑**，見下方 Cite 段。）**Production critical path**（不是 nice-to-have）— SessionEnd hook 觸發、失敗有 retry policy、結果影響 Mnemos 資料完整性。詳見 [`doc/specs/codeforge-ship.md`](../../doc/specs/codeforge-ship.md)。

## CodeForge ↔ Mnemos Interaction

Mnemos (`cookys/mnemos`) is the central brain that ingests from multiple sources (Slack, LINE, Email, Docs, ...). CodeForge is one of those sources — the **coding** source.

### Ship (session-end, daily L2 digest)

`codeforge ship` runs at session end (chained after `dream --quiet` in the SessionEnd hook). It reads:
- `.codeforge/store/concepts/*.md` (L1 compiled knowledge)
- `git log` for the day's commits
- `~/.claude/projects/<slug>/<uuid>.jsonl` for today's session transcripts
- `.codeforge/codeforge.db` for metrics

Then digests via Haiku and POSTs the L2 ledger payload to Mnemos at `http://127.0.0.1:8845/v1/ingest/ledger`. CodeForge spec: [`doc/specs/codeforge-ship.md`](../../doc/specs/codeforge-ship.md). Mnemos endpoint contract: `cookys/mnemos:docs/specs/10-source-contract.md` §5.1.

### Cite (natural citation, client-side write-back)

✅ **IMPLEMENTED (auto-cite-on-ship, B18, 2026-07-02)** — `ship` auto-cites: after a successful (or already-shipped / empty) ship, it scans the day's `~/.claude/projects/<repo-slug>/*.jsonl` transcripts and, for each candidate atom (`mnemos-cli context`) whose title appears, POSTs `mnemos-cli cite <atom_id>` → Mnemos increments `citation_count` / `last_cited_at` (Sprint 1: fulltext_match @ confidence 0.5; Sprint 5+: Haiku). Best-effort, never fails SessionEnd, gated on Mnemos opt-in. Code: `src/mnemos/autocite.rs` + `ship.rs::maybe_auto_cite`. The manual `codeforge mnemos-cli cite-detect <transcript>` subcommand remains. e2e proof: `scripts/e2e-mnemos-chain.sh`. Mnemos contract: `cookys/mnemos:docs/specs/10-source-contract.md` §11.

### Retry policy

Per Mnemos source-contract §9.1: 1s → 5s → 30s exponential backoff, 4 attempts. Failure writes `~/.codeforge/ship-failed/<ship_id>.json` for next-ship retry. `--no-hook` mode is single-attempt (never blocks SessionEnd).

**Opt-in gate** (`MnemosConfig::opted_in`): `ship --no-hook` only POSTs when Mnemos is explicitly opted-in — `~/.config/mnemos.env` exists OR `MNEMOS_INGEST_URL` is set. With no opt-in it returns early (no POST, no `ship-failed/` write), so codeforge-only users who run the global SessionEnd chain keep distilling via dream without accumulating dead-letter junk. Interactive `codeforge ship` (no `--no-hook`) ignores the gate — it's a deliberate user action. The dream→ship chain lives in the **global** `~/.claude/settings.json` SessionEnd (installed by `codeforge install --hooks`/`--all`), so it runs in every project.

### Critical path note

`codeforge ship` is **production critical path**, not a nice-to-have toy. It runs on every SessionEnd, has explicit retry policy, and its success affects Mnemos data completeness. Treat schema changes / API changes here with the same rigor as core memory pipeline changes.

### Cross-project sync rule (Mnemos edge)

- CodeForge-side code + spec lives here (this repo) — `src/cli/ship.rs`, `src/cli/mnemos_cli.rs`, `doc/specs/codeforge-ship.md`
- Mnemos-side ingest contract + atom schema lives in `cookys/mnemos:docs/specs/*.md` — never duplicated here
- When a design decision spans both (e.g., evidence_refs naming), the **Mnemos repo's `docs/specs/10-source-contract.md` is the single source of truth**; this repo's spec quotes / links it
