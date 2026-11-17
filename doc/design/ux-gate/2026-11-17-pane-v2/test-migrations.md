# 測試遷移

下列舊畫面契約依任務移除；同一 parser 的原斷言保留，新的畫面斷言在 pane-chat、pane-v2、overview 與 ux-shots。

## fleet-tab.test.tsx

- `test('without the file the strip is Pet | Usage and no frame mentions the fleet tab', async ($, on) => {`
- `test('with the file the strip gains the tab, and it draws warnings first, then one line per account', async ($, on) => {`
- `test('zh-TW draws the tab with the CLI wording', async ($, on) => {`
- `test('the warnings come from the JSON: a stale file is shown as it is, not re-evaluated', async ($, on) => {`
- `test('a narrow pane (40 cols) keeps every row inside the width, CJK too', async ($, on) => {`
- `test('the name labels the row, then the email; an acct- hash never shows; a writer without name falls back', async ($, on) => {`
- `test('the name and email label every row inside the pane', async ($, on) => {`
- `test(\`an old host and a reset window (${zh ? 'zh-TW' : 'en'}): 舊 / old and 已重置 / reset, never the old pct, no warning\`, async ($, on) => {`
- `test(\`${name}: the tab draws the one dim unreadable line\`, async ($, on) => {`
- `test(\`a no-source row carries its reason, and a note ends the row (${zh ? 'zh-TW' : 'en'})\`, async ($, on) => {`
- `test('a long CJK note is cut with … to the width; the numbers and the word stay', async ($, on) => {`
- `test('a no-data note row parses and is drawn with its note', async ($, on) => {`
- `test('the rule countdown is drawn dim after the note, and alone on a row without a note', async ($, on) => {`
- `test('a narrow pane cuts the rule row note with … and keeps the countdown', async ($, on) => {`
- `test(\`the all-accounts pane shows the plan, and ${unknown} when absent (${zh ? 'zh-TW' : 'en'})\`, async ($, on) => {`
- `test(\`five_hour_state no_source (${zh ? 'zh-TW' : 'en'}): the 5h slot says no source, never the old 99%\`, async ($, on) => {`

原因：第三個「全部帳號」分頁與逐筆無來源列已移除；改驗總覽排序、單列摘要、欄寬與缺檔狀態。

## usage.test.tsx

- `test('the pane has a tab strip and the usage tab shows accounts and today\'s models', async ($, on) => {`
- `test('a summary older than an hour names \`codeforge usage\` as the refresh', async ($, on) => {`
- `test('no summary file says how to make one', async ($, on) => {`
- `test('a summary path that is a link is refused before reading', async ($, on) => {`
- `test('a summary larger than the bound is refused before reading', async ($, on) => {`
- `test('\`/forge-stats usage\` opens the pane on the usage tab, \`/forge-stats pet\` back', async ($, on) => {`
- `test('a narrow pane drops the requests column and keeps the model id and the total whole', async ($, on) => {`
- `test('the account row drops countdowns, then 7d, as the pane narrows, and never cuts a field', async ($, on) => {`

原因：獨立 Usage 分頁與本機帳號列已移除；保留 summary parser，逐 model 顯示改在寵物頁且最多六列。

## pane-chat.test.tsx

- `test('short content: a dock pads blank rows on top and sits right above the tab strip; inline has no blank rows', async ($, on) => {`
- `test('overflow: the tab strip stays on its screen row through scrolling, the window is bodyRows, the indicator counts the hidden rows', async ($, on) => {`
- `test('End returns to the newest row, Home to the oldest, a page is the content area, an arrow is one row', async ($, on) => {`
- `test('resize while stuck to the newest row stays at the newest; a scrolled window keeps its top row', async ($, on) => {`
- `test('a snapshot refresh while scrolled keeps the same top row; at the bottom it stays at the bottom', async ($, on) => {`
- `test('the pet sprite is atomic: it draws whole when the content area holds it, else the text-only pet lines', async ($, on) => {`
- `test('pane-view arithmetic: window, clamps and the cell clip', () => {`
- `test('the sprite that does not fit with its rule row drops the rule and keeps the Raster; if it still does not fit, the stats alone without art', async ($, on) => {`
- `test('/forge-stats opens the pane asking for the keyboard, before it reads anything', async ($, on) => {`
- `test('a pane of three rows keeps the tab strip and the key hint in view: one content row, no indicator', async ($, on) => {`
- `test('/forge-stats layout pane opens the pane asking for the keyboard before it stores the layout', async ($, on) => {`

原因：補白、spare、預設跟隨末列及 Usage 的視窗契約已移除；改驗內容高度、兩列操作區、有效捲動與 resize。

## quota-ui.test.tsx

- `test('the usage tab draws the mirror blocks and keeps the models; the G36 accounts rows are gone once the mirror exists', async ($, on) => {`
- `test('rule 7: no quota directory keeps today\'s pane (the accounts rows); an empty one says nothing has arrived', async ($, on) => {`
- `test('rule 7: the mirror directory exists but holds nothing', async ($, on) => {`

原因：三個 Usage 分頁畫面測試退役；mirror reader 的同等精確斷言改讀原有 integrated-panel 輸出，band 測試保留。

