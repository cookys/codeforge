# Doc-Drift System — concept docs → audit → reusable skill → deterministic gate

> L-size. Started 2026-06-18, merged incrementally to main, archived 2026-06-22.
> Grew from a one-line question ("should the API-key fallback be documented?") into
> a full doc↔code drift methodology + tooling, shared across codeforge + autopilot.

## Project Goal

> **Final goal**: Make CodeForge's docs accurately reflect the code, and turn
> "are the docs in sync?" into a **reliably answerable, gate-able** question.
> **Success criteria** (all met):
> - User-facing concept doc exists (`doc/concepts.md`) + API-key fallback corrected — DONE.
> - All confirmed doc↔code drift fixed across the repo — ~120 findings over 7 audit rounds, DONE.
> - Drift audit is a **reusable** asset, not one-off — codeforge Workflow scripts + `autopilot:doc-sync` skill, DONE.
> - A **reliable** stopping condition exists — deterministic gate `scripts/check-doc-drift.py` (5 checks) green in CI, DONE.
> **Scope boundary**: codeforge docs + the cross-repo doc-sync tooling. Code behaviour
> unchanged except 2 clap help-string fixes; spec design-targets-not-built recorded in
> BACKLOG B18, not implemented.

## Phases (as-shipped, retroactive)

| Phase | What | Status |
|-------|------|--------|
| 1 | `doc/concepts.md` (solo vs brain / dream / ship / pet+CodePower) + API-key fallback fix | ✅ `7238c84` |
| 2 | Full LLM audit → fix 48 drift; integrate as reusable Workflow scripts + dev-flow triggers | ✅ `e6d540c` `228671d` |
| 3 | Promote audit to portable `autopilot:doc-sync` skill (v2.19.0) + orchestrator wiring; codeforge consumes it | ✅ `f321e87` (+ autopilot v2.19.0) |
| 4 | Validate wiring (scoped run caught 4) + loop full sweeps (rounds 2-7: 22/11/13/14/8/10 drift) | ✅ `0fbb222`…`8274c85` |
| 5 | **Convergence finding**: loop-to-zero is non-convergent (see Decisions); adopt pragmatic stop rule | ✅ recorded in `.claude/doc-audit-state.json` |
| 6 | **Deterministic gate** `scripts/check-doc-drift.py` (5 checks) + CI `doc-drift` job | ✅ `f08aeb2` |
| 7 | Generalize gate into autopilot doc-sync skill (v2.20.0): two-layer model + baseline `doc-drift-gate.py` + CI dogfood | ✅ autopilot v2.20.0 + `0be0be6` |

## Results

- **~120 doc↔code drift findings fixed** (round 1: 48; scoped: 4; rounds 2-7: 22+11+13+14+8+10), all adversarially verified.
- **Highest-value catches**: AI-commentary "3-layer chain" error (latent since round 1, surfaced round 7 — it's 2-layer, no `claude -p`); recall-ranking "strength 排序" (T2.3 made it `importance×recency×citation`); CLAUDE.md Phase Roadmap whole-table stale (`planned` vs shipped); install spec stuck at "待實作" though shipped.
- **New artifacts**: `doc/concepts.md`; `.claude/workflows/{doc-code-drift-audit,doc-drift-scoped}.js`; `scripts/check-doc-drift.py` + CI job; `.claude/doc-drift-config.md` + `dispatch-config.md` + `doc-audit-state.json`.
- **Cross-repo**: `autopilot:doc-sync` skill (v2.19.0 → v2.20.0) + generic `scripts/doc-drift-gate.py` + wired into autopilot's own CI (preflight-portability #16).
- **Both gates green**: codeforge 5/5, autopilot preflight 16/16.

## Key Decisions

1. **Spec design-targets**: when a spec describes a designed-but-unbuilt feature, KEEP the
   design but rewrite it from present-tense assertion → explicit "planned / NOT YET
   IMPLEMENTED" framing (banners alone don't satisfy a strict auditor — the body must not
   assert false current behavior). Backlog → B18.
2. **LLM sweep is DISCOVERY, not a gate** (the core finding). 7 rounds proved neither
   `total=0` nor `WRONG=0` is a provable fixed point of (sweep → doc-fix → sweep): finders
   are non-deterministic (latent errors surface stochastically — the commentary error sat 6
   rounds), and fixes themselves introduce ~1 error / few rounds. A "clean" sweep only means
   "this sample found nothing", never "nothing exists".
3. **Reliability comes from deterministic checks.** The stopping condition is
   `scripts/check-doc-drift.py` green (zero-variance, CI-gate-able), NOT "the LLM found
   nothing". The LLM sweep's job is to find NEW mechanizable classes, which get demoted into
   the deterministic gate — that loop converges.
4. **git tags NOT auto-created** for version-sync drift — `release.yml` triggers on `v*.*.*`
   tags (publishes a release); disclosed pending-version via CHANGELOG note instead.

## Links

- Audit trajectory + conclusion: [`.claude/doc-audit-state.json`](../../../../.claude/doc-audit-state.json)
- Deterministic gate: [`scripts/check-doc-drift.py`](../../../../scripts/check-doc-drift.py)
- Concept doc: [`doc/concepts.md`](../../../concepts.md)
- BACKLOG B18 (spec design-targets): [`doc/BACKLOG.md`](../../../BACKLOG.md)
