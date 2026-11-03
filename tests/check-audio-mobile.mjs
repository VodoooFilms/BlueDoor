import assert from 'node:assert/strict';
import { createServer } from 'vite';
import { transcriptionParagraphs } from '../src/lib/editor/content.ts';
for (const text of ['... ¿Cómo estás? ¡Bien!', 'Primera frase. '.repeat(50), 'Sin puntuación ni separadores', 'Un párrafo.\n\nOtro párrafo.']) {
 assert.equal(transcriptionParagraphs(text).replace(/\s+/g, ' ').trim(), text.replace(/\s+/g, ' ').trim(), 'Paragraph formatting preserves recognized content');
}
import { browserType } from './browser.mjs';
import { importErrorMessage } from '../src/lib/user-errors.ts';
const transportError = 'El audio debe enviarse como datos binarios.';
assert.equal(importErrorMessage(transportError, 'audio', 'en'), 'Could not send the recording to the local engine. Record again and retry.');
assert.equal(importErrorMessage(transportError, 'audio', 'es'), 'No se pudo enviar la grabación al motor local. Vuelve a grabar e inténtalo otra vez.');
const server = await createServer(); await server.listen();
const browser = await browserType.launch(process.env.BLUEDOOR_TEST_BROWSER === "chromium" ? {args:["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"]} : {});
try {
 const page = await browser.newPage({ viewport: { width: 412, height: 900 }, isMobile: true, hasTouch: true, reducedMotion: 'reduce' });
 await page.addInitScript(() => {
  localStorage.setItem('bluedoor-language', 'es'); localStorage.setItem('bluedoor-model-stt', 'vosk-small-es'); localStorage.setItem('bluedoor-welcome-seen', 'true');
  window.__calls = [];
  window.__TAURI_INTERNALS__ = { metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }, transformCallback: () => 1, invoke: async (cmd, args) => {
   window.__calls.push({ cmd, args });
   if (cmd === 'app_platform') return 'android';
   if (cmd === 'load_transcripts') return [];
   if (cmd === 'get_privacy_session') return null;
   if (cmd === 'diarization_available') return false;
   if (cmd === 'list_models') return { whisper: [], parakeet: [{ id: 'vosk-small-es', path: '/mock/vosk.zip', installed: true }, { id: 'vosk-small-en', path: '/mock/vosk-en.zip', installed: true }], diarization: [], privacy: [{ id: 'gliner-pii-small-int8', name: 'GLiNER Small', size: '86 MB', installed: true }] };
   if (cmd === 'choose_audio_file') return '/mock/reunion.wav';
   if (cmd === 'prepare_transcribe_blob') return 'test-audio-token';
   if (cmd === 'transcribe_file' || cmd === 'transcribe_blob') {
    await new Promise(resolve => { window.__finishAudio = resolve; });
    const text = 'Esta es una frase reconocida que conserva las palabras de la grabación. '.repeat(12);
    return { text, segments: [{ start: 0, end: 30, text }], language: 'es', duration: 30 };
   }
   if (cmd === 'prepare_share_export') return '/mock/reunion.pdf';
   if (cmd === 'generate_pdf') { window.__export = args; return '/mock/reunion.pdf'; }
   return null;
  }};
 });
 await page.goto(server.resolvedUrls.local[0]);
 await page.locator('#recordToggle').click();
 await page.locator('.voice-mobile').waitFor();
 assert.equal(await page.locator('.voice-intro, .voice-steps, .voice-mobile .local-models').count(), 0);
 await page.setViewportSize({width:412,height:900});
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/audio-record-clean.png'});
 assert.equal(await page.locator('.history').count(), 0);
 await page.getByRole('button', { name: 'Importar audio', exact: true }).click();
 await page.locator('.processing-dialog[open]').waitFor();
 await page.waitForFunction(() => !!window.__finishAudio);
 await page.screenshot({ path: 'artifacts/audio-mobile-progress.png' });
 await page.evaluate(() => window.__finishAudio());
 const editor = page.locator('.audio-mobile .rich-editor'); await editor.waitFor();
 const pdf = page.getByRole('button', { name: 'Exportar PDF', exact: true });
 assert(await pdf.isDisabled(), 'Export requires review');
 assert((await editor.locator(':scope > div').count()) >= 3, 'Recognized sentences form paragraphs');
 for (const width of [320, 360, 412]) {
  await page.setViewportSize({ width, height: 900 });
  const bounds = await editor.boundingBox();
  assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1, `Editor fits ${width}px`);
 }
 for (const width of [320, 360, 412]) {
  await page.setViewportSize({width,height:900});
  const rects = await page.locator('.audio-mobile .editor-toolbar').evaluate(el => {
   const rect = selector => { const e=el.querySelector(selector), r=e.getBoundingClientRect(); return {x:r.x,y:r.y,w:r.width,h:r.height, fits:e.scrollWidth<=e.clientWidth}; };
   return ['.audio-format-toggle','.save-btn','.audio-reset','.audio-undo','.audio-copy','.audio-export','.gpt-button'].map(rect);
  });
  assert(Math.abs(rects[0].w-rects[1].w)<1);
  assert(Math.abs(rects[5].w-rects[6].w)<1);
  assert(rects.every(r=>r.h===64 && r.fits), `Equal heights and fitting labels at ${width}`);
  assert(rects[0].y===rects[1].y && rects[2].y===rects[3].y && rects[3].y===rects[4].y && rects[5].y===rects[6].y);
 }
 await page.locator('.audio-format-toggle').click();
 await page.screenshot({path:'artifacts/audio-format-grid.png',fullPage:true});
 await page.locator('.audio-format-toggle').click();
 await page.screenshot({ path: 'artifacts/audio-mobile-paragraphs.png', fullPage: true });
 await page.getByLabel('Título de la transcripción').fill('Acta de reunión');
 await editor.fill('Primera corrección.\n\nSegundo párrafo revisado.');
 await page.getByRole('button', { name: 'Aprobar revisión', exact: true }).click();
 await page.waitForFunction(() => !document.querySelector('.audio-mobile .head-btn.primary[title]')?.disabled);
 await editor.fill('Texto cambiado después de aprobar.');
 assert(await pdf.isDisabled(), 'Editing revokes approval');
 await page.getByRole('button', { name: 'Aprobar revisión', exact: true }).click();
 await pdf.click();
 await page.waitForFunction(() => !!window.__export);
 const output = await page.evaluate(() => window.__export);
 assert.equal(output.title, 'Acta de reunión'); assert.equal(output.text, 'Texto cambiado después de aprobar.');
 assert.equal(output.model, 'vosk-small-es');
 await page.waitForTimeout(300);
 await page.screenshot({ path: 'artifacts/audio-mobile-editor.png', fullPage: true });
 await page.locator('.settings-menu summary').click();
 await page.locator('#modelsToggle').click();
 await page.getByRole('heading', { name: 'Modelo privado de transcripción de audio' }).waitFor();
 await page.locator('.settings-menu summary').click();
 await page.locator('#languageToggle').click();
 await page.getByRole('heading', {name:'Private Audio Transcription Model',exact:true}).waitFor();
 await page.locator('.settings-menu summary').click();
 await page.locator('#languageToggle').click();
 const cards = page.locator('.models .card');
 assert(await cards.nth(0).innerText().then(text => text.includes('GLiNER')));
 assert(await cards.nth(1).innerText().then(text => text.includes('Vosk Small Español')));
 assert.equal(await page.locator('[data-model-family="vosk"]').count(), 1);
 for (const width of [320, 412]) {
  await page.setViewportSize({width, height:900});
  const audio = cards.nth(1);
  const status = await audio.locator('.privacy-model-status').boundingBox();
  const controls = await audio.locator('.privacy-model-controls').boundingBox();
  assert(controls.y >= status.y + status.height - 1, 'Controls sit below download/status');
 }
 await page.locator('.settings-menu summary').click();
 await page.locator('#documentationToggle').click();
 await page.getByRole('heading', {name:'Grabar y transcribir audio en Android'}).waitFor();
 assert((await page.locator('#documentation-audio').locator('..').innerText()).includes('Vosk Small English'));
 await page.locator('.documentation .close').click();
 await cards.nth(1).locator('.model-picker-trigger').click();
 assert.equal(await page.locator('.model-picker').getByText(/NVIDIA Parakeet|Parakeet Redux/).count(),0);
 assert((await page.locator('.model-choice').filter({hasText:'Vosk Small English'}).innerText()).includes('Vosk · English'));
 await page.locator('.model-choice').filter({hasText:'Vosk Small English'}).click();
 await cards.nth(1).locator('.selected-model-identity').getByText('Vosk Small English',{exact:true}).waitFor();
 await page.locator('.shell').evaluate(el => el.scrollTop = 0);
 await page.waitForTimeout(350);
 await page.screenshot({path:'artifacts/audio-models-redux.png',fullPage:true});
 if (process.env.BLUEDOOR_TEST_BROWSER === 'chromium') {
  await page.locator('#recordToggle').click();
  await page.locator('.mic-btn').click();
  await page.getByText('Transcribir', {exact:true}).waitFor();
  await page.waitForTimeout(350);
  await page.locator('.mic-btn').click();
  await page.waitForFunction(() => window.__calls.some(c => c.cmd === 'transcribe_blob'));
  const call = await page.evaluate(() => window.__calls.find(c => c.cmd === 'transcribe_blob'));
  assert.equal(call.args.token, 'test-audio-token');
  const prepare = await page.evaluate(() => window.__calls.find(c => c.cmd === 'prepare_transcribe_blob'));
  assert.equal(prepare.args.config.modelPath, '/mock/vosk-en.zip');
  assert.equal(prepare.args.config.language, 'en');
  assert(Array.isArray(call.args.audio));
  assert.deepEqual(call.args.audio.slice(0,4), [82,73,70,70]);
  await page.evaluate(() => window.__finishAudio());
  await page.locator('.audio-mobile .rich-editor').waitFor();
 }
 console.log('PASS mobile audio: progress, paragraphs, title, edit, review invalidation, PDF payload');
} finally { await browser.close(); await server.close(); }
