# Shared-TUI P5 evidence

Task: pin autopilot tux v1 in forge-stats, replace P3 cross checks, migrate shared primitives,
verify both consumers, and commit on the current branch without pushing.
Pinned source: `d5fe692c2ad768fa053bfef173bb2f59514130f6`.
Frozen plan: `plan/shared-tui:doc/plans/2026-11-17-shared-tui-components.md` (§1.1, §2.1/2.3, §4.1–4.3, §6, P5).

## 1. Acceptance criteria

- Archive the pinned commit, commit the vendor tree/lock, and reproduce the same bytes on another sync.
- Reject changed/unlisted vendor files; enforce sibling version/content and release behavior.
- Replace pet-cross in check-all/release, including shellcheck coverage.
- Migrate F1–F5/F8–F10 and pet icon ownership; import the F7 panel schema.
- Pass plugin, lock, cross and complete check-all verification; preserve Rust statusline behavior.

## 2. Located source and exact pre-change lines

These quotations come from the task's original HEAD; the commit diff retains the full old/new context.

| Location | Exact original line(s) |
| --- | --- |
| `hooks/model.ts:11,19` | `export function cellWidth(c: number): number {` / `export function displayWidth(text: string): number {` |
| `hooks/model.ts:35` | `  latest: '●',` |
| `hooks/quota-view.ts:166` | `const pctOf = (u: number) => `${Math.round(u * 100)}%`` |
| `hooks/side.ts:44` | `const pctOf = (r: number) => `${Math.floor(Math.max(0, r) * 100)}%`` |
| `hooks/quota.ts:20,367` | `  "tight": 0.9,` / `export const isTight = (utilization: number) => utilization >= Q.tight` |
| `hooks/panel.ts:6,7` | `export const PANEL_SCHEMA = 'codeforge.panel/1'` / `export const PANEL_TIERS = [40, 60, 80, 100, 120] as const` |
| `hooks/side.ts:9,11` | `export const SIDE_USAGE_W = [30, 40, 50] as const` / `export const SIDE_MEMORY_W = [30, 40, 50] as const` |
| `hooks/model.ts:237,251,302` | `export const BAND_MIN_COLUMNS = 26` / `export function bandLayout(bodyColumns: number, spriteWidth: number): BandColumn[] {` / `export function bandTier(maxRows: number, bodyColumns: number, hasMini: boolean): BandTier {` |
| `hooks/model.ts:361` | `export function summaryParts(fleet: FleetSummary | null, petTag: string | null, cols: number, word: string, quota: SummaryPart[][] | null = null): SummaryPart[] | null {` |
| `hooks/model.ts:581` | `  if (m < 24 * 60) return `${Math.floor(m / 60)}h${m % 60}m`` |
| `hooks/register.tsx:663` | `          return <Text key={k} bold wrap="truncate">{s.text}</Text>` |
| `scripts/check-all.sh` | `./scripts/pet-cross.test.sh` / `./scripts/pet-cross.sh` |
| `scripts/release-local.sh` | `./scripts/pet-cross.sh --release >&2 || fail "pet-cross failed: forge-stats panel/1 and autopilot's pet-tab reader disagree"` |

Hook paths in this table are relative to `plugins/forge-stats/`. Width clipping helpers remain domain-specific, per the plan.

## 3–4. Incremental edits and verification

Full command logs are in `target/tux-p5-evidence/` in this worktree; receipts below record the observed exit codes.

| Increment | Result | Receipt |
| --- | --- | --- |
| Baseline plugin tests | exit 0; 220 pass / 0 fail | `01-baseline-plugin.log` |
| Commit archive + sync | exit 0; second sync byte-identical | `03-sync.log`, `20-sync-idempotency.log` |
| Lock and five controls | exit 0 | `04-lock.log` |
| Cross replacement + ten controls | exit 0; upstream 11 pass, plugin 220 pass | `05-cross.log`, `05-cross-controls.log` |
| Gate wiring + shellcheck | exit 0 | `06-wiring-cross.log`, `06-shellcheck.log` |
| JSON sandbox discovery | exit 1 (node:fs import forbidden); replaced with a checked code bridge | `07-width-red.log` |
| F1 width red → green | exit 1 (ANSI width 11 vs 2) → exit 0; 224 pass | `07-width-ansi-red.log`, `07-width-green.log` |
| Checked sandbox bridge | exit 0; lock and cross green | `08-bridge-lock.log`, `08-bridge-cross.log` |
| F2 percent red → green | exit 1 (rounding) → exit 0; 225 pass | `09-pct-red.log`, `09-pct-green.log` |
| F3 threshold red → green | exit 1 (80% not tight) → exit 0; 226 pass | `10-threshold-red.log`, `10-threshold-green.log` |
| F10 dead-code removal | exit 0; 226 pass | `11-dead-code.log` |
| F4 vocabulary + pet table removal | exit 0; 226 pass; lint five controls green | `12-vocabulary.log`, `12-lint.log` |
| F5 latest red → green | exit 1 (● vs ✦) → exit 0; 227 pass | `13-latest-red.log`, `13-latest-green.log` |
| F7/F8 schema and tier imports | exit 0; 227 pass | `14-tiers.log` |
| F9 duration red → green | exit 1 (0m vs 29s) → exit 0; 228 pass | `15-duration-red.log`, `15-duration-green.log` |
| Panel writer golden | exit 0; 234 pass; poison fixture corrected to put money in a rendered model name | `16-panel-contract-green.log` |
| Whole gate environment check | exit 1; existing ANSI test failed twice under NO_COLOR=1; unset NO_COLOR reproduces green | `17-check-all.log`, `17-ansi-env-fix.log` |
| Whole gate local-Git environment | exit 1 (local fixture transport disabled) → push-guard exit 0 with GIT_ALLOW_PROTOCOL=file | `23-final-check-all.log`, `24-local-git-env.log` |
| F3 visible yellow red → green | exit 1 (pane dropped supplied colors) → exit 0; 235 pass | `19-yellow-boundary.log`, `19-yellow-green.log` |

## 5. Final verification

All final commands exited **0**. The gate used the caller's `AUTOPILOT_DIR` (P4b checkout),
`unset NO_COLOR`, and `GIT_ALLOW_PROTOCOL=file` (only local test-fixture transport; network Git protocols remain disabled).

| Command | Exit | Pass / fail | Receipt |
| --- | --- | --- | --- |
| `claude plugin test plugins/forge-stats` | 0 | 235 / 0 (12 files) | `23-final-plugin.log` |
| `AUTOPILOT_DIR=<P4b> ./scripts/tux-cross.sh` | 0 | upstream 11 / 0; plugin 235 / 0; prints `tux-cross: ok` | `22-final-cross.log`; also repeated inside final gate |
| `./scripts/tux-lock.test.sh` | 0 | 5 / 0 controls | `22-final-lock.log`; also repeated inside final gate |
| `./scripts/check-all.sh` with the environment above | 0 | 18 / 0 gates; Rust 1318 / 0; plugin 235 / 0 | `25-final-check-all.log` |

The final gate additionally ran `tux-cross.test.sh` (10 controls), vocabulary lint (5 controls),
the pinned format check, clippy, shellcheck, scan/guard tests, archive hermetic tests and MSRV release smoke.
Its last line is `check-all: all gates green`. After staging all new files, the private-leak, doc-drift and tux-lock checks were also rerun and exited 0.

The additional full staged `git diff --check` exits 2 solely for padding spaces in the seven unmodified upstream tabrow/layout golden text files (`26-vendor-whitespace.log`). The non-vendor staged whitespace check exits 0; pinned golden bytes were preserved.

Final receipt SHA-256 values (paths relative to `target/tux-p5-evidence/`):

- `20-sync-idempotency.log` — `053b71fe24550bde8828158de967b32b864983ad437106abe9bf4fc0d41714ab`
- `22-final-lock.log` — `80fed29d1c3d905938439c488f358cb739be7f2d652feeb771d7a358c60eeac6`
- `22-final-cross.log` — `d9151e10d4708817dae0839d6c3bb831ff4f7cc57b15fde340cbc14185ada7d5`
- `23-final-plugin.log` — `64535cae9856cf6c6a5f2ae390b5ef54b0e07ad9394746eb9cc321e16f9f71ab`
- `25-final-check-all.log` — `c56bc34135b94cae8fdb94b761746c50201f2ac4b23936181ec80417ef129c7c`


## Source constraints and deferred work

- The pinned autopilot tree lacks `mods/live/panel-contract.test.js`. The exact required node invocation exits 0 and runs 11 tests from `mods/tux/golden.test.js`, including its panel reader contract cases. No substitute was written upstream. Evidence: `02-autopilot-required-tests.log`, `02-required-test-tree.log`.
- Claude's hooks test sandbox rejects JSON and node:fs imports even in tests. The generated `.ts` bridge is checked against pinned JSON by tux-lock; tux-cross regenerates it from TUX_GOLDEN_DIR in an isolated plugin copy. It imports no tabrow fixtures.
- The Rust mirror test compares a strict JSON block containing the historical 0.9 threshold. That block stays intact as compatibility metadata; exported runtime QUOTA_CONTRACT.tight and isTight use vendor THRESHOLDS.warn (80). No Rust files changed.
- F6 remains the plan's DROP (no warning/error tone added). P6 pane removal, fleet.console migration, redesign/UX screenshot review, deployment, publication and pushing are out of scope. Existing sprite and domain-specific clipping behavior remain.
- No autopilot checkout writes, hand-edited vendor bytes, git stash, disabled hooks, project hook-path changes, or Rust formatting/source changes. Shipped guard tests configure hooks only in their temporary fixtures. Empty receipts `23-autopilot-status.log` and `23-rust-diff.log` document the source boundaries.

## Commit-hook receipt

The first normal commit attempt exited 1 (`27-commit.log`): gitleaks flagged the `tokens.json`
and `tokens.test.js` SHA-256 fields in VERSION.manifest and VENDOR.lock as generic API keys.
`28-hook-hash-proof.log` reproduces both hashes from the pinned files, and tux-lock confirms them.
The hook explicitly accepts `Lab-Hook-Override: <why this is a false positive>` for this case;
the commit message documents that these four values are reproducible checksums, not credentials.
All hooks remain enabled; no vendor bytes were altered to evade scanning.

## Every changed file and reason

- `A artifacts/shared-tui-p5.md` — Record the ordered implementation, command exits, file reasons, evidence paths and scope boundaries.
- `M plugins/forge-stats/hooks/fleet-view.ts` — Use vendor percentages, semantic colors and age glyphs for fleet rows.
- `M plugins/forge-stats/hooks/model.ts` — Re-export vendor widths, adapt vendor glyphs, use dur, and remove dead band/fleet-summary code.
- `M plugins/forge-stats/hooks/panel.ts` — Import the shared schema and side tiers; map writer colors through PALETTE.
- `M plugins/forge-stats/hooks/pet-icon.ts` — Derive icon candidates from GLYPHS.pet.icons and remove the sender table.
- `M plugins/forge-stats/hooks/quota-view.ts` — Use vendor percent flooring, PALETTE and GLYPHS in quota output.
- `M plugins/forge-stats/hooks/quota.ts` — Use vendor warning threshold at runtime while retaining the Rust-tested wire snapshot.
- `M plugins/forge-stats/hooks/register.tsx` — Use shared formatting/colors/glyphs, simplify summary calls, and preserve segment colors when drawing.
- `M plugins/forge-stats/hooks/side.ts` — Import shared side tiers, percentages and semantic colors.
- `M plugins/forge-stats/hooks/strings.ts` — Generate threshold legend text and keyboard glyph reference from vendor constants.
- `M plugins/forge-stats/tests/band.test.tsx` — Remove dead layout/fleet-summary cases and retain parsing/fallback assertions.
- `M plugins/forge-stats/tests/fleet-tab.test.tsx` — Update duration expectations and prove visible yellow starts at 80%.
- `A plugins/forge-stats/tests/panel-contract.test.ts` — Compare writer schema/tones/tiers/pet/usage output and rejection behavior with panel golden cases.
- `M plugins/forge-stats/tests/quota-ui.test.tsx` — Retain live quota/pet drop-order coverage; update duration/threshold expectations and add percent-floor cases.
- `M plugins/forge-stats/tests/quota.test.tsx` — Assert warning threshold boundaries and the runtime contract value.
- `A plugins/forge-stats/tests/tux-golden-data.ts` — Generated code-only bridge for the five JSON fixtures accepted by the sandbox.
- `A plugins/forge-stats/tests/tux-golden.test.tsx` — Compare glyphs, widths, percentages and durations with golden data; distinguish latest from running.
- `M plugins/forge-stats/tests/usage.test.tsx` — Expect vendor zero-padded day/hour duration output.
- `A plugins/forge-stats/vendor/tux/VENDOR.lock` — Generated pin metadata and all 26 source-file hashes.
- `A plugins/forge-stats/vendor/tux/VERSION` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/VERSION.manifest` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/contracts/panel.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/format.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/glyphs.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden-render.js` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden.test.js` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/dur-cases.json` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/glyphs.json` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/layout-bottom-120.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/layout-bottom-80.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/panel-cases.json` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/pct-cases.json` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/tabrow-120.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/tabrow-160.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/tabrow-80.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/tabrow-console-120.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/tabrow-console-80.txt` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/golden/width-cases.json` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/index.js` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/palette.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/strip.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/tiers.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/tokens.json` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/tokens.test.js` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `A plugins/forge-stats/vendor/tux/width.ts` — Unmodified upstream tux v1 file copied by tux-sync from the pinned commit.
- `M scripts/check-all.sh` — Run tux vocabulary lint, lock and cross checks; update shellcheck inputs and expose skipped cross checks.
- `D scripts/pet-cross.sh` — Retired in favor of tux-cross.sh.
- `D scripts/pet-cross.test.sh` — Retired in favor of tux-cross.test.sh.
- `D scripts/pet-cross/cross.test.mjs` — Retired P3 writer/reader harness; replaced by shared writer/golden and sibling contract checks.
- `M scripts/release-local.sh` — Require tux-cross --release before packaging.
- `A scripts/tux-cross.sh` — Compare complete sibling tux bytes/version, run sibling tests and sandbox tests against sibling golden data.
- `A scripts/tux-cross.test.sh` — Exercise ten sibling/release/drift/runner controls in isolated fixtures.
- `A scripts/tux-golden-data.py` — Read TUX_GOLDEN_DIR or pinned JSON and generate/check the sandbox-importable fixture bridge.
- `A scripts/tux-lint.py` — Reject shared glyph literals and direct theme colors after migration; include five controls.
- `A scripts/tux-lock.sh` — Reject hash/file-list/version drift and stale generated fixture data.
- `A scripts/tux-lock.test.sh` — Exercise isolated matching, edited-byte, unlisted-file, missing-file and symlink controls.
- `A scripts/tux-sync.sh` — Archive the specified upstream commit and generate VENDOR.lock and sandbox fixtures idempotently.
