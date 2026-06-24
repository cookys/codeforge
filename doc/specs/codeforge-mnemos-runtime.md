# CodeForge ↔ Mnemos — Runtime / Operational Picture

> Status: **operational reference**（記「目前實際怎麼跑」，非設計 spec） · Owner: cookys · Created: 2026-06-24
>
> 為什麼有這份：ship/recall 的**設計**散在 [`codeforge-ship.md`](codeforge-ship.md)、[`codeforge-memory-contract.md`](codeforge-memory-contract.md) 與 CLAUDE.md 各段；但「現在這台跟腦到底怎麼連、哪條 live、哪條只是設計沒接」要翻 `~/.claude/settings.json` 的 hook + grep code 才拼得出來。這份把那個**運維現況**集中，免得每次重查。
>
> ⚠️ 這是**狀態快照**，會漂移。判斷真相的指令在 §5（自己跑、別信標籤）。機器專屬值（腦 IP / key / tunnel unit 名）**不寫死在這**（repo 共享、各 clone 不同）→ 看本機 `~/.config/mnemos.env` 註解 + `~/.config/systemd/user/mnemos-tunnel.service`。

---

## 1. 拓樸（central brain + fleet clients）

Mnemos 是中央腦，多來源收錄（Slack/LINE/Email/Docs/…）；CodeForge 是其中的 **coding 來源**。腦只開 **loopback（永不 0.0.0.0）**，所以非腦機（fleet client）一律走 **SSH forward-only 隧道**連，不直連。

```
  fleet client（如某台 dev 機）                    腦 host（cookys-openclaw）
  ┌──────────────────────────────┐               ┌──────────────────────────────┐
  │ codeforge CLI + .codeforge    │   ssh 隧道    │ mnemos-api.service :8845       │
  │ 各 project 自己的 L0/L1        │  ===========> │ loopback-only · /v1/ingest/…  │
  │ mnemos-tunnel.service(autossh)│ :1884x→:8845  │ /v1/whoami /health /context   │
  └──────────────────────────────┘               │ + 自身 corpus grind(NAS,另事)  │
                                                  └──────────────────────────────┘
```

- **同機 client**（codeforge 跑在腦 host 本機）：`MNEMOS_INGEST_URL=http://127.0.0.1:8845` 直連。
- **fleet client**（別台）：`ssh -fN -L 1884x:127.0.0.1:8845 mnemos-ship@<腦>` → `MNEMOS_INGEST_URL=http://127.0.0.1:1884x`（指本機 forwarded port）。
- 權威協定（連線、auth、/health 豁免、自檢）：`cookys/mnemos:docs/projects/fleet-ingest-rollout/HANDOVER-fleet-machine-checklist.md`。

## 2. WRITE 路徑（↑ 上行，本機 → 腦）

全域 hook 在 `~/.claude/settings.json`（由 `codeforge install --hooks`/`--all` 裝），**每個 project 的 SessionEnd 都跑**，CWD = 該 project root：

| 順序 | hook 指令 | 作用 | 依賴腦? |
|---|---|---|---|
| 1 | `codeforge dream --quiet` | 該 project `.codeforge` 的 L0 signals → L1 concepts（蒸餾） | ❌ 純本機，永遠跑 |
| 2 | `codeforge ship --no-hook` | L1 + git log + db metrics → Haiku digest → L2 ledger → POST | ✅ opt-in gate |

- **ship 的 opt-in gate**：`--no-hook` 模式只在 `~/.config/mnemos.env` 存在（或 `MNEMOS_INGEST_URL` 設了）才 POST；否則乾淨 no-op（不 POST、不寫死信）。所以**沒接腦的 codeforge-only 使用者**照樣 dream 蒸餾、ship 是 clean no-op。
- **POST 目標**：`MNEMOS_INGEST_URL` → `/v1/ingest/ledger`。fleet 機這個 URL 指隧道的 forwarded port。
- **retry**（Mnemos source-contract §9.1）：1s→5s→30s ×4；失敗寫 `~/.codeforge/ship-failed/<id>.json` 待下次 ship 重送。`--no-hook` 是 single-attempt（永不阻塞 SessionEnd）。
- **送什麼**（as-shipped，非設計全貌）：`build_lessons` 只讀 L1 + git；**不掃 session jsonl**（設計目標、code 未跟上，BACKLOG B18）。envelope：`source=codeforge_ledger`、`machine_id=<hostname>`。

## 3. READ 路徑（↓ 下行，腦 → 本機）

READ 鏡像 WRITE 的 local-always / central-opt-in 切分，但**目前 hook 只接了 local 那半**：

| | 指令 | 裝在哪 | 現況 |
|---|---|---|---|
| **本機 recall** | `codeforge memory context --hook` | 全域 SessionStart | ✅ **live** — 本機 active L1 排序成 lean index(~1.5K tok) 注入 `additionalContext` |
| **中央 recall** | `mnemos-cli context` | （未進 hook） | ⚠️ **未接** — 只能手動跑；跨源 atom（Slack/Email/…）不會自動回灌 |

> **關鍵不對稱**：接了腦的 fleet 機目前是 **WRITE-only**（ship 上行通），下行的中央 recall 還沒進 SessionStart hook。要雙向，把 `mnemos-cli context` 也加進全域 SessionStart。

## 4. 現況矩陣（live vs 設計未接）

| 能力 | 狀態 | 備註 |
|---|---|---|
| dream（L0→L1 本機蒸餾） | ✅ live | 全域 SessionEnd，每 project |
| ship（L2 ledger 上行） | ✅ live（接腦後） | opt-in + 隧道通才真送；否則 no-op |
| 本機 recall（SessionStart 注入 L1） | ✅ live | `memory context --hook` |
| 中央 recall（跨源 atom 回灌） | ⚠️ 未接 hook | `mnemos-cli context` 手動 |
| cite 回填（引用 atom → citation_count++） | ⚠️ 未自動 | 只有手動 `mnemos-cli cite-detect`；BACKLOG B18 |
| ship 掃 session jsonl | ❌ 未實作 | 設計目標；現只 L1+git+metrics |
| 腦端 auth | 現關閉 | tokenless POST；flip-prod 開 auth 後各 fleet 機補 `MNEMOS_TOKEN` |
| 央腦燈 readiness 即時化（whoami authed probe） | 🔜 B25 | trigger=flip-prod 開 auth |

## 5. 怎麼驗（別信標籤，自己跑）

```sh
# 全域 hook 真正裝了什麼（SessionStart/SessionEnd）
python3 -c "import json,os;d=json.load(open(os.path.expanduser('~/.claude/settings.json')));\
print(json.dumps(d.get('hooks',{}).get('SessionEnd',[]),ensure_ascii=False,indent=1))"

# fleet 機自檢（HANDOVER §3）
systemctl --user is-active mnemos-tunnel.service          # 隧道活著?（fleet 機才有此 unit）
curl -s -m5 -o /dev/null -w 'health=%{http_code}\n' "$MNEMOS_INGEST_URL/health"   # 期望 200
curl -s -m5 "$MNEMOS_INGEST_URL/v1/whoami"               # {auth,ok,version}；auth=disabled→tokenless
test -f ~/.config/mnemos.env && echo opt-in-OK           # 缺→ship 乾淨 no-op
CODEFORGE_DIR=$HOME/.codeforge codeforge ship --dry-run | head -20   # 驗 envelope(source/machine_id)

# 死信(ship 失敗堆積)?
ls ~/.codeforge/ship-failed/ 2>/dev/null || echo 'clean'
```

> `$MNEMOS_INGEST_URL` 從 `~/.config/mnemos.env` 來（fleet 機是隧道 port、同機是 :8845）。

## 6. 持久性

- **fleet 隧道**：`mnemos-tunnel.service`（autossh `-M 0` + SSH keepalive + `Restart=always`）+ user-linger → reboot/斷網/ssh 死都自動重連。
- **腦端**：`mnemos-api.service`（user unit + Linger）→ reboot 自啟。

## 7. 對應文件

- WRITE 設計（payload schema / as-shipped 修正）：[`codeforge-ship.md`](codeforge-ship.md)
- 共享 state 契約（L0/L1/L2 surface、producer/consumer）：[`codeforge-memory-contract.md`](codeforge-memory-contract.md)
- 腦端 ingest contract / atom schema（單一真實來源）：`cookys/mnemos:docs/specs/10-source-contract.md`
- fleet 連線自檢（權威）：`cookys/mnemos:docs/projects/fleet-ingest-rollout/HANDOVER-fleet-machine-checklist.md`
- 央腦燈 / whoami readiness：`doc/specs/codeforge-brain-indicators.md` + BACKLOG B25
