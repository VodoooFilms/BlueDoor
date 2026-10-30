import { browserType } from './browser.mjs';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';

mkdirSync('artifacts/bluedoor', { recursive:true });
const browser = await browserType.launch();
try {
  const page=await browser.newPage({viewport:{width:1440,height:960},deviceScaleFactor:1,reducedMotion:"reduce"});
  page.setDefaultTimeout(12000);
  const errors=[];
  page.on("console",m=>{if(m.type()==="error")console.error("BROWSER:",m.text());});
  page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
    localStorage.setItem('bluedoor-language','es');
    localStorage.setItem('bluedoor-welcome-seen','true');
    let session=null;
    window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,invoke:async(cmd,args,options)=>{
      if(cmd==='plugin:event|listen')return 1;
      if(['check_installation','save_transcripts','plugin:event|unlisten'].includes(cmd))return;
      if(cmd==='load_transcripts')return [];
      if(cmd==='diarization_available')return false;
      if(cmd==='check_model_exists')return !window.__blueMissingModels;
      if(cmd==='list_models')return {whisper:[{id:'small',installed:!window.__blueMissingModels,path:'/tmp/model.bin'}],parakeet:[],diarization:[],privacy:[{id:'qwen3-fast',name:'Qwen3 1.7B',size:'1.3 GB',installed:true}]};
      if(cmd==='prepare_transcribe_blob')return 'one-use-test-token';
      if(cmd==='transcribe_blob'){
        const headers=new Headers(options?.headers);
        if(headers.get('x-bluedoor-config')!=='one-use-test-token')throw Error('Missing native audio authorization header');
        if(!(args instanceof Uint8Array)||args.length<=44||String.fromCharCode(...args.slice(0,4))!=='RIFF')throw Error('Expected binary WAV');
        window.__blueRecordingAccepted=true;
        return {id:'recording',text:'Prueba de grabación local.',segments:[],language:'es',duration:1};
      }
      if(cmd==='get_privacy_session')return structuredClone(session);
      if(cmd==='import_private_document'){session={id:'blue-demo',revision:0,fileName:'Acuerdo de servicios.txt',original:'ACUERDO DE SERVICIOS\n\nMaría Ejemplo firma el acuerdo de servicios profesionales.\n\nLas partes acuerdan revisar las condiciones antes de compartir este documento.',entities:[],sanitized:'',warnings:[],advisories:[],analyzed:false,analysisComplete:false,reviewed:false,edited:false,layoutAvailable:false};return structuredClone(session);}
      if(cmd==='analyze_private_document'){session={...session,revision:1,entities:[{kind:'PERSON',original:'María Ejemplo',alias:'PERSONA_1',source:'review'}],sanitized:session.original.replace('María Ejemplo','PERSONA_1'),analyzed:true,analysisComplete:true};return structuredClone(session);}
      throw Error('Unmocked command: '+cmd);
    }};
  });
  await page.goto('http://localhost:1420');
  await page.locator('.welcome[role="dialog"] .close').click();
  await page.locator('.blue-import-screen').waitFor();
  await page.waitForFunction(()=>Array.from(document.images).every(img=>img.complete));
  const checkChrome=async()=>{
    assert.equal(await page.locator('.brand').textContent(),'Bluedoor');
    assert.equal(await page.locator('.masthead .nav-models').first().evaluate(el=>getComputedStyle(el).color),'rgb(255, 255, 255)');
    assert.equal(await page.locator('.masthead .nav-models').first().evaluate(el=>getComputedStyle(el).opacity),'1');
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no horizontal overflow');
  };
  await checkChrome();
  assert.equal(await page.locator('.protect-art').evaluate(el=>getComputedStyle(el).opacity),'0.8');
  assert.equal(await page.locator('.import-action').evaluate(el=>getComputedStyle(el).backgroundColor),'rgb(255, 255, 255)');
  await page.screenshot({path:'artifacts/bluedoor/01-documentos.png'});
  await page.setViewportSize({width:960,height:680});
  await checkChrome();
  const importBox=await page.locator('.welcome-import').boundingBox();
  assert(importBox.y+importBox.height<=680,'import stays inside the minimum native window');
  await page.screenshot({path:'artifacts/bluedoor/01-documentos-compacto.png'});
  await page.setViewportSize({width:1440,height:960});
  await page.evaluate(()=>window.__blueMissingModels=true);
  await page.locator('#recordToggle').click();
  await page.locator('.mic-panel').waitFor();
  await page.locator('.model-card').first().waitFor();
  await page.setViewportSize({width:1280,height:828});
  assert.equal(await page.locator('.state-title').evaluate(el=>getComputedStyle(el).textAlign),'center');
  assert(Number(await page.locator('.state-title').evaluate(el=>getComputedStyle(el).fontWeight))>=700);
  assert((await page.locator('.model-card').first().boundingBox()).width>500,'audio model cards have breathing room');
  assert(Number(await page.locator('.mic-panel .download-btn').first().evaluate(el=>getComputedStyle(el).fontWeight))>=700);
  await page.screenshot({path:'artifacts/bluedoor/02-audio.png'});
  await page.setViewportSize({width:960,height:648});
  await checkChrome();
  const lastAudioCard=await page.locator('.model-card').last().boundingBox();
  assert(lastAudioCard.y+lastAudioCard.height<=648,'both audio model cards fit at minimum size');
  await page.screenshot({path:'artifacts/bluedoor/02-audio-compacto.png'});
  await page.evaluate(()=>window.__blueMissingModels=false);
  await page.setViewportSize({width:1440,height:960});
  await page.locator('.settings-menu summary').click();
  await page.screenshot({path:'artifacts/bluedoor/03-ajustes.png'});
  await page.locator('#modelsToggle').click();
  await page.locator('.models .card').first().waitFor();
  await page.setViewportSize({width:1280,height:828});
  const header=await page.locator('.models-head').boundingBox();
  const card=await page.locator('.models .card').first().boundingBox();
  assert(Math.abs(header.x-card.x)<1,'settings heading aligns with cards');
  assert(await page.locator('.models .list').evaluate(el=>el.scrollHeight<=el.clientHeight+1),'all cards fit at the default native client size');
  assert(Number(await page.locator('.models .download-btn').first().evaluate(el=>getComputedStyle(el).fontWeight))>=600,'primary actions have a stronger weight');
  await page.screenshot({path:'artifacts/bluedoor/04-modelos.png'});
  await page.locator('.model-picker-trigger').first().click();
  await page.locator('dialog.model-picker').waitFor();
  assert(await page.locator('.model-choice').count()>2,'model cards replace the dropdown');
  await page.screenshot({path:'artifacts/bluedoor/04-selector-modelos.png'});
  await page.locator('.model-choice').filter({hasText:'Whisper Small'}).click();
  await page.locator('dialog.model-picker').waitFor({state:'hidden'});
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-model-stt')),'small');
  await page.locator('.model-picker-trigger').first().click();
  await page.keyboard.press('Escape');
  await page.locator('dialog.model-picker').waitFor({state:'hidden'});
  await page.waitForFunction(()=>document.activeElement===document.querySelector('.model-picker-trigger'));
  await page.locator('.model-picker-trigger').nth(1).click();
  await page.locator('dialog.model-picker').waitFor();
  await page.locator('.model-choice').first().click();
  await page.locator('dialog.model-picker').waitFor({state:'hidden'});
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')),'qwen3-fast');
  await page.setViewportSize({width:960,height:648});
  const footer=await page.locator('.models .footer').boundingBox();
  assert(footer.y+footer.height<=649,'footer remains visible at the minimum window size');
  await page.screenshot({path:'artifacts/bluedoor/04-modelos-compacto.png'});
  await page.setViewportSize({width:1440,height:960});
  await page.getByRole('button',{name:'Documentos',exact:true}).click();
  await page.getByRole('button',{name:'Importar documento',exact:true}).click();
  await page.getByRole('button',{name:'Proteger documento',exact:true}).click();
  await page.locator('.rich-editor').waitFor();
  await page.screenshot({path:'artifacts/bluedoor/05-editor.png'});
  assert.equal(await page.locator('.rich-editor').evaluate(el=>getComputedStyle(el).color),'rgb(32, 32, 32)');
  // A document's explicit ink/type are content, independent of the blue chrome.
  const editor=page.locator('.rich-editor');
  await editor.evaluate(el=>{
    el.innerHTML='<p style="color: rgb(157, 28, 64); font-family: Georgia; font-size: 22px">PERSONA_1 conserva este formato.</p>';
    el.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText'}));
  });
  const formatted=await editor.innerHTML();
  await page.locator('.settings-menu summary').click();
  assert.equal(await page.locator('#themeToggle').count(),0,'appearance toggle removed');
  assert.equal(await editor.innerHTML(),formatted,'theme never rewrites document HTML');
  assert.deepEqual(await editor.locator('p').evaluate(el=>({color:getComputedStyle(el).color,font:getComputedStyle(el).fontFamily,size:getComputedStyle(el).fontSize})),{color:'rgb(157, 28, 64)',font:'Georgia',size:'22px'});
  await page.keyboard.press('Escape');
  // Restore the fixture before leaving its unsaved draft.
  await page.getByRole('button',{name:'Restablecer',exact:true}).click();
  await page.getByRole('button',{name:'Mapa privado',exact:true}).click();
  await page.screenshot({path:'artifacts/bluedoor/06-mapa-privado.png'});
  // Exercise actual PCM capture and the renderer-to-native header contract.
  await page.locator('#recordToggle').click();
  await page.evaluate(()=>{
    Object.defineProperty(navigator,"mediaDevices",{configurable:true,value:{getUserMedia:async()=>{
      const context=new AudioContext({sampleRate:16000});
      await context.resume();
      const oscillator=context.createOscillator();
      const destination=context.createMediaStreamDestination();
      oscillator.connect(destination);oscillator.start();
      window.__blueTestAudio=context;
      return destination.stream;
    }}});
  });
  await page.locator('.orb-window').click();
  await page.locator('.mic-btn.recording').waitFor().catch(async error=>{console.error(await page.locator('.mic-panel').innerText());throw error;});
  await page.waitForTimeout(900);
  await page.locator('.orb-window').click();
  await page.waitForFunction(()=>window.__blueRecordingAccepted===true);
  await page.evaluate(()=>window.__blueTestAudio.close());
  assert.deepEqual(errors,[],'no runtime errors');
  console.log('BlueDoor: import, audio, settings, models, editor and map; 80% illustrations, white navigation, readable paper, minimum window size passed.');
} finally { await browser.close(); }
