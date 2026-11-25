# UX v5 參考調查：選單列 + 視窗式 TUI

調查日期 2026-11-06。圖檔都在同目錄 `refs/`。每張圖的來源與取得方式逐一列在各節。
引文都是我實際抓到的原文（英文照錄，不翻譯）；抓不到的我明說。

## 先講我們的硬限制（所有「可借用」都先對照這一節）

來源：`<codeforge repo>/plugins/forge-stats/.claude-plugin/types/claude-code/index.d.ts`（我讀過，原文如下）。

| 代號 | 限制 | 依據（d.ts 原文） |
|---|---|---|
| L1 | 只有 Box / Text / Button / Input 等元件，沒有自由定位、沒有疊層 | 任務書；`AbovePrompt`：「A hook draws a tree, or passes; one instance.」 |
| L2 | 高度上限 = 螢幕一半，含提示框 | `maxRows`：「That slot is capped at half the terminal's rows, the prompt's included.」；超過者改捲動：「a taller one scrolls in a window of `scroll.bodyRows`」 |
| L3 | 寬度要扣右側 5 欄（[-]） | `bodyColumns`：「its column's width less the engine's five at the right end」 |
| L4 | Button 熱鍵只能是單一數字或單一小寫字母 | `hotkey`：「One digit (`"1"`) or one lowercase letter (`"w"`) ... anything else is refused.」 |
| L5 | 提示框為空時，按數字直接按下 band 按鈕；捲出視窗外的按鈕不響應 | `hotkey`：「a bare digit in an empty one answers a band Button (a survey)」；`maxRows`：「A bare digit arms only the hotkeys of the Buttons wholly inside that window, never one scrolled out of view.」 |
| L6 | 焦點跟著 Button 的 key 走；兩個 Button 同熱鍵，後者勝 | `key`：「a surface finds one again by it after a redraw」；`hotkey`：「two clash, later wins」；取得焦點：「after ctrl+x tab, a click or `open({ focus })`」 |
| L7 | 不能自訂 Ctrl/Alt/F 鍵當熱鍵；`action` 只能綁引擎既有的 keybinding 動作名 | `action`：「An engine keybinding action (`"app:cycleDiffBase"`) whose chord, as the person bound it, presses this from the prompt; unknown names refused.」 |

**要講清楚的後果（L1）**：沒有疊層，「下拉選單」和「對話視窗」在 band 裡**只能用「重畫 band 的某一區」來假裝**：
- 選單項被按下 -> 重新渲染 band 樹，在選單列下方把一個區塊換成選單內容（其餘內容被擠下去或被取代，不是蓋在上面）。
- 對話框 -> 把 band 內容換成一個畫了框線的子樹（Box 邊框 + 標題 Text + 按鈕列），原內容暫時被取代，關閉後換回。
- 沒有「背景變暗 / 陰影透出下層 / 蓋住下層」。陰影只能畫成多佔一欄一列的深色字元（假陰影），而且要佔版面。
- 兩個觀察，僅供查證，不是結論：(a) d.ts 的 `RenderComponent` 另有 `Pane`（docked 側欄，另一個表面；我沒評估它能否承載選單）、`Placed`（放進對話紀錄的一列）、`Backdrop`（僅桌面與手機，終端機不畫）。(b) 它們都不是 band；下文的「硬限制檢查」只針對 band。

---

## 1. Turbo Vision（Borland 原版 + magiblot/tvision）

### 圖
| 檔案 | 內容 | 來源 URL |
|---|---|---|
| `refs/turbovision-1.png` | tvedit（Konsole 內）：選單列在頂；視窗雙線框 + 左上 `[■]` 關閉鈕 + 右上 `[↕]` 縮放鈕；Find 對話框含陰影；底部 F 鍵狀態列 | https://user-images.githubusercontent.com/20713561/81506401-4fffdd80-92f6-11ea-8826-ee42612eb82a.png （magiblot/tvision README 首圖；https://github.com/magiblot/tvision ） |
| `refs/turbovision-2.png` | 下拉選單展開：第一項反白，子選單方框 + 分隔線 + 熱鍵字母紅色 + 右側 Alt-X，右下有陰影；內容用 CJK/RTL 文字示範 Unicode | https://user-images.githubusercontent.com/20713561/103179255-5396a380-488a-11eb-88ad-0192adbe233e.png （同 README「Unicode Hello」） |
| `refs/turbovision-3.png` | 色彩模式比較的一張小圖（340x192，只作色彩參考） | https://user-images.githubusercontent.com/20713561/111095336-7c4f4080-853d-11eb-8331-798898a2af68.png （同 README） |
| `refs/turbovision-4.jpg` | Borland Turbo C++ IDE：頂端選單列 File Edit Search Run Compile…，熱鍵首字紅色，視窗標題置中嵌在雙線框，底部 F 鍵狀態列 | https://commons.wikimedia.org/wiki/File:Turbo_CPP_Compiler.jpg （CC BY-SA 3.0；經 Special:FilePath 下載） |

### 文字依據
- tvision README 首段：「A modern port of Turbo Vision 2.0, the classical framework for text-based user interfaces. Now cross-platform and with Unicode support.」；目標之一：「Being as compatible as possible at the source code level with old Turbo Vision applications.」 https://raw.githubusercontent.com/magiblot/tvision/master/README.md
- Wikipedia：「By default, Turbo Vision applications replicate the look and feel of these IDEs, including edit controls, list boxes, check boxes, radio buttons and menus, all of which have built-in mouse support.」；「It was used by Borland itself to write the integrated development environments (IDE) for these programming languages.」 https://en.wikipedia.org/wiki/Turbo_Vision
- turbovision-1 的範例程式碼畫面可見類別名：`TMenuBar`、`TSubMenu`、`TMenuItem`、`TStatusLine`、`TStatusItem`、`TDialog`、`TButton`、`TStaticText`。
- README 指向的官方文件：Turbo Vision For C++ User's Guide，https://archive.org/details/BorlandTurboVisionForCUserSGuide/mode/1up （我只引用連結，沒有逐頁讀）。

### 怎麼畫（從圖直接讀出）
- 選單列：固定在最上一行，灰底；項目**第一個字母紅色**標熱鍵（turbovision-1、-4）。
- 下拉：選項反白（綠底）；子選單是單線方框，項目以橫線分組，右側靠右放快捷鍵（`Alt-X`）。（turbovision-2）
- 視窗框：雙線框，`[■]` 關閉、`[↕]` 縮放，標題置中嵌在上框線。（turbovision-1）
- 對話框：灰底，標題置中；輸入欄深藍底；`[X]` / `[ ]` 核取框；`OK` `Cancel` 綠底按鈕，下緣與右緣有黑色半格陰影。（turbovision-1）
- 焦點：輸入欄尾端白色游標方塊；標籤首字母黃色（Alt+字母跳轉的提示）。
- 背景：不變暗，用黑色陰影表示層次；底部固定 F 鍵狀態列。

### 值得借用
- 「首字母高亮 = 熱鍵」與「右側靠右放快捷鍵」只需 Text 顏色即可；底部 `F2 Save  F3 Open …` 狀態列對應 band 最底一行按鈕列。
- 對話框骨架「標題嵌上框線 + 灰底 + 底部按鈕列」可做成可重用的 Box 子樹。

### 硬限制檢查
- **被 L4/L7 擋**：`Alt+字母`、`F1..F10`、`Ctrl-W`。熱鍵只能是 `f` 這類單字母或數字，且僅在 band 有焦點時有效（L6），或提示框為空時按數字（L5）。
- **被 L1 擋**：下拉方框蓋在視窗上、陰影、`[■]` 角落鈕（右上角還要扣 5 欄，L3）。
- **被 L2 擋**：展開的下拉 + 內容 + 提示框合計不能超過半高。
- **不被擋**：首字母高亮、右側快捷鍵字樣、標題框線、底部按鈕列。

---

## 2. Midnight Commander 與 Norton Commander

### 圖
| 檔案 | 內容 | 來源 URL |
|---|---|---|
| `refs/midnight-commander-1.png` | MC 4.6.1（法文介面）：選單列 Gauche/Fichier/Commande/Options/Droite + Fichier 下拉展開，熱鍵字母黃色、右側快捷鍵（F2、F3、C-x c…）；底部 F1..F10 按鈕列、提示行、命令列 | https://commons.wikimedia.org/wiki/File:Midnight_Commander_4.6.1_menu.png （GPL，作者 Didier Misson） |
| `refs/midnight-commander-2.png` | 同版「複製」對話框：標題嵌框線、綠底輸入欄、`[x]` 核取框、`[< Valider >]` `[ Fond ]` `[ Annuler ]` 按鈕、後方面板仍可見 | https://commons.wikimedia.org/wiki/File:Midnight_Commander_4.6.1_copy.png （GPL，同作者） |
| `refs/norton-commander-1.png` | Norton Commander 5.51（DOS）：雙面板 + Info 面板，底部 F 鍵列（`1Left 2Right 3Name …`） | https://en.wikipedia.org/wiki/File:Norton_Commander_5.51.png （英文維基標「Fair use」，僅作內部研究參考，不可散布） |

MC 圖是法文介面，不影響版型。我沒有找到 MC 的「刪除確認」對話框圖，不提供。

### 文字依據
- MC man page 原檔 https://raw.githubusercontent.com/MidnightCommander/mc/master/doc/man/mc.1.in ：
  - 「Almost all of the screen space is taken up by two directory panels. By default, the second line from the bottom of the screen is the shell command line, and the bottom line shows the function key labels. The topmost line is the menu bar line.」
  - 「The menu bar line may not be visible, but appears if you click the topmost line with the mouse or press the F9 key.」
  - 「The menu bar pops up when you press F9 or click the mouse on the top row of the screen. The menu bar has five menus: "Left", "File", "Command", "Options" and "Right".」
  - 「When this option is enabled, the pull down menus will be activated as soon as you press the F9 key. Otherwise, you will only get the menu title, and you will have to activate the menu either with the arrow keys or with the hotkeys.」
  - 「You can also use the hotkeys on the buttons to quickly activate them. Hotkeys are shown as highlighted letters on the buttons.」
- Wikipedia Norton Commander：「At the bottom of the screen, Norton Commander displays a list of commands that are extended on demand by the CTRL and ALT keys.」 https://en.wikipedia.org/wiki/Norton_Commander

### 怎麼畫
- 選單列在最頂行，**預設可隱藏**，F9 才出現（和我們 band 的收合/展開對應）。
- 下拉：青綠底方框，**熱鍵字母黃色**，快捷鍵靠右，分隔線分組，當前項黑底反白。
- 對話框：淺灰底、標題嵌上框線；**預設按鈕 `[< OK >]`，其餘 `[ Cancel ]`**（尖括號區分預設，與顏色無關）。
- 焦點：當前項反白；輸入欄游標方塊。背景不變暗，面板仍可見。
- 底部 F 鍵列：數字一色、標籤一色，交替排列。

### 值得借用
- 底部 `1Help 2Menu 3View …` 數字 + 標籤列，和 L5「數字鍵直接按 band 按鈕」天然相容；可畫成 `1:用量 2:艦隊 3:記憶`。
- `[< OK >]` vs `[ Cancel ]` 的括號區分預設按鈕，不靠顏色也看得出。

### 硬限制檢查
- **被 L4/L7 擋**：F1..F10、F9 開選單、`C-x c`；改用數字與單字母，F 鍵標籤可保留視覺但不能以 F 鍵觸發。
- **被 L1 擋**：下拉、對話框蓋在面板上；只能取代 band 內容。
- **被 L2/L3 擋**：MC 的 Fichier 下拉約 20 項（數自 mc-1），超過半高，必須分頁；右側快捷鍵靠右對齊要以 `bodyColumns` 計，不是螢幕寬。捲出視窗的項目熱鍵無效（L5）。
- **不被擋**：隱藏/展開選單列、黃色熱鍵字母、`[< >]` 預設按鈕。

---

## 3. dialog / whiptail（menubox、yesno、checklist、radiolist）

### 圖（注意：以下圖是我自己產生的，不是下載的）
我在本機以 whiptail 0.52.25（newt）用 tmux 80x20 實際執行，取 `capture-pane -e` 的真實輸出，轉 HTML 後用 chrome-headless-shell 截圖。底色藍是我的轉換器設定（newt 預設根背景為藍），其餘顏色來自擷取的 SGR。可用下列指令重現。

| 檔案 | 內容 | 指令 |
|---|---|---|
| `refs/dialog-whiptail-1.png` | menubox：標題嵌框線、當前項紅底反白、`<Ok>` `<Cancel>`、backtitle 在左上 | `whiptail --title "Main menu" --backtitle "CodeForge survey" --menu "Choose an action" 15 56 5 1 "Usage and quota" 2 "Fleet status" 3 "Memory" 4 "Settings" 5 "Quit"` |
| `refs/dialog-whiptail-2.png` | yesno：`<Yes>` 紅底，`<No>`；右、下有陰影 | `whiptail --title "Confirm" --backtitle "CodeForge survey" --yesno "Delete the snapshot?\nThis cannot be undone." 10 50` |
| `refs/dialog-whiptail-3.png` | checklist：`[*]` / `[ ]` | `whiptail --title "Panels" --backtitle "CodeForge survey" --checklist "Show which panels (SPACE toggles)" 14 56 4 usage "Usage" ON fleet "Fleet" OFF memory "Memory" ON pet "Pet" OFF` |
| `refs/dialog-whiptail-4.png` | radiolist | `whiptail --title "Account" --backtitle "CodeForge survey" --radiolist "Pick one (SPACE selects)" 14 56 4 a "Account A" ON b "Account B" OFF c "Account C" OFF` |
| `refs/dialog-whiptail-5.png` | 真正的 `dialog`（Thomas Dickey）editbox：粗陰影、標題嵌框 | https://en.wikipedia.org/wiki/File:Dialog-editbox.png （維基標示公共領域） |

本機沒有 `dialog` 指令，所以 dialog 本體只有第 5 張；whiptail 是 Debian 的 newt 重寫，man page 自述「Based on the man page for dialog(1)」。

### 文字依據（https://manpages.ubuntu.com/manpages/focal/man1/whiptail.1.html ）
- 「Currently, these types of dialog boxes are implemented: yes/no box, menu box, input box, message box, text box, info box, checklist box, radiolist box, gauge box, and password box.」
- yes/no：兩顆按鈕「the user can switch between by pressing the TAB key」。
- 「--checklist ... You can select and deselect items using the SPACE key.」
- 「--radiolist ... The only difference is that you can indicate which entry is currently selected, by setting its status to on.」
- 「Exit status is 0 if whiptail is exited by pressing the Yes or OK button, and 1 if the No or Cancel button is pressed. Otherwise, if errors occur inside whiptail or whiptail is exited by pressing the ESC key, the exit status is 255.」
- 版面以固定大小指定：「--menu text height width menu-height [ tag item ] ...」。

### 怎麼畫
- 對話框置中、灰白底、標題嵌上框線（`┤ Main menu ├`）、右/下各 1 格黑陰影；整個背景是藍色，沒有下層可透。
- 熱鍵：menubox 的 `tag`（1、2、3…）是數字；TAB 切按鈕；SPACE 切勾選；ESC 取消。
- 焦點：當前項/按鈕紅底白字反白。
- 背景：整個畫面被取代，不是疊在別的畫面上；backtitle 在左上。

### 值得借用
- **menubox 的 tag 就是數字**（`1 Usage and quota / 2 Fleet status …`），與 L5 完全同構，是最貼近我們條件的骨架。
- `<Yes> <No>` / `<Ok> <Cancel>` 一對按鈕 + 當前焦點反白，是最小的「小對話框」。

### 硬限制檢查
- **不被擋（最相容）**：數字 tag、單字母、勾選項（按數字直接切換，不需 SPACE）。
- **被 L1 擋**：陰影（要多佔一欄一列）、置中浮動。
- **被 L7 擋**：TAB / SPACE / ESC 當按鈕鍵；改數字或單字母，焦點切換可交給引擎的 `ctrl+x tab`。
- **被 L2 擋**：`list-height` 受半高限制；checklist 超出時分頁，捲出視窗外項目數字鍵無效（L5）。
- 標題嵌框線能否用 Box 做到，我沒查 Box 的 API，標為「待驗證」；做不到就改成框內第一行。

---

## 4. Ratatui（showcase + 官方範例）

### 圖
以下皆為官方 GIF 單一影格，用 chrome-headless-shell 的 `ImageDecoder` 解出，GIF 本身未留存，只留 PNG（影格編號供重現）。

| 檔案 | 內容 | 來源 GIF 與影格 |
|---|---|---|
| `refs/ratatui-1.png` | popup 範例：「Content」區塊上疊「Popup」小框（下層被清掉，不是變暗） | https://github.com/ratatui/ratatui/blob/images/examples/popup.gif?raw=true ，影格 80 |
| `refs/ratatui-2.png` | demo：頂端 Tabs（Tab0/Tab1/Tab2）、Gauge、Sparkline、List、BarChart、Chart、Paragraph，每塊都是標題嵌框線的 Block | https://github.com/ratatui/ratatui/blob/images/examples/demo.gif?raw=true ，影格 200 |
| `refs/ratatui-3.png` | demo2：右上分頁列（Recipe / Email / Traceroute / Weather，當前頁白底反白）、Inbox/Sent/Drafts 子分頁、清單 + 預覽 | https://github.com/ratatui/ratatui/blob/images/examples/demo2.gif?raw=true ，影格 250 |
| `refs/ratatui-4.png` | todo-list：藍底區塊標題列、選中項 `>` 前綴、底部說明列 | https://github.com/ratatui/ratatui/blob/images/examples/todo-list.gif?raw=true ，影格 300 |

GIF 網址來自 `examples/README.md`（https://raw.githubusercontent.com/ratatui/ratatui/main/examples/README.md）的連結定義，例如 `[popup.gif]: https://github.com/ratatui/ratatui/blob/images/examples/popup.gif?raw=true`。

### 文字依據
- examples/README：「## Popup Shows how to handle popups.」；「## Demo2 This is the demo example from the main README and crate page.」；「## Todo List Shows how to create a simple todo list application.」
- 我在該 README 搜尋 `menu`，沒有命中，所以官方範例沒有選單列範例，有的是 Tabs 與 List。（範圍僅此 README 目錄，不保證整個 repo。）
- popup 原始碼 https://raw.githubusercontent.com/ratatui/ratatui/main/examples/apps/popup/src/main.rs ：
  - `let centered_area = area.centered(Constraint::Percentage(60), Constraint::Percentage(20));`
  - 「// clears out any background in the area before rendering the popup」 `frame.render_widget(Clear, centered_area);`
  - `Line::from("Press 'p' to toggle popup, 'q' to quit").centered()`

### 怎麼畫
- popup：先 `Clear` 該區，再畫 Block，是「清掉再蓋上」，不變暗。
- Tabs：一行，當前頁反白，其餘淡色，`|` 分隔。
- 標題嵌在 Block 上框線（`┌Content────`）。
- 焦點/熱鍵：不內建，應用自行畫在底部說明列。

### 值得借用
- demo2 的「頂端 Tabs + 中間內容 + 底部說明列」就是我們要的選單列 + 內容骨架，「當前頁反白、其餘淡色」只靠 Text 顏色。
- todo-list 底部一行（`Use ↓↑ to move, ← to unselect, → to change status, g/G to go top/bottom.`）是最便宜的焦點提示。

### 硬限制檢查
- **被 L1 擋**：popup 核心技術（`Clear` + 疊在別的內容上 + `centered`）。
- **被 L1 擋**：Chart、Sparkline、BarChart 需要像素/半格精度（Raster 半格另見 `doc/specs/codeforge-mod-ui.md`，這裡不評估）。
- **被 L3 擋**：demo2 把 Tabs 靠右貼邊，我們要用 `bodyColumns` 並扣 5 欄。
- **不被擋**：Tabs 列、標題嵌框線、`>` 前綴、底部說明列。

---

## 5. Textual（ModalScreen、命令面板）

### 圖
以下是官方文件頁面內嵌的 SVG 截圖，我擷取後用 chrome 轉成 PNG，內容未改（圖底多出的黑條是我的視窗尺寸，不是原圖）。

| 檔案 | 內容 | 來源 |
|---|---|---|
| `refs/textual-1.png` | ModalScreen 範例（modal02.py）按 Q 後：下層文字仍可見但被淡化，中央對話框「Are you sure you want to quit?」，兩顆大按鈕 Quit（紅）/ Cancel（紫），Footer `q Quit`、`^p palette` | https://textual.textualize.io/guide/screens/ 「Modal screens」段，Output (after pressing Q) |
| `refs/textual-2.png` | 命令面板：頂端搜尋欄（🔎 t），下列模糊比對結果（名稱比對字母底線 + 說明），當前項紫底 | https://textual.textualize.io/guide/command_palette/ ，「Command Palette after 't'」 |
| `refs/textual-3.png` | 對照組：modal01.py 用 `Screen`（不是 `ModalScreen`）按 Q 後，對話框把整個畫面取代，下層消失 | https://textual.textualize.io/guide/screens/ |

### 文字依據（頁面原文）
- guide/screens：「We can solve both those issues by replacing our use of Screen with ModalScreen. This screen sub-class will prevent key bindings on the app from being processed. It also sets a background with a little alpha to allow the previous screen to show through.」
- 同頁：「Screens are containers for widgets that occupy the dimensions of your terminal. There can be many screens in a given app, but only one screen is active at a time.」
- guide/command_palette：「Commands are looked up via a fuzzy search, which means Textual will show commands that match the keys you type in the same order, but not necessarily at the start of the command. For instance the "Change theme" command will be shown if you type "ch" (for change), but you could also type "th" (to match theme).」；「Up or Down to select a command from the list, and Enter to invoke it.」

### 怎麼畫
- 選單列：圖中只看到 Header + Footer，沒有傳統選單列；它把命令集中在 `^p palette`。我沒有查是否有第三方選單元件，不下結論。
- 對話：ModalScreen 是半透明背景 + 置中對話框，並鎖住下層鍵位。
- 焦點：按鈕色塊；Footer 以「紫色鍵名 + 淡色說明」顯示可用鍵。熱鍵由綁定自動生成到 Footer。

### 值得借用
- **命令面板**：Input + 其下的篩選結果列，不需選單層級就到達所有動作；在 band 內是一個 Input + 數個 Button 結果列（數字鍵 1..N 直接選），只佔 3~6 行，對半高限制最友善。
- ModalScreen 的「鎖住下層鍵位」：我們對應成「對話 band 展開時只輸出對話框自己的 Button」，避免熱鍵衝突（L6：同熱鍵後者勝）。

### 硬限制檢查
- **被 L1 擋**：半透明背景與「下層仍可見」。band 只能取代；可把原內容縮成一行摘要放在對話框上方，表示「還在」。
- **被 L4/L7 擋**：`ctrl+p` 呼叫鍵；可改成 band 上一顆 `/` Button，或用引擎 `action` 綁既有動作。
- **被 L5 擋**：結果超過可視行數時，捲出者數字鍵無效，結果限制在 9 筆內。
- **不被擋**：模糊搜尋 Input、結果列兩行（名稱 + 說明）排法、Footer 熱鍵列、大按鈕色塊。

---

## 6. Bubble Tea（soft-serve、glow、gh-dash、bubbles list/help）

### 圖
皆為官方 README 引用的資源；GIF 我抽影格轉 PNG。

| 檔案 | 內容 | 來源 |
|---|---|---|
| `refs/bubbletea-1.png` | bubbles `list`：「Filter: ghe」輸入、「2 items • 70 filtered」、結果列（名稱 + 說明，比對字底線）、底部 `enter apply filter • esc cancel • q quit` | https://stuff.charm.sh/bubbles-examples/list.gif （bubbles README 引用），影格 140 |
| `refs/bubbletea-2.png` | Soft Serve：標題、水平 Tabs（Readme / Files / Commits / Branches / Tags，當前頁著色底線）、內容、底部色塊狀態列（`soft-serve` / hash / `0%` `main` `? Help`） | https://stuff.charm.sh/soft-serve/soft-serve-demo-commit.png （soft-serve README 引用） |
| `refs/bubbletea-3.png` | gh-dash：頂端分頁列（My Pull Requests (3) / Review Requested (0) / My Team (2) / Open Source (367)）、過濾輸入列、表格；**按 `?` 後底部展開完整說明面板，其餘內容被壓縮在上方** | https://raw.githubusercontent.com/dlvhdr/gh-dash/main/docs/src/assets/overview.gif （gh-dash README 引用），影格 60 |
| `refs/bubbletea-4.png` | bubbles `help`：多欄鍵位說明（`↑/k move up`、`? toggle help`、`q quit`） | https://stuff.charm.sh/bubbles-examples/help.gif ，影格 100 |
| `refs/bubbletea-5.png` | gh-dash 另一影格：切到 Open Source 分頁，底部狀態列 `PRs / Issues / gifs • @dlvhdr … ? help` | 同 gh-dash overview.gif，影格 250 |
| `refs/bubbletea-6.png` | 同 GIF 中的 diffnav：檔案樹 + 左右對照 diff + 底部說明列（畫面標題「DIFFNAV」） | 同上，影格 150 |

### 文字依據
- bubbles README https://raw.githubusercontent.com/charmbracelet/bubbles/main/README.md ：
  - List：「Features pagination, fuzzy filtering, auto-generated help, an activity spinner, and status messages, all of which can be enabled and disabled as needed. Extrapolated from Glow.」
  - Help：「A customizable horizontal mini help view that automatically generates itself from your keybindings. It features single and multi-line modes, which the user can optionally toggle between. It will truncate gracefully if the terminal is too wide for the content.」
- Soft Serve README https://raw.githubusercontent.com/charmbracelet/soft-serve/main/README.md ：「A tasty, self-hostable Git server for the command line.」；**同一份 README 開頭寫著「# Sunsetting Soft Serve」「we've sunset Soft Serve. The code will continue to be open source and publicly available.」——它已停止維護，借用其設計時需知道。**
- Glow README https://raw.githubusercontent.com/charmbracelet/glow/main/README.md ：「Glow is a terminal based markdown reader designed from the ground up to bring out the beauty—and power—of the CLI.」我下載了其 `glow-banner-github.gif`，那是橫幅動畫、不是選單畫面，所以不收入 refs。
- 我沒有找到 Bubble Tea 生態中有真正「下拉選單列」的成功應用；以上都是「分頁列 + 清單 + 底部說明列」。這是調查缺口，不是不存在的證明。

### 怎麼畫
- 導覽：**頂端水平分頁列**（非下拉）；當前項反白或底線，其餘淡色。
- 視窗：少用框線，靠留白、分隔線與色塊（gh-dash 側欄預覽）。
- 對話：沒有疊層；`?` 展開的說明是**原位換成另一塊內容**（bubbletea-3）。
- 熱鍵：底部 help 列由 keymap 自動生成（`key desc • key desc`）；焦點用顏色。背景不變暗。

### 值得借用
- **「`?` 切換說明 = 原位換內容」是我們唯一做得到的對話做法**；bubbles `help` 的單行/多行兩種模式可直接當 band 的收合/展開樣式。
- 過濾輸入 + 結果列的 `list`（bubbletea-1）與 Textual 命令面板同構，是半高限制下最省行數的互動。

### 硬限制檢查
- **被 L4/L7 擋**：`ctrl+d`、方向鍵、`←/h` 等；只能單字母（k/j 可）與數字，方向鍵不是 Button 熱鍵。
- **被 L2 擋**：gh-dash 展開說明面板的高度超過半螢幕；我們限 `maxRows`，多的捲動（捲出者熱鍵無效，L5）。
- **被 L3 擋**：Soft Serve 右側 `? Help` 色塊靠右貼邊；我們要扣 5 欄。
- **不被擋**：頂端分頁列、色塊狀態列、底部 help 列、過濾輸入 + 結果列、`?` 原位換內容。

---

## 7. BBS 選單畫面（Synchronet、Mystic、ANSI 主選單）

### 圖
| 檔案 | 內容 | 來源 URL |
|---|---|---|
| `refs/bbs-1.png` | PTT（批踢踢實業坊，台灣）「主功能表」：頂部標題列（左【主功能表】、中站名、右看板）、ANSI 美術、一行訊息，然後 `(A)nnounce 【 精華公佈欄 】` 形式的「字母熱鍵 + 英文名 + 中文說明」清單，當前項左側圓點游標，底部狀態列（時間、線上人數、`[扣機]打開`） | https://commons.wikimedia.org/wiki/File:Pttmainmenu.png （GPL） |
| `refs/bbs-2.jpg` | Reaktor7 BBS 主選單：三欄分組標題 `[ file commands ]` `[ message commands ]` `[ general commands ]`，每項 `( f ) File areas` 括號包熱鍵，ANSI 標題圖 | https://commons.wikimedia.org/wiki/File:Reaktor7menu.jpg （CC BY-SA 3.0） |

### 文字依據
- Synchronet 官方 wiki https://wiki.synchro.net/user:shell:classic （我抓到 HTML 並讀過）：
  - 「The Synchronet Classic command shell (default.js) is the default shell on most Synchronet BBSes. It emulates the original Synchronet v1 user interface and is what Synchronet "looks like" to most people who've never been told otherwise.」
  - 「The captures on this page are from a Synchronet v3.22 BBS in UTF-8 / ANSI mode. On a US-ASCII terminal the BBS substitutes plain ASCII (+–+) for the line-art glyphs shown below; the menu meaning and command keys are the same.」
  - 「Main prompt — the top-level command prompt, displayed once you've completed logon.」
  - 該頁章節含「Asking for the menu」「Quick key-sequence guide」「Main Menu」。其截圖是文字，頁面內沒有圖片檔（img 只有版面按鈕圖示），所以沒有圖。
- **未完成**：Synchronet 與 Mystic BBS 本身的圖檔我沒有取得。我查過維基頁面的圖片清單與 Commons 搜尋，沒有命中這兩者。16colo.rs（ANSI 檔案庫）是可能的補來源，但我沒有查證，不列為已完成。

### 怎麼畫
- 選單是**清單，不是下拉**：每行「熱鍵字母（高亮色）+ 名稱 + 說明」，並有分組標題（bbs-2）。
- 熱鍵：括號內字母 `(A)nnounce`，字母青色高亮，**單一字母直接選**，不需 Alt 或 Enter。
- 焦點：PTT 以圓點游標 + 反白；Synchronet Classic 以提示符等單鍵。
- 視窗：全螢幕整頁，換頁就是整頁取代（與 whiptail 一樣）。
- 背景：無層次；頂、底各一條狀態列，中間整頁選單。

### 值得借用
- **`(A)nnounce` 式「首字母括起來高亮」**符合 L4 的單一小寫字母格式；PTT 在繁中環境證明「字母 + 中文說明」雙語列好讀。按鈕列可畫成 `(u)用量 (f)艦隊 (m)記憶`，括號內小寫字母是 Button hotkey，其餘是 Text。
- 頂部標題列 + 底部狀態列夾住中間整頁選單，是不需疊層的「視窗感」。

### 硬限制檢查
- **被 L2 擋**：整頁選單（PTT 約 25 行含美術）超過半高；只能保留清單、丟美術或分頁。
- **被 L1 擋**：全螢幕 ANSI 美術與自由定位（圓點游標對齊、跑馬訊息）。
- **被 L4 擋**：大寫熱鍵（`(A)nnounce`）；顯示可大寫，按鍵必須小寫 `a`。
- **不被擋**：字母括號高亮清單、分組標題列、頂/底狀態列。

---

## 綜合速查（給兩個設計席）

做得到（不依賴疊層與組合鍵）：
1. 頂端一行「選單列」：`Text` 排 `檔案 檢視 …`，首字母/數字高亮，當前項反轉；各項是 Button（單字母/單數字熱鍵）。
2. 底部一行 F 鍵列式的 `1:用量 2:艦隊 3:記憶 …`（MC/NC 的直接移植，契合 L5）。
3. 「下拉」= 選單列下方展開一區塊（取代或下推原內容），受 `maxRows` 限制，超過就分頁。
4. 「對話框」= band 整區換成帶框線與標題的子樹，內含 `[< 確定 >] [ 取消 ]` 一列按鈕（預設用 `< >` 標記）。
5. `?` 原位換成說明（gh-dash / bubbles help），以及輸入即篩選的命令面板式 list。

做不到：陰影、半透明/變暗背景、對話框蓋住下層、置中浮動、Alt/F/Ctrl 自訂熱鍵、大寫熱鍵、右側 5 欄內的內容、超過半螢幕的展開。
