# Clan Content Provider — CodeForge-side Integration

> **Date**: 2026-04-24
> **Status**: Phase 2 skeleton shipped; production wiring deferred to a
> later Plan B Phase 6 follow-up.
> **Mirrors**: CodePower `feature/clan-mvp-and-provider-trait` →
> `doc/projects/2026-04-24-clan-mvp-and-provider-trait/phases/plan.md`
> **Canonical protocol**: see
> `~/projects/codepower/doc/specs/clan-content-protocol-v1.md`.

---

## Positioning

This plan is the CodeForge mirror of CodePower's
`clan-mvp-and-provider-trait` Plan A. Its Phase 2 deliverable is a
skeleton (`src/clan/`) that compiles, matches the canonical trait
signature, and ships a `StubClanContentProvider` that the rest of
CodeForge can already depend on. The `HttpClanContentProvider`
skeleton is wired to the CodePower endpoint surface but is NOT yet
integrated into `src/pet/village.rs` — that is Plan B Phase 6.

## Scope (Phase 2)

**In**
- Type mirrors: `ClanSummary`, `PetBlueprint` (field-for-field match
  with CodePower's `plugin_api::clan_content`)
- Trait mirror: `ClanContentProvider` with 3 methods
- `StubClanContentProvider` — returns `Ok(vec![])` / `Ok(None)` from
  every method. Ready for use as a fallback when CodePower is
  unreachable or the protocol handshake fails
- `HttpClanContentProvider` skeleton — reqwest client pointed at
  CodePower endpoints; runs the `X-Protocol-Version: 1` handshake on
  first call; degrades to Stub on mismatch
- Startup config read: `~/.codeforge/config.toml` `[codepower]` section
  + `CODEPOWER_TOKEN` env override

**Out** (Plan B Phase 6)
- Integration with `src/pet/village.rs` dynamic village expansion
- Local cache (SQLite / sled) for offline fallback beyond StubProvider
- Retry / exponential backoff
- `codeforge connect-codepower` wizard CLI
- `scope=codeforge:read` JWT-claim verification (CodePower side wires
  this in Plan B Phase 6 too)

## Trait parity checklist

Every change to CodePower's
`backend/src/plugin_api/clan_content.rs` MUST trigger a manual mirror
edit on this side. Until a shared crate lands (Open Q A3 trigger), the
sync is by hand.

| CodePower path                                                 | CodeForge mirror        |
|----------------------------------------------------------------|-------------------------|
| `backend/src/plugin_api/clan_content.rs`                       | `src/clan/provider.rs`  |
| `backend/src/services/clan_content_impl.rs`                    | *(N/A — consumer only)* |
| `backend/src/handlers/clan_content.rs`                         | `src/clan/http.rs`      |

Parity points to re-check on every pull:
1. `ClanSummary` — 6 fields, same order, same optionality.
2. `PetBlueprint` — 15 fields, `serde_json::Value` for
   `sprite_forms` / `evolution_tree` / `attributes` / `palette`.
3. Method return types — `Vec<T>` vs `Option<T>` must line up.
4. `X-Protocol-Version` constant — both sides agree on `"1"`.

## Failure modes (wire-up)

| Signal from CodePower            | CodeForge behaviour             |
|----------------------------------|---------------------------------|
| `X-Protocol-Version` missing     | Degrade to `StubClanContentProvider`, log WARN once |
| `X-Protocol-Version` ≠ `"1"`     | Degrade to Stub, log WARN with actual value  |
| 403 (feature flag off)           | Degrade to Stub, quiet log      |
| 401 (token invalid/expired)      | Prompt operator; do not degrade silently     |
| Network timeout > 3 s            | Per-call failure, do NOT persistent-degrade  |
| JSON deserialize error           | Log ERROR, degrade to Stub for this session  |

## Phase 3+ handoff (deferred)

Plan B Phase 6 will:
1. Wire `HttpClanContentProvider` into `src/pet/village.rs` so
   user-founded clans appear alongside the hardcoded starter villages.
2. Add a local cache (TTL 24 h) so CodeForge can render the last-known
   clan list offline.
3. Implement `codeforge connect-codepower` — guided paste of token +
   config.toml write with correct permissions.
4. Ship proper scope-claim middleware on CodePower side.

Until then, Phase 2's skeleton is intentionally dormant.

## See also

- `~/projects/codepower/doc/specs/clan-content-protocol-v1.md` —
  canonical protocol (both repos reference)
- `~/projects/codepower/doc/projects/2026-04-24-clan-mvp-and-provider-trait/phases/plan.md` —
  the authoritative plan
- `src/clan/` — this skeleton (shipped in Phase 2)
