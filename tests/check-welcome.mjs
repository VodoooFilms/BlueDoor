import { browserType } from './browser.mjs';
import assert from 'node:assert/strict';
const browser = await browserType.launch();
try {
  for (const scenario of [
    { voice: true, privacy: true, lang: null, cta: 'Get started' },
    { voice: true, privacy: true, lang: 'invalid', cta: 'Get started' },
    { voice: true, privacy: true, lang: 'es', cta: 'Comenzar' },
    { voice: true, privacy: false, lang: 'es', cta: 'Comenzar' },
    { voice: false, privacy: true, lang: 'en', cta: 'Get started' },
  ]) {
    const page = await browser.newPage({ viewport: { width: 1370, height: 800 }, locale: 'es-EC' });
    await page.addInitScript(({ voice, privacy, lang }) => {
      if (lang !== null) localStorage.setItem('bluedoor-language', lang);
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback: () => 1,
        invoke: async cmd => {
          if (cmd === 'app_platform') { await new Promise(resolve => setTimeout(resolve, 400)); return 'macos'; }
          if (cmd === 'list_models') return {
            whisper: [{ id: 'small', installed: voice }], parakeet: [], diarization: [],
            privacy: [{ id: 'qwen3-fast', installed: privacy, name: 'Qwen', size: '1 GB' }],
          };
          if (cmd === 'load_transcripts') return [];
          if (cmd === 'get_privacy_session') return null;
          if (cmd === 'plugin:event|listen') return 1;
          if (cmd === 'diarization_available') return false;
        },
      };
    }, scenario);
    await page.goto('http://localhost:1420');
    await page.locator('.welcome[role="dialog"]').waitFor();
    assert(await page.locator('.welcome[role="dialog"]').evaluate(el => { const r = el.getBoundingClientRect(); return r.width === innerWidth && r.height === innerHeight; }), 'welcome covers the first render while native startup is pending');
    const action = page.getByRole('button', { name: scenario.cta, exact: true });
    await action.waitFor();
    assert.equal(await page.locator('html').getAttribute('lang'), scenario.cta === 'Get started' ? 'en' : 'es', 'English is the default even with a Spanish browser; saved preferences remain effective');
    const brandBox = await page.locator('.welcome-brand').boundingBox();
    const actionBox = await action.boundingBox();
    assert(Math.abs(brandBox.x + brandBox.width / 2 - actionBox.x - actionBox.width / 2) < 1, 'welcome brand and action share a centre');
    assert(Number(await action.evaluate(el => getComputedStyle(el).fontWeight)) >= 700, 'welcome action is bold');
    await action.click();
    assert.equal(await page.getByRole('dialog').count(), 0);
    if (!scenario.voice || !scenario.privacy) await page.locator('.models').waitFor();
    else await page.locator('.blue-import-screen').waitFor();
    assert.equal(await page.evaluate(() => localStorage.getItem('bluedoor-welcome-seen')), 'true');
    await page.reload();
    await page.locator('.brand').waitFor();
    await page.waitForTimeout(250);
    assert.equal(await page.getByRole('dialog').count(), 1, 'welcome opens on every launch');
    await page.locator('.welcome[role="dialog"] .close').click();
    await page.locator('.brand').click();
    await page.getByRole('button', { name: scenario.cta, exact: true }).waitFor();
    if (scenario.voice && scenario.privacy) await page.screenshot({ path: 'artifacts/bluedoor-welcome.png' });
    await page.keyboard.press('Escape');
    assert.equal(await page.getByRole('dialog').count(), 0);
    await page.locator('.brand').click();
    await page.getByRole('button', { name: scenario.cta === 'Get started' ? 'Close welcome' : 'Cerrar bienvenida' }).click();
    assert.equal(await page.getByRole('dialog').count(), 0);
    await page.close();
  }
  console.log('Welcome: first launch, readiness, Models CTA, persistence, About, Escape and close passed; fresh/invalid settings default to English, saved ES/EN persist.');
} finally { await browser.close(); }
