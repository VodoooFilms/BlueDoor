import { browserType } from './browser.mjs';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
const pages=JSON.parse(readFileSync('artifacts/layout-workflow/layout-document.pdf.preview.json'));
const plan=JSON.parse(readFileSync('artifacts/layout-workflow/layout-document.pdf.plan.json'));
const originalPlan=structuredClone(plan);
originalPlan.redactions=[];
const originalPages=JSON.parse(execFileSync('src-tauri/resources/runtime/bluedoor-layout',['--preview'],{input:JSON.stringify(originalPlan),maxBuffer:64*1024*1024}));
const editedPlan=structuredClone(plan);
for(const area of editedPlan.redactions)if(area.entityIndex===0&&area.alias)area.alias='CLIENTE_01';
const editedPages=JSON.parse(execFileSync('src-tauri/resources/runtime/bluedoor-layout',['--preview'],{input:JSON.stringify(editedPlan),maxBuffer:64*1024*1024}));
const mobile = true;
const browser=await browserType.launch();
try {
 const page=await browser.newPage({viewport:{width:412,height:900},hasTouch:true,isMobile:true,reducedMotion:"reduce"});
 page.setDefaultTimeout(15000);
 await page.addInitScript(({pages,originalPages,editedPages,redactions,mobile})=>{
  localStorage.setItem('bluedoor-language','es');localStorage.setItem('bluedoor-privacy-model','qwen3-fast');localStorage.setItem('bluedoor-welcome-seen','true');
  const entities=[{kind:'PERSON',original:'María Pérez',alias:'PERSON_01',source:'review'},{kind:'EMAIL',original:'maria@example.com',alias:'EMAIL_01',source:'review'},{kind:'ID',original:'0912345678',alias:'ID_01',source:'review'}];
  const analyzedSession={id:'layout-1',revision:3,fileName:'layout-document.pdf',original:'María Pérez maria@example.com 0912345678',entities,sanitized:'PERSON_01 EMAIL_01 ID_01',warnings:[],advisories:[],analyzed:true,analysisComplete:true,reviewed:false,edited:false,layoutAvailable:true};
  let session=null;
  let manualRegions=[];
  window.__layoutCalls=[];
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,invoke:async(cmd,args)=>{
   window.__layoutCalls.push({cmd,args});
   if(cmd==='app_platform') return mobile ? 'android' : 'macos';
   if(cmd==='private_source_page') return {pages:originalPages.length,image:'data:image/png;base64,'+originalPages[args.page].imageBase64};
   if(cmd==='prepare_share_export')return '/tmp/layout-workflow.pdf';
   if(cmd==='share_export')return;
   if(cmd==='check_installation')return;
   if(cmd==='load_transcripts')return [];
   if(cmd==='save_transcripts')return;
   if(cmd==='plugin:event|listen')return 1;
   if(cmd==='plugin:event|unlisten')return;
   if(cmd==='list_models')return {whisper:[{id:'small',path:'/mock/stt',installed:true}],parakeet:[],diarization:[],privacy:[{id:"qwen3-fast",installed:true,name:"Qwen",size:"1 GB"}]};
   if(cmd==='diarization_available')return false;
   if(cmd==='get_privacy_session')return structuredClone(session);
   if(cmd==='import_private_document'){
    session={...structuredClone(analyzedSession),revision:1,analyzed:false,analysisComplete:false,entities:[],sanitized:''};
    return structuredClone(session);
   }
   if(cmd==='analyze_private_document'){
    if(args.id!==session.id||args.revision!==session.revision)throw new Error('Stale analysis');
    session={...structuredClone(analyzedSession),revision:2};return structuredClone(session);
   }
   if(cmd==='preview_private_layout'){
    if(args.id!==session.id||args.revision!==session.revision)throw new Error('Stale preview');
    const edited=session.entities[0]?.alias==='CLIENTE_01';
    return {id:session.id,revision:session.revision,pages:(!session.analyzed?originalPages:edited?editedPages:pages).map((p,i)=>({...p,manualRegions:i===0?manualRegions:[]})),redactions:(session.analyzed?redactions:[]).map(a=>({...a,alias:a.entityIndex===0&&a.alias?session.entities[0].alias:a.alias})),estimated:false};
   }
   if(cmd==='save_private_manual_cover'){
    if(args.revision!==session.revision)throw new Error('Stale cover');
    if(args.rect) manualRegions.push(args.rect); else manualRegions.pop();
    session.reviewed=false;session.edited=manualRegions.length>0;session.revision++;
    return structuredClone(session);
   }
   if(cmd==='review_private_layout'){
    if(args.id!==session.id||args.revision!==session.revision)throw new Error('Stale review');
    session.entities=structuredClone(args.entities);session.sanitized=session.entities.map(e=>e.alias).join(' ');session.reviewed=args.confirm;session.revision++;return structuredClone(session);
   }
   if(cmd==='choose_pdf_destination')return '/tmp/layout-workflow.pdf';
   if(cmd==='export_private_layout'){
    if(!session.reviewed||args.id!==session.id||args.revision!==session.revision)throw new Error('Unreviewed or stale export');
    window.__exportedLayout=structuredClone(session);return args.destPath;
   }
   throw new Error('Unexpected command '+cmd);
  }};
 },{pages,originalPages,editedPages,redactions:plan.redactions,mobile});
 await page.setViewportSize({width:412,height:900});
 await page.goto('http://localhost:1420');
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/ux-welcome-portrait.png'});
 await page.locator('.welcome[role="dialog"] .close').click();
 await page.getByRole('button',{name:'Importar documento',exact:true}).click();
 await page.getByRole('img',{name:'Página 1',exact:true}).waitFor();
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/ux-document-portrait.png',fullPage:true});
 await page.getByRole('button',{name:'Proteger documento',exact:true}).click();
 await page.getByRole('button',{name:'Exportar PDF',exact:true}).waitFor();
 await page.getByRole('button',{name:'Documentos',exact:true}).click();
 await page.locator('.original-toggle').waitFor();
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/ux-protected-portrait.png',fullPage:true});
 await page.locator('.original-toggle').scrollIntoViewIfNeeded();
 await page.screenshot({path:'artifacts/ux-original-toggle.png'});
 assert.equal(await page.locator('.analysis-result').count(),0);
 const positions=await page.evaluate(()=>{
  const box=s=>document.querySelector(s).getBoundingClientRect();
  return {documentBottom:box('.document-surface').bottom, toggleTop:box('.original-toggle').top};
 });
 assert(positions.toggleTop>=positions.documentBottom);
 await page.locator('.document-menu summary').click();
 const menu=await page.locator('.document-menu > div').boundingBox();
 assert(menu.x>=0 && menu.x+menu.width<=412);
 await page.locator('.document-menu summary').click();
 await page.setViewportSize({width:900,height:412});
 await page.evaluate(()=>{document.querySelector('.shell').scrollTop=0;});
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/ux-protected-landscape.png',fullPage:true});
 await page.locator('.document-actions').getByRole('button',{name:'Editar',exact:true}).click();
 await page.locator('.gpt-button').waitFor();
 await page.evaluate(()=>{document.querySelector('.shell').scrollTop=0;});
 assert.equal(await page.locator('.gpt-button').innerText(),'GPT');
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/ux-edit-landscape.png',fullPage:true});
 await page.setViewportSize({width:412,height:900});
 await page.getByRole('button',{name:'Cubrir zona manualmente',exact:true}).click();
 const surface=page.getByRole('button',{name:'Seleccionar cobertura, página 1',exact:true});
 await surface.scrollIntoViewIfNeeded();
 // Keyboard selection exposes accessible coordinates as an alternative to dragging.
 await surface.focus(); await page.keyboard.press('Enter');
 await page.getByRole('spinbutton',{name:'Cobertura X',exact:true}).fill('40');
 await page.getByRole('spinbutton',{name:'Cobertura Y',exact:true}).fill('50');
 assert(await page.getByRole('button',{name:'Exportar PDF',exact:true}).isDisabled());
 await page.screenshot({path:'artifacts/android-manual-cover.png'});
 await page.getByRole('button',{name:'Guardar cobertura',exact:true}).click();
 await page.getByRole('button',{name:'Deshacer última cobertura · página 1',exact:true}).waitFor();
 const saved=await page.evaluate(()=>window.__layoutCalls.filter(c=>c.cmd==='save_private_manual_cover').at(-1));
 assert.equal(saved.args.page,0);assert.equal(saved.args.rect[0],40);assert.equal(saved.args.rect[1],50);
 assert(await page.getByRole('button',{name:'Exportar PDF',exact:true}).isDisabled(),'cover requires fresh approval');
 await page.getByRole('button',{name:'Deshacer última cobertura · página 1',exact:true}).click();
 await page.getByRole('button',{name:'Deshacer última cobertura · página 1',exact:true}).waitFor({state:'detached'});
 assert.equal(await page.evaluate(()=>window.__layoutCalls.filter(c=>c.cmd==='save_private_manual_cover').at(-1).args.rect),null);
 await page.getByRole('button',{name:'Cubrir zona manualmente',exact:true}).click();
 await surface.scrollIntoViewIfNeeded();
 const dragBox=await surface.boundingBox();
 const startX=dragBox.x+20, startY=Math.max(20,dragBox.y+20);
 await page.mouse.move(startX,startY); await page.mouse.down();
 await page.mouse.move(startX+60,startY+30,{steps:5}); await page.mouse.up();
 await page.getByRole('button',{name:'Guardar cobertura',exact:true}).click();
 await page.getByRole('button',{name:'Deshacer última cobertura · página 1',exact:true}).waitFor();
 const dragged=await page.evaluate(()=>window.__layoutCalls.filter(c=>c.cmd==='save_private_manual_cover').at(-1).args.rect);
 assert(dragged[2]>0 && dragged[3]>0,'pointer drag produces a native page rectangle');
 // Settings language selection closes the dropdown as well.
 await page.locator('.settings-menu summary').click();
 await page.locator('#languageToggle').click();
 assert.equal(await page.locator('.settings-menu').evaluate(el=>el.open),false);
 console.log('PASS mobile preview: document toolbar, bounded menu, original toggle below document, GPT label');
} finally { await browser.close(); }
