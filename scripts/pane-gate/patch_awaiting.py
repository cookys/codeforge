#!/usr/bin/env python3
# patch_awaiting.py <cwd> [ignored] : add two answered cards that still wait on the operator's own step (sandbox store only).
import glob, json, os, sys, datetime
cwd = sys.argv[1]
ts = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
for p in glob.glob(os.path.join(cwd, '.git', 'autopilot', 'decisions', '*.json')):
    o = json.load(open(p, encoding='utf-8'))
    o['decisions'] = [d for d in o.get('decisions', []) if not str(d.get('id', '')).startswith('d-await')]
    o['decisions'] += [
        dict(id='d-await001', title='評測機第二槽插電', question='評測機第二槽可以插電上線嗎？', status='awaiting_operator_action', opened_at=ts, answer='A',
             operator_steps=['到機櫃把評測機第二槽的電源線插到備用插座（插座在機櫃背面左下角，標示 UPS-2 的那一排），插好後等前面板燈號由閃爍變成常亮，大約需要三十秒', '在操作台跑一次讀回指令，確認畫面顯示的 sha 和卡片上寫的一致', '回到這張卡按完成']),
        dict(id='d-await002', question='沒有標題的已答卡：備份硬碟要不要換新？', status='awaiting_operator_action', opened_at=ts, answer='B',
             operator_steps=['把舊硬碟拔下來貼上日期標籤']),
    ]
    json.dump(o, open(p, 'w', encoding='utf-8'), ensure_ascii=False)
