import { browserType } from './browser.mjs';
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
const lang=process.env.BLUEDOOR_TEST_LANG||'es';
const english=JSON.parse(readFileSync('src/lib/private-i18n.ts','utf8').split('= {')[1].split('\n};')[0].replace(/^/,'{')+'}');
const ui=s=>lang==='en'?(english[s]??s):s;
const mobile=process.env.BLUEDOOR_TEST_MOBILE==='1';
const layout=process.env.BLUEDOOR_TEST_BATCH_LAYOUT==='1';
const pages=layout?JSON.parse(readFileSync('artifacts/layout-workflow/layout-document.pdf.preview.json','utf8')):[];
const browser=await browserType.launch();
try {
 const page=await browser.newPage({viewport:{width:mobile?412:1280,height:900},reducedMotion:'reduce'});
 page.setDefaultTimeout(10000);
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(({lang,mobile,layout,pages})=>{
  localStorage.setItem('bluedoor-language',lang);localStorage.setItem('bluedoor-welcome-seen','true');
  let session=null,batch=null;const sessions=new Map();let concurrent=0;
  window.__batchCalls=[];window.__batchExported=[];
  const snapshot=()=>{
   if(batch?.active&&session)sessions.set(batch.active,structuredClone(session));
   return batch?{...structuredClone(batch),items:batch.items.map(i=>{
    const s=sessions.get(i.id);return {...i,sessionId:s?.id??null,status:s?(s.reviewed?'approved':'review'):i.status};
   })}:null;
  };
  let callbacks=0;
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>++callbacks,invoke:async(cmd,args)=>{
   window.__batchCalls.push({cmd,args});
   if(cmd==='app_platform')return mobile?'android':'macos';
   if(cmd==='list_models')return {whisper:[],parakeet:[],diarization:[],privacy:[{id:mobile?'gliner-pii-small-int8':'qwen3-fast',installed:true,name:'Local AI',size:'86 MB'}]};
   if(cmd==='get_privacy_session')return structuredClone(session);
   if(cmd==='get_private_batch')return snapshot();
   if(cmd==='interrupted_private_session'||cmd==='diarization_available')return false;
   if(cmd==='load_transcripts')return [];
   if(cmd==='save_transcripts'||cmd==='check_installation'||cmd==='plugin:event|unlisten')return;
   if(cmd==='plugin:event|listen')return 1;
   if(cmd==='choose_private_batch'){
    batch={id:'batch-1',started:false,processing:false,active:null,items:['One.txt','Two.txt','Broken.txt','Excluded.txt'].map((fileName,n)=>({id:String(n+1),fileName,status:'pending',error:'',sessionId:null}))};return snapshot();
   }
   if(cmd==='start_private_batch'){batch.items=batch.items.filter(i=>args.selected.includes(i.id));batch.started=true;return snapshot();}
   if(cmd==='process_private_batch_item'){
    if(++concurrent!==1)throw Error('Concurrent analysis');
    const item=batch.items.find(i=>i.id===args.itemId);item.status='processing';
    await new Promise(resolve=>window.__finishBatchItem=resolve);
    if(item.id==='3'){item.status='error';item.error='No se pudo leer el documento.';}
    else {
     const s={id:'session-'+item.id,fileName:item.fileName,revision:1,original:'María Pérez '+item.id,sanitized:'PERSON_01 '+item.id,entities:[{kind:'PERSON',original:'María Pérez',alias:'PERSON_01',source:'review'}],warnings:[],advisories:[],analyzed:true,analysisComplete:true,reviewed:false,edited:false,layoutAvailable:layout};
     sessions.set(item.id,s);item.sessionId=s.id;item.status='review';
    }
    concurrent--;batch.active=null;session=null;return snapshot();
   }
   if(cmd==='select_private_batch_item'){
    snapshot();session=structuredClone(sessions.get(args.id));batch.active=args.id;return structuredClone(session);
   }
   if(cmd==='preview_private_layout')return {id:session.id,revision:session.revision,pages,redactions:[],estimated:false,blocks:[]};
   if(cmd==='get_private_batch_aliases'){
    snapshot();const all=[...sessions.values()];
    return {groups:[{id:'person-1',kind:'PERSON',variants:['María Pérez'],alias:all[0]?.entities[0].alias??'PERSON_01',documents:all.length,reviewed:all.filter(s=>s.reviewed).length}],suggestions:[]};
   }
   if(cmd==='change_private_batch_aliases'){
    snapshot();let documents=0,reviewed=0;
    const alias=args.action.type==='map'?args.action.entities[0].alias:args.action.alias;
    for(const [key,s] of sessions){
     if(s.entities[0].alias===alias)continue;
     documents++;if(s.reviewed)reviewed++;
     if(args.commit){s.entities[0].alias=alias;s.sanitized=alias+' '+key;s.reviewed=false;s.revision++;}
    }
    if(args.commit)session=structuredClone(sessions.get(batch.active));
    return {documents,reviewed,session:args.commit?structuredClone(session):null};
   }
   if(cmd==='review_private_document'||cmd==='review_private_layout'){
    if(args.id!==session.id||args.revision!==session.revision)throw Error('Stale session');
    session.entities=args.entities;session.sanitized=session.entities[0].alias+' '+batch.active;
    session.reviewed=!!args.confirm;session.revision++;return structuredClone(session);
   }
   if(cmd==='cancel_private_batch_pending'){for(const i of batch.items)if(i.status==='pending')i.status='cancelled';return snapshot();}
   if(cmd==='export_private_batch'){
    const view=snapshot();window.__batchExported=view.items.filter(i=>i.status==='approved').map(i=>i.id);return {exported:window.__batchExported.length,failed:0};
   }
   if(cmd==='close_private_batch'){batch=null;session=null;sessions.clear();return null;}
   if(cmd==='get_private_extraction')return session.original;
   throw Error('Unexpected '+cmd);
  }};
 },{lang,mobile,layout,pages});
 await page.goto('http://localhost:1420');
 await page.locator('.blue-import-screen').waitFor();
 assert.equal(await page.locator('.batch-summary,.batch-footer').count(),0,'single-document UI has no batch chrome');
 await page.getByRole('button',{name:ui('Importar documento'),exact:true}).waitFor();
 await page.locator('.batch-entry-menu summary').click();
 await page.getByRole('button',{name:ui('Detectar varios archivos…'),exact:true}).click();
 const dialog=page.locator('.batch-dialog');await dialog.waitFor();
 mkdirSync('artifacts/batch',{recursive:true});
 async function checkDialogLayout(stage) {
  const widths=mobile?[360,412]:[1280];
  for(const width of widths){
   await page.setViewportSize({width,height:900});
   const problems=await dialog.evaluate(root=>[...root.querySelectorAll('.batch-dialog-actions button,.batch-close,.shared-alias-edit button')].flatMap(button=>{
    const box=button.getBoundingClientRect();
    if(!box.width)return [];
    const lines=[];
    const walker=document.createTreeWalker(button,NodeFilter.SHOW_TEXT);
    for(let node=walker.nextNode();node;node=walker.nextNode()){
     if(!node.textContent.trim())continue;
     const range=document.createRange();range.selectNodeContents(node);
     lines.push(...[...range.getClientRects()].map(rect=>Math.round(rect.top)));
    }
    return [...new Set(lines)].length>1||button.scrollWidth>button.clientWidth+1||box.height<44?[button.textContent.trim()]:[];
   }));
   assert.deepEqual(problems,[],`${stage}: single-line labels and full-size buttons at ${width}px`);
   await page.screenshot({path:`artifacts/batch/${lang}-${stage}-${width}.png`,fullPage:true});
  }
 }
 await checkDialogLayout('prepare');
 await dialog.getByRole('checkbox',{name:'Excluded.txt',exact:true}).uncheck();
 await dialog.getByRole('button',{name:ui('Detectar'),exact:true}).click();
 await page.waitForFunction(()=>typeof window.__finishBatchItem==='function');
 await page.locator('.batch-summary').getByText(ui('Analizando {0} de {1} documentos').replace('{0}','1').replace('{1}','3')).waitFor();
 assert(await page.locator('.batch-summary .processing-orbit').isVisible());
 await page.waitForFunction(()=>parseInt(document.querySelector('.batch-summary .processing-elapsed')?.textContent ?? '', 10)>0);
 assert(await page.locator('.batch-summary [role="progressbar"]').isVisible());
 for(let n=0;n<3;n++){
  await page.waitForFunction(n=>window.__batchCalls.filter(c=>c.cmd==='process_private_batch_item').length===n+1,n);
  await page.evaluate(()=>window.__finishBatchItem());
 }
 await page.locator('.batch-footer').waitFor();
 assert.equal(await page.locator('.batch-footer').count(),1,'one batch navigator');
 assert.equal(await page.locator('.batch-summary').count(),0,'completed loader summary is replaced by navigation');
 const navigationBox=await page.locator('.batch-footer').boundingBox();
 const headerBox=await page.locator('.workspace-header').boundingBox();
 const exportRowBox=await page.locator('.review-actions').boundingBox();
 const approvalBox=await page.locator('.approve-review').boundingBox();
 const documentBox=await page.locator('.document-surface').boundingBox();
 assert(navigationBox.y+navigationBox.height<=headerBox.y+1,'batch navigation is above document actions');
 assert(exportRowBox.y+exportRowBox.height<=approvalBox.y+1,'approval is below PDF/GPT');
 assert(approvalBox.y+approvalBox.height<=documentBox.y+1,'approval is above document preview');
 assert.equal(await page.locator('.workspace-title strong').innerText(),'One.txt');
 assert(await page.locator('.approve-review').isEnabled());
 assert(await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).isDisabled(),'batch export requires explicit approval');
 const footer=page.locator('.batch-footer');
 await footer.getByRole('button',{name:ui('Siguiente'),exact:true}).click();
 await page.locator('.workspace-title strong').filter({hasText:'Two.txt'}).waitFor();
 assert(await footer.getByRole('button',{name:ui('Siguiente'),exact:true}).isDisabled(),'navigation skips errors');
 await footer.getByRole('button',{name:ui('Anterior'),exact:true}).click();
 await page.locator('.workspace-title strong').filter({hasText:'One.txt'}).waitFor();
 await page.locator('.approve-review').click();
 await page.locator('.batch-footer-status').filter({hasText:ui('Aprobado')}).waitFor();
 // Aliases remain owned by their document; a draft prevents changing files.
 await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
 await page.getByLabel('Alias 1',{exact:true}).fill('CLIENT_01');
 assert(await footer.getByRole('button',{name:ui('Siguiente'),exact:true}).isDisabled());
 // Dismiss floating notices before interacting with the compact mobile map.
 while (await page.locator('.feedback-host button').count()) await page.locator('.feedback-host button').first().click();
 await page.getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
 await dialog.getByText(ui('Se actualizarán {0} documentos.').replace('{0}','2'),{exact:true}).waitFor();
 await dialog.getByRole('button',{name:ui('Actualizar documentos'),exact:true}).click();
 await page.getByRole('button',{name:ui('Volver al documento'),exact:true}).click();
 await page.locator('.approve-review').click();
 await footer.getByRole('button',{name:ui('Siguiente'),exact:true}).click();
 await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
 assert.equal(await page.getByLabel('Alias 1',{exact:true}).inputValue(),'CLIENT_01','exact identities share aliases after explicit confirmation');
 await page.getByRole('button',{name:ui('Alias del lote'),exact:true}).click();
 const sharedInput=dialog.getByRole('textbox',{name:ui('Alias compartido para {0}').replace('{0}','María Pérez'),exact:true});
 await checkDialogLayout('aliases');
 await sharedInput.fill('CANCELLED_NAME');
 await dialog.getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
 await dialog.getByRole('button',{name:ui('Cancelar'),exact:true}).click();
 assert.equal(await page.evaluate(()=>window.__batchCalls.filter(c=>c.cmd==='change_private_batch_aliases'&&c.args.commit&&c.args.action.type==='rename').length),0,'cancelled shared rename is never committed');
 await dialog.getByRole('button',{name:ui('Listo'),exact:true}).click();
 await page.getByRole('button',{name:ui('Volver al documento'),exact:true}).click();
 await page.locator('.workspace-identity summary').click();
 await page.getByRole('button',{name:ui('Exportar aprobados a…'),exact:true}).click();
 await checkDialogLayout('export');
 await dialog.getByRole('button',{name:ui('Elegir carpeta y exportar'),exact:true}).click();
 await page.waitForFunction(()=>window.__batchExported.length===1);
 assert.deepEqual(await page.evaluate(()=>window.__batchExported),['1'],'only manually approved sessions exported');
 await footer.getByRole('button',{name:ui('Ver lista'),exact:true}).click();
 await checkDialogLayout('list');
 assert.equal(await dialog.locator('.batch-file').count(),3);
 assert(await dialog.getByText(ui('Error'),{exact:true}).isVisible());
 await dialog.getByRole('button',{name:ui('Listo'),exact:true}).click();
 if(mobile){assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));}
 mkdirSync('artifacts/batch',{recursive:true});
 await page.screenshot({path:`artifacts/batch/${lang}-${mobile?'mobile':'desktop'}.png`,fullPage:true});
 await page.locator('.workspace-identity summary').click();
 await page.getByRole('button',{name:ui('Cerrar lote'),exact:true}).click();
 await dialog.getByRole('button',{name:ui('Cerrar lote'),exact:true}).click();
 await page.locator('.blue-import-screen').waitFor();
 assert.equal(await page.locator('.batch-footer,.batch-summary').count(),0);
 // Cancellation preserves the current result and skips all remaining files.
 await page.locator('.batch-entry-menu summary').click();
 await page.getByRole('button',{name:ui('Detectar varios archivos…'),exact:true}).click();
 const before=await page.evaluate(()=>window.__batchCalls.filter(c=>c.cmd==='process_private_batch_item').length);
 await dialog.getByRole('button',{name:ui('Detectar'),exact:true}).click();
 await page.waitForFunction(before=>window.__batchCalls.filter(c=>c.cmd==='process_private_batch_item').length===before+1,before);
 await page.getByRole('button',{name:ui('Cancelar pendientes'),exact:true}).click();
 await page.evaluate(()=>window.__finishBatchItem());
 await page.locator('.batch-footer').waitFor();
 assert.equal(await page.evaluate(()=>window.__batchCalls.filter(c=>c.cmd==='process_private_batch_item').length),before+1,'pending cancellation does not interrupt or start another file');
 await page.locator('.batch-footer').getByRole('button',{name:ui('Ver lista'),exact:true}).click();
 assert.equal(await dialog.getByText(ui('Cancelado'),{exact:true}).count(),3);
 await dialog.getByRole('button',{name:ui('Listo'),exact:true}).click();
 await page.locator('.workspace-identity summary').click();
 await page.getByRole('button',{name:ui('Cerrar lote'),exact:true}).click();
 await dialog.getByRole('button',{name:ui('Cerrar lote'),exact:true}).click();
 assert.deepEqual(errors,[]);
 console.log(`${lang}/${mobile?'mobile':'desktop'}/${layout?'layout':'text'}: batch confirmation, sequential detection, errors, isolated edits, manual approval, approved export and cleanup passed`);
}finally{await browser.close();}
