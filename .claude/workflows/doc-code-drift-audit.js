export const meta = {
  name: 'doc-code-drift-audit',
  description: 'Full 6-domain audit of CodeForge docs vs actual code, adversarially verified, graded report (report-only, no edits)',
  whenToUse: 'Periodic (>30 days since last) or after L-size work that touches user-facing behavior / 3+ modules. EXPENSIVE (~2M tokens, ~56 agents). Report-only — does not edit.',
  phases: [
    { title: 'Review', detail: '6 domain agents verify doc claims vs source code' },
    { title: 'Verify', detail: 'skeptic refutes each finding to kill false positives' },
  ],
}

// Agents run with cwd = repo root. Paths are repo-relative.
const REPO = '.'

const FINDINGS_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    domain: { type: 'string' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        properties: {
          doc_file: { type: 'string' },
          location: { type: 'string', description: 'section heading or line number in the doc' },
          severity: { type: 'string', enum: ['WRONG', 'STALE', 'MISSING'] },
          claim: { type: 'string', description: 'what the doc says' },
          actual: { type: 'string', description: 'what the code actually does' },
          evidence: { type: 'string', description: 'source file:line proving the actual behavior' },
        },
        required: ['doc_file', 'location', 'severity', 'claim', 'actual', 'evidence'],
      },
    },
  },
  required: ['domain', 'findings'],
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    verdict: { type: 'string', enum: ['confirmed', 'refuted', 'uncertain'] },
    reason: { type: 'string' },
    corrected: { type: 'string', description: 'if the finding itself is imprecise, the corrected statement; else empty' },
  },
  required: ['verdict', 'reason'],
}

const DOMAINS = [
  {
    key: 'memory-pipeline',
    prompt: `Audit CodeForge docs for doc-vs-code DRIFT in the MEMORY PIPELINE domain. Repo root: ${REPO}.
DOCS: doc/concepts.md (§1-4), doc/specs/codeforge-ship.md, doc/specs/codeforge-memory-contract.md, dream/ship sections of README.md + CLAUDE.md.
CODE: src/dream/ (compile.rs, runner.rs, absorb.rs, ingest_digests.rs), src/cli/ship.rs, src/cli/mnemos_cli.rs, src/mnemos/*, src/llm.rs, src/db/mod.rs.
Verify EVERY factual claim (LLM fallback chain, endpoints/URLs, opted_in gate, retry schedule, ship-failed/ship-rejected/ship-state, no-ship opt-out, cite method/confidence, L0/L1/L2 paths, origin filtering, --quiet, SessionEnd hook chain, what ship actually reads). Report WRONG (contradicts code) / STALE (was true, now outdated) / MISSING (code has important behavior docs omit). Every finding needs source file:line in evidence. Be skeptical. Empty findings array if all accurate.`,
  },
  {
    key: 'pet-daemon-combat',
    prompt: `Audit CodeForge docs for doc-vs-code DRIFT in the PET / DAEMON / COMBAT domain. Repo root: ${REPO}.
DOCS: doc/concepts.md (§5-6), doc/specs/codeforge-mud-engine.md, pet/daemon sections of README.md + CLAUDE.md.
CODE: src/pet/* , src/power/, src/daemon/* , src/clan/.
Verify EVERY claim (PetState fields, XP values per event, leveling math, 4 stats meanings, village count+names, daemon-write vs CLI-overlay ownership, badges status, clan skeleton wired-or-not, MOB scan rate, elite thresholds, ability effects, combat formula). Report WRONG/STALE/MISSING with file:line. Be skeptical. Empty array if accurate.`,
  },
  {
    key: 'install-hooks-statusline',
    prompt: `Audit CodeForge docs for doc-vs-code DRIFT in the INSTALL / HOOKS / STATUSLINE domain. Repo root: ${REPO}.
DOCS: README.md hook-setup section, doc/specs/codeforge-install-subcommand.md, doc/getting-started.md, CLAUDE.md Hook Path Note + Statusline sections, .env.example.
CODE: src/cli/install.rs, src/cli/statusline.rs, .claude/scripts/*.js, src/cli/init.rs, env var handling (CODEFORGE_DIR/DATA_DIR/LOCALE in src/main.rs).
Verify (Layer-1 clone-only vs Layer-2 global hooks, what each install flag writes, hook chain order + hook-type count, what session-digest.js writes and WHERE, statusline line count + invocation, env var defaults, uninstall surface). Report WRONG/STALE/MISSING with file:line. Be skeptical. Empty array if accurate.`,
  },
  {
    key: 'cli-command-table',
    prompt: `Audit CodeForge docs for doc-vs-code DRIFT in the CLI SURFACE domain. Repo root: ${REPO}.
Enumerate the real command tree from clap (src/main.rs + src/cli/*), then compare to docs (README quickstart/tables, getting-started, concepts, CLAUDE.md Development Commands).
Report: documented commands/flags that DON'T exist (WRONG), renamed/changed (STALE), implemented commands/flags never mentioned in any user doc (MISSING). Every finding needs file:line. Empty array if docs match the real CLI surface.`,
  },
  {
    key: 'phase-status',
    prompt: `Audit CodeForge docs for doc-vs-code DRIFT in the PHASE/ROADMAP STATUS domain. Repo root: ${REPO}.
DOCS: README.md Phase Roadmap, CLAUDE.md Phase Roadmap + Mnemos Integration Roadmap, doc/concepts.md §6.
VERIFY against doc/projects/INDEX.md (archived = shipped), doc/projects/_archive/ listing, and spot-check code modules exist for each "shipped" phase / are absent for each "planned". Report WRONG (claimed shipped but not in code) / STALE (claimed planned but actually shipped) / MISSING with dir/file evidence. Empty array if accurate.`,
  },
  {
    key: 'changelog-version-nation',
    prompt: `Audit CodeForge docs for doc-vs-code DRIFT in the CHANGELOG/VERSION/NATION-SPEC domain. Repo root: ${REPO}.
Part A: Read CHANGELOG.md + Cargo.toml. Verify version sync, and notable shipped features (run: git log --oneline -40) MISSING from CHANGELOG [Unreleased]. Note: do NOT flag already-released dated entries as needing rewrite — only missing Unreleased entries.
Part B: Read doc/specs/nation-p2p-design.md + concepts.md §6. Verify Nation P2P is consistently labeled Phase-5 ROADMAP; grep src/ for ed25519/nation/P2P to confirm genuinely unimplemented.
Report WRONG/STALE/MISSING with file:line (or commit sha). Be skeptical. Empty array if accurate.`,
  },
]

phase('Review')

const results = await pipeline(
  DOMAINS,
  (d) => agent(d.prompt, { label: `review:${d.key}`, phase: 'Review', schema: FINDINGS_SCHEMA }),
  (review, d) => {
    const findings = (review && review.findings) || []
    if (!findings.length) return { domain: d.key, verified: [] }
    return parallel(
      findings.map((f) => () =>
        agent(
          `Adversarially verify this documentation-drift finding. Repo root: ${REPO}. Default to "refuted" if you cannot independently confirm it from the code.
Finding (domain ${d.key}):
- doc_file: ${f.doc_file}
- location: ${f.location}
- severity claimed: ${f.severity}
- doc claims: ${f.claim}
- auditor says actual code does: ${f.actual}
- cited evidence: ${f.evidence}
Open the cited evidence file:line AND the doc location yourself. Confirm the doc genuinely says what's claimed AND the code genuinely does what's claimed. If the auditor's "actual" is itself imprecise, set verdict=uncertain and put the correct statement in "corrected". Only verdict=confirmed if the drift is real and the evidence checks out.`,
          { label: `verify:${d.key}:${f.doc_file}`, phase: 'Verify', schema: VERDICT_SCHEMA }
        ).then((v) => ({ ...f, domain: d.key, verdict: v }))
      )
    ).then((arr) => ({ domain: d.key, verified: arr.filter(Boolean) }))
  }
)

const all = results.filter(Boolean).flatMap((r) => r.verified || [])
const confirmed = all.filter((f) => f.verdict && f.verdict.verdict === 'confirmed')
const uncertain = all.filter((f) => f.verdict && f.verdict.verdict === 'uncertain')
const refuted = all.filter((f) => f.verdict && f.verdict.verdict === 'refuted')
const bySev = (sev) => confirmed.filter((f) => f.severity === sev)

return {
  summary: {
    domains_reviewed: DOMAINS.length,
    raw_findings: all.length,
    confirmed: confirmed.length,
    uncertain: uncertain.length,
    refuted: refuted.length,
    confirmed_WRONG: bySev('WRONG').length,
    confirmed_STALE: bySev('STALE').length,
    confirmed_MISSING: bySev('MISSING').length,
  },
  confirmed,
  uncertain,
}
