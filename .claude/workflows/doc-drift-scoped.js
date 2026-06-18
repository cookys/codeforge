export const meta = {
  name: 'doc-drift-scoped',
  description: 'Lightweight doc-vs-code drift check scoped to the modules changed in this diff (report-only). Cheap — for routine L-size doc-sync.',
  whenToUse: 'Default doc-sync check at the end of L-size work. Pass args = base ref (default "main"). Cheap (~1-3 agents). For the full 6-domain sweep use doc-code-drift-audit instead.',
  phases: [
    { title: 'Scope+Find', detail: 'map changed modules → doc claims, verify against code' },
    { title: 'Verify', detail: 'skeptic confirms each finding' },
  ],
}

// args may be a base ref string ("main"), or {base}. Agents run at repo root.
const base = (typeof args === 'string' && args) || (args && args.base) || 'main'

const FINDINGS_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    changed_modules: { type: 'array', items: { type: 'string' }, description: 'src modules detected as changed' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        properties: {
          doc_file: { type: 'string' },
          location: { type: 'string' },
          severity: { type: 'string', enum: ['WRONG', 'STALE', 'MISSING'] },
          claim: { type: 'string' },
          actual: { type: 'string' },
          evidence: { type: 'string', description: 'source file:line' },
        },
        required: ['doc_file', 'location', 'severity', 'claim', 'actual', 'evidence'],
      },
    },
  },
  required: ['changed_modules', 'findings'],
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    verdict: { type: 'string', enum: ['confirmed', 'refuted', 'uncertain'] },
    reason: { type: 'string' },
    corrected: { type: 'string' },
  },
  required: ['verdict', 'reason'],
}

phase('Scope+Find')

const scan = await agent(
  `Scoped doc-vs-code drift check. Repo root = cwd.
STEP 1 — find what code changed: run \`git diff --name-only ${base}...HEAD\` (and also \`git diff --name-only ${base}\` to include uncommitted work). Collect the changed files under src/. Map them to logical modules (e.g. src/cli/ship.rs → "ship", src/daemon/* → "daemon/combat", src/pet/* → "pet").
STEP 2 — for EACH changed module, find the docs that describe it. Search README.md, CLAUDE.md, doc/concepts.md, doc/getting-started.md, doc/specs/*.md, .env.example, CHANGELOG.md for any factual claim about that module's behavior, flags, defaults, or status.
STEP 3 — verify each such claim against the ACTUAL changed code. Report drift: WRONG (contradicts code), STALE (was true, now outdated by this change), MISSING (this change added/removed behavior the docs should mention but don't — e.g. a new flag, new command, changed default, new CHANGELOG-worthy feature).
Only audit docs whose claims relate to the changed modules — do NOT do a full-repo sweep. Every finding needs a source file:line in evidence. Be skeptical. If no doc drift from these changes, return an empty findings array (still fill changed_modules).`,
  { label: 'scope+find', phase: 'Scope+Find', schema: FINDINGS_SCHEMA }
)

const findings = (scan && scan.findings) || []
let confirmed = []
let uncertain = []
if (findings.length) {
  const verified = await parallel(
    findings.map((f) => () =>
      agent(
        `Adversarially verify this scoped doc-drift finding. Repo root = cwd. Default to "refuted" if you can't independently confirm from code.
- doc_file: ${f.doc_file}
- location: ${f.location}
- severity: ${f.severity}
- doc claims: ${f.claim}
- actual (auditor): ${f.actual}
- evidence: ${f.evidence}
Open the cited file:line AND the doc yourself. confirmed only if the drift is real; uncertain (+corrected) if the auditor's "actual" is imprecise.`,
        { label: `verify:${f.doc_file}`, phase: 'Verify', schema: VERDICT_SCHEMA }
      ).then((v) => ({ ...f, verdict: v }))
    )
  )
  const ok = verified.filter(Boolean)
  confirmed = ok.filter((f) => f.verdict.verdict === 'confirmed')
  uncertain = ok.filter((f) => f.verdict.verdict === 'uncertain')
}

return {
  base,
  changed_modules: (scan && scan.changed_modules) || [],
  summary: {
    raw_findings: findings.length,
    confirmed: confirmed.length,
    uncertain: uncertain.length,
    WRONG: confirmed.filter((f) => f.severity === 'WRONG').length,
    STALE: confirmed.filter((f) => f.severity === 'STALE').length,
    MISSING: confirmed.filter((f) => f.severity === 'MISSING').length,
  },
  confirmed,
  uncertain,
}
