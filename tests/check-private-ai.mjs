import { webkit } from 'playwright';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const language = process.env.LOCAL_LEGAL_TEST_LANG || 'es';
const copySource = readFileSync(new URL('../src/lib/private-i18n.ts',import.meta.url),'utf8');
const english = JSON.parse(copySource.slice(copySource.indexOf('= {')+2,copySource.indexOf('\n};')+2));
const ui = text => language === 'en' ? (english[text] || text.replace(/^Eliminar entidad/, 'Remove entity')) : text;


const browser = await webkit.launch();
try {
  const page = await browser.newPage({viewport:{width:1370,height:800}});
  const requests=[];
  page.on('request',r=>requests.push(r.url()));
  await page.addInitScript(lang => { localStorage.setItem('local-legal-language',lang); localStorage.setItem('local-legal-welcome-seen','true'); }, language);
  await page.addInitScript(() => {
    window.isTauri=true;
    const original='María Pérez demanda a Banco Andino. Email: maria@example.com. Reclama USD 1500 el 2026-09-30.';
    // El OCR perdió una letra; la extracción inicial difiere del texto actual.
    const extracted=original.replace('demanda','0demanda');
    let session=null;
    const calls=[];window.__privacyCalls=calls;
    let callbackId=0;const callbacks=new Map();
    const dropCallbacks=new Map();
    window.__emitDrop=(event,payload)=>dropCallbacks.get(event)?.({event,id:1,payload});
    window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:callback=>{callbacks.set(++callbackId,callback);return callbackId;},invoke:async(cmd,args)=>{
      calls.push({cmd,args});
      if(cmd==='check_installation')return;
      if(cmd==='load_transcripts')return [];
      if(cmd==='save_transcripts')return;
      if(cmd==='plugin:event|listen'){
        if(args.event.startsWith('tauri://drag-'))dropCallbacks.set(args.event,callbacks.get(args.handler));
        if(args.event==='private-analysis-progress')window.__emitAnalysisProgress=payload=>callbacks.get(args.handler)?.({event:args.event,id:1,payload});
        return 1;
      }
      if(cmd==='plugin:event|unlisten')return;
      if(cmd==='list_models')return {whisper:[{id:'small',path:'/mock/stt',installed:true}],parakeet:[],diarization:[],privacy:[{installed:true}]};
      if(cmd==='diarization_available')return false;
      if(cmd==='get_privacy_session')return session;
      if(cmd==='get_private_extraction'){assertRevision(args);return extracted;}
      if(cmd==='import_private_document') {
        session={id:'local-1',revision:0,fileName:'private-client-file.txt',original,entities:[],sanitized:'',warnings:[],advisories:[],analyzed:false,analysisComplete:false,reviewed:false};
        return structuredClone(session);
      }
      if(cmd==='analyze_private_document') {
        assertRevision(args);window.__analysisRequest=args;
        await new Promise(resolve => { window.__finishAnalysis = resolve; });
        session.entities=[{kind:'PERSON',original:'María Pérez',alias:'PERSON_01',source:'qwen'},{kind:'COMPANY',original:'Banco Andino',alias:'COMPANY_01',source:'qwen'},{kind:'EMAIL',original:'maria@example.com',alias:'EMAIL_01',source:'pattern'}];
        session.sanitized='PERSON_01 demanda a COMPANY_01. Email: EMAIL_01. Reclama USD 1500 el 2026-09-30.';
        session.analyzed=true;session.analysisComplete=true;session.advisories=advisories(session.entities);session.revision++;
        return structuredClone(session);
      }
      if(cmd==='review_private_document') {
        assertRevision(args);
        // Same invariants as privacy.rs::require_sanitized, so the UI test fails
        // if the UI ever offers an approval the Rust side would refuse.
        if(!args.entities.length)throw new Error('El mapa no tiene entidades: la versión protegida sería el documento original.');
        const mapChanged=JSON.stringify(session.entities)!==JSON.stringify(args.entities);
        session.entities=args.entities;
        let sanitized=original;
        for(const e of args.entities)sanitized=sanitized.replaceAll(e.original,e.alias);
        if(sanitized.trim()===original.trim())throw new Error('La versión protegida es idéntica al original.');
        if(!session.edited || mapChanged){session.sanitized=sanitized;session.edited=false;}
        session.warnings=session.sanitized.includes('@')?['Posible EMAIL residual: corrige el mapa antes de aprobar.']:[];
        session.advisories=advisories(session.entities);
        session.reviewed=args.confirm && !session.warnings.length;session.revision++;
        return structuredClone(session);
      }
      if(cmd==='update_private_document') {
        assertRevision(args);
        session.sanitized=args.text;session.edited=true;session.reviewed=false;session.revision++;
        return structuredClone(session);
      }
      if(cmd==='preview_private_outbound') {
        assertRevision(args);
        if(!session.reviewed || session.warnings.length)throw new Error('Blocked');
        return {schemaVersion:1,document:session.sanitized};
      }
      if(cmd==='reset_private_analysis') {
        assertRevision(args);
        session.sanitized='';session.entities=[];session.warnings=[];session.advisories=[];
        session.analyzed=false;session.reviewed=false;session.edited=false;session.revision++;
        return structuredClone(session);
      }
      if(cmd==='clear_privacy_session'){session=null;return;}
      if(cmd==='validate_private_export') {
        assertRevision(args);
        if(!session.reviewed)throw new Error('El documento privado no tiene una revisión confirmada.');
        if(!session.entities.length)throw new Error('El mapa no tiene entidades.');
        if(args.text.trim()!==session.sanitized.trim())throw new Error('El PDF usaría texto sin revisar.');
        if(args.title&&/@|\d{8,}/.test(args.title))throw new Error('PDF bloqueado: el título contiene posibles datos privados.');
        return;
      }
      if(cmd==='choose_pdf_destination')return '/tmp/local-legal-audit.pdf';
      if(cmd==='generate_pdf'){
        // The writer, not the renderer, decides what reaches the disk.
        if(args.private&&(!args.private.id||args.private.revision!==session.revision))throw new Error('Stale private export');
        if(args.private&&(!session.reviewed||!session.entities.length))throw new Error('Private PDF blocked by the writer');
        if(args.private&&/@/.test(args.title||''))throw new Error('Private PDF title blocked by the writer');
        return args.destPath;
      }
      throw new Error('Unexpected command: '+cmd);
      function assertRevision(args){if(args.id!==session.id || args.revision!==session.revision)throw new Error('Stale revision');}
      function advisories(entities){return entities.some(e=>e.kind==='PERSON')?[]:['El mapa no contiene ninguna persona. Los nombres no tienen un detector determinista: Qwen puede omitirlos y las reglas no los comprueban. Revísalos tú.'];}
    }};
  });
  await page.goto('http://localhost:1420');
  await page.getByRole('heading',{name:ui('Lo privado permanece aquí.'),exact:true}).waitFor();
  assert.equal(await page.getByRole('button',{name:ui('Proteger documento'),exact:true}).count(),0,'no protection without a document');
  assert.equal(await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).count(),0);
  const importZone=page.getByRole('button',{name:ui('Importar documento'),exact:true});
  const zone=await importZone.boundingBox();
  assert(zone.width>=500 && zone.height>=220,'import area is a large drop zone');
  const emitDrop=async(event,paths=[],inside=true)=>page.evaluate(({event,paths,zone,inside})=>{
    const position={x:(inside?zone.x+zone.width/2:0)*devicePixelRatio,y:(inside?zone.y+zone.height/2:0)*devicePixelRatio};
    window.__emitDrop(event,{paths,position});
  },{event,paths,zone,inside});
  await emitDrop('tauri://drag-enter',['/tmp/client.pdf']);
  await page.getByText(ui('Suelta el documento aquí'),{exact:true}).waitFor();
  await emitDrop('tauri://drag-over',[],false);
  assert.equal(await importZone.evaluate(el=>el.classList.contains('dragging-document')),false,'drag outside clears highlight');
  await emitDrop('tauri://drag-drop',['/tmp/outside.pdf'],false);
  assert.equal(await page.evaluate(()=>window.__privacyCalls.filter(c=>c.cmd==='import_private_document').length),0,'outside drop does not import');
  await emitDrop('tauri://drag-drop',['/tmp/one.pdf','/tmp/two.pdf']);
  await page.getByRole('alert').filter({hasText:ui('Arrastra un solo documento a la vez.')}).waitFor();
  await emitDrop('tauri://drag-enter',['/tmp/client.pdf']);
  await page.screenshot({path:`artifacts/import-dropzone-${language}.png`});
  await emitDrop('tauri://drag-drop',['/tmp/client.pdf']);
  await page.locator('.original-page').waitFor();
  assert.equal(await page.evaluate(()=>window.__privacyCalls.find(c=>c.cmd==='import_private_document').args.path),'/tmp/client.pdf','drop imports native path directly');
  assert.equal(await page.getByRole('heading',{name:ui('Lo privado permanece aquí.'),exact:true}).count(),0,'welcome copy is not part of the workspace');
  assert((await page.locator('.original-page').innerText()).includes('María Pérez'));
  assert.equal(await page.getByTestId('private-initial-extraction').count(),0);
  const before=await page.locator('.workspace-header').boundingBox();
  await page.getByRole('button',{name:ui('Proteger documento'),exact:true}).click();
  await page.getByRole('progressbar',{name:ui('Procesando documento')}).waitFor();
  assert.equal(before.y,(await page.locator('.workspace-header').boundingBox()).y,'processing does not shift the action row');
  await page.getByRole('status').filter({hasText:ui('Analizando con AI local')}).waitFor();
  await page.evaluate(()=>window.__emitAnalysisProgress(['stale-operation',0,'sanitizing']));
  assert.equal(await page.getByRole('status').filter({hasText:ui('Analizando con AI local')}).count(),1,'ignore progress from another operation');
  const {id,revision}=await page.evaluate(()=>window.__analysisRequest);
  const fragment=(n,total)=>ui('Leyendo con AI local · fragmento {0} de {1}').replace('{0}',n).replace('{1}',total);
  await page.evaluate(([id,revision])=>window.__emitAnalysisProgress([id,revision,'reading:1/2:24']),[id,revision]);
  await page.getByRole('status').filter({hasText:fragment(1,2)}).waitFor();
  assert.match(await page.getByRole('status').innerText(),/24 tokens/);
  await page.evaluate(([id,revision])=>window.__emitAnalysisProgress([id,revision,'reading:2/2:96']),[id,revision]);
  await page.getByRole('status').filter({hasText:fragment(2,2)}).waitFor();
  await page.evaluate(([id,revision])=>window.__emitAnalysisProgress([id,revision,'sanitizing']),[id,revision]);
  await page.getByRole('status').filter({hasText:ui('Protegiendo el documento')}).waitFor();
  await page.evaluate(()=>window.__finishAnalysis());
  const map=page.getByRole('button',{name:ui('Mapa privado'),exact:true});
  await map.waitFor();
  await page.waitForFunction(()=>!document.querySelector('.workspace-views button:last-child').disabled);
  assert.equal(await page.getByRole('progressbar').count(),0);
  assert.match(await page.locator('.workspace-title span').innerText(),new RegExp(ui('Pendiente de revisión')));
  assert(!/SAFE/.test(await page.locator('.workspace-title span').innerText()),'analysis alone does not promise safety');
  assert.equal(await page.locator('.boundary-grid').count(),0,'legacy split comparison is gone');
  assert.equal(await page.getByRole('button',{name:ui('Proteger documento'),exact:true}).count(),0,'no repeated protection inside Edit');
  await map.click();
  await page.getByLabel('Alias 1',{exact:true}).fill('CLIENTE_01');
  const document=page.getByRole('button',{name:ui('Documento'),exact:true});
  assert(await document.isDisabled(),'unapplied map blocks view switching');
  await page.locator('.map-page').getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.workspace-views button:first-child').disabled);
  await document.click();
  await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).click();
  await page.waitForFunction(()=>window.__privacyCalls.some(c=>c.cmd==='generate_pdf'));
  const exported=await page.evaluate(()=>window.__privacyCalls.find(c=>c.cmd==='generate_pdf'));
  const destination=await page.evaluate(()=>window.__privacyCalls.find(c=>c.cmd==='choose_pdf_destination'));
  assert.match(destination.args.defaultName,/^(Documento protegido|Protected document)_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}\.pdf$/);
  assert.equal(exported.args.title,language==='en'?'Protected document':'Documento protegido');
  assert.equal(exported.args.private.id,'local-1');
  assert(Number.isInteger(exported.args.private.revision));
  assert.match(await page.locator('.workspace-title span').innerText(),new RegExp(ui('Revisión confirmada')));
  await page.locator('.rich-editor').fill(exported.args.text+' Texto añadido.');
  await page.locator('.settings-menu summary').click();
  await page.locator('#languageToggle').click();
  assert((await page.locator('.rich-editor').innerText()).includes('Texto añadido.'),'language switch preserves the draft');
  await page.locator('#languageToggle').click();
  await page.keyboard.press('Escape');
  await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).click();
  await page.waitForFunction(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').length===2);
  const latest=await page.evaluate(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').at(-1));
  assert(latest.args.private.revision>exported.args.private.revision);
  assert(latest.args.text.includes('Texto añadido.'));
  await map.click();
  await page.getByRole('button',{name:ui('Eliminar entidad 3'),exact:true}).click();
  await page.locator('.map-page').getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
  await page.getByText(ui('Posibles filtraciones · vista previa bloqueada'),{exact:true}).waitFor();
  await document.click();
  assert(await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).isDisabled(),'residual warnings disable export');
  assert.equal(await page.evaluate(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').length),2,'residual leak blocks export');
  await map.click();
  while(await page.getByRole('button',{name:ui('Eliminar entidad 1'),exact:true}).count())await page.getByRole('button',{name:ui('Eliminar entidad 1'),exact:true}).click();
  await page.locator('.map-page').getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
  await page.getByRole('alert').filter({hasText:ui('El mapa no tiene entidades: la versión protegida sería el documento original.')}).waitFor();
  assert(await document.isDisabled(),'failed map save retains the draft');
  assert.equal(await page.evaluate(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').length),2,'empty map cannot export');
  await page.getByRole('button',{name:ui('Restablecer cambios'),exact:true}).click();
  assert.equal(await page.locator('.map-page .entity-row').count(),2,'restoring drafts recovers the saved map');
  await page.getByRole('button',{name:ui('Eliminar entidad 1'),exact:true}).click();
  await page.locator('.map-page').getByRole('button',{name:ui('Guardar cambios'),exact:true}).click();
  await page.locator('.review-advisories summary').click();
  await page.getByText(ui('El mapa no contiene ninguna persona. Los nombres no tienen un detector determinista: Qwen puede omitirlos y las reglas no los comprueban. Revísalos tú.'),{exact:true}).waitFor();
  assert(requests.every(url=>url.startsWith('http://localhost:1420/')||url.startsWith('http://127.0.0.1:1420/')),'no external browser requests');
  console.log(language+': PASS privacy workflow: progress, drafts, aliases, reviewed revision/title export, residual leak blocking, empty-map refusal, reset and name advisory');
} finally {await browser.close();}
