Engine: opus
Role: TUI UX designer and implementer
Lane: codeforge-ux-v4

# 任務：從 UX 角度重新設計 band 上的整合面板（以 L2 lazygit 式為骨架）

所有說明和文件都用正體中文（台灣用語）。程式碼和識別字保持原文。

## 背景
- 操作員要一個鍵盤優先的面板，畫在 Claude Code 輸入框正上方那整塊全寬區域（AbovePrompt band）。面板用來處理 autopilot 的「待決」卡片（要他回答的問題），也有圓桌、用量、寵物、更多等分頁。
- 上一輪交了三種版面，操作員選了 L2（左側卡片清單、右側內容）當方向。截圖在本資料夾的上一層：../band-v3-L2-*.png。L1、L3 是 ../band-v3-L1-*.png 和 ../band-v3-L3-*.png。
- 操作員原話：「這個版本很混亂，做個 UX 好又 fency 的？不會的話就網路上找一下？」
- 所以這次不是在 L2 上修修補補，而是以「左側清單、右側內容」為骨架，從 UX 角度重新設計。

## 要處理的設計問題
1. 資訊層級：現在是哪一張、還剩幾張、每張選了什麼，要一眼看得到。
2. 視覺重量和留白：現在的版本太亂。
3. 顏色系統：深色和淺色終端都要能用。只用 theme 的文字色 key，不用底色、不反白（反白留給 engine 的焦點環）。規範在 <codeforge repo>/doc/specs/tui-guideline.md §3（狀態符號與顏色）、§5（淺色／深色）、§10（不能只靠顏色）。
4. 焦點和目前選項的強調方式：engine 會把焦點所在的 Button 反白，這點 mod 改不了，要把它當成設計的一部分。
5. 快捷列的寫法：要短，只列現在能用的鍵，再加一個「?」展開完整說明。
6. 連續答 8 張時的節奏感：打一個數字選好，畫面自動換到下一張；位置不跳，視線不用移。

## 硬限制（都在真機上量過，不能違反）
- 畫在 AbovePrompt band。`ui.render` hook 的 matcher 是 `{ component: 'AbovePrompt' }`。尺寸從 `e.props.bodyColumns` 和 `e.props.maxRows` 讀。
- band 最多佔半個畫面，含輸入框。超過 maxRows 的部分會被 engine 捲走，所以要自己控制總高度；建議 min(maxRows, 16)。
- 操作區（快捷列、選項）固定在最下面，貼著輸入框；資訊區貼底往上長。
- 鍵盤：
  - band 要靠使用者按 ctrl+x tab 才拿得到鍵盤；Esc 還給輸入框。
  - 輸入框空著時，直接打數字會按下 band 上帶那個數字 hotkey 的 Button，不必拿鍵盤。所以答題用的動作一律綁數字：選項 1–6、0 送出。
  - 字母 hotkey 要先拿鍵盤才有效。
  - Button 的 hotkey 只能是一個數字或一個小寫字母；兩個 Button 綁同一個 hotkey 時，後畫的那個生效。
- 焦點：
  - 重畫後焦點跟著 Button 的 key 走；
  - 焦點所在的 Button 消失時，焦點會跳到第一個 Button；
  - autoFocus 只在 band 剛拿到鍵盤時生效。
  - 所以選項 Button 的 key 一律用 opt:1、opt:2…，不帶卡片 id。
- Button 用 `plain`。帶 hotkey 的 plain Button，engine 會在前面畫「1: 」；這個前綴 mod 拿不掉，設計時要算進去。
- 寬度：band 右邊 5 格是 engine 的 [-]，可用寬度是 bodyColumns，再保守留 1 格。中文字佔 2 格。
- 要做兩種寬度：120 欄和 80 欄；80 欄時左欄要縮。

## 步驟和產出（全部放在 <codeforge repo>/ui-preview/codeforge/ux-v4/）

1. 參考（references.md，圖片放 refs/）。至少涵蓋 lazygit、gitui、k9s、btop、Charm 系（soft-serve、gum、glow）、Textual 範例和 posting、harlequin。
   - 每個參考寫：網址、讀取日期（UTC）、一張截圖（從官方 README 或文件下載的圖，存成 refs/<name>.png 或 .gif，並寫下原圖網址），或逐字摘錄；
   - 再寫一行「借了什麼」。
   - 抓不到的照實寫。不要編造。
   - 另外兩份已經整理好的版面規範來源可以引用：../tui-sources.md、../sources-codex.md、../sources-claude.md。
2. 設計說明（design.md）：先寫共同原則（資訊層級、顏色對照表、焦點、快捷列、節奏），再寫 2–3 個變體。每個變體三行：借了什麼、好在哪、代價。最後寫一段：目前的 L2 當對照組，它的問題在哪。
3. 預覽 mod：放在 <舊 session scratchpad>/tui-v4/，結構照上一輪的 ../tui-layout（同一個 scratchpad 下）。
   - 斜線命令：/ux <variant> <screen>，variant 是 L2（對照）、V1、V2、V3，screen 是 menu、answering、after；/ux off 關閉。
   - 用假資料 8 張卡，可以沿用 tui-layout 的 CARDS。
   - 按 1–6 選選項、0 送出，要真的能動：選完自動換到下一張。
   - 寫完跑 `claude plugin validate <mod 資料夾>`，必須通過。
   - 參考：scratchpad/tui-layout/hooks/register.tsx；型別在 <codeforge repo>/plugins/forge-stats/.claude-plugin/types/claude-code/index.d.ts，要找什麼就 grep。
4. 真機截圖：照 scratchpad/tui-layout-cap.py 的做法。
   - 每個變體加 L2 對照：menu、answering、after 用 120x44 拍；narrow 用 80x44 拍，拍 answering 畫面。
   - 拍法：在 tmux 打 /ux 指令，按 ctrl+x tab 讓焦點顯示出來，再截圖。
   - 轉圖：用 <codeforge repo>/scripts/ansi2html.py 轉 HTML，再跑 scratchpad/cjkfix.py 修正中文字寬，最後用 chrome-headless-shell 截圖。chrome 在 ~/.cache/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-linux64/chrome-headless-shell，參數 `--no-sandbox --hide-scrollbars --window-size=1100,840 --screenshot=<png> file://<html>`；80 欄用 `--window-size=760,840`。
   - 圖片存成 ux-v4/<variant>-<screen>.png。
   - 每拍完一輪都要自己看圖，確認沒有折行、沒有錯位、沒有超過寬度，有問題就修 mod 重拍。
5. 比較網頁 ux-v4/index.html：單一 HTML 檔，圖片用相對路徑引用同目錄的檔案。
   - 一列一個畫面（主選單／答題中／送出後／80 欄），變體並排，L2 對照放最左邊。
   - 每張圖上方用大字標出變體名和畫面名。
   - 每個變體欄位最上面放三行說明：借了什麼、好在哪、代價。
   - 要支援深色和淺色背景；寬螢幕並排，窄螢幕改成上下排。
   - 不要放任何真實的金額、主機名稱或個人資料。

## 不能做的事
- 不改 git repo 的任何檔案；ui-preview/ 已經排除在 git 之外，可以寫。
- 不寫 ~/.claude，也不讀 ~/.claude/settings.json 或任何憑證檔。
- 截圖用的 claude session 只在 ~/projects/codeforge 開，一定要帶 `--strict-mcp-config --mcp-config '{"mcpServers":{}}'`，拍完 kill 掉 tmux session。
- 不要在 shell 裡寫 `grep -r /`。
- zsh 下，含 glob 的路徑要加引號。

## 回報
最後只回四件事：
- ux-v4/ 的檔案清單；
- 每個變體一句話；
- 截圖有沒有已知問題；
- 哪些參考抓不到。
