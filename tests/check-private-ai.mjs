import { webkit } from 'playwright';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const language = process.env.BELEGAL_TEST_LANG || 'es';
const copySource = readFileSync(new URL('../src/lib/private-i18n.ts',import.meta.url),'utf8');
const english = JSON.parse(copySource.slice(copySource.indexOf('= {')+2,copySource.indexOf('\n};')+2));
const ui = text => language === 'en' ? (english[text] || text.replace(/^Eliminar entidad/, 'Remove entity')) : text;


const browser = await webkit.launch();
try {
  const page = await browser.newPage({viewport:{width:1370,height:800}});
  const requests=[];
  page.on('request',r=>requests.push(r.url()));
  await page.addInitScript(lang => { localStorage.setItem('belegal-local-demo-language',lang); localStorage.setItem('belegal-local-demo-welcome-seen','true'); }, language);
  await page.addInitScript(() => {
    const original='María Pérez demanda a Banco Andino. Email: maria@example.com. Reclama USD 1500 el 2026-09-30.';
    // El OCR perdió una letra; la extracción inicial difiere del texto actual.
    const extracted=original.replace('demanda','0demanda');
    let session=null;
    const calls=[];window.__privacyCalls=calls;
    let callbackId=0;const callbacks=new Map();
    window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:callback=>{callbacks.set(++callbackId,callback);return callbackId;},invoke:async(cmd,args)=>{
      calls.push({cmd,args});
      if(cmd==='check_installation')return;
      if(cmd==='load_transcripts')return [];
      if(cmd==='save_transcripts')return;
      if(cmd==='plugin:event|listen'){
        if(args.event==='private-analysis-progress')window.__emitAnalysisProgress=payload=>callbacks.get(args.handler)?.({event:args.event,id:1,payload});
        return 1;
      }
      if(cmd==='plugin:event|unlisten')return;
      if(cmd==='list_models')return {whisper:[{id:'small',path:'/mock/stt',installed:true}],parakeet:[],diarization:[],privacy:[{installed:true}]};
      if(cmd==='diarization_available')return false;
      if(cmd==='get_privacy_session')return session;
      if(cmd==='get_private_extraction'){assertRevision(args);return extracted;}
      if(cmd==='import_private_document') {
        session={id:'local-1',revision:0,fileName:'private-client-file.txt',original,entities:[],sanitized:'',warnings:[],advisories:[],analyzed:false,reviewed:false};
        return structuredClone(session);
      }
      if(cmd==='analyze_private_document') {
        assertRevision(args);window.__analysisRequest=args;
        await new Promise(resolve => { window.__finishAnalysis = resolve; });
        session.entities=[{kind:'PERSON',original:'María Pérez',alias:'PERSON_01',source:'qwen'},{kind:'COMPANY',original:'Banco Andino',alias:'COMPANY_01',source:'qwen'},{kind:'EMAIL',original:'maria@example.com',alias:'EMAIL_01',source:'pattern'}];
        session.sanitized='PERSON_01 demanda a COMPANY_01. Email: EMAIL_01. Reclama USD 1500 el 2026-09-30.';
        session.analyzed=true;session.advisories=advisories(session.entities);session.revision++;
        return structuredClone(session);
      }
      if(cmd==='review_private_document') {
        assertRevision(args);
        // Same invariants as privacy.rs::require_sanitized, so the UI test fails
        // if the UI ever offers an approval the Rust side would refuse.
        if(!args.entities.length)throw new Error('El mapa no tiene entidades: la versión sanitizada sería el documento original.');
        const mapChanged=JSON.stringify(session.entities)!==JSON.stringify(args.entities);
        session.entities=args.entities;
        let sanitized=original;
        for(const e of args.entities)sanitized=sanitized.replaceAll(e.original,e.alias);
        if(sanitized.trim()===original.trim())throw new Error('La versión sanitizada es idéntica al original.');
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
      if(cmd==='choose_pdf_destination')return '/tmp/belegal-audit.pdf';
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
  if (language === 'en') {
    await page.getByRole('heading',{name:'Your private information stays here.',exact:true}).waitFor();
    assert.equal(await page.locator('html').getAttribute('lang'),'en');
    await page.locator('.nav-models').filter({hasText:'Models'}).click();
    await page.getByText('Local detection of private entities in documents',{exact:true}).waitFor();
    await page.locator('.nav-models').filter({hasText:'Private Legal'}).click();
  }
  assert.equal(await page.locator('.brand-mark').count(),0);
  assert.equal(await page.getByRole('heading',{name:ui('Revisión del OCR'),exact:true}).count(),0);
  assert(await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).isDisabled());
  assert(await page.locator('.document-actions').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).isDisabled());
  await page.getByRole('heading',{name:ui('Primero, importa un documento privado.'),exact:true}).waitFor();
  await page.getByRole('button',{name:ui('Importa un documento')}).click();
  await page.getByTestId('private-original').waitFor();
  assert.equal(await page.getByTestId('private-initial-extraction').count(),0);
  assert.equal(await page.getByRole('button',{name:ui('Ver extracción inicial sin correcciones')}).count(),0);
  assert(await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).isDisabled());
  assert.equal(await page.locator('.document-actions').getByRole('button',{name:ui('Cerrar documento')}).count(),0);
  const actionsBefore = await page.locator('.document-actions').boundingBox();
  const documentBefore = await page.locator('.boundary-grid').boundingBox();
  await page.getByRole('button',{name:ui('Proteger documento'),exact:true}).click();
  await page.getByRole('progressbar',{name:ui('Procesando documento')}).waitFor();
  const actionsDuring = await page.locator('.document-actions').boundingBox();
  const documentDuring = await page.locator('.boundary-grid').boundingBox();
  assert.equal(actionsBefore.y,actionsDuring.y,'Processing status does not move the buttons');
  assert.equal(documentBefore.y,documentDuring.y,'Processing status does not move the document');
  const editBounds = await page.locator('.document-actions').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).boundingBox();
  const loaderBounds = await page.getByRole('progressbar').boundingBox();
  assert(Math.abs(loaderBounds.y+loaderBounds.height-editBounds.y-editBounds.height)<1,'Loader aligns with the bottom of the buttons');
  await page.getByRole('status').filter({hasText:ui('Analizando con AI local')}).waitFor();
  const labelBounds=await page.locator('.processing-label').boundingBox();
  assert(labelBounds.y+labelBounds.height<loaderBounds.y,'Small status text sits above the loader');
  const formatBounds = await page.locator('.format-note').boundingBox();
  assert(loaderBounds.x > editBounds.x + editBounds.width,'Loader follows Edit');
  assert(loaderBounds.x + loaderBounds.width < formatBounds.x,'Loader ends before the formats');
  await page.screenshot({path:`artifacts/private-legal-processing-${language}.png`,fullPage:true});
  await page.evaluate(()=>window.__emitAnalysisProgress(['stale-operation',0,'sanitizing']));
  assert.equal(await page.getByRole('status').filter({hasText:ui('Analizando con AI local')}).count(),1,'Ignore progress from another operation');
  await page.evaluate(()=>{const {id,revision}=window.__analysisRequest;window.__emitAnalysisProgress([id,revision,'sanitizing']);});
  await page.getByRole('status').filter({hasText:ui('Sanitizando el documento')}).waitFor();
  await page.screenshot({path:`artifacts/private-legal-sanitizing-${language}.png`,fullPage:true});
  await page.evaluate(()=>window.__finishAnalysis());
  await page.waitForFunction(()=>!document.querySelector('.analyze-button').disabled);

  assert.equal(await page.locator('.private-panel .entity-list').count(),0);
  await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
  await page.getByLabel('Alias 1',{exact:true}).waitFor();
  await page.locator('.nav-models').filter({hasText:ui('Private Legal')}).click();
  await page.locator('.boundary-grid').waitFor();
  assert.equal(await page.locator('.map-page').count(),0,'Private Legal tab leaves the map');
  await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
  await page.getByLabel('Alias 1',{exact:true}).waitFor();
  const columnWidths = await page.locator('.entity-row').first().evaluate(row => {
    const fields = row.querySelectorAll('input');
    return [...fields].map(field => field.getBoundingClientRect().width);
  });
  assert(Math.abs(columnWidths[0] - columnWidths[1]) < 1, 'Private value and alias have equal width');
  const previewButton=page.getByRole('button',{name:ui('Preview what leaves this computer')});
  assert.equal(await previewButton.count(),0,'Map has its own internal page');
  await page.getByLabel('Alias 1',{exact:true}).fill('CLIENTE_01');
  assert(await page.locator('.document-actions').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).isDisabled(),'Unapplied map blocks Edit');
  await page.getByRole('button',{name:ui('Aplicar cambios al mapa')}).click();
  await page.getByRole('button',{name:ui('← Volver al documento'),exact:true}).click();
  assert.equal(await page.getByRole('button',{name:ui('Confirmar revisión local')}).count(),0);
  await page.locator('.privacy-workflow').evaluate(el => el.scrollTop = 0);
  const layout = await page.locator('.privacy-workflow').evaluate(root => {
    const panel = root.querySelector('.sanitized-panel');
    const openEdit = panel.querySelector('.review-controls button').getBoundingClientRect();
    return {height:root.clientHeight, scrollHeight:root.scrollHeight, bottom:root.getBoundingClientRect().bottom, editBottom:openEdit.bottom};
  });
  assert(layout.scrollHeight <= layout.height + 1,'Private Legal needs no outer scroll');
  assert(layout.editBottom <= layout.bottom,'Edit stays visible');
  await page.screenshot({path:`artifacts/private-legal-review-${language}.png`,fullPage:true});
  const originalWidth = (await page.locator('.private-panel').boundingBox()).width;
  await page.getByRole('button',{name:ui('Ampliar documento original'),exact:true}).click();
  assert(!(await page.locator('.sanitized-panel').isVisible()),'Original expansion hides the other panel');
  assert((await page.locator('.private-panel').boundingBox()).width > originalWidth * 1.5,'Original uses the full comparison width');
  await page.screenshot({path:`artifacts/private-legal-expanded-${language}.png`,fullPage:true});
  await page.getByRole('button',{name:ui('Volver a pantalla dividida'),exact:true}).click();
  assert(await page.locator('.sanitized-panel').isVisible(),'Back arrow restores the split view');
  await page.getByRole('button',{name:ui('Abrir versión sanitizada en Edit'),exact:true}).click();
  await page.locator('.private-document').waitFor();
  await page.getByRole('button',{name:ui('Volver a pantalla dividida'),exact:true}).click();
  await page.locator('.boundary-grid').waitFor();
  await page.getByRole('button',{name:ui('Abrir versión sanitizada en Edit'),exact:true}).click();

  const rulerBounds = await page.locator('.private-document').evaluate(view => {
    const ruler = view.querySelector('.ruler').getBoundingClientRect();
    const editor = view.querySelector('.corrected');
    const paper = editor.getBoundingClientRect();
    const css = getComputedStyle(editor);
    return { left: ruler.left, right: ruler.right, textLeft: paper.left + parseFloat(css.paddingLeft), textRight: paper.right - parseFloat(css.paddingRight) };
  });
  assert(Math.abs(rulerBounds.left - rulerBounds.textLeft) < 2, 'Left ruler margin follows the document');
  assert(Math.abs(rulerBounds.right - rulerBounds.textRight) < 2, 'Right ruler margin follows the document');
  await page.screenshot({path:`artifacts/private-legal-editor-${language}.png`,fullPage:true});
  await page.getByRole('button',{name:ui('Exportar PDF')}).click();
  await page.waitForFunction(()=>window.__privacyCalls.some(c=>c.cmd==='generate_pdf'));
  const exported=await page.evaluate(()=>window.__privacyCalls.find(c=>c.cmd==='generate_pdf'));
  const destination=await page.evaluate(()=>window.__privacyCalls.find(c=>c.cmd==='choose_pdf_destination'));
  assert.match(destination.args.defaultName, /^(Documento sanitizado|Sanitized document)_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}\.pdf$/, 'PDF filename includes the safe title, local date and time');
  assert.equal(exported.args.title, language === 'en' ? 'Sanitized document' : 'Documento sanitizado');
  assert(exported.args.private&&exported.args.private.id==='local-1','Private PDF carries the reviewed session id');
  assert(Number.isInteger(exported.args.private.revision),'Private PDF carries the reviewed revision');
  // El título se sella en el encabezado y en los metadatos del PDF, así que
  // también viaja como contenido exportado.
  assert(!/@/.test(exported.args.title||''),'Private PDF title is not private data');
  // Export saves and revalidates an edited document in one click.
  await page.waitForFunction(()=>!document.querySelector('.head-btn.primary').disabled);
  await page.locator('.rich-editor').fill(exported.args.text + ' Texto añadido.');
  await page.locator('#languageToggle').click();
  assert((await page.locator('.rich-editor').textContent()).includes('Texto añadido.'),'Language switch preserves unsaved text');
  await page.locator('#languageToggle').click();

  await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).click();
  await page.waitForFunction(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').length===2);
  assert.equal(await page.getByRole('region',{name:ui('Revisión antes de exportar')}).count(),0);
  const latest = await page.evaluate(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').at(-1));
  assert(latest.args.private.revision > exported.args.private.revision, 'Export uses the newly validated revision');
  assert(latest.args.text.includes('Texto añadido.'), 'Export includes the current edits');
  await page.locator('.nav-models').filter({hasText:ui('Private Legal')}).click();
  await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
  await page.getByRole('button',{name:ui('Eliminar entidad 3'),exact:true}).click();
  assert(await page.locator('.document-actions').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).isDisabled());
  await page.getByRole('button',{name:ui('Aplicar cambios al mapa')}).click();
  await page.getByRole('button',{name:ui('← Volver al documento'),exact:true}).click();
  await page.getByText(ui('Posibles filtraciones · vista previa bloqueada')).waitFor();
  await page.locator('.sanitized-panel').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).click();
  await page.getByRole('button',{name:ui('Exportar PDF'),exact:true}).click();
  await page.locator('.status').filter({hasText:ui('Posible EMAIL residual')}).waitFor();
  assert.equal(await page.evaluate(()=>window.__privacyCalls.filter(c=>c.cmd==='generate_pdf').length),2,'Residual leak blocks export');
  await page.locator('.nav-models').filter({hasText:ui('Private Legal')}).click();

  // Vaciar el mapa no puede convertir el original en una salida aprobada.
  await page.getByRole('button',{name:ui('Mapa privado'),exact:true}).click();
  while(await page.getByRole('button',{name:ui('Eliminar entidad 1'),exact:true}).count())await page.getByRole('button',{name:ui('Eliminar entidad 1'),exact:true}).click();
  assert.equal(await page.locator('.map-page .entity-row').count(),0,'Map emptied');
  await page.getByRole('button',{name:ui('Aplicar cambios al mapa')}).click();
  await page.getByText(ui('El mapa no tiene entidades')).waitFor();
  await page.getByRole('button',{name:ui('← Volver al documento'),exact:true}).click();
  assert(await page.locator('.sanitized-panel').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).isDisabled(),'Unapplied empty map blocks Edit');
  await page.getByRole('button',{name:ui('Borrar versión sanitizada'),exact:true}).click();
  assert.equal(await page.getByTestId('sanitized-document').count(),0);
  assert((await page.getByTestId('private-original').textContent()).includes('María Pérez'));
  assert(await page.locator('.document-actions').getByRole('button',{name:ui('Abrir en Edit'),exact:true}).isDisabled());
  assert(!(await page.getByRole('button',{name:ui('Proteger documento'),exact:true}).isDisabled()));
  await page.getByRole('button',{name:ui('Borrar documento original'),exact:true}).click();
  await page.getByRole('heading',{name:ui('Primero, importa un documento privado.')}).waitFor();
  assert.equal(await page.getByTestId('private-original').count(),0);
  assert.equal(await previewButton.count(),0);
  assert(requests.every(url=>url.startsWith('http://localhost:1420/') || url.startsWith('http://127.0.0.1:1420/')),'No external browser requests');
  console.log(language + ': PASS: Private Legal import, no initial-extraction UI, local analysis UI, alias edit, review gate, exact outbound payload, leak blocking, empty-map refusal, name advisory, identified private PDF export and no external requests. Native privacy logic is covered by Rust tests.');
} finally {await browser.close();}
