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
        { id: 'qwen3-fast', name: 'Qwen3-1.7B Q4_K_M', size: '1.36 GB', installed: installed.includes('qwen3-fast') },
        { id: 'gliner-pii-small-int8', name: 'GLiNER PII Small v1.0 · INT8 (experimental)', size: '82.7 MB + tokenizador 3.6 MB', installed: installed.includes('gliner-pii-small-int8') },
      ] };
      if (cmd === 'download_privacy_model') {
        await new Promise(resolve => { window.__finishDownload = resolve; });
        localStorage.setItem('qa-installed-models', JSON.stringify([...installed, args.id]));
      }
      if (cmd === 'delete_privacy_model') localStorage.setItem('qa-installed-models', JSON.stringify(installed.filter(id => id !== args.id)));
    } };
  });
  await page.goto('http://localhost:1420');
  await page.getByRole('heading', { name: 'Modelos locales:' }).waitFor();
  assert.equal(await page.locator('.models .card').count(), 1, 'No audio model cards on Android');
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).count(), 1);
  await page.getByRole('button', { name: 'Descargar', exact: true }).click();
  await page.getByRole('button', { name: 'Cancelar descarga', exact: true }).waitFor();
  assert(await page.getByRole('button', { name: 'Modelos', exact: true }).isDisabled());
  await page.evaluate(() => window.__finishDownload());
  await page.getByText('Instalado', { exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).count(), 0);
  await page.reload();
  await page.locator('.privacy-container:not(.hidden)').waitFor();
  await page.getByRole('button', { name: 'Modelos', exact: true }).click();
  await page.getByText('Instalado', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.__modelCalls.filter(call => call.cmd === 'download_privacy_model').length), 0, 'Reopening must not download again');
  await page.screenshot({ path: 'artifacts/android-models-installed.png', fullPage: true });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'No horizontal overflow');
  await page.getByRole('button', { name: 'Cambiar', exact: true }).click();
  await page.getByRole('button', { name: /GLiNER PII Small/ }).click();
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-privacy-model')), 'gliner-pii-small-int8');
  await page.getByRole('button', { name: 'Descargar', exact: true }).click();
  await page.waitForFunction(() => typeof window.__finishDownload === 'function');
  await page.evaluate(() => window.__finishDownload());
  await page.getByText('Instalado', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Eliminar', exact: true }).click();
  await page.locator('.danger').click();
  await page.getByRole('button', { name: 'Descargar', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('heading', { name: 'Modelos locales:' }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).count(), 1);
  console.log('PASS Android models: startup routing, progress, installed after reload, no redownload, GLiNER selection, delete and missing-model recovery');
} finally { await browser.close(); }
