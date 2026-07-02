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

READ 鏡像 WRITE 的 local-always / central-opt-in 切分。**per-host 狀態不同**（下表「現況」欄分本機 = 腦 host vs 其他 fleet 機）：

| | 指令 | 裝在哪 | 現況 |
|---|---|---|---|
| **本機 recall** | `codeforge memory context --hook` | 全域 SessionStart | ✅ **live** — 本機 active L1 排序成 lean index(~1.5K tok) 注入 `additionalContext` |
| **中央 recall** | `mnemos-cli context --hook` | 全域 SessionStart（**`install --hooks` 已寫**，P1.2） | ✅ **live（本機）+ 下行機制已備（fleet）** — `codeforge install --hooks` 現在把 `codeforge mnemos-cli context --hook --max 5 --with-themes --max-sensitivity work` 寫進全域 SessionStart。`--hook` 自我 gate on opt-in（未 opt-in→乾淨 no-op；空/腦不可達→不注入空區塊），故每台機都安全。跨源 atom（Slack/Email/ledger…）注入 stdout。本機原手動寫的無 marker 版,再跑 install 會被 legacy-sweep 收掉,不 dual-fire。 |

> **對稱補完（per-host，2026-07-02）**：**本機（腦 host）已雙向** —— ship 上行 + 中央 recall 下行都 live。**fleet 機下行機制已備**：`install --hooks`（含 `codeforge bootstrap`）現在會寫中央 recall SessionStart 行,`--hook` 自我 gate,已接腦（tunnel + `~/.config/mnemos.env` opt-in）的 fleet 機**下次跑 `codeforge bootstrap` / `install --hooks` 即取得下行**,從 WRITE-only 變雙向。**遠端部署**:CLI 只能本機跑（無法遠端代裝）→ 各 fleet 機逐台 `git pull && codeforge bootstrap`（沿用 BACKLOG B14 runbook）。本次只在本機驗（loopback :8845 + `--hook` opt-in/no-op/dry-run 三態）；實機 fleet rollout 列 follow-up。

## 4. 現況矩陣（live vs 設計未接）

| 能力 | 狀態 | 備註 |
|---|---|---|
| dream（L0→L1 本機蒸餾） | ✅ live | 全域 SessionEnd，每 project |
| ship（L2 ledger 上行） | ✅ live（接腦後） | opt-in + 隧道通才真送；否則 no-op |
| 本機 recall（SessionStart 注入 L1） | ✅ live | `memory context --hook` |
| 中央 recall（跨源 atom 回灌） | ✅ live（本機）/ ✅ 下行機制已備（fleet，P1.2） | `install --hooks` 現寫 `mnemos-cli context --hook`（自我 gate on opt-in）；fleet 機下次 `bootstrap` 即下行。實機 rollout = follow-up |
| cite 回填（引用 atom → citation_count++） | ✅ auto-cite-on-ship（B18 已接，2026-07-02） | `ship` 結束掃當日本 repo session transcript，比對 atom 標題自動 cite（`src/mnemos/autocite.rs`，confidence 0.5 + `session_jsonl` provenance）；手動 `mnemos-cli cite-detect` 仍在。**部署**：需裝含此的新 binary（跑舊 binary 的機器 cite 仍不自動）。 |
| ship 掃 session jsonl（digest source_evidence） | ❌ 未實作 | ship **digest** 仍只讀 L1+git（source_evidence 不帶 jsonl locator）；B18 auto-cite 另讀 jsonl 僅供 cite 偵測，非 digest 證據。此列指 digest 端，仍為設計目標 |
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
