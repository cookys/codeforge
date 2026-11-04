// G40 quota model: the pure reader side of autopilot's `fleet.quota/1` mirror (no I/O, no rendering).
// Contract: autopilot `project/plans/2026-11-03-g40-agent-lane-quota.md` §5.2 (merge), §20-§22 (record, view, refusal, token state);
// display rules: doc/plans/2026-11-03-g40-usage-accounts.md §10.

// contract:begin quota — `fleet.quota/1` mirror constants. Every number the contract names lives here and nowhere else.
export const QUOTA_CONTRACT = {
  "schema": "fleet.quota/1",
  "view_schema": "fleet.quota-view/1",
  "ns_schema": "fleet.quota-ns/1",
  "kinds": ["rolling_5h", "weekly", "daily", "monthly", "credits", "rpm", "tpm"],
  "sources": ["passive", "endpoint", "probe", "estimated"],
  "units": ["requests", "tokens", "credits"],
  "refusal_reasons": ["rate_limit", "quota", "other"],
  "token_states": ["ok", "invalid", "unknown"],
  "visibility": ["L0", "L1", "L2", "L3"],
  "provider": "^[a-z0-9-]{1,24}$",
  "alias": "^[a-z0-9-]{1,24}$",
  "max_age_s": 21600,
  "future_skew_s": 300,
  "tight": 0.9,
  "token_warn_days": { "notice": 30, "urgent": 7 },
  "refusal_default_cooldown_s": 1800,
  "max_windows": 7,
  "view_max_bytes": 32768,
  "max_rows": 256,
  "epoch_s_max": 100000000000
} as const
// contract:end

const Q = QUOTA_CONTRACT
const PROVIDER_RE = new RegExp(Q.provider)
const ALIAS_RE = new RegExp(Q.alias)
const COURIER_RE = /^[A-Za-z0-9._:@\/-]{1,128}$/
const ROLE_RE = /^[A-Za-z0-9._-]{1,64}$/
const DAY_S = 86400
/** Field names the contract rejects (never renames); `*_ms` is matched by suffix. */
const DRIFT = new Set(['pct', 'resetsAt', 'five_hour', 'seven_day', 'namespace', 'writer_courier', 'sent_at'])

export type QKind = (typeof QUOTA_CONTRACT.kinds)[number]
export type QSource = (typeof QUOTA_CONTRACT.sources)[number]
export type QUnit = (typeof QUOTA_CONTRACT.units)[number]
export type QLevel = (typeof QUOTA_CONTRACT.visibility)[number]
export type QTokenState = (typeof QUOTA_CONTRACT.token_states)[number]
export type QReason = (typeof QUOTA_CONTRACT.refusal_reasons)[number]

export type QWindow = {
  kind: QKind
  utilization: number | null
  used: number | null
  limit: number | null
  remaining: number | null
  unit: QUnit | null
  resetsAt: number | null
  source: QSource
  observedAt: number
}
export type QRefusal = { observedAt: number; retryAt: number | null; reason: QReason }
export type QRecord = {
  visibility: QLevel
  windows: QWindow[]
  refusal: QRefusal | null
  tokenState: QTokenState
  tokenStateAt: number | null
  hostRole: string
  tokenIssuedAt: number | null
  tokenExpiresAt: number | null
  placeholder: boolean
}
export type QRow = { writerCourier: string; hostRole: string; observedAtHub: number | null; skewed: boolean; record: QRecord }
export type QuotaView = { status: 'ok'; namespace: string | null; hubOffset: number; rows: QRow[]; rejected: number }
export type QuotaParse = QuotaView | { status: 'bad'; reason: string }

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)
const isSec = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= Q.epoch_s_max
const secOrNull = (v: unknown): number | null | undefined => (v === null || v === undefined ? null : isSec(v) ? v : undefined)
const inSet = (v: unknown, set: readonly string[]) => typeof v === 'string' && set.includes(v)
const numGe0 = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v) && v >= 0
const numOrNull = (v: unknown): number | null | undefined => (v === null || v === undefined ? null : numGe0(v) ? v : undefined)
const onlyKeys = (v: Record<string, unknown>, required: readonly string[], optional: readonly string[]): boolean => {
  const keys = Object.keys(v)
  return required.every(k => k in v) && keys.every(k => required.includes(k) || optional.includes(k))
}
const hasDrift = (v: Record<string, unknown>) => Object.keys(v).some(k => DRIFT.has(k) || k.endsWith('_ms'))

function parseWindow(v: unknown): QWindow | null {
  if (!isObj(v) || hasDrift(v)) return null
  if (!onlyKeys(v, ['kind', 'utilization', 'source', 'observed_at'], ['used', 'limit', 'unit', 'remaining', 'resets_at', 'clamped'])) return null
  if (!inSet(v.kind, Q.kinds) || !inSet(v.source, Q.sources) || !isSec(v.observed_at)) return null
  if (v.clamped !== undefined && typeof v.clamped !== 'boolean') return null
  const resetsAt = secOrNull(v.resets_at)
  const used = numOrNull(v.used)
  const limit = numOrNull(v.limit)
  const remaining = numOrNull(v.remaining)
  if (resetsAt === undefined || used === undefined || limit === undefined || remaining === undefined) return null
  if (v.unit !== null && v.unit !== undefined && !inSet(v.unit, Q.units)) return null
  const unit = (v.unit ?? null) as QUnit | null
  if ((used === null) !== (limit === null)) return null
  if ((used !== null || remaining !== null) && unit === null) return null
  if (remaining !== null) {
    if (v.kind !== 'credits') return null
    if (used !== null && limit !== null && Math.abs(remaining - (limit - used)) > 1e-9) return null
  }
  const remainingOnly = remaining !== null && used === null
  let utilization: number | null
  if (v.utilization === null) {
    // null is legal only on `credits`, remaining-only shape (§21.2)
    if (v.kind !== 'credits' || !remainingOnly) return null
    utilization = null
  } else {
    if (typeof v.utilization !== 'number' || !Number.isFinite(v.utilization) || v.utilization < 0 || v.utilization > 1) return null
    if (remainingOnly) return null
    utilization = v.utilization
  }
  return { kind: v.kind as QKind, utilization, used, limit, remaining, unit, resetsAt, source: v.source as QSource, observedAt: v.observed_at }
}

function parseRecord(v: unknown, provider: string, alias: string): QRecord | null {
  if (!isObj(v) || hasDrift(v)) return null
  if (!onlyKeys(v, ['schema', 'provider', 'alias', 'visibility', 'windows', 'host_role'], ['refusal', 'token_state', 'token_state_at', 'token_issued_at', 'token_expires_at', 'source'])) return null
  if (v.schema !== Q.schema || v.provider !== provider || v.alias !== alias) return null
  if (!inSet(v.visibility, Q.visibility) || typeof v.host_role !== 'string' || !ROLE_RE.test(v.host_role)) return null
  if (!Array.isArray(v.windows) || v.windows.length > Q.max_windows) return null
  const windows: QWindow[] = []
  for (const w of v.windows) {
    const p = parseWindow(w)
    if (!p || windows.some(o => o.kind === p.kind)) return null
    windows.push(p)
  }
  let refusal: QRefusal | null = null
  if (v.refusal !== null && v.refusal !== undefined) {
    const r = v.refusal
    if (!isObj(r) || hasDrift(r) || !onlyKeys(r, ['observed_at', 'retry_at', 'reason'], [])) return null
    const retryAt = secOrNull(r.retry_at)
    if (!isSec(r.observed_at) || retryAt === undefined || !inSet(r.reason, Q.refusal_reasons)) return null
    refusal = { observedAt: r.observed_at, retryAt, reason: r.reason as QReason }
  }
  const tokenState = (v.token_state === undefined ? 'unknown' : v.token_state) as string
  if (!inSet(tokenState, Q.token_states)) return null
  const tokenStateAt = secOrNull(v.token_state_at)
  const tokenIssuedAt = secOrNull(v.token_issued_at)
  const tokenExpiresAt = secOrNull(v.token_expires_at)
  if (tokenStateAt === undefined || tokenIssuedAt === undefined || tokenExpiresAt === undefined) return null
  if ((tokenState === 'unknown') !== (tokenStateAt === null)) return null
  if (v.source !== undefined && v.source !== 'registry') return null
  const visibility = v.visibility as QLevel
  if (refusal && visibility === 'L0') return null
  // §20.4: L0 shows nothing, L1 only a refusal; a record carrying windows at either level is malformed
  if (windows.length > 0 && (visibility === 'L0' || visibility === 'L1')) return null
  let placeholder = false
  if (windows.length === 0) {
    // §21.6: empty windows are valid only as (a) placeholder, (b) refusal-only, (c) credential-state-only
    if (visibility === 'L2' || visibility === 'L3') return null
    if (refusal) {
      if (visibility !== 'L1' || v.source !== undefined) return null
    } else if (tokenState !== 'unknown') {
      if (v.source !== undefined) return null
    } else {
      if (v.source !== 'registry' || visibility !== 'L0') return null
      placeholder = true
    }
  } else if (v.source !== undefined) return null
  return { visibility, windows, refusal, tokenState: tokenState as QTokenState, tokenStateAt, hostRole: v.host_role, tokenIssuedAt, tokenExpiresAt, placeholder }
}

function parseRow(v: unknown, provider: string, alias: string): QRow | null {
  if (!isObj(v) || !onlyKeys(v, ['writer_courier', 'host_role', 'observed_at_hub', 'skewed', 'record'], [])) return null
  if (typeof v.writer_courier !== 'string' || !COURIER_RE.test(v.writer_courier)) return null
  if (typeof v.host_role !== 'string' || !ROLE_RE.test(v.host_role) || typeof v.skewed !== 'boolean') return null
  const oah = v.observed_at_hub === null ? null : isSec(v.observed_at_hub) ? v.observed_at_hub : undefined
  if (oah === undefined) return null
  const record = parseRecord(v.record, provider, alias)
  if (!record) return null
  // row-level host_role is the hub's copy; the record's value is the one a reader uses (§22.2)
  return { writerCourier: v.writer_courier, hostRole: record.hostRole, observedAtHub: oah, skewed: v.skewed, record }
}

export const byteLength = (s: string) => (typeof TextEncoder !== 'undefined' ? new TextEncoder().encode(s).length : s.length)

/** Parse one mirror view file (`<live_dir>/quota/<provider>/<alias>.json`). A bad wrapper is `bad`; a bad row is dropped and counted. */
export function parseQuotaView(text: string, provider: string, alias: string): QuotaParse {
  if (!PROVIDER_RE.test(provider) || !ALIAS_RE.test(alias)) return { status: 'bad', reason: 'path' }
  if (typeof text !== 'string' || byteLength(text) > Q.view_max_bytes) return { status: 'bad', reason: 'size' }
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    return { status: 'bad', reason: 'json' }
  }
  if (!isObj(raw) || !onlyKeys(raw, ['schema', 'namespace', 'hub_offset', 'rows'], []) || raw.schema !== Q.view_schema) return { status: 'bad', reason: 'schema' }
  if (raw.namespace !== null && typeof raw.namespace !== 'string') return { status: 'bad', reason: 'namespace' }
  if (typeof raw.hub_offset !== 'number' || !Number.isFinite(raw.hub_offset) || Math.abs(raw.hub_offset) > Q.epoch_s_max) return { status: 'bad', reason: 'hub_offset' }
  if (!Array.isArray(raw.rows) || raw.rows.length > Q.max_rows) return { status: 'bad', reason: 'rows' }
  const rows: QRow[] = []
  let rejected = 0
  for (const r of raw.rows) {
    const p = parseRow(r, provider, alias)
    // one row per writer_courier: a repeat is a bad row
    if (!p || rows.some(o => o.writerCourier === p.writerCourier)) rejected++
    else rows.push(p)
  }
  return { status: 'ok', namespace: raw.namespace, hubOffset: raw.hub_offset, rows, rejected }
}

export type MergedWindow = {
  kind: QKind
  utilization: number | null
  remaining: number | null
  unit: QUnit | null
  used: number | null
  limit: number | null
  /** The reset time on the LOCAL clock (record `resets_at` minus `hubOffset`): what a countdown against the local clock needs. */
  resetsAtLocal: number | null
  observedAt: number
  ageS: number
  writerCourier: string
  hostRole: string
  source: QSource
  estimated: boolean
}
export type MergedQuota = {
  windows: MergedWindow[]
  /** `endS` is on the local clock. */
  refusal: { endS: number; reason: QReason; ageS: number } | null
  tokenState: QTokenState
  tokenExpiresAt: number | null
  placeholder: boolean
  hadObservation: boolean
  level: QLevel
  hosts: string[]
  multiHost: boolean
}

type Cand = { w: QWindow; row: QRow; age: number; local: boolean }
const util = (c: Cand) => c.w.utilization ?? -1
/** Smaller age wins; an exact tie goes to the local row, a tie between peers to the higher utilization (§5.2 rule 3). */
function better(a: Cand, b: Cand): Cand {
  if (a.age !== b.age) return a.age < b.age ? a : b
  if (a.local !== b.local) return a.local ? a : b
  return util(b) > util(a) ? b : a
}

/** Merge one view by contract §5.2. `nowS` is the local clock in epoch seconds; `nsFileNamespace` is `quota/namespace.json`'s value (or null). */
export function mergeQuota(view: QuotaView, nsFileNamespace: string | null, nowS: number): MergedQuota {
  const peersOk = view.namespace !== null && nsFileNamespace !== null && view.namespace === nsFileNamespace
  // a row is local when its observed_at_hub is null (§22.2); a peer row is used only past the namespace guard
  const rows = view.rows.filter(r => r.observedAtHub === null || peersOk)
  const nowC = nowS + view.hubOffset

  const measured = new Map<QKind, Cand>()
  const estimated = new Map<QKind, Cand>()
  let refusal: { endS: number; reason: QReason; ageS: number } | null = null
  let hadObservation = false
  let placeholder = false
  const liveRows = new Map<string, string>()
  const stateCands: { state: QTokenState; age: number }[] = []
  let expiry: { age: number; at: number } | null = null

  for (const row of rows) {
    const rec = row.record
    const local = row.observedAtHub === null
    const refNow = local ? nowS : nowC
    const stamps = [...rec.windows.map(w => w.observedAt)]
    if (rec.refusal) stamps.push(rec.refusal.observedAt)
    if (rec.tokenStateAt !== null) stamps.push(rec.tokenStateAt)
    if (stamps.length > 0) hadObservation = true
    else if (rec.placeholder) placeholder = true
    const basis = stamps.length ? Math.max(...stamps) : 0
    // §5.2 rule 1: a peer stamp x has hub time t = observed_at_hub - (B - x); a local stamp is its own time
    const tOf = (x: number) => (local ? x : (row.observedAtHub as number) - (basis - x))
    const ageOf = (x: number) => refNow - tOf(x)

    for (const w of rec.windows) {
      const age = ageOf(w.observedAt)
      if (age > Q.max_age_s || age < -Q.future_skew_s) continue
      if (w.resetsAt !== null && w.resetsAt <= nowC) continue
      liveRows.set(row.writerCourier, rec.hostRole)
      const c: Cand = { w, row, age, local }
      const map = w.source === 'estimated' ? estimated : measured
      const cur = map.get(w.kind)
      map.set(w.kind, cur ? better(cur, c) : c)
    }

    if (rec.refusal) {
      const r = rec.refusal
      const t = tOf(r.observedAt)
      const age = refNow - t
      const end = t + (r.retryAt !== null ? r.retryAt - r.observedAt : Q.refusal_default_cooldown_s)
      const expires = Math.min(end, t + Q.max_age_s)
      if (age >= -Q.future_skew_s && refNow < expires) {
        liveRows.set(row.writerCourier, rec.hostRole)
        if (!refusal || age < refusal.ageS) refusal = { endS: end - (refNow - nowS), reason: r.reason, ageS: age }
      }
    }

    // token_state is a state, not a measurement: the 6 h age rule never retires it (§21.1)
    if (rec.tokenStateAt !== null) {
      stateCands.push({ state: rec.tokenState, age: ageOf(rec.tokenStateAt) })
      liveRows.set(row.writerCourier, rec.hostRole)
    }
    if (rec.tokenExpiresAt !== null) {
      const age = stamps.length ? ageOf(basis) : Infinity
      if (!expiry || age < expiry.age) expiry = { age, at: rec.tokenExpiresAt }
    }
  }

  const out: MergedWindow[] = []
  for (const kind of Q.kinds) {
    const m = measured.get(kind)
    const e = estimated.get(kind)
    let pick: Cand | null = m ?? e ?? null
    let isEst = !m && !!e
    // an estimate can only raise (max(measured, estimated)), never lower or replace (§5.2 rule 4)
    if (m && e && e.w.utilization !== null && m.w.utilization !== null && e.w.utilization > m.w.utilization) {
      pick = e
      isEst = true
    }
    if (!pick) continue
    const w = pick.w
    out.push({
      kind, utilization: w.utilization, remaining: w.remaining, unit: w.unit, used: w.used, limit: w.limit, resetsAtLocal: w.resetsAt === null ? null : w.resetsAt - view.hubOffset,
      observedAt: w.observedAt, ageS: pick.age, writerCourier: pick.row.writerCourier, hostRole: pick.row.record.hostRole, source: w.source, estimated: isEst,
    })
  }

  // newest state change wins; an exact tie goes to `invalid` (the dangerous direction)
  let tokenState: QTokenState = 'unknown'
  let best: { state: QTokenState; age: number } | null = null
  for (const s of stateCands) {
    if (!best || s.age < best.age || (s.age === best.age && s.state === 'invalid')) best = s
  }
  if (best) tokenState = best.state

  const level: QLevel = out.length > 0 ? (out.every(w => w.resetsAtLocal !== null) ? 'L3' : 'L2') : refusal ? 'L1' : 'L0'
  const hosts = [...liveRows.values()]
  return {
    windows: out, refusal, tokenState, tokenExpiresAt: expiry ? expiry.at : null, placeholder: placeholder && !hadObservation,
    hadObservation, level, hosts, multiHost: liveRows.size >= 2,
  }
}

/** Token expiry warn level (§21.4): at or under `notice` days warns, at or under `urgent` warns harder, past the date is `expired`. */
export function tokenWarn(expiresAtS: number | null, nowS: number): { level: 'notice' | 'urgent' | 'expired'; days: number } | null {
  if (expiresAtS === null) return null
  const left = expiresAtS - nowS
  if (left <= 0) return { level: 'expired', days: Math.floor(left / DAY_S) }
  const days = Math.ceil(left / DAY_S)
  if (left <= Q.token_warn_days.urgent * DAY_S) return { level: 'urgent', days }
  if (left <= Q.token_warn_days.notice * DAY_S) return { level: 'notice', days }
  return null
}

const TIGHT_KINDS: readonly QKind[] = ['rolling_5h', 'weekly', 'daily', 'monthly', 'credits']
/** The window with the largest utilization among the gauge kinds; null-utilization credits and rpm/tpm never count. */
export function tightest(merged: MergedQuota): MergedWindow | null {
  let best: MergedWindow | null = null
  for (const w of merged.windows) {
    if (!TIGHT_KINDS.includes(w.kind) || w.utilization === null) continue
    if (!best || w.utilization > (best.utilization as number)) best = w
  }
  return best
}

export const isTight = (utilization: number) => utilization >= Q.tight
