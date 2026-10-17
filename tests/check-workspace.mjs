import { webkit } from 'playwright';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';

// Exercise the real UI with a deterministic IPC boundary, including rejected
// saves, stale revisions, repeated aliases and both supported document surfaces.
const language = process.env.BLUEDOOR_TEST_LANG || 'es';
const english = language === 'en';
const label = (es, en) => english ? en : es;
const browser = await webkit.launch();
mkdirSync('artifacts/workspace', { recursive: true });
try {
 for (const layout of [true, false]) {
  const page = await browser.newPage({viewport:{width:1440,height:900}});
  page.setDefaultTimeout(15000);
  const errors=[];
  page.on('pageerror',error=>{errors.push(error.message);console.error('PAGE ERROR:',error.message);});
  await page.addInitScript(({layout,language})=>{
   localStorage.setItem('bluedoor-language',language);
   localStorage.setItem('bluedoor-welcome-seen','true');
   let session=null;
   const original='Cláusula original. María Ejemplo firma el acuerdo.';
   let blocks=[{id:0,page:0,rect:[40,100,400,35],bounds:[40,100,400,35],html:'<p>Cláusula original.</p>',font:'georgia',size:16,align:'left'}];
   window.__workspaceCalls=[];
   const revision=args=>{if(!session||args.id!==session.id||args.revision!==session.revision)throw Error('Stale revision');};
   window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,invoke:async(cmd,args)=>{
    window.__workspaceCalls.push({cmd,args});
    if(['check_installation','save_transcripts','plugin:event|unlisten'].includes(cmd))return;
    if(cmd==='plugin:event|listen')return 1;
    if(cmd==='load_transcripts')return [];
    if(cmd==='list_models')return {whisper:[{installed:true}],parakeet:[],privacy:[{installed:true}]};
    if(cmd==='get_privacy_session')return structuredClone(session);
    if(cmd==='import_private_document'){session={id:'workspace',revision:0,fileName:layout?'ejemplo.pdf':'ejemplo.txt',original,entities:[],sanitized:'',warnings:[],advisories:[],analyzed:false,analysisComplete:false,reviewed:false,edited:false,layoutAvailable:layout};return structuredClone(session);}
    if(cmd==='analyze_private_document'){revision(args);session.entities=[{kind:'PERSON',original:'María Ejemplo',alias:'PERSONA_1',source:'review'}];session.sanitized=original.replace('María Ejemplo','PERSONA_1');session.analyzed=true;session.analysisComplete=true;session.revision++;return structuredClone(session);}
    if(cmd==='preview_private_layout'){revision(args);return {id:session.id,revision:session.revision,estimated:false,pages:[{width:600,height:700,imageBase64:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aV1cAAAAASUVORK5CYII='}],blocks:structuredClone(blocks),redactions:session.analyzed?[{page:0,entityIndex:0,rect:[40,200,160,25],alias:session.entities[0].alias}]:[]};}
    if(cmd==='save_private_layout_text'){revision(args);if(args.edit.html.includes('No cabe'))throw Error('El texto corregido no cabe en su zona.');blocks[0]={...blocks[0],...args.edit};session.revision++;session.edited=true;session.reviewed=false;window.__savedBlock=structuredClone(args.edit);return structuredClone(session);}
    if(cmd==='update_private_document'){revision(args);session.sanitized=args.text;session.edited=true;session.reviewed=false;session.revision++;return structuredClone(session);}
    if(cmd==='review_private_layout'||cmd==='review_private_document'){revision(args);if(!args.entities.length)throw Error('Empty map');const changed=JSON.stringify(args.entities)!==JSON.stringify(session.entities);session.entities=structuredClone(args.entities);if(changed)session.sanitized=original.replace('María Ejemplo',session.entities[0].alias);session.reviewed=!!args.confirm;session.revision++;window.__savedMap=structuredClone(session);return structuredClone(session);}
    if(cmd==='validate_private_export'){revision(args);if(!session.reviewed||args.text!==session.sanitized)throw Error('Unreviewed export');return;}
    if(cmd==='choose_pdf_destination')return '/tmp/workspace.pdf';
    if(cmd==='export_private_layout'){revision(args);if(!session.reviewed)throw Error('Unreviewed export');window.__exported=true;return args.destPath;}
    if(cmd==='generate_pdf'){if(!session.reviewed)throw Error('Unreviewed export');window.__exported=true;return args.destPath;}
    throw Error('Unexpected command '+cmd);
   }};
  },{layout,language});
  await page.goto('http://localhost:1420');
  await page.getByRole('button',{name:label('Importar documento','Import document'),exact:true}).click();
  assert.equal(await page.getByRole('heading',{name:label('Lo privado permanece aquí.','Your private information stays here.'),exact:true}).count(),0,'welcome copy leaves the functional workspace');
  assert.equal(await page.getByRole('button',{name:label('Ver texto','View text'),exact:true}).count(),0,'no route to legacy comparison');
  await page.getByRole('button',{name:label('Proteger documento','Protect document'),exact:true}).click();
  const map=page.getByRole('button',{name:label('Mapa privado','Private map'),exact:true});
  await map.waitFor();
  assert.equal(await map.count(),1,'one map entry point');
  assert.equal(await page.getByRole('button',{name:label('Proteger documento','Protect document'),exact:true}).count(),0,'protection is not duplicated inside Edit');
  assert.equal(await page.getByRole('button',{name:label('Editar aliases','Edit aliases'),exact:true}).count(),0,'aliases are selected directly on the page');
  const editor=page.locator('.rich-editor');
  if(layout)await page.getByRole('button',{name:label('Editar texto 1, Página 1','Edit text 1, Page 1'),exact:true}).click();
  await editor.waitFor({state:'visible'});
  if(layout){
   assert.equal(await editor.evaluate(el=>!!el.closest('.paper-content')),true,'native text remains on its original page');
   await editor.fill('No cabe');
   assert(await map.isDisabled(),'map navigation cannot discard text draft');
   await page.locator('.save-btn').click();
   await page.locator('.feedback-host .editor-feedback').filter({hasText:'no cabe'}).waitFor();
   assert.equal(await editor.innerText(),'No cabe','rejected save preserves the draft');
  }
  await editor.fill('Cláusula corregida. PERSONA_1 firma el acuerdo.');
  assert(await map.isDisabled());
  await page.locator('.save-btn').click();
  await map.waitFor();
  await page.waitForFunction(()=>!document.querySelector('.workspace-views button:last-child').disabled);
  const bar=page.locator('.editor-toolbar');
  assert.equal(await page.locator('.page-editor .viewer-head').count(),0,'no duplicate action toolbar');
  // Repeated viewport changes exercise ResizeObserver delivery on WebKit;
  // the strict pageerror assertion below must catch resize feedback loops.
  for(const width of [1440,960,1200,960,1440,1000,1440,960]){
   await page.setViewportSize({width,height:900});
   await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
   const geometry=await bar.evaluate(el=>{const align=el.querySelector('.tb-align').getBoundingClientRect(),actions=el.querySelector('.editor-actions').getBoundingClientRect(),r=el.getBoundingClientRect();return {alignTop:align.top,actionTop:actions.top,right:actions.right,barRight:r.right,scroll:el.scrollWidth,width:el.clientWidth};});
   assert(Math.abs(geometry.alignTop-geometry.actionTop)<8,'actions share the alignment row');
   assert(geometry.right<=geometry.barRight+1&&geometry.scroll<=geometry.width+1,'toolbar fits at the minimum app width');
  }
  await page.setViewportSize({width:1440,height:900});
  await map.click();
  const alias=page.getByRole('textbox',{name:'Alias 1',exact:true});
  await alias.fill('CLIENTE_1');
  const document=page.getByRole('button',{name:label('Documento','Document'),exact:true});
  assert(await document.isDisabled(),'map draft cannot be lost on view switch');
  await page.locator('.map-page').getByRole('button',{name:label('Guardar cambios','Save changes'),exact:true}).click();
  await page.waitForFunction(()=>window.__savedMap?.entities[0].alias==='CLIENTE_1');
  await document.click();
  if(layout){
   await page.getByRole('button',{name:label('Editar alias CLIENTE_1, Página 1','Edit alias CLIENTE_1, Page 1'),exact:true}).click();
   await page.getByRole('textbox',{name:label('Alias en el documento','Alias in the document'),exact:true}).fill('CLIENTE_2');
   const exportButton=page.getByRole('button',{name:label('Exportar PDF','Export PDF'),exact:true});
   assert(await exportButton.isDisabled(),'unsaved alias cannot export');
   await page.locator('.alias-actions').getByRole('button',{name:label('Guardar cambios','Save changes'),exact:true}).click();
   await page.waitForFunction(()=>window.__savedMap?.entities[0].alias==='CLIENTE_2');
  }
  const settings=page.locator('.settings-menu summary');
  await settings.click();
  assert(await page.locator('#modelsToggle').isVisible());
  await page.locator('#themeToggle').click();
  assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
  await page.locator('#themeToggle').click();
  await page.locator('#languageToggle').click();
  assert.equal(await page.locator('.workspace-views button:last-child').getAttribute('aria-label'),english?'Mapa privado':'Private map');
  await page.locator('#languageToggle').click();
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.settings-menu').evaluate(el=>el.open),false);
  await page.getByRole('button',{name:label('Exportar PDF','Export PDF'),exact:true}).click();
  await page.waitForFunction(()=>window.__exported);
  const calls=await page.evaluate(()=>window.__workspaceCalls);
  assert(calls.some(c=>c.cmd===(layout?'review_private_layout':'review_private_document')&&c.args.confirm===true),'export still confirms native review');
  assert.equal(calls.filter(c=>c.cmd==='analyze_private_document').length,1,'no extra protection from Edit');
  assert.equal(calls.filter(c=>c.cmd==='open_server_home').length,0,'private workflow stays local');
  assert.deepEqual(errors,[],'no runtime errors');
  await page.screenshot({path:`artifacts/workspace/${layout?'original-page':'text-page'}-${language}.png`,fullPage:true});
  await page.close();
 }
 console.log(`PASS unified workspace (${language}): original page + text page, drafts, save rejection, map/alias editing, compact toolbar, settings and reviewed local export`);
} catch(error) {
 for(const context of browser.contexts()) for(const page of context.pages()) { console.error((await page.locator("body").innerText()).slice(0,4000)); await page.screenshot({path:"artifacts/workspace/failure.png",fullPage:true}); }
 throw error;
} finally { await browser.close(); }
