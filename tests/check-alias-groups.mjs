import {webkit} from 'playwright';
import assert from 'node:assert/strict';

// Exercise the real map/PDF components with a deterministic native-command
// boundary. Distinct private values deliberately share the same alias.
const browser = await webkit.launch();
try {
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  await page.addInitScript(() => {
    localStorage.setItem('bluedoor-language','es');
    localStorage.setItem('bluedoor-welcome-seen','true');
    const entities = [
      {kind:'COMPANY',original:'Empresa Ejemplo S.A.',alias:'COMPANY_01',source:'review'},
      {kind:'COMPANY',original:'*EMPRESAEJEMPLO',alias:'COMPANY_01',source:'review'},
      {kind:'COMPANY',original:'Otra Sociedad',alias:'COMPANY_02',source:'review'},
    ];
    let session = {id:'alias-groups',revision:1,fileName:'companies.pdf',original:'Empresa Ejemplo S.A.\n*EMPRESAEJEMPLO\nOtra Sociedad\nEmpresa Ejemplo S.A.',entities,sanitized:'COMPANY_01\nCOMPANY_01\nCOMPANY_02\nCOMPANY_01',warnings:[],advisories:[],analyzed:true,analysisComplete:true,reviewed:false,edited:false,layoutAvailable:true};
    window.__TAURI_INTERNALS__ = {metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,invoke:async(cmd,args)=>{
      if (['check_installation','save_transcripts','plugin:event|unlisten'].includes(cmd)) return;
      if (cmd==='load_transcripts') return [];
      if (cmd==='plugin:event|listen') return 1;
      if (cmd==='list_models') return {whisper:[],parakeet:[],diarization:[],privacy:[{installed:true}]};
      if (cmd==='diarization_available') return false;
      if (cmd==='get_privacy_session') return structuredClone(session);
      if (cmd==='preview_private_layout') return {id:session.id,revision:session.revision,estimated:false,pages:[0,1].map(()=>({width:500,height:200,imageBase64:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aV1cAAAAASUVORK5CYII='})),redactions:[{page:0,entityIndex:0,rect:[30,30,180,20]},{page:0,entityIndex:2,rect:[30,80,180,20]},{page:1,entityIndex:1,rect:[30,30,180,20]},{page:1,entityIndex:0,rect:[30,80,180,20]}].map(a=>({...a,alias:session.entities[a.entityIndex].alias}))};
      if (cmd==='review_private_layout') {
        if(args.id!==session.id || args.revision!==session.revision) throw Error('Stale review');
        session.entities=structuredClone(args.entities);
        session.sanitized=session.original;
        for(const e of session.entities) session.sanitized=session.sanitized.replaceAll(e.original,e.alias);
        session.reviewed=args.confirm;session.revision++;
        window.__reviewed=structuredClone(session);
        return structuredClone(session);
      }
      if(cmd==='choose_pdf_destination') return '/tmp/alias-groups.pdf';
      if(cmd==='export_private_layout') {
        if(!session.reviewed || args.revision!==session.revision) throw Error('Unreviewed export');
        window.__exported=structuredClone(session);return args.destPath;
      }
      throw Error('Unexpected command '+cmd);
    }};
  });
  await page.goto('http://localhost:1420');
  await page.getByRole('button',{name:'Mapa privado',exact:true}).waitFor();
  assert.equal(await page.getByRole('button',{name:'Mapa privado',exact:true}).count(),1);
  await page.getByRole('combobox',{name:'Zoom',exact:true}).selectOption('400');
  await page.getByRole('button',{name:'Editar alias COMPANY_01, Página 1',exact:true}).first().click();
  const panel=page.getByRole('complementary',{name:'Editar alias',exact:true});
  const alias=panel.getByRole('textbox',{name:'Alias en el documento',exact:true});
  const previous=panel.getByRole('button',{name:'Caja de alias anterior',exact:true});
  const next=panel.getByRole('button',{name:'Caja de alias siguiente',exact:true});
  assert(await previous.isDisabled());assert.match(await panel.innerText(),/1 \/ 4/);
  await alias.fill('BORRADOR');
  await next.click();assert.match(await panel.innerText(),/Otra Sociedad/);assert.equal(await alias.inputValue(),'COMPANY_02');
  await next.click();assert.match(await panel.innerText(),/\*EMPRESAEJEMPLO/);assert.equal(await alias.inputValue(),'BORRADOR','navigation keeps shared unsaved aliases');
  assert.match(await panel.innerText(),/Página 2/);
  const selected=page.locator('.alias-zone[data-alias-region].selected');
  assert.equal(await selected.count(),1,'highlight identifies one physical box');
  const box=await selected.boundingBox(),viewport=await page.locator('.page-editor .viewer-body').boundingBox();
  assert(box.y+box.height/2>=viewport.y && box.y+box.height/2<=viewport.y+viewport.height,'navigation scrolls to a box on another page');
  await next.click();assert.match(await panel.innerText(),/Empresa Ejemplo S.A./);assert(await next.isDisabled());
  await previous.click();await previous.click();await previous.click();
  assert.equal(await alias.inputValue(),'BORRADOR','returning to first box preserves its draft');assert(await previous.isDisabled());
  await alias.fill('COMPANY_01');
  await page.locator('.alias-actions').getByRole('button',{name:'Guardar cambios',exact:true}).click();
  await page.waitForFunction(()=>window.__reviewed?.revision===2);
  await panel.getByRole('button',{name:'Cerrar editor de alias',exact:true}).click();
  await page.getByRole('combobox',{name:'Zoom',exact:true}).selectOption('100');
  await page.getByRole('button',{name:'Mapa privado',exact:true}).first().click();
  const first=page.getByRole('textbox',{name:'Alias 1',exact:true});
  await first.fill('');
  await first.pressSequentially('COMPANY_02_PERSONALIZADA');
  assert.equal(await page.getByRole('textbox',{name:'Alias 2',exact:true}).inputValue(),'COMPANY_02_PERSONALIZADA','distinct original follows group during typing');
  assert.equal(await page.getByRole('textbox',{name:'Alias 3',exact:true}).inputValue(),'COMPANY_02','temporary typing collision must not recruit another group');
  await page.locator('.map-page').getByRole('button',{name:'Guardar cambios',exact:true}).click();
  await page.waitForFunction(()=>window.__reviewed?.revision===3);
  assert.equal(await page.evaluate(()=>window.__reviewed.sanitized),'COMPANY_02_PERSONALIZADA\nCOMPANY_02_PERSONALIZADA\nCOMPANY_02\nCOMPANY_02_PERSONALIZADA');
  await page.getByRole('button',{name:'Documento',exact:true}).click();
  // Edit from the other original on page 2, then verify all three occurrences.
  await page.getByRole('button',{name:'Editar alias COMPANY_02_PERSONALIZADA, Página 2',exact:true}).first().click();
  const editor=page.getByRole('textbox',{name:'Alias en el documento',exact:true});
  await editor.fill('');
  await editor.pressSequentially('PROVEEDOR PRINCIPAL');
  assert(await page.getByRole('button',{name:'Exportar PDF'}).isDisabled());
  await page.locator('.alias-actions').getByRole('button',{name:'Guardar cambios',exact:true}).click();
  await page.waitForFunction(()=>window.__reviewed?.revision===4);
  assert.deepEqual(await page.evaluate(()=>window.__reviewed.entities.map(e=>e.alias)),['PROVEEDOR PRINCIPAL','PROVEEDOR PRINCIPAL','COMPANY_02']);
  assert.equal(await page.getByRole('button',{name:'Editar alias PROVEEDOR PRINCIPAL, Página 1',exact:true}).count(),1);
  assert.equal(await page.getByRole('button',{name:'Editar alias PROVEEDOR PRINCIPAL, Página 2',exact:true}).count(),2);
  await page.getByRole('button',{name:'Exportar PDF'}).click();
  await page.getByRole('status').filter({hasText:'PDF guardado con el formato original.'}).waitFor();
  assert.equal(await page.evaluate(()=>window.__exported.sanitized),'PROVEEDOR PRINCIPAL\nPROVEEDOR PRINCIPAL\nCOMPANY_02\nPROVEEDOR PRINCIPAL');
  await page.getByRole('button',{name:'Mapa privado',exact:true}).first().click();
  assert.equal(await first.inputValue(),'PROVEEDOR PRINCIPAL');
  assert.equal(await page.getByRole('textbox',{name:'Alias 2',exact:true}).inputValue(),'PROVEEDOR PRINCIPAL');
  console.log('PASS shared aliases: map typing, collision isolation, PDF editing from second original, all occurrences, reviewed export payload, reopen map');
} finally { await browser.close(); }
