# Rubric — 2026-11-02-autopilot-hook-bridge.md

> Source plan: doc/plans/2026-11-02-autopilot-hook-bridge.md

R1: `codeforge hook on` 在任何情況（未知事件、stdin 非 JSON、子指令失敗或逾時、資料目錄不可寫）都 exit 0，自身錯誤只寫 stderr，絕不 exit 2。
R2: session-start 的 stdout 只能是空的，或恰好一個 `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":…}}`；session-end、pre-compact 的 stdout 一律為空。
R3: 事件名固定為 `session-start`、`session-end`、`pre-compact`；探測 `codeforge hook on --help` 回 0；既有 `codeforge hook <name>` 行為不變。
R4: `hook on` 及其子指令的 cwd 就是 hook 的 cwd；每個子指令收到原樣的 stdin bytes。
R5: SessionEnd 中 dream 完成後才跑 ship；同一 session 的 detached 工作不重疊執行。
R6: session-end、pre-compact 在 bridge 的 5s 內返回；session-start 在 bridge 的 15s 內返回；detached 工作在父行程與其 process group 收到 SIGHUP／SIGTERM 後仍跑完。
R7: 偵測不到 autopilot bridge 時，install／bootstrap 的行為與輸出與 0.0.7 byte 相同。
R8: 共用 settings.json 上，沒有 `--hooks-via autopilot` 就絕不清掉 codeforge hooks；單機檔偵測成立時清掉所有 codeforge 擁有的 hook 項目、不加回，非 codeforge hook 原樣保留。
R9: 不新增 crate 依賴。
R10: 不在作者的開發機上安裝 codeforge hooks；e2e 一律用 `CODEFORGE_CLAUDE_SETTINGS`、`CODEFORGE_DATA_DIR`、`CODEFORGE_CLAUDE_PLUGINS_FILE` 指向 scratch。
R11: 使用者看得到的訊息用正體中文；CJK 截斷用 `.chars().take(N)`；格式化只用 `./scripts/fmt.sh`。
