import { webkit } from 'playwright';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

const browser = await webkit.launch();
const startLabel = 'Descargar todos los modelos y aprobar permisos';
async function scenario(options = {}) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await page.addInitScript(options => {
    localStorage.setItem('bluedoor-welcome-seen', 'true');
    localStorage.setItem('bluedoor-language', options.lang || 'es');
    if (options.choices) {
      localStorage.setItem('bluedoor-model-stt', 'small');
      localStorage.setItem('bluedoor-privacy-model', 'qwen3-q8');
    }
    if (options.pending) localStorage.setItem('bluedoor-setup-pending', 'true');
    const installed = new Set(options.installed || []);
    window.setupCalls = [];
    let fail = options.fail;
    let release;
    let callbackId = 0;
    const callbacks = new Map();
    window.releaseSetup = () => { release?.(); release = null; };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: callback => { callbacks.set(++callbackId, callback); return callbackId; },
      invoke: async (cmd, args) => {
        if (cmd === 'list_models') return {
          whisper: [{ id: 'small', installed: installed.has('small') }],
          parakeet: [{ id: 'parakeet-tdt-0.6b-v3', installed: installed.has('parakeet-tdt-0.6b-v3') }],
          privacy: ['qwen3-fast', 'qwen3-q8'].map(id => ({ id, name: id === 'qwen3-fast' ? 'Qwen3-1.7B Q4_K_M' : 'Qwen3-1.7B Q8_0', size: '1.36 GB', installed: installed.has(id) })),
          diarization: [{ id: 'nemotron3-diarization', installed: installed.has('nemotron3-diarization') }],
        };
        if (cmd === 'load_transcripts') return [];
        if (cmd === 'get_privacy_session') return null;
        if (cmd === 'plugin:event|listen') {
          if (args.event === 'model-download-progress') window.emitProgress = (id, pct) => callbacks.get(args.handler)?.({ event: args.event, id: 1, payload: [id, pct] });
          return 1;
        }
        if (cmd === 'diarization_available') return !!options.engine;
        if (cmd === 'prepare_local_setup') {
          window.setupCalls.push('permissions');
          if (options.holdPermissions) await new Promise(resolve => { release = resolve; });
          if (options.denyFolders) throw 'Acceso a carpetas denegado';
        }
        if (cmd.startsWith('download_')) {
          const id = args?.id || 'nemotron3-diarization';
          window.setupCalls.push(id);
          window.emitProgress?.(id, 42);
          if (options.holdDownload === id) await new Promise(resolve => { release = resolve; });
          if (fail === id) { fail = null; throw 'Descarga interrumpida'; }
          installed.add(id);
        }
        if (cmd === 'cancel_model_download') { window.setupCalls.push('cancel'); window.releaseSetup(); }
      },
    };
    Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: {
      getUserMedia: async () => {
        window.setupCalls.push('microphone');
        if (options.denyMicrophone) throw new DOMException('Denied', 'NotAllowedError');
        return { getTracks: () => [{ stop: () => window.setupCalls.push('stop-microphone') }] };
      },
    } });
  }, options);
  await page.goto('http://localhost:1420');
  await page.locator('.settings-menu summary').click();
  await page.locator('#modelsToggle').click();
  return page;
}
try {
  const page = await scenario({ holdPermissions: true, engine: true, choices: true });
  const start = page.getByRole('button', { name: startLabel, exact: true });
  await start.click();
  await page.waitForFunction(() => window.setupCalls.includes('permissions'));
  assert.deepEqual(await page.evaluate(() => window.setupCalls), ['microphone', 'stop-microphone', 'permissions']);
  assert.equal(await page.locator('#recordToggle').isDisabled(), true);
  assert.equal(await page.getByRole('button', { name: 'Descargar', exact: true }).first().isDisabled(), true);
  assert.equal(await page.getByRole('button', { name: 'Cambiar', exact: true }).first().isDisabled(), true);
  await page.evaluate(() => window.releaseSetup());
  await page.locator('.privacy-container:not(.hidden)').waitFor();
  assert.equal(await page.locator('#modelsToggle').evaluate(el => el.classList.contains('nav-active')), false);
  await page.locator('.settings-menu summary').click();
  await page.locator('#modelsToggle').click();
  assert.deepEqual(await page.evaluate(() => window.setupCalls), ['microphone', 'stop-microphone', 'permissions', 'parakeet-tdt-0.6b-v3', 'qwen3-fast', 'nemotron3-diarization']);
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-privacy-model')), 'qwen3-fast');
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-model-stt')), 'parakeet-tdt-0.6b-v3');
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-setup-pending')), null);
  assert.equal(await page.getByText('✓ Descargado', { exact: true }).count(), 3);
  await mkdir('artifacts', { recursive: true });
  await page.screenshot({ path: 'artifacts/model-setup-complete.png' });
  await page.close();

  for (const failure of [{ denyMicrophone: true }, { denyFolders: true }]) {
    const page = await scenario(failure);
    await page.getByRole('button', { name: startLabel, exact: true }).click();
    await page.getByRole('button', { name: 'Reanudar preparación', exact: true }).waitFor();
    assert.equal(await page.evaluate(() => window.setupCalls.some(c => c.includes('parakeet') || c.includes('qwen') || c.includes('nemotron'))), false);
    assert.equal(await page.locator('#recordToggle').isDisabled(), false);
    await page.close();
  }

  const retry = await scenario({ fail: 'qwen3-fast' });
  await retry.getByRole('button', { name: startLabel, exact: true }).click();
  await retry.getByRole('alert').filter({ hasText: 'Descarga interrumpida' }).waitFor();
  assert.deepEqual(await retry.evaluate(() => window.setupCalls.slice(3)), ['parakeet-tdt-0.6b-v3', 'qwen3-fast']);
  await retry.getByRole('button', { name: 'Reanudar preparación', exact: true }).click();
  await retry.locator('.privacy-container:not(.hidden)').waitFor();
  assert.equal(await retry.evaluate(() => window.setupCalls.filter(c => c === 'parakeet-tdt-0.6b-v3').length), 1);
  await retry.close();

  const cancel = await scenario({ holdDownload: 'parakeet-tdt-0.6b-v3' });
  await cancel.getByRole('button', { name: startLabel, exact: true }).click();
  await cancel.getByText('42%', { exact: true }).waitFor();
  await cancel.screenshot({ path: 'artifacts/model-setup-progress.png' });
  await cancel.getByRole('button', { name: 'Cancelar descarga', exact: true }).click();
  await cancel.getByRole('button', { name: 'Reanudar preparación', exact: true }).waitFor();
  assert.equal(await cancel.evaluate(() => window.setupCalls.includes('qwen3-fast')), false);
  await cancel.close();

  const partial = await scenario({ installed: ['parakeet-tdt-0.6b-v3'], pending: true });
  await partial.getByRole('button', { name: 'Reanudar preparación', exact: true }).click();
  await partial.locator('.privacy-container:not(.hidden)').waitFor();
  assert.equal(await partial.evaluate(() => window.setupCalls.includes('parakeet-tdt-0.6b-v3')), false);
  await partial.close();

  const existing = await scenario({ installed: ['small'] });
  assert.equal(await existing.getByRole('button', { name: startLabel, exact: true }).count(), 0);
  await existing.close();

  const preview = await scenario();
  await preview.getByRole('button', { name: startLabel, exact: true }).waitFor();
  await preview.screenshot({ path: 'artifacts/model-setup-initial.png' });
  await preview.close();
  const english = await scenario({ lang: 'en', engine: true });
  await english.getByRole('button', { name: 'Download all models and approve permissions', exact: true }).click();
  await english.locator('.privacy-container:not(.hidden)').waitFor();
  await english.close();
  console.log('Setup: permissions before sequential default downloads, stopped microphone, navigation locks, denial, failure/retry, cancellation, reload resume, visibility and automatic navigation to Private Legal passed.');
} finally { await browser.close(); }
