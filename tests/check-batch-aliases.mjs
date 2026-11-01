import {browserType} from './browser.mjs';
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
const lang=process.env.BLUEDOOR_TEST_LANG||'es';
const mobile=process.env.BLUEDOOR_TEST_MOBILE==='1';
const source=readFileSync('src/lib/private-i18n.ts','utf8');
const english=JSON.parse(source.slice(source.indexOf('= {')+2,source.indexOf('\n};')+2));
const ui=s=>lang==='en'?(english[s]??s):s;
const browser=await browserType.launch();
try{
 const page=await browser.newPage({viewport:{width:mobile?412:1280,height:900},reducedMotion:'reduce'});page.setDefaultTimeout(10000);
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(({lang,mobile})=>{
  localStorage.setItem('bluedoor-language',lang);localStorage.setItem('bluedoor-welcome-seen','true');
  const groups=[{id:'full',kind:'PERSON',variants:['Juan Pérez'],alias:'PERSON_01'},{id:'short',kind:'PERSON',variants:['Sr. Pérez'],alias:'PERSON_02'}];
  const sessions=[1,2,3].map((n)=>({id:'s'+n,fileName:`File-${n}.txt`,revision:1,original:(n===3?'Sr. Pérez':'Juan Pérez')+' firma.',sanitized:(n===3?'PERSON_02':'PERSON_01')+' firma.',entities:[{kind:'PERSON',original:n===3?'Sr. Pérez':'Juan Pérez',alias:n===3?'PERSON_02':'PERSON_01',source:'review'}],analyzed:true,analysisComplete:true,reviewed:n!==2,edited:false,layoutAvailable:false,warnings:[],advisories:[]}));
  let active=0;const calls=[];window.__aliasCalls=calls;
  function view(){return {id:'batch',started:true,processing:false,active:String(active+1),items:sessions.map((s,i)=>({id:String(i+1),fileName:s.fileName,sessionId:s.id,status:s.reviewed?'approved':'review',error:''}))};}
  function catalog(){return {groups:groups.map(g=>{const docs=sessions.filter(s=>s.entities.some(e=>g.variants.includes(e.original)));return {...g,documents:docs.length,reviewed:docs.filter(s=>s.reviewed).length};}),suggestions:groups.length===2?[['full','short']]:[]};}
  let cb=0;
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>++cb,invoke:async(cmd,args)=>{
   calls.push({cmd,args});
   if(cmd==='app_platform')return mobile?'android':'macos';
   if(cmd==='list_models')return {whisper:[],parakeet:[],diarization:[],privacy:[{id:mobile?'gliner-pii-small-int8':'qwen3-fast',installed:true,name:'AI',size:'86 MB'}]};
   if(cmd==='get_privacy_session')return structuredClone(sessions[active]);
   if(cmd==='get_private_batch')return view();
   if(cmd==='get_private_batch_aliases')return catalog();
   if(cmd==='select_private_batch_item'){active=Number(args.id)-1;return structuredClone(sessions[active]);}
   if(cmd==='get_private_extraction')return sessions[active].original;
   if(cmd==='diarization_available'||cmd==='interrupted_private_session')return false;
   if(cmd==='plugin:event|listen')return 1;
   if(cmd==='plugin:event|unlisten'||cmd==='check_installation'||cmd==='save_transcripts')return;
   if(cmd==='load_transcripts')return [];
   if(cmd==='change_private_batch_aliases'){
    const next=structuredClone(groups),action=args.action;
    if(action.type==='merge'){const a=next.find(g=>g.id===action.sourceId),b=next.find(g=>g.id===action.targetId);b.variants.push(...a.variants);next.splice(next.indexOf(a),1);}
    else if(action.type==='rename')next.find(g=>g.id===action.groupId).alias=action.alias;
    else throw Error('Unexpected action');
    let documents=0,reviewed=0;
    for(const s of sessions){const alias=next.find(g=>g.variants.includes(s.entities[0].original)).alias;if(alias===s.entities[0].alias)continue;documents++;if(s.reviewed)reviewed++;if(args.commit){s.entities[0].alias=alias;s.sanitized=alias+' firma.';s.reviewed=false;s.revision++;}}
    if(args.commit)groups.splice(0,groups.length,...next);
    return {documents,reviewed,session:args.commit?structuredClone(sessions[active]):null};
   }
   throw Error('Unexpected '+cmd);
  }};
 },{lang,mobile});
 await page.goto('http://localhost:1420');
 await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
 await page.getByRole('button',{name:ui('Alias del lote'),exact:true}).click();
 const dialog=page.locator('.batch-dialog');
 await dialog.getByRole('heading',{name:ui('Alias del lote'),exact:true}).waitFor();
 assert.equal(await dialog.locator('.shared-identity').count(),2);
 await dialog.getByRole('button',{name:ui('Revisar unión'),exact:true}).click();
 await dialog.getByText(ui('Se actualizará 1 documento.'),{exact:true}).waitFor();
 assert.equal(await page.evaluate(()=>window.__aliasCalls.filter(c=>c.cmd==='change_private_batch_aliases'&&c.args.commit).length),0,'preview never commits a suggestion');
 await dialog.getByRole('button',{name:ui('Confirmar unión'),exact:true}).click();
 await page.waitForFunction(()=>document.querySelectorAll('.shared-identity').length===1);
 const input=dialog.getByRole('textbox',{name:ui('Alias compartido para {0}').replace('{0}','Juan Pérez'),exact:true});
 await input.fill('CLIENTE_01');
 await dialog.getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
 await dialog.getByText(ui('Se actualizarán {0} documentos.').replace('{0}','3'),{exact:true}).waitFor();
 await dialog.getByText(ui('1 documento aprobado volverá a Por revisar.'),{exact:true}).waitFor();
 await dialog.getByRole('button',{name:ui('Actualizar documentos'),exact:true}).click();
 await dialog.getByRole('heading',{name:ui('Alias del lote'),exact:true}).waitFor();
 assert.equal(await input.inputValue(),'CLIENTE_01');
 mkdirSync('artifacts/batch',{recursive:true});await page.screenshot({path:`artifacts/batch/shared-aliases-${lang}-${mobile?'mobile':'desktop'}.png`});
 await dialog.getByRole('button',{name:ui('Listo'),exact:true}).click();
 assert.equal(await page.getByLabel('Alias 1',{exact:true}).inputValue(),'CLIENTE_01');
 assert(await page.locator('.approve-review').isEnabled(),'shared changes invalidate the active approval');
 await page.getByRole('button',{name:ui('Volver al documento'),exact:true}).click();
 assert(await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).isDisabled());
 await page.locator('.batch-footer').getByRole('button',{name:ui('Siguiente'),exact:true}).click();
 await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
 assert.equal(await page.getByLabel('Alias 1',{exact:true}).inputValue(),'CLIENTE_01');
 assert.deepEqual(errors,[]);
 console.log(`${lang}/${mobile?'mobile':'desktop'}: shared-alias panel, merge confirmation, affected-review warning and global rename passed`);
}finally{await browser.close();}
