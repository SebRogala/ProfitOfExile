// node <this file> <scratch root containing overlay.html>
import fs from 'node:fs'
import { spawn } from 'node:child_process'
import assert from 'node:assert/strict'
const root = process.argv[2]
assert.ok(root, 'Pass the scratch evidence directory')
const browser = spawn('/root/.cache/ms-playwright/chromium_headless_shell-1194/chrome-linux/headless_shell', ['--no-sandbox', '--disable-gpu', '--remote-debugging-port=0', `--user-data-dir=${root}/profile`, 'about:blank'])
const endpoint = await new Promise((resolve, reject) => {
 let output = ''
 browser.stderr.on('data', chunk => { output += chunk; const match = output.match(/DevTools listening on (ws:\/\/\S+)/); if (match) resolve(match[1]) })
 browser.on('error', reject)
})
const ws = new WebSocket(endpoint)
await new Promise(resolve => ws.addEventListener('open', resolve, { once: true }))
let sequence = 0
const requests = new Map()
ws.addEventListener('message', event => {
 const message = JSON.parse(event.data)
 if (!message.id) return
 const request = requests.get(message.id); requests.delete(message.id)
 message.error ? request.reject(message.error) : request.resolve(message.result)
})
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
 const id = ++sequence; requests.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params, sessionId }))
})
const { targetId } = await send('Target.createTarget', { url: 'about:blank' })
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true })
const call = (method, params = {}) => send(method, params, sessionId)
const evaluate = async expression => {
 const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
 if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails))
 return result.result.value
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms))
async function until(expression) {
 for (let attempt = 0; attempt < 150; attempt++) { if (await evaluate(expression)) return; await pause(20) }
 throw Error(`Timed out: ${expression}`)
}
const observations = {}
let failures = 0
async function scenario(name, work, query = '') {
 try {
  await call('Page.navigate', { url: `file://${root}/overlay.html?case=${encodeURIComponent(name)}${query}` })
  await until(`window.ready===true && location.search.includes(${JSON.stringify(encodeURIComponent(name))})`)
  if (!query.includes('zero') && !query.includes('temple')) await until('document.querySelectorAll(".pick-btn").length===2')
  observations[name] = await work()
  console.log(`PASS ${name}: ${JSON.stringify(observations[name])}`)
 } catch (error) { failures++; console.log(`FAIL ${name}: ${String(error)}`) }
}
try {
 await scenario('window scoped forwarding uses Tauri scale', async () => {
  assert.equal(await evaluate('Boolean(window.listeners["overlay-click"])&&!window.bareListeners["overlay-click"]'), true)
  await evaluate(`window.calls=[];window.emissions=[];window.forward('[data-action=pick][data-index="0"]')`)
  await until('window.emissions.length===1')
  assert.deepEqual(await evaluate('window.emissions'), [['overlay-pick', { name: 'First', variant: '20/20', roi: 17 }]])
  assert.equal(await evaluate('document.querySelector(".gem-row.selected .gem-name").textContent'), 'First')
  await evaluate('window.data.results[1].name="Second refreshed"')
  await until('document.body.textContent.includes("Second refreshed")')
  assert.equal(await evaluate('document.querySelector(".gem-row.selected .gem-name").textContent'), 'First')
  return await evaluate('window.emissions')
 })
 for (const action of ['pick', 'refresh']) for (const index of [null, 'NaN', '2', '3']) {
  await scenario(`${action} ignores index ${index}`, async () => {
   await evaluate(`window.calls=[];window.emissions=[];const button=document.querySelector('[data-action="${action}"]');${index === null ? 'delete button.dataset.index' : `button.dataset.index=${JSON.stringify(index)}`};window.forward('[data-action="${action}"]')`)
   await pause(40)
   const result = await evaluate('({emissions:window.emissions,intents:window.calls.filter(([name])=>name==="request_trade_refresh")})')
   assert.deepEqual(result, { emissions: [], intents: [] }); return result
  })
 }
 await scenario('refresh emits native lookup intent', async () => {
  await evaluate(`window.calls=[];window.forward('[data-action=refresh][data-index="1"]')`)
  await until('window.calls.some(([name])=>name==="request_trade_refresh")')
  const result = await evaluate('window.calls.filter(([name])=>name==="request_trade_refresh")')
  assert.deepEqual(result, [['request_trade_refresh', { gem: 'Second', variant: '1/20' }]]); return result
 })
 await scenario('loading refresh is inert span', async () => {
  await evaluate('window.data.tradeLoading.First=true')
  await until('document.querySelector(".loading-btn")')
  assert.equal(await evaluate('document.querySelector(".loading-btn").tagName'), 'SPAN')
  await evaluate('window.calls=[];window.forward(".loading-btn")')
  await pause(40)
  assert.deepEqual(await evaluate('window.calls.filter(([name])=>name==="request_trade_refresh")'), [])
  return 'no refresh intent'
 })
 await scenario('clear resets before emitting overlay clear', async () => {
  await evaluate('window.emissions=[];window.forward("[data-action=clear]")')
  await until('window.emissions.length===1 && !document.querySelector(".gem-row")')
  const result = await evaluate('window.emissions'); assert.deepEqual(result, [['overlay-clear', {}]]); return result
 })
 await scenario('clear forgets explicit selection on next poll', async () => {
  await evaluate('window.forward(".pick-btn")')
  await until('document.querySelector(".gem-row.selected .gem-name").textContent==="First"')
  await evaluate('window.forward("[data-action=clear]")')
  await until('!document.querySelector(".gem-row")')
  await evaluate('window.data.results[1].name="After clear"')
  await until('document.body.textContent.includes("After clear")')
  const selected = await evaluate('document.querySelector(".gem-row.selected .gem-name").textContent')
  assert.equal(selected, 'After clear'); return selected
 })
 await scenario('cancel invokes trade cancel', async () => {
  await evaluate('window.bareListeners["trade-queue"]({payload:{source:"gem",kind:"queued",position:1,total:2}})')
  await until('document.querySelector("[data-action=cancel]")')
  await evaluate('window.calls=[];window.forward("[data-action=cancel]")')
  await until('window.calls.some(([name])=>name==="trade_cancel")')
  const result = await evaluate('window.calls.filter(([name])=>name==="trade_cancel")'); assert.deepEqual(result, [['trade_cancel', null]]); return result
 })
 await scenario('unknown action has no intent', async () => {
  await evaluate('document.querySelector(".pick-btn").dataset.action="unknown";window.calls=[];window.emissions=[];window.forward(".pick-btn")')
  await pause(40)
  assert.deepEqual(await evaluate('window.emissions'), [])
  assert.deepEqual(await evaluate('window.calls.filter(([name])=>["trade_cancel","request_trade_refresh"].includes(name))'), [])
  return 'ignored'
 })
 await scenario('null hit emits exact DPI diagnostic', async () => {
  await evaluate('window.calls=[];document.elementFromPoint=()=>null;window.fire("overlay-click",{x:100,y:100})')
  await until('window.calls.some(([name])=>name==="app_log_from_frontend")')
  const result = await evaluate('window.calls.filter(([name])=>name==="app_log_from_frontend")')
  assert.deepEqual(result, [['app_log_from_frontend', { msg: '[lab-widgets] elementFromPoint found nothing at the forwarded click — DPI mismatch?' }]]); return result
 })
 await scenario('no action hit is silent', async () => {
  await evaluate('window.calls=[];window.emissions=[];document.elementFromPoint=()=>document.body;window.fire("overlay-click",{x:100,y:100})')
  await pause(40)
  assert.deepEqual(await evaluate('window.calls.filter(([name])=>name==="app_log_from_frontend")'), [])
  assert.deepEqual(await evaluate('window.emissions'), []); return 'silent'
 })
 await scenario('unresolved scale declines before hit testing', async () => {
  await until('Boolean(window.listeners["overlay-click"])')
  assert.equal(await evaluate('window.hits=0;document.elementFromPoint=()=>{window.hits++;return null};window.fire("overlay-click",{x:100,y:100});window.hits'), 0)
  return 'zero hit tests'
 }, '&zero')
 await scenario('temple probe only logs', async () => {
  await until('document.querySelector(".widget")')
  await evaluate('const button=document.createElement("button");button.dataset.hot="";button.dataset.action="hot-probe";button.textContent="probe";document.querySelector(".widget").append(button);window.calls=[];window.forward("[data-action=hot-probe]")')
  await until('window.calls.some(([name])=>name==="app_log_from_frontend")')
  const result = await evaluate('window.calls.filter(([name])=>name!=="set_overlay_hot_rects"&&name!=="set_overlay_has_content")')
  assert.deepEqual(result, [['app_log_from_frontend', { msg: '[temple-widgets] hot-rect probe clicked — the mouse hook forwarded a click' }]]); return result
 }, '&module=temple&debug')
 fs.writeFileSync(`${root}/observations.json`, JSON.stringify({ failures, observations }, null, 2))
 assert.equal(failures, 0, 'characterization scenarios failed')
} finally { ws.close(); browser.kill('SIGTERM') }
