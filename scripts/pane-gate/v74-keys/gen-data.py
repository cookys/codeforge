#!/usr/bin/env python3
# gen-data.py <data_dir> <session_id> <cwd> [noproject] : fake autopilot live files + repo card store for the v7.4 key gate (idempotent; rerun refreshes timestamps).
# All data is made up (no real host, org, amount or account). The store's first three cards share their ids with the fake fleet's
# decision list (mods/live/fixtures/v74/decide.json), so both sources read together dedupe to 3; d-keys0004 is the store-only test card.
# noproject: the cwd is not mapped to a project (the PM seat's case: the band must still show fleet, decisions and tok/s).
import hashlib, json, os, sys, time, datetime
D, SID, CWD = sys.argv[1:4]
CWD = os.path.realpath(CWD)
now = time.time()
def iso(ago_s=0): return datetime.datetime.fromtimestamp(now - ago_s, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.000Z')
def put(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + '.tmp'
    open(tmp, 'w', encoding='utf-8').write(json.dumps(obj, ensure_ascii=False))
    os.rename(tmp, path)
H = D + '/home'; AH = H + '/.autopilot'; LIVE = D + '/live'
NOPROJ = 'noproject' in sys.argv[4:]
LONGCARD = 'longcard' in sys.argv[4:]  # one more card whose background is 80 lines (taller than the full-screen Pane): the PgDn step
# project: the card store lives in the session's own repo (run.py git-inits the cwd), and the key is the one autopilot derives
# from that repo (src/status/project-key.js: sha256 of the identity, 16 hex), so open-decision.js close finds and closes the card.
REPO = CWD if not NOPROJ else D + '/demo'
os.makedirs(REPO + '/.git', exist_ok=True)
IDENT = 'git-common-dir:' + os.path.realpath(REPO + '/.git')
KEY = hashlib.sha256(IDENT.encode()).hexdigest()[:16]
put(AH + '/live-pointer.json', dict(schema='autopilot.live-pointer/1', live_base=LIVE, autopilot_home=AH, written_at=iso()))
if not NOPROJ: put(LIVE + '/runs/paths/demo.json', {CWD: dict(project_key=KEY)})
put(AH + '/session-mode/%s.json' % SID, dict(session_id=SID, level=None, project_key=KEY, root_run_id=None, started_at=iso(3*3600+26*60), expires_at=iso(-6*3600)))
def row(i, runner, role, ago, **o):
    r = dict(run_id=i, role=role, runner=runner, model='', root_run_id=None, started_at=iso(ago), fact_at=iso(ago), elapsed_s=ago, phase='running', alive=True, probe_age_s=5, stall=False, rc=None, final_status=None)
    r.update(o); return r
runs = [row('甲線', 'codex', 'feat/tux-sync 測試', 12*60),
        row('乙線', 'kimi', 'scripts README 盤點', 55*60, phase='exited', alive=False, rc=0, final_status='done', ended_at=iso(40*60), elapsed_s=15*60),
        row('丙線', 'kimi', 'learn/search 整合測試', 30*60, phase='exited', alive=False, rc=124, final_status='timeout', ended_at=iso(10*60), elapsed_s=1200)]
put(LIVE + '/runs/%s.json' % KEY, dict(schema='autopilot.runs-live/1', scope=dict(project_key=KEY, repo_identity=IDENT, root_run_id=None),
    published_at=iso(), observed_at=iso(), valid_for_s=86400, writer=None, runs=runs,
    counts=dict(confirmed_live=1, exited=2, unknown=0, fresh_bound_s=30)))
put(LIVE + '/context/%s.json' % SID, dict(schema_version=1, written_at=iso(), context_window=dict(used_percentage=11)))
put(LIVE + '/turn/%s.json' % SID, dict(schema='autopilot.session-turn/1', session_id=SID, state='active', since=iso(60)))
put(LIVE + '/runs/%s.qc.json' % KEY, dict(schema='autopilot.qc-status/1', scope=dict(project_key=KEY, root_run_id=None), state='not_needed', range='a1b2c3d4..HEAD', protected_files_count=0, evidence=None))
put(LIVE + '/runs/%s.decisions.json' % KEY, dict(schema='autopilot.decisions-sidecar/1', scope=dict(project_key=KEY, repo_identity=IDENT, root_run_id=None), count=2, irreversible_count=0, undocumented_dispatches=0, writers_wired=['depth0'],
    rows=[dict(round=1, decision='合併 feat/tux-sync 進 develop（check-all 綠）', irreversible=False, writer='depth0', at=iso(10*60)),
          dict(round=2, decision='改用 codex 跑 learn/search 測試（kimi 逾時兩次）', irreversible=False, writer='depth0', at=iso(25*60))]))
put(LIVE + '/runs/%s.review.json' % KEY, dict(schema='autopilot.review/1', project_key=KEY, plan=None,
    code=dict(phase='code', generation=2, base='a1b2c3d4', head='e5f60718', converged=False, at=iso(5*60),
              seats=[dict(id='G 引擎', family=None, status='2 Major', verdict='FIX-THEN-SHIP')])))
put(LIVE + '/runs/%s.residue.json' % KEY, dict(schema='autopilot.residue/1', project_key=KEY, reapable_worktrees=0, by_class={}, at=iso(60),
    needs_human_count=1, needs_human_bytes=None, auto_ran_at=iso(3600), auto_removed_count=2, auto_archived_count=0,
    needs_human=[dict(kind='worktree', path='/work/demo/.claude/worktrees/feat-old-probe', **{'class': 'clean-unintegrated'}, age_days=5, bytes=None, reason='5 天沒有 commit', command=None)]))
put(LIVE + '/load-source.json', dict(schema='autopilot.load-source/1', plugin_version=None, source='dev', marketplace='ok', marketplace_path=None, behind_upstream=None, flags=[], stale_cache_dirs=[]))
samples = []; cum = 0.0
for k in range(26, -1, -1):
    cum += (k % 5 + 1) * 0.1 if k else 0.2; samples.append(dict(at=iso(k*1800), usd=round(cum, 2)))
put(LIVE + '/runs/spend-history.json', dict(schema='autopilot.spend-history/1', samples=samples))
def card(i, title, q, bg, opts, rec, why, ago):
    return dict(id=i, title=title, question=q, background=bg, options=[dict(label=a, description=b) for a, b in opts], recommendation=rec, why=why, status='open', opened_at=iso(ago), source='demo', origin='depth0')
LONGC = [card('d-keys0005', '驗收長卡', '這張卡的背景有八十行，用來驗證 PgDn 的步長', '\n'.join('第 %02d 行：假資料，只在沙盒卡片庫，用來把內文撐得比 Pane 高。' % n for n in range(1, 81)), [('收到', ''), ('不收', '')], '收到', None, 2*60)] if LONGCARD else []
# AWAIT_CARD: a JSON list of extra cards (pane-history shoot.py --cards), e.g. an awaiting_operator_action card with operator_steps
EXTRA = json.load(open(os.environ['AWAIT_CARD'])) if os.environ.get('AWAIT_CARD') else []
put(REPO + '/.git/autopilot/decisions/%s.json' % KEY, dict(schema='autopilot.decision/2', project_key=KEY, repo_identity=IDENT, decisions=[
    card('d-a1b2c3e1', '合併 plan', '要不要把 shared-tui plan 合進 develop？', None, [('合併', ''), ('再審一輪', ''), ('待辦', '')], '合併', None, 30*60),
    card('d-a1b2c3e2', '外掛更新時機', 'forge-stats 外掛今晚還是明早更新？', None, [('今晚', ''), ('明早', ''), ('待辦', '')], '明早', None, 20*60),
    card('d-a1b2c3e3', '預覽先拍幾欄', '預覽先拍 80 欄還是 120 欄？', '操作員平常用寬螢幕，外出用筆電只有 80 欄。', [('80 欄', ''), ('120 欄', ''), ('待辦', '')], '120 欄', None, 10*60),
    card('d-keys0004', '驗收測試卡', '這是驗收腳本的測試卡，送出後應進到卡片佇列', '假資料，只在沙盒卡片庫。這段背景故意寫得很長，用來檢查閱讀規則：放得下就要顯示全文，不能截成一行加刪節號；長文要自動換行，換行後對齊內文的起點，不回到最左邊；內文欄寬限制在七十到九十欄之間，不能一行拉滿整個終端機。選項之間要空一行，建議的選項要比其他選項明顯。', [('收到', ''), ('不收', '')], '收到', None, 5*60)] + LONGC + EXTRA))
# fleet card mirror (other hosts): 2 cards per fixture
def iso2(ago): return datetime.datetime.fromtimestamp(now - ago, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
put(D + '/mirror/fleet-card-mirror.json', dict(schema='autopilot.fleet-card-mirror/1', synced_at=iso2(60), last_error=None, closed={}, cards={
  'd-a1b2c3d0': dict(card_id='d-a1b2c3d0', question='清分支', title='清分支', namespace='lab', courier='claude', member='line-a', session=None, opened_at=iso2(1800)),
  'd-a1b2c3d1': dict(card_id='d-a1b2c3d1', question='換引擎', title='換引擎', namespace='lab', courier='claude', member='line-b', session=None, opened_at=iso2(1500))}))
put(D + '/mirror/fleet-card-todo.json', dict(items=[]))
