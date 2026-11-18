# 公開 repo 歷史改寫（操作員卡 d-64b40a9b，PM 01M7R28E）

- 決定：操作員在面板上親自選了「改寫歷史 — 清掉歷史後強制推送，維持公開」，答覆 id 01M7R21N76855MSXFQSQAS71W0。屬最高一級破壞性操作，由操作員本人決定。
- 執行：codeforge lane。強制推送前要再拿一次 PM 的 go，PM 要先看過對照表計數、check-all、防護掃描和 fork 數。

## 替換清單（只列類別和數量，原值只存在執行者本機 0600 的規則檔）

| 類別 | 不同值 | 改寫前出現次數（blob 加提交訊息） | 換成 |
|---|---|---|---|
| 主機名和裝置名 | 8 | 1571 | 角色名：`host-console`、`gpu-host-1..3`、`host-mini`、`host-infra`、`host-recovery` |
| 公司名 | 1 | 12 | `company-b` |
| 公司網域 | 2 | 200 | `company-a.example`、`company-b.example` |
| 家目錄使用者（`/home/<u>`、`-home-<u>`） | 2 | 592 | `user` |
| 內文 email | 4 | 229 | 操作員的 GitHub noreply 地址；測試假地址改用 `.invalid` |
| 作者和提交者 email（mailmap） | 4 | 864 個作者／提交者欄位 | 操作員的 GitHub noreply 地址 |

保留不換的：
- `noreply@openai.com`、`noreply@google.com`：機器人共同作者的公開通用地址，防護腳本的允許清單會加上這兩個。
- `cookys`：GitHub 的擁有者名稱，本來就公開。

規則：一律用 `git filter-repo`，同一份規則同時套在 `--replace-text`、`--replace-message` 和 `--mailmap`。比對時不分大小寫，而且要求前後都不是英數字，避免誤改其他單字。

## 影響範圍

- origin：17 個分支、13 個 tag（其中 12 個是 annotated），共 30 個 ref，所有 sha 全部換新。沒有 PR，也沒有 PR ref；fork 0、star 0、watcher 0（匿名 API，2026-11-19）。
- 只推送 origin 上原本就有的 30 個 ref 的改寫版。本機 develop 上還沒公開的 commit 不會跟著推，那要另外問 PM（01M7QPJP）。
- 本機：主 clone 和它的 worktree、14 個帳號上的建置目錄和開發 clone，都要照新 sha 重建，清單由 hangar 盤點。
- 文件裡寫死的舊 sha 會失效，包括推版紀錄和 fleet 訊息裡的 sha。提交訊息裡的短 sha 由 filter-repo 自動換成新的；文件內文裡的不會換。
- 收不回的部分：GitHub 快取裡舊 commit 的 sha 可能還連得到，要清掉得由操作員向 GitHub Support 申請（申請內文和 sha 清單另交）。

## 驗證（強制推送前都要過）

1. 每個 ref 的 tree：把規則套在舊檔案上，結果必須和新檔案逐位元組相同（10568 個檔案，0 處不一致）。
2. 防護掃描：用 `scripts/private-denylist.sha256` 掃 origin 改寫後的全部物件，主機、網域、家目錄都是 0；email 只剩上面那兩個機器人地址。
3. 改寫後的 develop 跑 `./scripts/check-all.sh`，rc=0。
4. 本機和 origin 共同的 commit 改寫後 sha 相同（main、v0.0.9 都一致），確認改寫是確定性的。
5. 推送前一刻再跑 `git ls-remote origin`，和備份裡的 `origin-refs.txt` 比對；有任何不同就停。

## 回滾

- 備份：`git clone --mirror` 加 `git bundle --all`，各兩份，一份在 team 帳號的 `~/backup/codeforge-rewrite-20261118T1931Z/`，一份由 hangar 放在 NAS 上同名目錄。SHA256SUMS 的 sha16 是 8098e9eb，讀回驗證通過，還原測試 30 個 ref 完全一致。
- 還原 origin：`git -C <備份>/origin-mirror.git push --force --mirror git@github-codeforge:cookys/codeforge.git`。這只能還原 origin 的 ref，已經被看過或快取的內容收不回。
- 還原本機：`git clone --mirror <備份>/local-all.bundle`，再把需要的 ref fetch 回主 clone。

## 結果（2026-11-19）

- 強制推送：PM go 01M7R2SC。推送前 ls-remote 和備份 30/30 一致；推送後和改寫版 30/30 一致，新 `main` 7f17389、`develop` 98701ef。
- 匿名驗證：3 個舊 commit 抽查，`branches-where-head` 都是空的，和新 `main` 沒有共同祖先；commit 搜尋找舊 email（4 個）和主機名、網域（11 個）都是 0。舊 commit 頁面在 GitHub 跑 GC 前仍打得開，清除申請由操作員送出（卡 d-022fdbeb，送 GitHub 快取清除申請）。
- 本機主 clone 用同一份規則原地改寫，結果和離線改寫的 sha 相同（develop、main 都一致），17 個 worktree 已同步；check-all 綠。改寫前的主 clone 備份：`main-clone-pre-inplace-20261118T1946Z.bundle`（sha16 4316c6ce），和上面的備份放在同一目錄。
- 防護腳本：`noreply@openai.com`、`noreply@google.com` 加進允許的地址；`doc/BACKLOG.md` 已不再命中，移除它的允許額度。
