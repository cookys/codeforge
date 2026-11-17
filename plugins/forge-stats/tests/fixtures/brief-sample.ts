// Generated from brief-sample.json; the UX export driver checks equality.
export const BRIEF_SAMPLE = {
  "schema": "fleet.brief/1",
  "generated_at_ms": 1794200000000,
  "source": {
    "schema": "fleet.console/1",
    "generated_at_ms": 1794200000000
  },
  "error": null,
  "sources": {
    "tasks": {
      "at_ms": 1794200000000,
      "state": "ok",
      "error": null
    },
    "stuck": {
      "at_ms": 1794200000000,
      "state": "ok",
      "error": null
    },
    "peers": {
      "at_ms": 1794200000000,
      "state": "ok",
      "error": null
    },
    "hosts": {
      "at_ms": 1794200000000,
      "state": "ok",
      "error": null
    },
    "cards": {
      "at_ms": 1794200000000,
      "state": "ok",
      "error": null
    },
    "ctx": {
      "at_ms": 1794200000000,
      "state": "ok",
      "error": null
    }
  },
  "sources_down": [],
  "counts": {
    "needs_human": 3,
    "waiting_pm": 0,
    "stuck": 6,
    "offline": 2,
    "high_context": 0,
    "waiting": 9,
    "info": 1,
    "red": 8,
    "total": 21,
    "excluded_lanes": 3,
    "excluded_test_cards": 1,
    "excluded": 4
  },
  "items": [
    {
      "id": "d-a1b2c3d4",
      "kind": "needs_human",
      "source_kind": "card",
      "severity": 2,
      "level": "amber",
      "state": "need_you",
      "needs_operator": true,
      "subject": {
        "display": "bridge（host-b）"
      },
      "title": "正式站要不要先切到新版結帳流程？合進 release 分支後由 bridge 線直接切換上線，可以嗎？",
      "text": "決策卡等你回覆",
      "waiting_on": "你（操作員）",
      "since_ms": 1793768000000,
      "next_evidence_at_ms": null,
      "next_step_zh": "回覆這張決策卡",
      "line_id": "bridge--claude",
      "to_console": false,
      "ids": {
        "card_id": "d-a1b2c3d4"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": "bridge 在等你回覆 d-a1b2c3d4",
      "blocked_lanes": [
        "bridge（host-b）"
      ],
      "opener": "bridge（host-b）",
      "context": "舊流程每週掉單兩次，新流程已在測試站跑過三天",
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [],
      "options": [
        "可以",
        "先不要"
      ],
      "recommended": [
        "可以"
      ],
      "answerable_by": [],
      "status": "open"
    },
    {
      "id": "d-b2c3d4e5",
      "kind": "needs_human",
      "source_kind": "card",
      "severity": 2,
      "level": "amber",
      "state": "need_you",
      "needs_operator": true,
      "subject": {
        "display": "pm（host-a）"
      },
      "title": "下次到現場時，照附件量 24 項並回報數字，可以排在週五嗎？",
      "text": "決策卡等你回覆",
      "waiting_on": "你（操作員）",
      "since_ms": 1794142400000,
      "next_evidence_at_ms": null,
      "next_step_zh": "回覆這張決策卡",
      "line_id": null,
      "to_console": false,
      "ids": {
        "card_id": "d-b2c3d4e5"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": "pm（host-a）",
      "context": "週五只有半天可用，量完才能決定是否補貨",
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": "週五只有半天可用，量完才能決定是否補貨",
      "next_cmds": [],
      "options": [
        "好",
        "改期"
      ],
      "recommended": [
        "好"
      ],
      "answerable_by": [],
      "status": "open"
    },
    {
      "id": "d-d4e5f6a7",
      "kind": "needs_human",
      "source_kind": "card",
      "severity": 2,
      "level": "amber",
      "state": "need_you",
      "needs_operator": true,
      "subject": {
        "display": "pm（host-a）"
      },
      "title": "網頁總覽要不要另外發一把給你用的唯讀鑰匙，讓你在手機上看？",
      "text": "決策卡等你回覆",
      "waiting_on": "你（操作員）",
      "since_ms": 1794198500000,
      "next_evidence_at_ms": null,
      "next_step_zh": "回覆這張決策卡",
      "line_id": null,
      "to_console": false,
      "ids": {
        "card_id": "d-d4e5f6a7"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": "pm（host-a）",
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [],
      "options": [
        "要",
        "不要"
      ],
      "recommended": [
        "要"
      ],
      "answerable_by": [],
      "status": "open"
    },
    {
      "id": "stuck:delta--claude:d64a2061",
      "kind": "stuck",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "stalled",
      "needs_operator": false,
      "subject": {
        "display": "delta（host-c）"
      },
      "title": "換池規則：只重用該主機上已有的登入，不複製憑證，請 PM 裁示",
      "text": "任務逾時沒回報進度",
      "waiting_on": null,
      "since_ms": 1794182000000,
      "next_evidence_at_ms": null,
      "next_step_zh": "查看這條線的任務",
      "line_id": "delta--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000007"
      },
      "facts_zh": [
        "超過回報門檻 30 分，已 5 小時沒有回報"
      ],
      "suggest_zh": "建議：delta 的根因在 falcon（host-e）；先 fleet task show 01KA0000000000000000000003，再傳訊息給 falcon（host-e）問進度",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": "等待 iris 中（對方 5 小時前 只回報狀態）",
      "session_zh": "工作階段在線",
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "等待依賴",
      "root_lane": "falcon（host-e）",
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "stuck:cobalt--codex:6c54bf20",
      "kind": "stuck",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "stalled",
      "needs_operator": false,
      "subject": {
        "display": "cobalt（host-b）"
      },
      "title": "跑完 nightly 基準並把結果寫進報告",
      "text": "任務逾時沒回報進度",
      "waiting_on": null,
      "since_ms": 1794189200000,
      "next_evidence_at_ms": null,
      "next_step_zh": "向對方追問進度，或改派",
      "line_id": "cobalt--codex",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000002"
      },
      "facts_zh": [
        "超過回報門檻 30 分，已 3 小時沒有回報"
      ],
      "suggest_zh": "建議：cobalt 在等 ember（host-d）（任務 …0002）；fleet task show 01KA0000000000000000000002 看最後回報，再傳訊息給 ember（host-d）問進度",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": "等待 ember 中（對方最後進度 跑到第 4/10 組基準，3 小時前）",
      "session_zh": "工作階段在線",
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "等待依賴",
      "root_lane": "ember（host-d）",
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "stuck:ember--claude:d9142026",
      "kind": "stuck",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "stalled",
      "needs_operator": false,
      "subject": {
        "display": "ember（host-d）"
      },
      "title": "跑完 nightly 基準並把結果寫進報告",
      "text": "任務逾時沒回報進度",
      "waiting_on": null,
      "since_ms": 1794189200000,
      "next_evidence_at_ms": null,
      "next_step_zh": "向對方追問進度，或改派",
      "line_id": "ember--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000002"
      },
      "facts_zh": [
        "超過回報門檻 30 分，已 3 小時沒有回報"
      ],
      "suggest_zh": "建議：fleet task show 01KA0000000000000000000002 看最後回報，再傳訊息給 ember（host-d）問進度",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": "最後回報 3 小時前：跑到第 4/10 組基準",
      "session_zh": "工作階段在線",
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "進度待確認",
      "root_lane": "ember（host-d）",
      "affected_lanes": [
        "cobalt（host-b）（3 小時沒回報，任務 …0002）"
      ],
      "chain_lanes": [
        "cobalt",
        "ember"
      ],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "stuck:pm--claude:0b3a5dd1",
      "kind": "stuck",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "stalled",
      "needs_operator": false,
      "subject": {
        "display": "pm（host-a）"
      },
      "title": "整理這週的待辦並派工",
      "text": "任務逾時沒回報進度",
      "waiting_on": null,
      "since_ms": 1794192800000,
      "next_evidence_at_ms": null,
      "next_step_zh": "向對方追問進度，或改派",
      "line_id": "pm--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000003"
      },
      "facts_zh": [
        "超過回報門檻 30 分，已 2 小時沒有回報"
      ],
      "suggest_zh": "建議：pm 在等 falcon（host-e）（同一任務 …0003）；下游擋住 iris。先查根因 falcon（host-e）：fleet task show 01KA0000000000000000000003，再傳訊息給 falcon（host-e）",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": "等待 falcon 中（對方 2 小時前 只回報狀態）",
      "session_zh": "工作階段在線",
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "等待依賴",
      "root_lane": "falcon（host-e）",
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "stuck:falcon--claude:3893559f",
      "kind": "stuck",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "stalled",
      "needs_operator": false,
      "subject": {
        "display": "falcon（host-e）"
      },
      "title": "整理這週的待辦並派工",
      "text": "任務逾時沒回報進度",
      "waiting_on": null,
      "since_ms": 1794192800000,
      "next_evidence_at_ms": null,
      "next_step_zh": "向對方追問進度，或改派",
      "line_id": "falcon--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000003"
      },
      "facts_zh": [
        "超過回報門檻 30 分，已 2 小時沒有回報"
      ],
      "suggest_zh": "建議：fleet task show 01KA0000000000000000000003 看最後回報，再傳訊息給 falcon（host-e）問進度",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": "最後回報 2 小時前：只有狀態，沒有內容",
      "session_zh": "工作階段在線",
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "進度待確認",
      "root_lane": "falcon（host-e）",
      "affected_lanes": [
        "pm（host-a）（2 小時沒回報，任務 …0003）",
        "iris（host-h）（等待中）",
        "delta（host-c）（5 小時沒回報，任務 …0007）"
      ],
      "chain_lanes": [
        "delta",
        "iris",
        "pm",
        "falcon"
      ],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "stuck:kilo--claude:9673f05d",
      "kind": "stuck",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "stalled",
      "needs_operator": false,
      "subject": {
        "display": "kilo（host-d）"
      },
      "title": "[atlas 排程] 把舊版匯入器全部改成串流寫入，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，並且逐一核對每一筆欄位對應、時區換算與空值處理，",
      "text": "任務逾時沒回報進度",
      "waiting_on": null,
      "since_ms": 1794194600000,
      "next_evidence_at_ms": null,
      "next_step_zh": "向對方追問進度，或改派",
      "line_id": "kilo--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000011"
      },
      "facts_zh": [
        "超過回報門檻 30 分，已 1 小時沒有回報"
      ],
      "suggest_zh": "建議：fleet task show 01KA0000000000000000000011 看最後回報，再傳訊息給 kilo（host-d）問進度",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": "最後回報 1 小時前：核對時區欄位中，已處理 1200/5000 筆",
      "session_zh": "工作階段在線",
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "進度待確認",
      "root_lane": "kilo（host-d）",
      "affected_lanes": [],
      "chain_lanes": [
        "kilo"
      ],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "host:host-f",
      "kind": "offline",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "offline",
      "needs_operator": false,
      "subject": {
        "display": "主機 host-f"
      },
      "title": "整台主機",
      "text": "gamma 2 分鐘前仍有回報：疑似 courier 連線問題，不是關機",
      "waiting_on": null,
      "since_ms": 1794189200000,
      "next_evidence_at_ms": null,
      "next_step_zh": "建議：先查主機 host-f 的 courier 連線與網路；fleet doctor；先不用去查電源",
      "line_id": null,
      "to_console": false,
      "ids": {},
      "facts_zh": [],
      "suggest_zh": "",
      "members": [
        "gamma（host-f）"
      ],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": true,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet doctor"
      ]
    },
    {
      "id": "host:host-g",
      "kind": "offline",
      "source_kind": "exception",
      "severity": 1,
      "level": "red",
      "state": "offline",
      "needs_operator": false,
      "subject": {
        "display": "主機 host-g"
      },
      "title": "整台主機",
      "text": "主機離線",
      "waiting_on": null,
      "since_ms": 1794189200000,
      "next_evidence_at_ms": null,
      "next_step_zh": "建議：確認主機 host-g 是否開機／連網；fleet doctor",
      "line_id": null,
      "to_console": false,
      "ids": {},
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet doctor"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000703",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "（負責人未知）"
      },
      "title": "原任務已不在清單｜等待 未排期",
      "text": "原任務已不在清單",
      "waiting_on": null,
      "since_ms": 1794153200000,
      "next_evidence_at_ms": null,
      "next_step_zh": "查無此任務（殘留紀錄）：先 fleet task show 01KA0000000000000000000703 確認；仍查無 → fleet task cancel 01KA0000000000000000000703 清掉這筆等待",
      "line_id": null,
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000703"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000703",
        "fleet task cancel 01KA0000000000000000000703"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000702",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "（負責人未知）"
      },
      "title": "原任務已不在清單｜等待 未排期",
      "text": "原任務已不在清單",
      "waiting_on": null,
      "since_ms": 1794156800000,
      "next_evidence_at_ms": null,
      "next_step_zh": "查無此任務（殘留紀錄）：先 fleet task show 01KA0000000000000000000702 確認；仍查無 → fleet task cancel 01KA0000000000000000000702 清掉這筆等待",
      "line_id": null,
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000702"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000702",
        "fleet task cancel 01KA0000000000000000000702"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000701",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "（負責人未知）"
      },
      "title": "原任務已不在清單｜等待 未排期",
      "text": "原任務已不在清單",
      "waiting_on": null,
      "since_ms": 1794160400000,
      "next_evidence_at_ms": null,
      "next_step_zh": "查無此任務（殘留紀錄）：先 fleet task show 01KA0000000000000000000701 確認；仍查無 → fleet task cancel 01KA0000000000000000000701 清掉這筆等待",
      "line_id": null,
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000701"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000701",
        "fleet task cancel 01KA0000000000000000000701"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000010",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "iris（host-h）"
      },
      "title": "等一個過期的目標",
      "text": "等待對象寫錯",
      "waiting_on": null,
      "since_ms": 1794164000000,
      "next_evidence_at_ms": null,
      "next_step_zh": "先 fleet task show 01KA0000000000000000000010；等待目標寫錯 → 發起者 atlas（host-a） 用 fleet task waiting-on 01KA0000000000000000000010 none 清掉；不用做了 → fleet task cancel 01KA0000000000000000000010",
      "line_id": "iris--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000010"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000010",
        "fleet task waiting-on 01KA0000000000000000000010 none",
        "fleet task cancel 01KA0000000000000000000010"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000700",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "（負責人未知）"
      },
      "title": "原任務已不在清單｜等待 未排期",
      "text": "原任務已不在清單",
      "waiting_on": null,
      "since_ms": 1794164000000,
      "next_evidence_at_ms": null,
      "next_step_zh": "查無此任務（殘留紀錄）：先 fleet task show 01KA0000000000000000000700 確認；仍查無 → fleet task cancel 01KA0000000000000000000700 清掉這筆等待",
      "line_id": null,
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000700"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000700",
        "fleet task cancel 01KA0000000000000000000700"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000009",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "juno（host-h）"
      },
      "title": "等一個寫錯的目標",
      "text": "等待對象寫錯",
      "waiting_on": null,
      "since_ms": 1794167600000,
      "next_evidence_at_ms": null,
      "next_step_zh": "先 fleet task show 01KA0000000000000000000009；等待目標寫錯 → 發起者 atlas（host-a） 用 fleet task waiting-on 01KA0000000000000000000009 none 清掉；不用做了 → fleet task cancel 01KA0000000000000000000009",
      "line_id": "juno--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000009"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000009",
        "fleet task waiting-on 01KA0000000000000000000009 none",
        "fleet task cancel 01KA0000000000000000000009"
      ]
    },
    {
      "id": "patrol:wait_invalid:01KA0000000000000000000008",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "juno（host-h）"
      },
      "title": "等一個不存在的目標",
      "text": "等待對象寫錯",
      "waiting_on": null,
      "since_ms": 1794171200000,
      "next_evidence_at_ms": null,
      "next_step_zh": "先 fleet task show 01KA0000000000000000000008；等待目標寫錯 → 發起者 atlas（host-a） 用 fleet task waiting-on 01KA0000000000000000000008 none 清掉；不用做了 → fleet task cancel 01KA0000000000000000000008",
      "line_id": "juno--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000008"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet task show 01KA0000000000000000000008",
        "fleet task waiting-on 01KA0000000000000000000008 none",
        "fleet task cancel 01KA0000000000000000000008"
      ]
    },
    {
      "id": "patrol:operator_decision_unmatched:",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "iris（host-h）"
      },
      "title": "換池規則：只重用該主機上已有的登入，不複製憑證，請 PM 裁示",
      "text": "等 PM，但 PM 線沒在處理",
      "waiting_on": null,
      "since_ms": 1794182000000,
      "next_evidence_at_ms": null,
      "next_step_zh": "iris（host-h）（等待中）\n→ pm（host-a）（2 小時沒回報）\n→ falcon（host-e）（卡住）｜共用任務 …0003\n根因見上方 falcon（任務 …0003）",
      "line_id": "iris--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000007"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": true,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    },
    {
      "id": "patrol:operator_decision_unmatched:m-fx-0001",
      "kind": "waiting",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "（負責人未知）"
      },
      "title": "一則裁示請求沒有對應任務（訊息 m-fx-0001）",
      "text": "裁示請求沒人接",
      "waiting_on": null,
      "since_ms": 1794194600000,
      "next_evidence_at_ms": null,
      "next_step_zh": "先 fleet receipt m-fx-0001 看這則請求的下落；PM 線目前沒回報，先查 PM：fleet task show 01KA0000000000000000000003",
      "line_id": null,
      "to_console": false,
      "ids": {
        "message_id": "m-fx-0001"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": [
        "fleet receipt m-fx-0001",
        "fleet task show 01KA0000000000000000000003"
      ]
    },
    {
      "id": "patrol:wait_over:01KA0000000000000000000008",
      "kind": "info",
      "source_kind": "exception",
      "severity": 3,
      "level": "info",
      "state": "findings",
      "needs_operator": false,
      "subject": {
        "display": "juno（host-h）"
      },
      "title": "等一個不存在的目標",
      "text": "等待已結束，可以接著做",
      "waiting_on": null,
      "since_ms": 1794199400000,
      "next_evidence_at_ms": null,
      "next_step_zh": "通知等待方繼續",
      "line_id": "juno--claude",
      "to_console": false,
      "ids": {
        "task_id": "01KA0000000000000000000008"
      },
      "facts_zh": [],
      "suggest_zh": "",
      "members": [],
      "linked_zh": null,
      "blocked_lanes": [],
      "opener": null,
      "context": null,
      "last_report_zh": null,
      "session_zh": null,
      "pm_trouble": false,
      "blocked_confirmed": false,
      "evidence_tag": "",
      "root_lane": null,
      "affected_lanes": [],
      "chain_lanes": [],
      "courier_link_suspect": false,
      "recommend_reason_zh": null,
      "next_cmds": []
    }
  ],
  "missing_sources": []
} as const
