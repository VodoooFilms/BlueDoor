import { webkit } from 'playwright';
import assert from 'node:assert/strict';
const browser = await webkit.launch();
try {
  for (const scenario of [
    { voice: true, privacy: true, lang: 'es', cta: 'Comenzar' },
    { voice: true, privacy: false, lang: 'es', cta: 'Descargar modelos locales' },
    { voice: false, privacy: true, lang: 'en', cta: 'Download local models' },
  ]) {
    const page = await browser.newPage({ viewport: { width: 1370, height: 800 } });
    await page.addInitScript(({ voice, privacy, lang }) => {
      localStorage.setItem('local-legal-language', lang);
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback: () => 1,
        invoke: async cmd => {
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
    await page.getByRole('button', { name: scenario.cta, exact: true }).click();
    assert.equal(await page.getByRole('dialog').count(), 0);
    if (!scenario.voice || !scenario.privacy) await page.locator('.models').waitFor();
    assert.equal(await page.evaluate(() => localStorage.getItem('local-legal-welcome-seen')), 'true');
    await page.reload();
    await page.locator('.brand').waitFor();
    await page.waitForTimeout(250);
    assert.equal(await page.getByRole('dialog').count(), 0, 'welcome must not reopen automatically');
    await page.locator('.brand').click();
    await page.getByRole('button', { name: scenario.cta, exact: true }).waitFor();
    if (scenario.voice && scenario.privacy) await page.screenshot({ path: 'artifacts/local-legal-welcome.png' });
    await page.keyboard.press('Escape');
    assert.equal(await page.getByRole('dialog').count(), 0);
    await page.locator('.brand').click();
    await page.getByRole('button', { name: scenario.lang === 'en' ? 'Close welcome' : 'Cerrar bienvenida' }).click();
    assert.equal(await page.getByRole('dialog').count(), 0);
    await page.close();
  }
  console.log('Welcome: first launch, readiness, Models CTA, persistence, About, Escape and close passed (ES/EN).');
} finally { await browser.close(); }
