// Run from desktop: node --input-type=module < src/lib/boss-loot/art.browser-render.mjs
// Compile the shipped route with real stores and icon resolver; replace native I/O only.
import { build, transform } from 'esbuild'
import { compile, compileModule } from 'svelte/compiler'
import fs from 'node:fs'

const program = `
import {mount,tick,unmount} from 'svelte';
import Loot from './src/routes/overlay/loot/+page.svelte';
import Results from './src/lib/boss-loot/BossResultsTables.svelte';
import {buildResultsView} from './src/lib/boss-loot/page-view';
import {ssot} from './src/lib/stores/ssot.svelte';
import {bossLootIcon} from './src/lib/boss-loot/art';
import {createFixtureProfiles} from './src/lib/boss-loot/fixture-adapters';
import fixture from './src/lib/boss-loot/__fixtures__/boss-loot.json';
const profile=(await createFixtureProfiles(fixture.profiles).load())[0];
// A12 supervisor override retains retired Astrolabe coverage after A13 removes Keeper's Corruption.
profile.extra.push({id:'Metadata/Items/Currency/AstrolabeGeneric',name:'Templar Astrolabe',base:'Astrolabe',wikiRate:'—'});
const items=[...profile.groups.flatMap(g=>g.items),...profile.extra];
window.handlers={};window.errors=[];
window.addEventListener('error',e=>window.errors.push(e.message));
window.addEventListener('unhandledrejection',e=>window.errors.push(String(e.reason)));
window.harnessInvoke=async(command,args)=>{
 if(command==='get_status')return await new Promise(resolve=>window.deliverStatus=()=>resolve({server_url:location.origin,device_id:'fixture'}));
 if(command==='get_logs')return [];
 if(command==='boss_loot_profiles')return [profile];
 if(command==='boss_loot_stats')return {runs:0,devices:0,nextRun:1,items:{}};
 if(command==='get_widget_geometries')return [{id:'loot.confirmation',geometry:{x:16,y:16,width:420,height:0,visible:true,host_width:1000,host_height:1000}}];
 if(command==='get_overlay_config_mode')return false;
 return null;
};
window.fetch=async()=>new Promise(()=>{});
ssot.bossLoot={enabled:true,numberFormat:'comma',nowMs:1000,frame:null,rebind:null,
 settings:{layout:'grid',autoSaveSeconds:0,notCounted:['Communion Support'],captureShortcut:{key:2,shift:true},previewShortcut:{key:119,shift:true}},
 pending:{id:1,attribution:{area:profile.area,level:profile.level,seed:'fixture'},league:'fixture',profile,layout:'grid',rect:[0,0,900,750],
 incoming:{'Cinderswallow Urn':1},items:{quantities:{'Cinderswallow Urn':1},counted:items.filter(i=>i.id!=='Communion Support').map(i=>i.id)},readLines:['SILVER FLASK'],unknownLines:['Spidersilk Robe'],ambiguous:[],phase:'open',deadlineMs:null,closeAtMs:null,target:null,forceNew:false,run:1,nextRun:1,mergeBlocked:false,droppedPositiveKeys:[],error:null}};
const app=mount(Loot,{target:document.body});await tick();
window.renderResults=async()=>{
 const target=document.createElement('div');target.hidden=true;document.body.append(target);
 const view=buildResultsView({profile,scope:'own',sort:'name',notCounted:['Communion Support'],stats:{runs:0,devices:0,nextRun:1,items:{}}});
 const results=mount(Results,{target,props:{profile,view,sort:'name',onSort:()=>{},iconFor:item=>bossLootIcon(item.id)}});await tick();
 const urls=[...target.querySelectorAll('img')].map(img=>img.getAttribute('src'));
 await unmount(results);target.remove();return urls;
};
window.destroy=async()=>{await unmount(app);await tick()};
window.ready=true;
`
const tauri = `export const invoke=(command,args)=>Promise.resolve().then(()=>window.harnessInvoke(command,args));export const listen=async(name,handler)=>{window.handlers[name]=handler;return()=>{delete window.handlers[name]}};export const getCurrentWebviewWindow=()=>({label:'loot',scaleFactor:async()=>1,listen,emit:async()=>{}});export const emit=async()=>{};export const getVersion=async()=>'test';export const check=async()=>null;export class Update{};`
const result = await build({stdin:{contents:program,resolveDir:process.cwd(),sourcefile:'loot-icons-browser.ts'},bundle:true,platform:'browser',format:'esm',write:false,alias:{'$lib':process.cwd()+'/src/lib'},conditions:['browser'],define:{'import.meta.env':'{}'},plugins:[{name:'native-boundary',setup(b){
 b.onResolve({filter:/^@tauri-apps\//},a=>({path:a.path,namespace:'tauri'}))
 b.onLoad({filter:/.*/,namespace:'tauri'},()=>({contents:tauri,loader:'js'}))
 b.onResolve({filter:/^\$app\/environment$/},a=>({path:a.path,namespace:'env'}))
 b.onLoad({filter:/.*/,namespace:'env'},()=>({contents:'export const browser=true;export const dev=false;export const building=false;',loader:'js'}))
 b.onLoad({filter:/\.svelte\.ts$/},async a=>({contents:compileModule((await transform(fs.readFileSync(a.path,'utf8'),{loader:'ts'})).code,{filename:a.path,generate:'client'}).js.code,loader:'js'}))
 b.onLoad({filter:/\.svelte$/},a=>({contents:compile(fs.readFileSync(a.path.endsWith('/routes/overlay/loot/+page.svelte') && process.env.LOOT_ROUTE_SOURCE ? process.env.LOOT_ROUTE_SOURCE : a.path,'utf8'),{filename:a.path,generate:'client',css:'injected'}).js.code,loader:'js'}))
}}]})
console.log('<!doctype html><html><head><style>'+fs.readFileSync('src/tokens.css','utf8')+'</style></head><body><script type="module">'+result.outputFiles[0].text+'</script></body></html>')
