import assert from 'node:assert/strict';
import { browserType } from './browser.mjs';
const browser = await browserType.launch();
try {
  const page = await browser.newPage({ viewport: { width: 412, height: 850 }, reducedMotion: "reduce" });
  await page.addInitScript(() => {
    localStorage.setItem('bluedoor-language', 'es');
    localStorage.setItem('bluedoor-welcome-seen', 'true');
    window.__modelCalls = [];
    let callback = 0;
    const callbacks = new Map();
    window.__TAURI_INTERNALS__ = { metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }, transformCallback: fn => { callbacks.set(++callback, fn); return callback; }, invoke: async (cmd, args) => {
      window.__modelCalls.push({ cmd, args });
      if (cmd === 'app_platform') return 'android';
      if (cmd === 'plugin:event|listen') return 1;
      if (cmd === 'get_privacy_session') return null;
      if (cmd === 'interrupted_private_session') return false;
      if (cmd === 'diarization_available') return false;
      const installed = JSON.parse(localStorage.getItem('qa-installed-models') || '[]');
      if (cmd === 'list_models') return { whisper: [], parakeet: [], diarization: [], privacy: [
        { id: 'gliner-multi-pii-fp16', name: 'GLiNER Multi PII · español · FP16', size: '596 MB', installed: installed.includes('gliner-multi-pii-fp16') },
        { id: 'qwen3-fast', name: 'Qwen3-0.6B Q4_K_M', size: '397 MB', installed: installed.includes('qwen3-fast') },
        { id: 'gliner-pii-small-int8', name: 'GLiNER PII Small v1.0 · INT8', size: '82,7 MB + tokenizador 3,6 MB', installed: installed.includes('gliner-pii-small-int8') },
      ] };
      if (cmd === 'download_privacy_model') {
        await new Promise(resolve => { window.__finishDownload = resolve; });
        localStorage.setItem('qa-installed-models', JSON.stringify([...installed, args.id]));
      }
      if (cmd === 'delete_privacy_model') localStorage.setItem('qa-installed-models', JSON.stringify(installed.filter(id => id !== args.id)));
    } };
  });
  await page.goto('http://localhost:1420');
  await page.locator('.welcome[role="dialog"] .close').click();
  await page.getByRole('heading', { name: 'Modelos de IA locales:' }).waitFor();
  assert(await page.locator('.privacy-model-row').getByText('GLiNER PII Small v1.0 · INT8', {exact:true}).isVisible(), 'GLiNER is the default on Android');
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')), 'gliner-pii-small-int8', 'Former default migrates once to small PII');
  assert.equal(await page.locator('.models .card').count(), 1, 'No audio model cards on Android');
  assert.equal(await page.locator('.mobile-model-settings').count(), 0, 'Technical tuning card is removed');
  assert(await page.locator('#editToggle').isDisabled(), 'Edit is unavailable before protection');
  assert(Number(await page.locator('#editToggle').evaluate(el => getComputedStyle(el).opacity)) < .6);
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).count(), 1);
  await page.getByRole('button', { name: 'Descargar', exact: true }).click();
  await page.getByRole('button', { name: 'Cancelar descarga', exact: true }).waitFor();
  assert(await page.getByRole('button', { name: 'Modelos', exact: true }).isDisabled());
  const geometry = await page.evaluate(() => {
    const box = selector => { const { x, y, width, height } = document.querySelector(selector).getBoundingClientRect(); return { x, y, width, height }; };
    return { title:box('.models h1'), cancel:box('.cancel-download-btn'), name:box('.privacy-model-row .row-copy strong'), edit:box('.privacy-model-row .model-picker-trigger'), trash:box('.privacy-model-delete'), progress:box('.privacy-model-status .download-progress') };
  });
  const middle = box => box.y + box.height / 2;
  assert(Math.abs(middle(geometry.title) - middle(geometry.cancel)) < 5, 'Title and cancel share one row');
  assert(Math.abs(middle(geometry.edit) - middle(geometry.trash)) < 5, 'Management actions share a row');
  assert(geometry.progress.y >= geometry.name.y + geometry.name.height, 'Download follows model name');
  assert(geometry.edit.y >= geometry.progress.y + geometry.progress.height, 'Management actions sit below download');
  assert(geometry.edit.x < geometry.trash.x, 'Management actions align to the right');
  assert.equal(await page.locator('.selected-model-identity [data-model-family=gliner]').count(), 1);
  await page.screenshot({ path:'artifacts/android-models-downloading-v3.png', fullPage:true });
  await page.evaluate(() => window.__finishDownload());
  await page.getByText('Instalado', { exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).count(), 0);
  await page.reload();
  await page.locator('.welcome[role="dialog"]').waitFor({state:'detached'});
  await page.locator('.privacy-container:not(.hidden)').waitFor();
  await page.getByRole('button', { name: 'Modelos', exact: true }).click();
  await page.getByText('Instalado', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.__modelCalls.filter(call => call.cmd === 'download_privacy_model').length), 0, 'Reopening must not download again');
  await page.screenshot({ path: 'artifacts/android-models-installed.png', fullPage: true });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'No horizontal overflow');
  await page.getByRole('button', { name: 'Cambiar', exact: true }).click();
  assert.equal(await page.locator('.model-picker [data-model-family]').count(), 3);
  for (const family of ['qwen', 'gliner']) assert(await page.locator(`.model-picker [data-model-family=${family}]`).first().isVisible());
  await page.screenshot({path:'artifacts/android-model-picker-families.png',fullPage:true});
  await page.getByRole('button', { name: /Qwen3-0.6B/ }).click();
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-privacy-model')), 'qwen3-fast');
  await page.reload();
  await page.locator('.welcome[role="dialog"] .close').click();
  await page.locator('.privacy-model-row').getByText('Qwen3-0.6B Q4_K_M', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-privacy-model')), 'qwen3-fast', 'An explicit pending Qwen download must survive reload while GLiNER is installed');
  await page.getByText('Incluye GLiNER Multi auxiliar', { exact: true }).waitFor();

  await page.getByRole('button', { name: 'Descargar', exact: true }).click();
  await page.waitForFunction(() => typeof window.__finishDownload === 'function');
  await page.evaluate(() => window.__finishDownload());
  await page.getByText('Instalado', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Eliminar', exact: true }).click();
  await page.locator('.danger').click();
  await page.getByText('Instalado', {exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')), 'gliner-pii-small-int8', 'Deleting Qwen selects installed GLiNER');
  const deleted = page.locator('.models .models-feedback').getByRole('status');
  await deleted.waitFor();
  assert.match(await deleted.innerText(), /Modelo eliminado/);
  assert.equal(await page.locator('.feedback-host').getByText('Modelo eliminado', { exact: true }).count(), 0, 'model messages are not sent to the bottom toast host');
  for (const [width, height] of [[320, 568], [412, 850], [844, 390]]) {
    await page.setViewportSize({ width, height });
    const footer = page.locator('.models > .footer');
    await footer.scrollIntoViewIfNeeded();
    const messageBox = await deleted.boundingBox();
    const backBox = await page.locator('.models .back-btn').boundingBox();
    assert(messageBox && backBox && messageBox.y >= 0 && messageBox.y + messageBox.height <= backBox.y,
      `deletion message sits above Back at ${width}x${height}`);
    assert(backBox.y + backBox.height <= height + 1 && messageBox.x >= 0 && messageBox.x + messageBox.width <= width + 1,
      `message and Back fit at ${width}x${height}`);
  }
  await page.setViewportSize({ width: 412, height: 850 });
  await deleted.getByRole('button', { name: 'Cerrar aviso', exact: true }).click();
  assert.equal(await page.locator('.models .models-feedback').count(), 0, 'model notice can be dismissed');

  await page.reload();
  await page.locator('.welcome[role="dialog"]').waitFor({state:'detached'});
  await page.locator('.privacy-container:not(.hidden)').waitFor();
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')), 'gliner-pii-small-int8');
  await page.getByRole('button', {name:'Modelos',exact:true}).click();
  const back=page.locator('.models .back-btn');
  await back.scrollIntoViewIfNeeded();
  const backBox=await back.boundingBox();
  const footerBox=await page.locator('.models > .footer').boundingBox();
  assert(Math.abs(backBox.x+backBox.width-footerBox.x-footerBox.width)<2,'Back is aligned to bottom-right footer');
  console.log('PASS Android models: startup routing, progress, installed after reload, no redownload, GLiNER selection, delete and missing-model recovery');
} finally { await browser.close(); }
