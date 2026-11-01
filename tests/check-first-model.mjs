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
  async function openModels() {
    await page.goto('http://localhost:1420');
    const welcome = page.locator('.welcome[role="dialog"] .close');
    await welcome.waitFor({state:"visible", timeout:3000}).catch(() => {});
    if (await welcome.count()) await welcome.click();
    await page.getByRole('heading', { name: 'Modelos de IA locales:' }).waitFor();
  }
  async function download() {
    await page.locator('.privacy-model-status .download-btn').click();
    await page.waitForFunction(() => typeof window.__finishDownload === 'function');
    await page.evaluate(() => window.__finishDownload());
    await page.locator('.privacy-model-status .ok').waitFor();
  }
  await openModels();
  await download();
  await page.getByText('Modelo listo', { exact: true }).waitFor();
  await page.waitForTimeout(900);
  assert(await page.locator('.models').isVisible(), 'success remains visible before the two-second delay');
  await page.locator('.blue-import-screen').waitFor();
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-first-privacy-model-ready')), 'true');

  // Removing/reinstalling a model later must never repeat first-use navigation.
  await page.evaluate(() => localStorage.setItem('qa-installed-models', '[]'));
  await openModels();
  await download();
  await page.waitForTimeout(2300);
  assert(await page.locator('.models').isVisible(), 'later downloads stay on Models');

  // Explicit interaction cancels navigation, while retaining the once-only marker.
  await page.evaluate(() => { localStorage.setItem('qa-installed-models', '[]'); localStorage.removeItem('bluedoor-first-privacy-model-ready'); });
  await openModels();
  await download();
  await page.getByText('Modelo listo', { exact: true }).waitFor();
  await page.getByRole('heading', { name: 'Modelos de IA locales:' }).click();
  await page.waitForTimeout(2300);
  assert(await page.locator('.models').isVisible(), 'interaction cancels automatic navigation');
  assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-first-privacy-model-ready')), 'true');
  console.log('First privacy model: delayed navigation, persistence and interaction cancellation passed.');
} finally { await browser.close(); }
