#!/usr/bin/env python3
"""Nine-tab console design plates, using the v2 drawing primitives."""
import importlib.util
import json
import os
import sys

spec = importlib.util.spec_from_file_location('v2', os.path.join(os.path.dirname(__file__), '..', 'v2', 'gen.py'))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
T, G = v.T, v.G
OUT = sys.argv[1] if len(sys.argv) > 1 else 'out'
TABS = v.TABS
states = [('N','正常'),('E','空白'),('M','多筆'),('L','長內容'),('X','缺源'),('S','過時')]
ids = ['1-N','1-E','2-N'] + [f'{i}-{s}' for i in range(1,9) for s,_ in states if f'{i}-{s}' not in ('1-N','1-E','2-N')] + ['9-N','9-L','2-L2','2-I','2-P','2-Pc','2-R','2-F','2-Fl','1-X2','5-X2','6-X2','7-X2','C1','C2']
assert len(ids) == 63
names = dict(states)
extra = {'L2':'長步驟','I':'輸入附註','P':'送出預覽','Pc':'取消預覽','R':'送出成功','F':'送出失敗','Fl':'填入建議','X2':'缺源降級'}
keys = {'2-N':'a','2-I':'s','2-P':'y','2-Pc':'x','2-L2':'↑↓','8-L':'↑↓','C1':'2','C2':'2'}
flow = []
for n, ident in enumerate(ids,1):
    tab = 7 if ident == 'C2' else 2 if ident == 'C1' else int(ident[0])
    state = ident.split('-')[1] if '-' in ident else ident
    title = names.get(state, extra.get(state, '跨頁保留' if ident == 'C1' else '艦隊摘要'))
    flow.append(dict(step=n,id=ident,tab=TABS[tab-1],state=state,title=title,key_to_next=keys.get(ident,'1–9'),next=n%len(ids)+1))

sources = [
('pm status','1-X','1-S'),('console','1-X2','1-S'),('live','1-X','1-S'),('decision list','2-X','2-S'),
('pm／console／fleetSummary','3-X','3-S'),('live.rows','3-X','3-S'),('live.review、qc、decisions','4-X','4-S'),
('seats --quota','5-X','5-S'),('額度池','5-X2','5-S'),('今日用量、24h 趨勢','5-X2','5-S'),('輸出速度 steps','5-X','5-S'),
('residue','6-X','6-S'),('load_source','6-X2','6-S'),('本機卡 decide','7-X','7-S'),('他機卡 others','7-X','7-S'),
('runs','7-X2','7-S'),('pm／console／fleetFeed','7-X2','7-S'),('agentsColumn','7-X',None),('fleet todo','8-X','8-S')]
coverage = [dict(zip(('來源','狀態','步號'),(src,status,step))) for src,x,s in sources for status,step in [('缺源',x),('過時',s)] if step]

indexes = [ ['要你處理 3','艦隊產線 12','本席','QC 審查','代理決策 4'], ['待決 3 張','合併 plan','外掛更新時機','預覽先拍幾欄','等你動手 1 張','時冶 PL02 定案'], ['執行中 甲線 codex 12分','執行中 乙線 claude 8分','失敗 丙線 codex 31分','完成 丁線 claude 40分'], ['程式碼審查','計畫審查','推送前檢查','我替你決定的事'], ['5h 額度 42%','7d 額度 18%','額度池 A 42%','今日用量 380k','輸出速度 41 tok/s'], ['要你處理 2','舊工作樹 console','自動清理 1','外掛載入 1'], ['執行中 codeforge','執行中 autopilot','失敗 hangar','已完成 docs','本機待決 3','他機待決 1'], ['11-30 合併計畫','12-02 外掛更新','12-05 核對報表'], ['狀態圖示','符號','按鍵'] ]
replies = ['待決 3 張，殘留 2 項需要處理。','要不要把共用計畫合進 develop？','甲線執行中，已過 12 分。','程式碼審查通過，QC 欠審需介入。','照目前速度約 2h10m 後撞頂。','請人工檢查舊工作樹後再清理。','codeforge 正在執行測試。','到期前要採用哪個合併方案？','按 1–9 切頁 · Esc 回輸入框']
details = [ ['艦隊 12 條產線，運作中 8，失敗 2，卡住 2。','本席 ctx 42%，session 240k tokens。','QC 欠審 1，代理決策 4 條。'], ['來源 PM，開卡 3 小時前。','前因：計畫已審查兩輪。','背景：兩家審查均通過。','卡住：等操作員決定合併時間。'], ['run_id run-2407，runner codex，model gpt。','role implementer，開始 09:20，尚未結束。','產線摘要：測試 8 項已通過 6 項。'], ['codex verdict pass，第 2 輪。','審查範圍 develop 到目前提交；受保護檔案 0。','QC 欠審，需操作員介入。'], ['5h 42%，重置 2h10m；7d 18%，重置 4d。','A 池 42%，燃燒率每小時 19%。','今日 380k tokens，24h 趨勢上升。','最近速度 41 tok/s。'], ['原因：舊工作樹仍有未合併的變更。','路徑 projects/console，分支 feature/console。','最後提交 74f24a9；外掛旗標 dirty。'], ['模型 opus，最近 run run-2407。','速度 41 tok/s，近 5 分鐘持續執行。','產線摘要：測試進行中，尚無失敗。'], ['到期日 11-30，既有備註：先確認審查結果。','退回原因：新增資料，尚無資料來源。'], ['狀態：執行中、完成、失敗、需要介入。','符號：▸ 焦點，★ 建議，✓ 已選，▲ 動手。','按鍵：數字切頁，Esc 回輸入框。'] ]

def content(f):
    ident, state = f['id'], f['state']; tab = TABS.index(f['tab']); ix = indexes[tab][:]; detail = details[tab][:]; reply = [replies[tab]]
    if state == 'E': ix = ['目前沒有條目']; reply = ['沒有要你處理的']; detail = ['目前沒有資料。']
    if state == 'M':
        ix = ([f'第 {i} 張 待決卡' for i in range(1,15)] if tab == 1 else [f'第 {i} 筆 {TABS[tab]}項目' for i in range(1,21 if tab == 3 else 13)])
        if tab == 1: ix = ['⋯ 上面還有 3 張'] + ix[3:8] + ['⋯ 下面還有 6 張']
        if tab == 0: detail = ['異常產線 12 條：甲線失敗，乙線卡住。'] + [f'產線 {i}：執行逾時' for i in range(1,13)]
    if state in ('X','X2','S'):
        affected = [src for src,x,s in sources if (s if state == 'S' else x) == ident]
        detail = [f'{src}：'+('資料過時，顯示快取。' if state == 'S' else '讀不到，已降級。') for src in affected] + detail
        if ident == '1-X': detail.insert(1,'本席 no_live；pm status 退回 console。')
        if ident == '1-X2': detail.insert(0,'pm 和 console 讀不到，退回 fleetSummary。')
        if ident == '5-X': detail.append('本 session 還沒有請求。')
        if ident == '7-X': detail.append('他機卡尚未接線；本機卡讀不到。')
        if ident == '7-S': detail.append('他機卡過時（接線後 fixture）。')
        if ident == '8-X': reply = ['待辦尚未接線。']
    if state in ('L','L2'):
        detail = [f'時間線 {i}：審查與執行紀錄，等待後續結果。' for i in range(1,17)] + detail
    if tab in (1,7) and state not in ('E','X'):
        reply += ['a: 合併，今天開始執行','b: 再審一輪，明天決定','c: 待辦，到期前再處理']
        if tab == 1: reply += ['送出後不能改']
    if ident == '2-L2': reply = ['等你動手：核對 staging 的設定','1. 開啟 staging 頁面','2. 核對各項設定','3. 記錄差異並回報','4. 確認結果後選擇','a: 已完成','b: 有差異','c: 待辦','送出後不能改']
    if ident == '8-L': reply = ['這次合併涉及多個產線，請先確認審查和部署時程，再決定採用哪個方案？','a: 現在合併，並由值班人員觀察結果','b: 等明天完成額外審查後再合併','c: 延後到下週，保留目前版本','還有選項沒看到']
    specials = {'2-I':'附註輸入中：請確認部署時間','2-P':'預覽：合併 plan a；外掛更新 b。送出後不能改','2-Pc':'已取消預覽，選擇與附註保留。','2-R':'送出成功：2 張已記錄。','2-F':'送出失敗：連線逾時，選擇仍保留。','2-Fl':'已填入建議，原先選擇保留；已選 3 張。','2-S':'卡片在預覽期間改變，確認被拒；請重新預覽。','C1':'已選 a；附註：請確認部署時間。','C2':'艦隊摘要：codeforge 執行中，41 tok/s。'}
    if ident in specials: reply.insert(0,specials[ident])
    return ix, reply, detail

def compose(W,H,f):
    ix, reply, detail = content(f); tab = TABS.index(f['tab'])+1
    long = f['state'] in ('L','L2')
    hints = [('p/n','換張') if tab == 2 else ('j/k','換筆')]
    if tab in (2,8) and f['state'] not in ('E','X'): hints += [('a/b/c','選')]
    if tab == 2: hints += [('m','附註'),('f','填建議'),('s','送出')]
    if f['id'] == '2-P': hints += [('y','確認送出'),('x','取消')]
    if f['id'] == '2-L2': hints += [('g','複製網址')]
    if long: hints += [('↑↓','捲'),('PgUp/PgDn','捲來龍去脈')]
    hints += [('Esc','回輸入框')]
    hint = []
    for k,d in hints:
        if hint: hint.append(G(' · '))
        hint += [T(k,'merged'),T(' '+d,'inactive')]
    narrow = W < 110
    count = min(max(len(reply), 6), 7)
    visible = reply[:count]
    if len(reply)>count: visible[-1] = f'⋯ 下面還有 {len(reply)-count+1} 列'
    if narrow:
        band = [v.frame_top([T(f'{tab} {f["tab"]} · {f["title"]}')],W),v.frame_row([T('▸ '+ix[0])],W)]
        band += [v.frame_row([T(line)],W) for line in visible]
        band += [v.frame_bottom(hint,W)]
    else:
        lw=37; rw=W-lw; tail=[G(' '),T('[-]','inactive'),G(' ─╮')]
        band=[v.hline('╭',[T(f'{tab} {f["tab"]} · 清單')],lw)+v.hline('┬',[T(f['title'])],rw-v.rowlen(tail))+tail]
        for i in range(max(count,min(len(ix),7))):
            left = ('▸ ' if i == 0 else '  ')+ix[i] if i<len(ix) else ''
            right = visible[i] if i<len(visible) else ''
            # The option segment begins at the same column as in v2 for the R6 probe.
            rr = [T('    '),T(right[:2],'merged'),T(right[2:])] if right.startswith(('a:','b:','c:')) else [T(right)]
            band.append([G('│ ')]+v.pad(v.fit([T(left)],34),34)+[G(' │ ')]+v.pad(v.fit(rr,W-41),W-41)+[G(' │')])
        band.append(v.hline('╰',[],lw)+v.hline('┴',hint,rw-1)+[G('╯')])
    st = dict(mode='card',pane='ctx',focus=0,ans={})
    status = v.status_line(W,st)
    bottom = band+[status]+v.prompt(W)
    top_h = H-len(bottom); pw = W if narrow else (78 if W>=200 else 52)
    cap = max(1,top_h-(5 if narrow else 4))
    if len(detail)>cap: detail = [f'⋯ 上面還有 {len(detail)-cap+1} 列']+detail[-(cap-1):] if cap>1 else [f'⋯ 上面還有 {len(detail)} 列']
    pane = [v.frame_top([T(f'{tab} {f["tab"]} · 來龍去脈')],pw,ctrl='')]+[v.frame_row([T(x)],pw) for x in detail]+[v.frame_bottom([T('目前焦點的明細','inactive')],pw)]
    if narrow:
        ch = top_h-len(pane); rows = [v.pad(v.fit(x,W),W) for x in ([[]]+v.CONV*6)[:ch]]+pane
    else:
        cw=W-pw-1; off=top_h-len(pane); conv=([[]]+v.CONV*6)[:top_h]
        rows=[v.pad(v.fit(conv[i],cw),cw)+[T(' ')]+(pane[i-off] if i>=off else [T(' '*pw)]) for i in range(top_h)]
    nxt=flow[f['next']-1]['id']; caption=f'設計稿註記 · {f["id"]} {f["tab"]} · {f["title"]} · 按 {f["key_to_next"]} → {nxt}'
    rows[0]=v.pad(v.fit([T(caption,'inactive')],W),W)
    rows += bottom
    assert len(rows)==H and all(v.rowlen(r)==W for r in rows), (W,H,f['id'])
    return rows

def main():
    os.makedirs(OUT,exist_ok=True)
    for W,H in ((209,49),(120,40),(80,24)):
        for f in flow:
            rows=compose(W,H,f); d=os.path.join(OUT,f'{W}x{H}-s{f["step"]}'); os.makedirs(d,exist_ok=True)
            with open(os.path.join(d,'screen.txt'),'w') as o: o.write('\n'.join(''.join(s for s,_,_ in r) for r in rows)+'\n')
            with open(os.path.join(d,'roles.json'),'w') as o: json.dump(rows,o,ensure_ascii=False)
            for theme in ('dark','light'):
                with open(os.path.join(d,theme+'.ans'),'w') as o: o.write(v.ans(rows,theme))
    for name,data in [('flow',flow),('coverage',coverage)]:
        with open(os.path.join(OUT,name+'.json'),'w') as o: json.dump(data,o,ensure_ascii=False,indent=2)
    print('ok',len(flow),'steps x 3 sizes')
if __name__ == '__main__': main()
