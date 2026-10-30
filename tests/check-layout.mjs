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
const mobile = process.env.BLUEDOOR_TEST_PLATFORM === "android";
const browser=await browserType.launch();
try {
 const page=await browser.newPage({viewport:{width:1440,height:1000}});
 page.setDefaultTimeout(15000);
 await page.addInitScript(({pages,originalPages,editedPages,redactions,mobile})=>{
  localStorage.setItem('bluedoor-language','es');localStorage.setItem('bluedoor-privacy-model','qwen3-fast');localStorage.setItem('bluedoor-welcome-seen','true');
  const entities=[{kind:'PERSON',original:'María Pérez',alias:'PERSON_01',source:'review'},{kind:'EMAIL',original:'maria@example.com',alias:'EMAIL_01',source:'review'},{kind:'ID',original:'0912345678',alias:'ID_01',source:'review'}];
  const analyzedSession={id:'layout-1',revision:3,fileName:'layout-document.pdf',original:'María Pérez maria@example.com 0912345678',entities,sanitized:'PERSON_01 EMAIL_01 ID_01',warnings:[],advisories:[],analyzed:true,analysisComplete:true,reviewed:false,edited:false,layoutAvailable:true};
  let session=null;
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
    return {id:session.id,revision:session.revision,pages:!session.analyzed?originalPages:edited?editedPages:pages,redactions:(session.analyzed?redactions:[]).map(a=>({...a,alias:a.entityIndex===0&&a.alias?session.entities[0].alias:a.alias})),estimated:false};
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
 await page.goto('http://localhost:1420');
 await page.locator('.welcome[role="dialog"] .close').click();
 assert(await page.locator('#editToggle').isDisabled());
 await page.getByRole('button',{name:'Importar documento',exact:true}).click();
 assert(await page.locator('#editToggle').isDisabled());
 assert.equal(await page.getByRole('combobox',{name:'Zoom',exact:true}).count(),1,'one zoom control on import');
 assert.equal(await page.getByRole('combobox',{name:'Ampliación del original',exact:true}).count(),0,'no second source zoom selector');
 await page.getByRole('img',{name:'Página 1',exact:true}).waitFor();
 const zoom=page.getByRole('combobox',{name:'Zoom',exact:true});
 async function realPinch() {
  if (browser.browserType().name() === 'chromium') {
   const client=await page.context().newCDPSession(page);
   await client.send('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:2});
   const center=await page.locator('.page-wrapper').first().evaluate(el=>{const r=el.getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+120};});
   const points=offset=>[{x:center.x-offset,y:center.y,id:1},{x:center.x+offset,y:center.y,id:2}];
   await client.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:points(40)});
   await client.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:points(70)});
   await page.waitForTimeout(80);
   await client.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
   assert.notEqual(await zoom.inputValue(),'fit','real Chromium two-finger input zooms the document');
   assert.equal(await page.evaluate(()=>visualViewport.scale),1,'real pinch keeps application chrome at its original scale');
   await client.detach();await zoom.selectOption('fit');
  }
 }
 async function checkFit(scrollSelector) {
  assert.equal(await zoom.inputValue(),'fit','fit to width is the default');
  await page.waitForFunction(selector=>{
   const viewer=document.querySelector(selector);
   if(!viewer)return false;
   const style=getComputedStyle(viewer);
   const available=viewer.clientWidth-parseFloat(style.paddingLeft)-parseFloat(style.paddingRight);
   const widest=Math.max(...Array.from(viewer.querySelectorAll('.page-wrapper')).map(el=>el.getBoundingClientRect().width));
   return Math.abs(available-widest)<1 && viewer.scrollWidth<=viewer.clientWidth;
  },scrollSelector);
 }
 await checkFit('.page-scroll');
 if (mobile) {
  await page.setViewportSize({width:412,height:900});await checkFit('.page-scroll');
  const width=await page.locator('.page-wrapper').first().evaluate(el=>el.getBoundingClientRect().width);
  await page.locator('.page-wrapper img').first().evaluate(async el=>{
   const r=el.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+100;
   const fire=(type,offset)=>{const event=new Event(type,{bubbles:true,cancelable:true});Object.defineProperty(event,'touches',{value:offset?[{clientX:x-offset,clientY:y},{clientX:x+offset,clientY:y}]:[]});el.dispatchEvent(event);};
   fire('touchstart',35);fire('touchmove',70);await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));fire('touchend',0);
  });
  assert(Math.abs((await page.locator('.page-wrapper').first().evaluate(el=>el.getBoundingClientRect().width))/width-2)<.02,'pinch zoom works immediately after import');
  assert.notEqual(await zoom.inputValue(),'fit');
  await zoom.selectOption('fit');await checkFit('.page-scroll');
  await realPinch();await checkFit('.page-scroll');
 }
 await page.setViewportSize({width:1200,height:1000});
 await checkFit('.page-scroll');
 await zoom.selectOption('125');
 await page.setViewportSize({width:1440,height:1000});
 assert.equal(await zoom.inputValue(),'125','window resizing preserves manual zoom');
 await zoom.selectOption('fit');
 await checkFit('.page-scroll');

 async function checkZoomViewport(scrollSelector) {
  const bounds=await zoom.boundingBox();
  assert(bounds.x>=0 && bounds.x+bounds.width<=1440,'zoom control remains inside the window at high zoom');
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth),'page zoom cannot widen the app shell');
  const scroll=page.locator(scrollSelector);
  assert(await scroll.evaluate(el=>el.scrollWidth>el.clientWidth),'large pages scroll horizontally inside the viewer');
  const header=await page.locator('.workspace-header').boundingBox();
  await scroll.evaluate(el=>{el.scrollLeft=el.scrollWidth;el.scrollTop=100;});
  assert(await scroll.evaluate(el=>el.scrollLeft>0),'document can pan to its right edge');
  const after=await page.locator('.workspace-header').boundingBox();
  assert.equal(after.x,header.x);assert.equal(after.y,header.y);assert.equal(after.width,header.width,'panning never moves the controls');
 }
 for(const level of ['600','800']) {await zoom.selectOption(level);await checkZoomViewport('.page-scroll');}
 await zoom.selectOption('100');
 assert.equal(await page.getByRole('button',{name:'Exportar PDF'}).count(),0,'export belongs to Edit');
 assert.equal(await page.getByRole('button',{name:'Mapa privado',exact:true}).count(),0,'import stage has no duplicate map controls');
 await page.getByRole('button',{name:'Proteger documento',exact:true}).click();
 await page.getByRole('button',{name:'Exportar PDF',exact:true}).waitFor();
 assert(await page.locator('#editToggle').evaluate(el=>el.classList.contains('nav-active')),'protection leads to Edit');
 await checkFit('.page-editor .viewer-body');
 await page.setViewportSize({width:1200,height:1000});
 await checkFit('.page-editor .viewer-body');
 await page.setViewportSize({width:1440,height:1000});
 await checkFit('.page-editor .viewer-body');
 assert.equal(await page.getByRole('button',{name:'Mapa privado',exact:true}).count(),1,'exactly one map entry point in Edit');
 await zoom.selectOption('800');await checkZoomViewport('.page-editor .viewer-body');await zoom.selectOption('100');
 assert.equal(await page.locator('[contenteditable=true]').count(),0,'PDF Edit must not flatten into the transcript editor');
 assert.equal(await page.getByRole('img',{name:'Página 1',exact:true}).getAttribute('src'),'data:image/png;base64,'+pages[0].imageBase64,'Edit uses the native page rendering with original margins and graphics');
 assert.equal(await page.locator('.page-wrapper').first().evaluate(el=>el.style.width),`${pages[0].width}px`,'original page width is passed unchanged to CSS');
 // WebKit represents layout coordinates in 1/64 CSS-pixel units.
 assert.equal(await page.locator('.page-wrapper').first().evaluate(el=>el.getBoundingClientRect().width),Math.floor(pages[0].width*64)/64,'rendered width matches WebKit subpixel quantization');
 await page.getByRole('img',{name:'Página 1',exact:true}).waitFor();
 assert.equal(await page.getByRole('img',{name:'Página 2',exact:true}).count(),1,'all physical pages shown');
 if (mobile) {
  await page.setViewportSize({width:412,height:900});
  await zoom.selectOption('fit');
  await checkFit('.page-editor .viewer-body');
  const rows = await page.locator('.editor-toolbar').evaluate(el => {
   const box = selector => { const r=el.querySelector(selector).getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height}; };
   return {font:box('.tb-font'),size:box('.tb-size'),styles:box('.style-controls'),actions:box('.toolbar-actions'),bar:el.getBoundingClientRect().toJSON()};
  });
  await page.screenshot({path:'artifacts/layout-workflow/ui-mobile-toolbar.png',fullPage:true});
  assert(Math.abs(rows.font.y-rows.size.y)<2,'font and size share the first mobile row');
  const zoomBox=await zoom.boundingBox();
  const titleBox=await page.locator('.workspace-title').boundingBox();
  assert(zoomBox.x>=titleBox.x+titleBox.width && Math.abs(zoomBox.y-titleBox.y)<30,'zoom sits beside the file name');
  assert(rows.styles.y>=rows.font.y+rows.font.height,'format controls occupy the second row');
  assert(rows.actions.y>=rows.styles.y+rows.styles.height,'save actions occupy the third row');
  assert(zoomBox.x+zoomBox.width<=412,'zoom fits the phone');
  const initial=await page.locator('.page-wrapper').first().evaluate(el=>el.getBoundingClientRect().width);
  await page.locator('.page-wrapper img').first().evaluate(async el => {
   const r=el.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+Math.min(140,r.height/2);
   const fire=(type,points)=>{const event=new Event(type,{bubbles:true,cancelable:true});Object.defineProperty(event,'touches',{value:points.map(([clientX,clientY])=>({clientX,clientY}))});el.dispatchEvent(event);return event.defaultPrevented;};
   if(fire('touchstart',[[x,y]]))throw Error('one-finger scrolling must remain native');
   if(!fire('touchstart',[[x-40,y],[x+40,y]]))throw Error('pinch must not zoom the WebView');
   fire('touchmove',[[x-80,y],[x+80,y]]);
   await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
   fire('touchend',[]);
  });
  const enlarged=await page.locator('.page-wrapper').first().evaluate(el=>el.getBoundingClientRect().width);
  assert(Math.abs(enlarged/initial-2)<.02,'pinch doubles document scale');
  assert.notEqual(await zoom.inputValue(),'fit','pinch exits fit mode and updates the zoom control');
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'pinch does not widen the app shell');
  await page.screenshot({path:'artifacts/layout-workflow/ui-mobile-toolbar-pinch.png',fullPage:true});
  await zoom.selectOption('fit');await checkFit('.page-editor .viewer-body');
  await realPinch();
  await page.setViewportSize({width:1440,height:1000});await zoom.selectOption('100');
 }
 await page.getByRole('button',{name:'Editar alias PERSON_01, Página 1',exact:true}).first().click();
 await page.getByRole('textbox',{name:'Alias en el documento'}).fill('CLIENTE_01');
 assert(await page.getByRole('button',{name:'Exportar PDF'}).isDisabled(),'unapplied map cannot export');
 assert(await page.getByRole('button',{name:'Documentos',exact:true}).isDisabled(),'unsaved map blocks navigation');
 await page.locator('.alias-actions').getByRole('button',{name:'Guardar cambios',exact:true}).click();
 await page.getByRole('button',{name:'Editar alias CLIENTE_01, Página 1',exact:true}).first().waitFor();
 assert.equal(await page.getByRole('button',{name:'Editar alias CLIENTE_01, Página 2',exact:true}).count(),1,'repeat uses same edited alias');
 if(mobile) await page.getByRole('button',{name:'Aprobar revisión',exact:true}).click();
 await page.getByRole('button',{name:'Exportar PDF'}).click();
 await page.getByRole('status').filter({hasText:'PDF guardado con el formato original.'}).waitFor();
 const exported=await page.evaluate(()=>window.__exportedLayout);
 assert.equal(exported.entities[0].alias,'CLIENTE_01');assert.equal(exported.reviewed,true);
 const calls=await page.evaluate(()=>window.__layoutCalls);
 assert(calls.some(c=>c.cmd==='review_private_layout'&&c.args.confirm===true));
 assert.equal(calls.filter(c=>c.cmd==='export_private_layout').length,1);
 assert(!calls.some(c=>c.cmd==='generate_pdf'),'layout workflow uses the original-page exporter');
 await page.screenshot({path:'artifacts/layout-workflow/ui-layout.png',fullPage:true});
 console.log('PASS import → protect → Edit → export: native page images/width preserved, one map control, drafts survive navigation, repeated aliases, reviewed native export');
}finally{await browser.close();}
