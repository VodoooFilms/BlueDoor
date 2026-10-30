import assert from 'node:assert/strict';
import { browserType } from './browser.mjs';
const browser = await browserType.launch();
try {
  const page = await browser.newPage({ viewport: { width: 412, height: 850 }, reducedMotion: "reduce" });
  await page.addInitScript(() => {
    localStorage.setItem('bluedoor-language', 'es');
    localStorage.setItem('bluedoor-welcome-seen', 'true');
    localStorage.setItem('bluedoor-privacy-model', 'gliner-pii-small-int8');
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
        { id: 'qwen3-fast', name: 'Qwen3-1.7B Q4_K_M', size: '1.36 GB', installed: installed.includes('qwen3-fast') },
        { id: 'gliner-multi-pii-fp16', name: 'GLiNER Multi PII · español · FP16', size: '579,7 MB + tokenizador 16,3 MB', installed: installed.includes('gliner-multi-pii-fp16') },
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
  await page.getByRole('heading', { name: 'Modelos locales:' }).waitFor();
  assert(await page.locator('.privacy-model-row').getByText('GLiNER Multi PII · español · FP16', {exact:true}).isVisible(), 'GLiNER is the default on Android');
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')), 'gliner-multi-pii-fp16', 'Former default migrates once to multilingual PII');
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
  assert(Math.abs(middle(geometry.name) - middle(geometry.edit)) < 5 && Math.abs(middle(geometry.edit) - middle(geometry.trash)) < 5, 'Name, edit and delete share one row');
  assert(geometry.progress.y >= geometry.edit.y + geometry.edit.height, 'Loader sits on its own next row');
  await page.screenshot({ path:'artifacts/android-models-downloading-v3.png', fullPage:true });
  await page.evaluate(() => window.__finishDownload());
  await page.getByText('Instalado', { exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).count(), 0);
  await page.reload();
  await page.locator('.welcome[role="dialog"] .close').click();
  await page.locator('.privacy-container:not(.hidden)').waitFor();
  await page.getByRole('button', { name: 'Modelos', exact: true }).click();
  await page.getByText('Instalado', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.__modelCalls.filter(call => call.cmd === 'download_privacy_model').length), 0, 'Reopening must not download again');
  await page.screenshot({ path: 'artifacts/android-models-installed.png', fullPage: true });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'No horizontal overflow');
  await page.getByRole('button', { name: 'Cambiar', exact: true }).click();
  await page.getByRole('button', { name: /Qwen3-1.7B/ }).click();
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-privacy-model')), 'qwen3-fast');
  await page.getByRole('button', { name: 'Descargar', exact: true }).click();
  await page.waitForFunction(() => typeof window.__finishDownload === 'function');
  await page.evaluate(() => window.__finishDownload());
  await page.getByText('Instalado', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Eliminar', exact: true }).click();
  await page.locator('.danger').click();
  await page.getByText('Instalado', {exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')), 'gliner-multi-pii-fp16', 'Deleting Qwen selects installed GLiNER');
  await page.reload();
  await page.locator('.welcome[role="dialog"] .close').click();
  await page.locator('.privacy-container:not(.hidden)').waitFor();
  assert.equal(await page.evaluate(()=>localStorage.getItem('bluedoor-privacy-model')), 'gliner-multi-pii-fp16');
  await page.getByRole('button', {name:'Modelos',exact:true}).click();
  const back=page.locator('.models .back-btn');
  await back.scrollIntoViewIfNeeded();
  const backBox=await back.boundingBox();
  const footerBox=await page.locator('.models > .footer').boundingBox();
  assert(Math.abs(backBox.x+backBox.width-footerBox.x-footerBox.width)<2,'Back is aligned to bottom-right footer');
  console.log('PASS Android models: startup routing, progress, installed after reload, no redownload, GLiNER selection, delete and missing-model recovery');
} finally { await browser.close(); }
