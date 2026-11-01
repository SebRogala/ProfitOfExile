// Run from desktop: node --input-type=module < src/lib/overlay/widgets/overlay-actions.characterization-render.mjs
// Compiles the actual routes; only the native boundary is substituted.
import { build, transform } from 'esbuild'
import { compile, compileModule } from 'svelte/compiler'
import fs from 'node:fs'
const program = `
import {mount,tick} from 'svelte';
import Lab from './src/routes/overlay/lab/+page.svelte';
import Temple from './src/routes/overlay/temple/+page.svelte';
const params=new URLSearchParams(location.search);const label=params.get('module')||'lab';
window.calls=[];window.emissions=[];window.listeners={};window.bareListeners={};window.errors=[];
window.data={results:[{name:'First',variant:'20/20',roi:17,transPrice:10,low7d:9,high7d:11,confidence:'HIGH',recommendation:'OK',priceTier:'MID',signal:'STABLE'},{name:'Second',variant:'1/20',roi:29,transPrice:30,low7d:28,high7d:32,confidence:'HIGH',recommendation:'BEST',priceTier:'HIGH',signal:'STABLE'}],tradeData:{},tradeLoading:{}};
window.nativeWindow={label,scaleFactor:()=>new Promise(resolve=>{window.resolveScale=resolve;if(!params.has('zero'))resolve(1.5)}),listen:async(name,callback)=>{window.listeners[name]=callback;return()=>{delete window.listeners[name]}},emit:async(name,payload)=>window.emissions.push([name,payload])};
window.harnessInvoke=async(command,args)=>{window.calls.push([command,args]);
 if(command==='get_widget_geometries')return (label==='lab'?['lab.comparator','lab.compass','lab.timer','lab.pathstrip']:['temple.door','temple.waiting','temple.offers']).map(id=>({id,geometry:{x:30,y:500,width:0,height:0,visible:id==='lab.comparator'||id==='temple.door'}}));
 if(command==='get_comparator_data')return JSON.parse(JSON.stringify(window.data));
 if(command==='get_overlay_config_mode')return false;
 if(command==='get_status')return {};
 return null;
};
window.fire=(name,payload)=>window.listeners[name]?.({payload});
window.forward=selector=>{const node=document.querySelector(selector);if(!node)throw Error('Missing '+selector);const r=node.getBoundingClientRect();window.fire('overlay-click',{label,x:(r.x+r.width/2)*1.5,y:(r.y+r.height/2)*1.5})};
window.fetch=async()=>{throw Error('No live HTTP in characterization')};
window.addEventListener('error',event=>window.errors.push(event.message));
window.addEventListener('unhandledrejection',event=>window.errors.push(String(event.reason)));
mount(label==='lab'?Lab:Temple,{target:document.body});await tick();window.ready=true;
`;
const tauri = `export const invoke=(command,args)=>Promise.resolve().then(()=>window.harnessInvoke(command,args===undefined?undefined:JSON.parse(JSON.stringify(args))));export const listen=async(name,callback)=>{window.bareListeners[name]=callback;return()=>{}};export const getCurrentWebviewWindow=()=>window.nativeWindow;export const emit=async()=>{};export const getVersion=async()=>'test';export const check=async()=>null;export class Update{};`;
const result = await build({stdin:{contents:program,resolveDir:process.cwd(),sourcefile:'overlay-actions-characterization.ts'},bundle:true,platform:'browser',format:'esm',write:false,alias:{'$lib':process.cwd()+'/src/lib'},conditions:['browser'],define:{'import.meta.env':'{}'},plugins:[{name:'native-boundary',setup(b){
 b.onResolve({filter:/^@tauri-apps\//},a=>({path:a.path,namespace:'tauri'}));
 b.onLoad({filter:/.*/,namespace:'tauri'},()=>({contents:tauri,loader:'js'}));
 b.onResolve({filter:/^\$app\/environment$/},a=>({path:a.path,namespace:'env'}));
 b.onLoad({filter:/.*/,namespace:'env'},()=>({contents:'export const browser=true;export const dev=false;export const building=false;',loader:'js'}));
 b.onLoad({filter:/\.css$/},()=>({contents:'',loader:'js'}));
 b.onLoad({filter:/\.svelte\.ts$/},async a=>({contents:compileModule((await transform(fs.readFileSync(a.path,'utf8'),{loader:'ts'})).code,{filename:a.path,generate:'client'}).js.code,loader:'js'}));
 b.onLoad({filter:/\.svelte$/},a=>({contents:compile(fs.readFileSync(a.path,'utf8'),{filename:a.path,generate:'client',css:'injected'}).js.code,loader:'js'}));
}}]});
console.log('<!doctype html><html><head><style>'+fs.readFileSync('src/tokens.css','utf8')+'html,body{margin:0;background:transparent}</style></head><body><script type="module">'+result.outputFiles[0].text+'</script></body></html>');
