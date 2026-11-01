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
 assert.equal(await page.locator('.welcome[role="dialog"]').count(),0,'Installed model opens import directly');
 await page.getByRole('button',{name:'Importar documento',exact:true}).click();
 await page.getByRole('img',{name:'Página 1',exact:true}).waitFor();
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/ux-document-portrait.png',fullPage:true});
 for (const lang of ['es', 'en']) {
  if (lang === 'en') {
   await page.locator('.settings-menu summary').click();
   await page.locator('#languageToggle').click();
  }
  for (const width of [320, 360, 412]) {
   await page.setViewportSize({width,height:900});
   if (lang === 'en' && width === 412) await page.screenshot({path:'artifacts/ux-document-portrait-en.png',fullPage:true});
   const buttons = await page.locator('.document-actions > button').evaluateAll(elements => elements.map(el => {
    const rect = el.getBoundingClientRect(), label = el.querySelector('span');
    return {width:rect.width, height:rect.height, fits:el.scrollWidth <= el.clientWidth, labelHeight:label.getBoundingClientRect().height};
   }));
   assert(Math.abs(buttons[0].width-buttons[1].width)<1, `${lang}/${width}: equal action widths`);
   assert(buttons.every(button => button.fits && button.labelHeight < 24), `${lang}/${width}: labels fit one line`);
  }
 }
 await page.locator('.settings-menu summary').click();
 await page.locator('#languageToggle').click();
 await page.setViewportSize({width:412,height:900});
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
 assert.equal(await page.getByRole('button',{name:'Cubrir zona manualmente',exact:true}).count(),0);
 const actions=await page.locator('.review-actions').boundingBox();
 const approval=await page.locator('.approve-review').boundingBox();
 assert(Math.abs(actions.width-approval.width)<2,'Approval spans the action row width');
 assert(approval.y>=actions.y+actions.height,'Approval sits below export and GPT');
 // Settings language selection closes the dropdown as well.
 await page.locator('.settings-menu summary').click();
 await page.locator('#languageToggle').click();
 assert.equal(await page.locator('.settings-menu').evaluate(el=>el.open),false);
 console.log('PASS mobile preview: document toolbar, bounded menu, original toggle below document, GPT label');
} finally { await browser.close(); }
