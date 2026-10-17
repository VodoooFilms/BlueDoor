import { webkit } from 'playwright';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
mkdirSync('artifacts/feedback', { recursive: true });
const browser = await webkit.launch();
try {
  for (const lang of ['es', 'en']) {
    const page = await browser.newPage({ viewport: { width:1280, height:828 }, reducedMotion:'reduce' });
    await page.addInitScript(lang => {
      localStorage.setItem('bluedoor-language',lang);
      localStorage.setItem('bluedoor-welcome-seen','true');
      localStorage.setItem('bluedoor-model-stt','small');
      window.micFailure = 'NotFoundError';
      window.importFailure = 'Selecciona un archivo de audio o vídeo compatible.';
      window.__TAURI_INTERNALS__ = {
        metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}}, transformCallback:()=>1,
        invoke:async cmd => {
          if(cmd==='list_models')return {whisper:[{id:'small',installed:!window.modelsEmpty,path:'/tmp/small.bin'}],parakeet:[],privacy:[{id:'qwen3-fast',name:'Qwen',size:'1 GB',installed:!window.modelsEmpty}],diarization:[]};
          if(cmd==='load_transcripts')return [];
          if(cmd==='get_privacy_session')return null;
          if(cmd==='diarization_available')return false;
          if(cmd==='plugin:event|listen')return 1;
          if(cmd==='choose_audio_file')return '/tmp/example.invalid';
          if(cmd==='transcribe_file')throw window.importFailure;
          if(cmd==='import_private_document')throw 'Formatos admitidos: TXT, MD, DOCX, PDF, PNG, JPG, TIFF y HEIC.';
          if(cmd==='prepare_local_setup') await new Promise((_, reject)=>window.rejectSetup=reject);
        }
      };
      Object.defineProperty(navigator,'mediaDevices',{configurable:true,value:{getUserMedia:async()=>{
        if(window.micFailure)throw new DOMException('Raw browser diagnostic',window.micFailure);
        return {getTracks:()=>[{stop(){}}]};
      }}});
    },lang);
    await page.goto('http://localhost:1420');
    await page.locator('#recordToggle').click();
    const geometry=()=>page.locator('.orb-window,.state-title,.import-inline,.mic-panel .model-card,.masthead').evaluateAll(els=>els.map(el=>{
      const r=el.getBoundingClientRect();return [r.x,r.y,r.width,r.height];
    }));
    const dismiss=async()=>{for(const btn of await page.locator('.feedback-host button').all())await btn.click();};
    for (const [name, pattern] of [
      ['NotFoundError',lang==='es'?/No se encontró ningún micrófono/:/No microphone found/],
      ['NotAllowedError',lang==='es'?/Permiso de micrófono denegado/:/Microphone permission denied/],
      ['NotReadableError',lang==='es'?/ocupado por otra app/:/in use by another app/],
      ['AbortError',lang==='es'?/No se pudo iniciar la grabación/:/Could not start recording/],
    ]) {
      await page.evaluate(name=>window.micFailure=name,name);
      const before=await geometry();
      await page.locator('.mic-btn').click();
      const alert=page.locator('.feedback-error');
      await alert.waitFor();
      assert.match(await alert.innerText(),pattern);
      assert.doesNotMatch(await alert.innerText(),/Raw browser diagnostic/);
      assert.deepEqual(await geometry(),before,'microphone errors never move audio controls');
      assert.equal(await alert.evaluate(el=>getComputedStyle(el).backgroundColor),'rgb(9, 38, 101)');
      if(name==='NotFoundError')await page.screenshot({path:`artifacts/feedback/no-microphone-${lang}.png`});
      await dismiss();
    }
    for (const raw of ['Selecciona un archivo de audio o vídeo compatible.','No se pudo leer el audio. El archivo puede estar dañado, no contener audio o usar un formato no compatible.']) {
      await page.evaluate(raw=>window.importFailure=raw,raw);
      const before=await geometry();
      await page.locator('.import-inline').click();
      const alert=page.locator('.feedback-error');
      await alert.waitFor();
      assert.match(await alert.innerText(),raw.startsWith('Selecciona')?(/WAV, MP3, M4A/):(lang==='es'?/dañado o no contener/:/damaged or have no audio/));
      assert.deepEqual(await geometry(),before,'format errors never move audio controls');
      await dismiss();
    }
    await page.getByRole('button',{name:lang==='es'?'Documentos':'Documents',exact:true}).click();
    const importAction=page.locator('.welcome-import');
    const before=await importAction.boundingBox();
    await importAction.click();
    await page.locator('.feedback-error').waitFor();
    assert.match(await page.locator('.feedback-error').innerText(),lang==='es'?/formato de documento no es compatible/:/document format is not supported/);
    assert.deepEqual(await importAction.boundingBox(),before,'document errors never move import action');
    await dismiss();
    await page.evaluate(()=>window.modelsEmpty=true);
    await page.locator('.settings-menu summary').click();
    await page.locator('#modelsToggle').click();
    const cards=()=>page.locator('.models .card,.models .head-copy').evaluateAll(els=>els.map(el=>{
      const r=el.getBoundingClientRect();return [r.x,r.y,r.width,r.height];
    }));
    await page.locator('.models .card').first().waitFor();
    const modelBefore=await cards();
    await page.evaluate(()=>window.micFailure=null);
    const icon = await page.locator('.setup-btn > svg').boundingBox();
    const lines = page.locator('.setup-copy > span');
    const firstLine = await lines.first().boundingBox(), secondLine = await lines.last().boundingBox();
    assert(icon.x + icon.width < firstLine.x, 'setup icon sits to the left of both lines');
    assert(Math.abs(firstLine.x - secondLine.x) < 1, 'setup lines have the same left edge');
    assert.deepEqual(await lines.evaluateAll(els => els.map(el => getComputedStyle(el).fontWeight)), ['700','700']);
    assert.deepEqual(await lines.allTextContents(), lang === 'es' ? ['Descargar todos los modelos','Aprobar todos los permisos'] : ['Download all models','Approve all permissions']);
    await page.screenshot({path:`artifacts/feedback/setup-button-${lang}.png`});
    await page.locator('.setup-btn').click();
    await page.waitForFunction(()=>!!window.rejectSetup);
    assert.deepEqual(await cards(),modelBefore,'setup progress never moves the heading or cards');
    await page.evaluate(()=>window.rejectSetup('Un aviso muy largo '.repeat(160)));
    await page.locator('.feedback-error').waitFor();
    assert.deepEqual(await cards(),modelBefore,'long model error never moves the heading or cards');
    await page.setViewportSize({width:960,height:648});
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'long notice fits minimum window');
    assert(await page.locator('.feedback-host').evaluate(el=>el.scrollHeight>el.clientHeight),'long notices scroll rather than expanding the app');
    await page.screenshot({path:`artifacts/feedback/long-error-${lang}.png`});
    await dismiss();
    await page.close();
  }
  console.log('PASS: ES/EN microphone absence, denial, busy device, audio/document formats, corrupt audio and long progress/errors keep controls still; dark blue, dismissible notices.');
} finally {await browser.close();}
