// Run with node <this file> <compiled HTML> <evidence directory>.
// Browser regression for the shipped overlay's initialization, URL updates and teardown.
import fs from 'node:fs'
import http from 'node:http'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'

const [html, evidence] = process.argv.slice(2)
const expected = {
 'Cinderswallow Urn': 'items/Cinderswallow%20Urn',
 Spinehail: 'items/Spinehail',
 'Cane of Kulemak': 'items/Cane%20of%20Kulemak',
 'The Devouring Diadem': 'items/The%20Devouring%20Diadem',
 'Bitterbind Point': 'items/Bitterbind%20Point',
 "The Queen's Hunger": "items/The%20Queen's%20Hunger",
 'Communion Support': 'items/Communion%20Support',
 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly': 'currency-exchange/Metadata%2FItems%2FMapFragments%2FAbyssPinnacleKeyGhastly',
 'Metadata/Items/Currency/CurrencyRerollRareVeiled': 'currency-exchange/Metadata%2FItems%2FCurrency%2FCurrencyRerollRareVeiled',
 'Metadata/Items/MapFragments/BetrayalAllflamePack': 'currency-exchange/Metadata%2FItems%2FMapFragments%2FBetrayalAllflamePack',
 'Metadata/Items/DivinationCards/DivinationCardNooksCrown': 'currency-exchange/Metadata%2FItems%2FDivinationCards%2FDivinationCardNooksCrown',
 'Metadata/Items/DivinationCards/DivinationCardKeepersCorruption': 'currency-exchange/Metadata%2FItems%2FDivinationCards%2FDivinationCardKeepersCorruption',
 'Metadata/Items/Currency/AstrolabeGeneric': 'currency-exchange/Metadata%2FItems%2FCurrency%2FAstrolabeGeneric'
}
const assets = new Map()
const itemArt = JSON.parse(fs.readFileSync(new URL('../../../../internal/icons/urls/items.json', import.meta.url)))
const exchangeArt = JSON.parse(fs.readFileSync(new URL('../../../../internal/exchange/itemdata/icon-urls.json', import.meta.url)))
const server = http.createServer(async (req, res) => {
 try {
  if (req.url === '/') { res.end(fs.readFileSync(html)); return }
  const key = req.url.replace(/^\/changed/, '')
  if (!key.startsWith('/api/icon/')) { res.writeHead(404).end(); return }
  if (!assets.has(key)) {
   const [, , , type, encoded] = key.split('/')
   const response = await fetch((type === 'items' ? itemArt : exchangeArt)[decodeURIComponent(encoded)])
   assert.equal(response.status, 200, key)
   assets.set(key, { type: response.headers.get('content-type'), bytes: Buffer.from(await response.arrayBuffer()) })
  }
  const asset = assets.get(key)
  res.writeHead(200, { 'Content-Type': asset.type }).end(asset.bytes)
 } catch (error) { res.writeHead(502).end(String(error)) }
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const origin = `http://127.0.0.1:${server.address().port}`
const browser = spawn(process.env.CHROMIUM_PATH || '/root/.cache/ms-playwright/chromium_headless_shell-1194/chrome-linux/headless_shell', ['--no-sandbox', '--disable-gpu', '--remote-debugging-port=0', 'about:blank'])
const endpoint = await new Promise((resolve, reject) => {
 let output = ''
 browser.stderr.on('data', chunk => { output += chunk; const match = output.match(/DevTools listening on (ws:\/\/\S+)/); if (match) resolve(match[1]) })
 browser.on('error', reject)
})
const ws = new WebSocket(endpoint)
await new Promise(resolve => ws.addEventListener('open', resolve, { once: true }))
let sequence = 0
const pending = new Map()
ws.addEventListener('message', event => { const message = JSON.parse(event.data); if (!message.id) return; const p = pending.get(message.id); pending.delete(message.id); message.error ? p.reject(message.error) : p.resolve(message.result) })
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => { const id = ++sequence; pending.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params, sessionId })) })
const { targetId } = await send('Target.createTarget', { url: 'about:blank' })
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true })
const call = (method, params = {}) => send(method, params, sessionId)
const evaluate = async expression => { const response = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (response.exceptionDetails) throw Error(JSON.stringify(response.exceptionDetails)); return response.result.value }
async function until(expression) {
 for (let attempt = 0; attempt < 100; attempt++) {
  if (await evaluate(expression)) return
  await new Promise(resolve => setTimeout(resolve, 100))
 }
 throw Error('Timed out: ' + expression)
}
function urls(base) { return Object.fromEntries(Object.entries(expected).map(([id, suffix]) => [id, `${base}/api/icon/${suffix}`])) }
const readIcons = () => evaluate(`Object.fromEntries([...document.querySelectorAll('[data-action="loot.toggle"]')].map(button=>[button.dataset.item,button.querySelector('img')?.getAttribute('src') ?? '[image request failed]']))`)
try {
 await call('Page.enable')
 await call('Emulation.setDeviceMetricsOverride', { width: 1000, height: 1050, deviceScaleFactor: 1, mobile: false })
 await call('Page.navigate', { url: origin })
 await until(`window.ready === true && document.querySelectorAll('[data-action="loot.toggle"]').length === 13`)
 // No event is delivered: startup must read saved settings in this WebView.
 await evaluate('window.deliverStatus?.(); new Promise(resolve=>setTimeout(resolve,100))')
 assert.deepEqual(await readIcons(), urls(origin), 'all 13 overlay URLs use the saved server')
 assert.deepEqual((await evaluate('window.renderResults()')).sort(), [...Object.values(urls(origin)), ...Object.values(urls(origin))].sort(), 'both main-window Results layouts render the independent expected URLs')
 console.log('PASS: 13 item URLs match the saved server on both surfaces after delayed startup')
 await until('[...document.querySelectorAll("img")].length === 13 && [...document.querySelectorAll("img")].every(i=>i.complete && i.naturalWidth>0)')
 assert.equal(await evaluate('document.querySelectorAll(".item-icon-fallback").length'), 0)
 fs.writeFileSync(`${evidence}/overlay-icons.png`, Buffer.from((await call('Page.captureScreenshot', { format: 'png' })).data, 'base64'))
 console.log('PASS: 13 real item images decoded; screenshot captured')
 await evaluate(`window.handlers['status-changed']({payload:{server_url:${JSON.stringify(origin + '/changed')}}})`)
 await until(`document.querySelector('img')?.src.startsWith(${JSON.stringify(origin + '/changed')})`)
 assert.deepEqual(await readIcons(), urls(origin + '/changed'))
 console.log('PASS: rendered icon URLs follow a changed saved server')
 await evaluate('window.destroy()')
 assert.equal(await evaluate('!!window.handlers["status-changed"] || !!window.handlers["logs-changed"]'), false)
 console.log('PASS: overlay teardown releases status listeners')
 assert.deepEqual(await evaluate('window.errors'), [])
 console.log('PASS: no browser errors or unhandled rejections')
 await evaluate('window.ready=false')
 await call('Page.navigate', { url: origin })
 await until('window.ready === true')
 await evaluate('window.destroy(); new Promise(resolve=>setTimeout(resolve,100))')
 await evaluate('window.deliverStatus()')
 await until('!window.handlers["status-changed"] && !window.handlers["logs-changed"]')
 assert.equal(await evaluate('document.querySelectorAll("[data-action]").length'), 0)
 console.log('PASS: closing before saved status arrives releases late listeners')
} finally {
 await send('Browser.close')
 ws.close()
 browser.kill()
 server.closeAllConnections()
 await new Promise(resolve => server.close(resolve))
}
