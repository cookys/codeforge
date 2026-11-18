#!/usr/bin/env python3
# fake fleet backend: all data invented (h1-h6, seat1.., pool names). scenario = first word of $GATE_DIR/scenario
import json, os, sys, time, random
GD = os.environ.get("GATE_DIR") or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
args = sys.argv[1:]
try: sc = open(GD + "/scenario").read().split()[0]
except Exception: sc = "full"
with open(GD + "/fleet.log", "a") as f: f.write(f"{time.strftime('%H:%M:%S')} scenario={sc} args={' '.join(args)}\n")
now = int(time.time() * 1000); MIN = 60000; H = 60 * MIN
CROCK = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
def ulid(ms, n):
    s = ""
    for _ in range(10): s = CROCK[ms % 32] + s; ms //= 32
    return s + "".join(CROCK[(n * 7 + i * 3) % 32] for i in range(16))

LINES = [("line-a","甲線","h1"),("line-b","乙線","h2"),("line-c","丙線","h3"),("line-d","丁線","h4"),("line-e","戊線","h5"),("line-f","己線","h6"),("line-g","庚線","h1"),("line-h","辛線","h2")]
def pool(name, w, f5, proj, reset_h, **o):
    d = dict(pool=name, seven_day_pct=w, five_hour_pct=f5, resets_at_ms=now + int(reset_h * H), burn_pct_per_h=1.0, burn_window_h=6, projected_pct_at_reset=proj,
             projected_exhaustion_at_ms=None, verdict=None, limit_hit=None, stale=None, stale_age_min=None, stale_note=None, observed_at_ms=now - 40000,
             stranded_seats=None, stranded_seat_list=None, switched=None, seats=2, projection_source="fleet")
    d.update(o); return d
POOLS12 = [pool("personal",100,100,100,10,limit_hit=True,seats=5), pool("team-a",68,17,130,40,projected_exhaustion_at_ms=now+17*H,seats=3), pool("team-b",55,30,88,45),
 pool("lab",22,4,43,60), pool("codex",80,12,98,30,seats=2), pool("grok",10,0,10,90,seats=1), pool("pool-c",31,0,47,20,stale=True,stale_age_min=180,stale_note="statusline 沒在跑",observed_at_ms=now-180*MIN,seats=1),
 pool("pool-d",49,0,94,22,stale_note="席 seat7 過期 14 分，舊值 55% 較高",stale_age_min=14), pool("pool-e",12,3,25,50), pool("pool-f",91,40,104,25,projected_exhaustion_at_ms=now+14*H),
 pool("pool-g",5,0,9,70,seats=1), pool("pool-h",74,60,99,35)]
POOLS3 = [POOLS12[1], POOLS12[3], POOLS12[5]]
SEATS = [dict(seat=f"seat{i}--claude", pool=POOLS12[i%5]["pool"], seven_day_pct=POOLS12[i%5]["seven_day_pct"], five_hour_pct=POOLS12[i%5]["five_hour_pct"], stale=(i==3), age_min=i) for i in range(1,7)]

def item(i, kind, state, title, text, line=None, sev=2, since_min=30, op=True, cc=True):
    return dict(id=i, kind=kind, severity=sev, state=state, needs_operator=op, title=title, text=text, facts={}, since_ms=now-since_min*MIN, line_id=line, link="/i/"+i, answerable_by=[], to_console=cc)

def console(sc):
    full = sc in ("full", "s5stale")
    n_stuck = 6 if full else 0
    lines = []; items = []
    for k, (lid, label, host) in enumerate(LINES if full else LINES[:3]):
        issues = []
        if full and k < 6:
            st = "failed" if k < 2 else "stalled"
            issues = [dict(rule="T4" if k < 2 else "T3", state=st, subject=label+"成員", since_ms=now-(20+k*40)*MIN, to_console=True)]
            items.append(item(f"exc:T{3+(k<2)}:{k:016x}", "exception", st, (label+"成員離線") if k < 2 else (label+f"閒置 {20+k*40} 分"), label+"成員 "+("離線" if k < 2 else "閒置")+f" {20+k*40} 分", lid, sev=2+k//2, since_min=20+k*40))
        lines.append(dict(line_id=lid, label=label, state="stalled" if issues else "running", lead=f"ns/c{k}/lead-{k}--claude", lead_display=f"lead-{lid[-1]} · Claude ({host})", issues=issues,
                          tasks=dict(running=2, waiting=1, open=0)))
    if full: items.append(item("quota:pool-f:weekly","quota","failed","F 池重置前用完","F 池週 91% 預估 104%，約 14 小時後用完",sev=1,since_min=45))
    cards = []
    if full:
        qs = [("d-fe000001","丙線：測試綠了，合併到 develop 嗎？","open",300),("d-fe000002","測試機第二槽插電","waiting_step",1500),("d-fe000003","乙線：F 池快用完，3 個評估改派本地？","awaiting_operator_action",12)]
    else:
        qs = [("d-fe000009","甲線：測試綠了，合併嗎？","open",20)]
    for cid, q, st, m in qs:
        cards.append(dict(id=cid, question=q, background="假資料", options=[dict(label="准", recommended=True)], status=st, opened_at_ms=now-m*MIN, executor_display=f"lead-x · Claude (h{1+m%6})"))
        items.append(item("card:"+cid,"card","need_you",q[:20],q,sev=1,since_min=m,cc=None))
    pools = POOLS12 if full else POOLS3
    age = 12 * MIN if sc == "s5stale" else 20000
    st = "stale" if sc == "s5stale" else "ok"
    srcs = {k: dict(at_ms=now-age, state=st) for k in ("tasks","stuck","peers","hosts","goals","cards","card_mirror","quota_now","quota_forecast","changes")}
    if sc == "s5stale":
        srcs["hosts"] = dict(at_ms=now-3*H, state="expired"); srcs["quota_forecast"] = dict(state="no_source")
    sev6 = 6 if full else 0
    return dict(schema="fleet.console/1", generated_at_ms=now-age, cursor=str(now), since=None, filter=dict(line=None,lead=None,host=None), sources=srcs,
      counts=dict(decisions=len(cards), exceptions=len([i for i in items if i["kind"]=="exception"]), quota_risk=2 if full else 0, tabs={},
        summary=dict(lines=dict(running=2 if full else 3, waiting=1 if full else 0, stalled=sev6), hosts=dict(online=4,total=6,gpu_busy=5,gpu_total=8) if full else dict(online=3,total=3,gpu_busy=1,gpu_total=4), hints=0, quota_worst="pool-f" if full else None)),
      brief="假資料 brief", items=items, cards=cards, lines=lines, quota=dict(pools=pools, seats=SEATS if full else SEATS[:2]), health=dict(), hosts=[])

def tasks(sc):
    full = sc in ("full","s5stale"); rows = []
    n = 14 if full else 4
    for i in range(n):
        host = f"h{1+i%6}"; created = now - (90 + i*17) * MIN
        state = ["running","running","waiting","open","blocked_human","suspect","running"][i % 7] if full else "running"
        rows.append(dict(task_id=ulid(created, i), assignee=f"ns/c{i%6}/{host}-lane{i}--claude", title=f"假任務 {i+1}：整理 seat{i%5+1} 的工作", progress=f"進行到第 {i%4+1} 步",
                         state=state, watch_ids=["w1"] if state=="waiting" else [], observed_at=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now/1000-(i%3)*3600)), parent_task_id=""))
    return rows

if args[:1] == ["console"]:
    if sc == "s5fail": sys.stderr.write("fleet: hub unreachable\n"); sys.exit(1)
    print(json.dumps(console(sc), ensure_ascii=False))
elif args[:1] == ["tasks"]:
    print(json.dumps(tasks(sc), ensure_ascii=False))
elif args[:1] == ["seats"]:
    print(json.dumps([dict(seat="seat1--claude", pool="team-a", five_hour_pct=17, seven_day_pct=68, stale=False, self=True), dict(seat="seat2--claude", pool="lab", five_hour_pct=4, seven_day_pct=22, stale=False, self=False)]))
else:
    sys.stderr.write("fake fleet: unknown subcommand: " + " ".join(args) + "\n"); sys.exit(1)
