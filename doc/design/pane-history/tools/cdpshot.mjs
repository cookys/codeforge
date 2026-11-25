// 頁面截圖（等 load、字型、圖片 decode 完才拍；PM 01M89CEC：舊法在 29 MB 頁面畫完前就拍，整片空白）。
// node cdpshot.mjs <url> <out.png> <w> <h> [selector|-] [theme] → 印出 JSON：每個 .term-grid 的位置、列距、直框線段
import { spawn } from 'node:child_process';
import { writeFileSync, mkdtempSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
const [url, out, w, h, sel = '-', theme = 'dark'] = process.argv.slice(2);
const C = process.env.CHROME_BIN || `${homedir()}/.cache/ms-playwright/chromium_headless_shell-1248/chrome-headless-shell-linux64/chrome-headless-shell`;
const port = 9300 + Math.floor(Math.random() * 600);
const ch = spawn(C, ['--no-sandbox', '--disable-gpu', '--hide-scrollbars', `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(`${tmpdir()}/cdp-`)}`, 'about:blank'], { stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let tabs = []; for (let i = 0; i < 100 && !tabs.length; i++) { try { tabs = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).filter(t => t.type === 'page'); } catch {} if (!tabs.length) await sleep(200); }
const ws = new WebSocket(tabs[0].webSocketDebuggerUrl); await new Promise(r => ws.onopen = r);
let id = 0; const pend = new Map(); const evs = new Set();
ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id) { pend.get(d.id)?.(d); pend.delete(d.id); } else evs.add(d.method); };
const cmd = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await cmd('Page.enable');
await cmd('Emulation.setDeviceMetricsOverride', { width: +w, height: +h, deviceScaleFactor: 1, mobile: false });
await cmd('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] });
await cmd('Page.navigate', { url });
for (let i = 0; i < 1200 && !evs.has('Page.loadEventFired'); i++) await sleep(100);
const js = `(async()=>{await document.fonts.ready;
for(const i of document.images){i.loading='eager';}
await Promise.all([...document.images].map(i=>Promise.race([i.decode().catch(()=>0),new Promise(r=>setTimeout(r,8000))])));
${sel !== '-' ? `document.querySelector(${JSON.stringify(sel)}).scrollIntoView({block:'start'});` : ''}
await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
const vis=g=>{if(g.checkVisibility&&!g.checkVisibility({opacityProperty:true,visibilityProperty:true}))return false;${sel !== '-' ? `const r=g.getBoundingClientRect();const x=r.left+r.width/2,y=Math.max(r.top,0)+4;if(y>innerHeight)return true;const e=document.elementFromPoint(x,y);return !!e&&g.contains(e);` : 'return true;'}};
const grids=[...document.querySelectorAll('.term-grid')].filter(vis).map(g=>{const r=g.getBoundingClientRect();const ln=[...g.querySelectorAll('.ln')];
 const sc=r.width/g.offsetWidth; const pitch=ln.length>1?(ln[1].getBoundingClientRect().top-ln[0].getBoundingClientRect().top):0;
 const rows=ln.map(l=>[...l.children].map(i=>i.textContent));
 const segs=[];const colOf=row=>{let c=0;return row.map(t=>{const x=c;c+=(t.length&&t.codePointAt(0)>0x2e7f?2:1);return [x,t]})};
 const cols=rows.map(colOf);const open={};
 cols.forEach((row,y)=>row.forEach(([x,t])=>{if('╭┌'.includes(t))open[x]=y;else if('╰└'.includes(t)&&open[x]!=null){segs.push([x,open[x],y]);delete open[x];}}));
 return {sec:(g.closest('section[id],[id^=drift],[id]')||{}).id||'',left:r.left,top:r.top+scrollY,scale:sc,pitch,cellW:8*sc,rows:ln.length,segs};});
return {scrollY,images:document.images.length,grids};})()`;
const r = await cmd('Runtime.evaluate', { expression: js, awaitPromise: true, returnByValue: true });
const v = r.result.result.value; console.log(JSON.stringify(v ?? r.result));
const full = sel === '-';
let clip;
if (full) { const m = await cmd('Page.getLayoutMetrics'); clip = { x: 0, y: 0, width: +w, height: Math.ceil(m.result.cssContentSize.height), scale: 1 }; }
const s = await cmd('Page.captureScreenshot', full ? { format: 'png', clip, captureBeyondViewport: true } : { format: 'png' });
writeFileSync(out, Buffer.from(s.result.data, 'base64'));
ws.close(); ch.kill(); process.exit(0);
