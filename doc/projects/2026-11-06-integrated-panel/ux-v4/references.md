# 參考：鍵盤優先的終端介面（ux-v4）

讀取日期：全部是 2026-11-06（UTC）。
圖片都從官方 README 或官方 repo 下載，存在 `refs/`。動畫 GIF 原檔照存；文中「看到的畫面」是我用 headless Chrome 的 `ImageDecoder` 解出某一格來看的，會註明是第幾格。
逐字摘錄都從原始 README（`raw.githubusercontent.com`）的文字複製。

另外三份已整理好的版面規範來源，本文直接引用、不重抓：
- `../tui-sources.md`：兩份調查的對照與逐字抽查。
- `../sources-claude.md`：Textual Footer／Header、Bubbles help、k9s、nano、mc 等 18 個來源。
- `../sources-codex.md`：htop FunctionBar.c、k9s app.go、CUA、ncurses 等 18 個來源。

---

## 1. lazygit

- 網址：https://github.com/jesseduffield/lazygit （README：https://raw.githubusercontent.com/jesseduffield/lazygit/master/README.md）
- 圖：`refs/lazygit.gif`，原圖網址 https://github.com/jesseduffield/lazygit/raw/assets/demo/commit_and_push-compressed.gif
  - 注意：README 現在嵌的是影片（`<video src="https://github.com/user-attachments/assets/031799d1-740d-4cda-90cf-689513fa9860">`），不是這張 GIF。這張 GIF 來自 lazygit repo 的 `assets` 分支，是同一段 commit_and_push 示範的舊版素材。本機沒有 ffmpeg，影片抽不出畫面，所以用這張。
  - 看到的畫面：第 56／67 格。左欄由上到下是 Status、Files、Local branches、Commits、Stash 五個有框面板，框線上寫標題（「Files - Worktrees - Submodules」同一個面板用分頁），右邊是大的主內容區，最底一列是說明列。目前所在的列用整列底色標示，面板右下角寫「1 of 31」這種位置。
- 逐字摘錄（Config.md，見 `../sources-claude.md` S6）："The side panels, in the order they appear from top to bottom."；"If true, show the bottom line that contains keybinding info and useful buttons."
- 借了什麼：左側清單、右側內容的骨架；**標題寫在框線上**（不多佔一列）；清單角落的「第幾個／共幾個」；整列反白當作「目前這一列」。

## 2. gitui

- 網址：https://github.com/gitui-org/gitui
- 圖：`refs/gitui.gif`，原圖網址 https://raw.githubusercontent.com/gitui-org/gitui/master/demo.gif （README 的 `![](demo.gif)`）
  - 看到的畫面：第 1 格（靜態載入顯示的那一格）。最上一列是分頁「Status [1] | Log [2] | Stashing [3] | Stashes [4]」，左邊兩個有框面板，右邊 Diff，最底一列是指令列：「Commit [c] Unstage Item [enter] … more [.]」。
- 逐字摘錄（README）："Context based help (**no need to memorize** tons of hot-keys)"
- 借了什麼：底列只放**目前情境**用得到的指令，最後一個是「more」展開其餘；不能用的指令直接不畫（gitui 是畫成暗色，我們只能用字色，所以選擇不畫）。
- 不借：分頁名後面帶數字熱鍵「[1]」——我們的數字已經保留給選項。

## 3. k9s

- 網址：https://github.com/derailed/k9s
- 圖：`refs/k9s.png`，原圖網址 https://raw.githubusercontent.com/derailed/k9s/master/assets/screen_po.png
  - 看到的畫面：上方左邊是情境資訊（Context、Cluster…），中間是按鍵提示（`<?> Help`、`<d> Describe`），右邊是 logo；下面一個大框，**框線正中寫標題「Pods(all)[12]」**，裡面是表格，目前列整列反白。
- 逐字摘錄（README 第 383 列附近的表格）："Show active keyboard mnemonics and help"，對應鍵 `?`。
- 借了什麼：框線上的標題直接帶數量（`Pods(all)[12]` → 我們的「待決 6／8」）；`?` 開完整說明的習慣。
- 不借：k9s 把按鍵提示放在**上方**表頭。我們的操作區要貼著輸入框，所以提示一律在最下面（`../tui-sources.md` 規則 1、3）。

## 4. btop

- 網址：https://github.com/aristocratos/btop
- 圖：`refs/btop.png`，原圖網址 https://raw.githubusercontent.com/aristocratos/btop/main/Img/normal.png ；另存 `refs/btop-help.png`（https://raw.githubusercontent.com/aristocratos/btop/main/Img/help-menu.png ）
  - 看到的畫面：每個區塊是一個細框，**框線上同時放標題和功能**：「¹cpu」「menu」「²mem」「disks」「⁴proc」「filter」「per-core」「reverse」「tree」，功能名裡的熱鍵字母用另一個顏色。程序清單的**下框線**寫「↑ select ↓ info terminate kill signals」和「0/455」。
- 逐字摘錄（README）："Full mouse support: all buttons with a highlighted key are clickable and mouse scrolling works in process list and menu boxes."
- 借了什麼：**把按鍵提示畫進框線**（下框線就是快捷列，省一列）；位置計數「0/455」放在框線右端；量表 `▰▱` 式的進度。

## 5. Soft Serve（Charm）

- 網址：https://github.com/charmbracelet/soft-serve （README 開頭寫明專案已停止維護："To continue innovating in this space and supporting our most important work, we’ve sunset Soft Serve."；畫面仍可當視覺參考）
- 圖：`refs/soft-serve.png`，原圖網址 https://stuff.charm.sh/soft-serve/soft-serve-demo-commit.png （README「The Soft Serve TUI」一節）
  - 看到的畫面：標題、一行灰色副標，一條細線，分頁「Readme │ Files │ Commits │ Branches │ Tags」，**目前分頁只用顏色加底線**標示；內容大量留白；最底是狀態列，右端「? Help」。
- 逐字摘錄（README）："Easy to navigate TUI available over SSH"
- 借了什麼：目前分頁只用「字色＋底線」，不加框、不加括號；靠留白分區，而不是靠線；狀態列右端固定放說明鍵。

## 6. Gum（Charm）

- 網址：https://github.com/charmbracelet/gum
- 圖：`refs/gum-choose.gif`，原圖網址 https://vhs.charm.sh/vhs-3zV1LvofA6Cbn5vBu1NHHl.gif （README「Choose」一節）
  - 看到的畫面：第 258／323 格。`gum choose --limit 2 Banana Cherry Orange` 之後：「• Banana」「> ✓ Cherry」「✓ Orange」，選中的項目變成強調色加 `✓`，游標是前面的 `>`。
- 逐字摘錄（README）："Choose an option from a list of choices."
- 借了什麼：**游標符號、已選符號、未選符號三者分開**，每一列只靠前面一格的符號就讀得出狀態；已選的整列換成強調色。

## 7. Glow（Charm）

- 網址：https://github.com/charmbracelet/glow
- 圖：`refs/glow.gif`，原圖網址 https://github.com/user-attachments/assets/c2246366-f84b-4847-b431-32a61ca07b74 （README 的「Glow UI Demo」。檔案其實是 GIF，所以存成 .gif）
  - 看到的畫面：第 1 格。Markdown 內文置中留白，最底一列是狀態列：左邊「Glow」徽章和檔名，右邊「6%」和「? Help」。
- 逐字摘錄（README「The TUI」）："Most of the keystrokes you know from `less` are the same, but you can press `?` to list the hotkeys."
- 借了什麼：狀態列「左邊是你在哪、右邊是位置和說明鍵」的分法；短提示＋一個鍵展開完整說明（同 Bubbles 的 ShortHelp／FullHelp，見 `../sources-claude.md` S12）。

## 8. Textual（範例）

- 網址：https://github.com/Textualize/textual ；Footer：https://textual.textualize.io/widgets/footer/
- 圖：`refs/textual.png`（README 的 splash：https://github.com/user-attachments/assets/4caeb77e-48c0-4cf7-b14d-c53ded855ffd ），`refs/textual-buttons.gif`（README widget 範例「buttons」：https://github.com/user-attachments/assets/2ac26387-aaa3-41ed-bc00-7d488600343c ）
  - 看到的畫面（buttons，第 41／82 格）：一排 Default／Primary／Warning／Error 按鈕，焦點所在的按鈕顏色反轉。splash 只是展示圖，不是版面範例。
- 逐字摘錄（buttons 範例畫面上的字）："Press return to active a button when focused (or click it)."（原文就是 active）；Footer（`../sources-claude.md` S9）："Displays available keybindings for the currently focused widget."
- 借了什麼：**焦點＝反轉**是框架的標準做法，和 Claude Code engine 的焦點環一樣，所以設計上把反白當成「Enter 會按到這個」的唯一記號，其他地方都不用反白；頁尾只列目前焦點能用的鍵。

## 9. Posting

- 網址：https://github.com/darrenburns/posting
- 圖：`refs/posting.gif`，原圖網址 https://raw.githubusercontent.com/darrenburns/posting/main/docs/assets/features/jump-mode.gif
  - 看到的畫面：第 1 格。左側是請求清單（樹狀，目前那一列整列底色），右側上半是請求、下半是回應；面板標題和分頁都很輕（底線標示目前分頁）；最底一列是快捷：「/ Search  d Duplicate  Backspace Delete  ^o Jump  ^j Send  ^s Save  ^n New tab」，**鍵用強調色、說明用一般色**。
- 逐字摘錄（README「Jump mode」）："Press a shortcut, type a label, and jump straight to any part of the interface."
- 借了什麼：快捷列「鍵＋說明」兩段配色、彼此空兩格的寫法；左清單右內容、清單目前列整列標示。

## 10. Harlequin

- 網址：https://github.com/tconbeer/harlequin
- 圖：`refs/harlequin.svg`，原圖網址 https://raw.githubusercontent.com/tconbeer/harlequin/main/harlequin.svg （README 的 `![Harlequin](./harlequin.svg)`，Rich 產生的 SVG）；`refs/harlequin.png` 是我用 headless Chrome 把這個 SVG 畫成的 PNG，方便直接看。
  - 看到的畫面：左邊「Data Catalog」、右上「Query Editor」、右下「Query Results (850 Records)」，**標題都寫在框線上**，筆數直接放在標題裡；有焦點的面板框線換成黃色；最底一列「^q Quit  f1 Help  f8 History」。
- 逐字摘錄（README）："The SQL IDE for Your Terminal."
- 借了什麼：用**框線顏色**標出「鍵盤現在在哪個面板」；筆數寫在標題裡；底列只有三個鍵，其中一個是 Help。

---

## 抓不到的

- harlequin.sh 的文件頁（`https://harlequin.sh/docs/keymaps`、`/docs/keymaps/index`、`/docs/bindings/index`）：2026-11-06 用 curl 抓，三個都回 HTTP 429（請求太多），沒有拿到按鍵說明的原文。上面 harlequin 的內容只根據 README 和 SVG。
- lazygit README 目前的示範是 mp4 影片；本機沒有 ffmpeg，抽不出影片畫面。改用 repo `assets` 分支的同名 GIF（見上）。
- Textual 沒有找到專門的「版面範例」圖；用 README 的 widget 範例（buttons）和 Footer 文件頁（引自 `../sources-claude.md`）代替。

## 總結：三條借來的路

| 路線 | 來源 | 帶進這次設計的東西 |
|---|---|---|
| 框線面板 | lazygit、gitui、harlequin、k9s | 框線上寫標題和數量；焦點面板換框線顏色；清單角落寫位置 |
| 留白清單 | soft-serve、gum、glow、posting | 不畫線，用留白分區；分頁只用字色＋底線；游標／已選／未選三種符號分開 |
| 單框儀表 | btop、k9s | 一個外框，標題、量表、按鍵提示都畫進框線，省下列數 |
