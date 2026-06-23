# Deterministic Gate Patterns — CodeForge

<!-- last-verified: 2026-06-23 -->

寫 grep/awk 風格「確定性 gate」script（`scripts/check-*.{sh,py}` + CI job）的非顯而易見陷阱。
適用整個 gate 家族：`check-doc-drift.py`、`check-cjk-safe.sh`。核心哲學見
auto-memory `reference_doc_drift_system`（兩層模型：確定性 gate = 停止條件，LLM sweep = 探勘）。

## 比對前不要 strip inline comment（會對「正要防的 class」漏抓）

**Date**: 2026-06-23 | **Context**: B9 `check-cjk-safe.sh`，1 輪 reviewer 抓到
**Problem**: 為了避免註解內的 pattern 誤報，naive 取「第一個 `//` 之前」當 code 來比對。
但字串字面量內也可能有 `//`（URL、path、regex），例如
`let u = "http://h"; let _ = &s[..4];` —— 取第一個 `//` 會把後面真正要抓的 `&s[..4]`
截掉 → 對 gate 存在的「正是要防的 class」產生 **false negative**（最糟的失敗：gate 綠燈但漏網）。
**Solution**: 比對**原始整行**，不 strip inline comment。只 skip **純註解行**
（`sub(/^[ \t]+/,"",trimmed); trimmed ~ /^\/\//`）—— 這已覆蓋 doc-comment 提及 pattern 的誤報。
代價：行尾 inline comment 若「提及」pattern 會誤報（罕見）→ 用 allow-marker 消音。
對 gate 而言「誤報 > 漏報目標 class」是正確取捨。

## allow-marker / suppression 子字串必須加錨點

**Date**: 2026-06-23 | **Context**: 同上
**Problem**: escape hatch 用無錨點子字串比對（`index(line,"cjk-ok")>0`），
則無關字詞如 `cjk-okay`、ticket slug、URL 含該子字串 → 整行檢查被誤放行（false negative）。
**Solution**: marker 加錨點 —— 用帶冒號的文件化形 `cjk-ok:`（`cjk-okay` 不含 `cjk-ok:`）。
比照 clippy `#[allow]` 的明確形。header 要寫明冒號是必須的。

## CI gate 精準優先於召回（會誤報的 gate 會被停用）

**Date**: 2026-06-23 | **Context**: 同上 + `check-doc-drift.py` 設計一致
**Problem**: 想「抓好抓滿」會讓 gate 對安全 code 誤報 —— 例如 `[N..]` / `[N..M]` /
變數邊界 `[..n]` 在 string 上危險，但被安全 Vec 慣用法（`v[1..]`、`&buf[..len]`）主導，
無型別資訊無法分辨。誤報多的 CI gate 最終會被 `|| true` / 停用，等於沒有 gate。
**Solution**: 只抓高訊號、零誤報的形（cjk-safe 只抓 start-anchored literal `[..N]`/`[..=N]`，
即文件化的 `&s[..N]`）。難辨形**文件化為 known gap**（header 寫清楚 + 仍靠 convention/review），
不要為了召回犧牲精準。零變異才能當「停止條件」。
**Related**: auto-memory `reference_doc_drift_system`（loop-to-zero 不收斂，可靠性靠確定性檢查）
