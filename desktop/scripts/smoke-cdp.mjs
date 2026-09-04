// Drive a running debug build over WebView2 CDP and invoke `temple_debug_capture` on a saved capture.
// Run ON WINDOWS (the port is Windows-localhost only): see docs/OVERLAY-GUIDE.md "Replaying a saved capture".
// Usage: node scripts/smoke-cdp.mjs C:\path\to\screen.png   (CDP_PORT=9222 by default)
const PORT = process.env.CDP_PORT ?? '9222';
const IMAGE = process.argv[2];
const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
const page = list.find(t => t.type === 'page' && /localhost:1420/.test(t.url) && !/overlay/.test(t.url)) ?? list.find(t => t.type === 'page');
if (!page) { console.error('no page target', JSON.stringify(list)); process.exit(2); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pending = new Map();
ws.onmessage = ev => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
const send = (method, params) => new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
const expr = `(async () => { const inv = window.__TAURI_INTERNALS__.invoke; try { const r = await inv('temple_debug_capture', { imagePath: ${JSON.stringify(IMAGE)} }); return JSON.stringify({ ok: true, report: r }); } catch (e) { return JSON.stringify({ ok: false, error: String(e) }); } })()`;
const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true, timeout: 600000 });
console.log(r.result?.result?.value ?? JSON.stringify(r));
ws.close();
