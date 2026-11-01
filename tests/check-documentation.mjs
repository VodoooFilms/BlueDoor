import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { browserType } from './browser.mjs';

mkdirSync('artifacts/documentation', { recursive: true });
const browser = await browserType.launch();
try {
  for (const lang of ['es', 'en']) {
    for (const mobile of [false, true]) {
      const page = await browser.newPage({ viewport: mobile ? { width: 390, height: 844 } : { width: 1280, height: 828 } });
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.addInitScript(({ lang, mobile }) => {
        localStorage.setItem('bluedoor-language', lang);
        window.__repositoryCalls = [];
        window.__supportCalls = [];
        window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
        window.__TAURI_INTERNALS__ = {
          metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
          transformCallback: () => 1,
          invoke: async (cmd, args) => {
            if (cmd === 'app_platform') return mobile ? 'android' : 'macos';
            if (cmd === 'list_models') return { whisper: [], parakeet: [], diarization: [], privacy: [{ id: mobile ? 'gliner-pii-small-int8' : 'qwen3-fast', installed: true }] };
            if (cmd === 'get_privacy_session') return null;
            if (cmd === 'load_transcripts') return [];
            if (cmd === 'plugin:event|listen') return 1;
            if (cmd === 'interrupted_private_session' || cmd === 'diarization_available') return false;
            if (cmd === 'open_bluedoor_support') window.__supportCalls.push(args);
            if (cmd === 'open_documentation_repository') {
              window.__repositoryCalls.push(args ?? null);
              if (window.__repositoryCalls.length === 1) throw Error('Browser unavailable');
            }
          },
        };
      }, { lang, mobile });
      await page.goto('http://localhost:1420');
      await page.locator('.blue-import-screen').waitFor();
      const settings = page.locator('.settings-menu summary');
      await settings.click();
      const toggle = page.locator('#documentationToggle');
      assert.match(await toggle.evaluate(el => el.nextElementSibling.textContent), /Bluedoor/,
        'Documentation sits immediately above About');
      await toggle.click();
      const dialog = page.getByRole('dialog', { name: lang === 'es' ? 'Documentación' : 'Documentation', exact: true });
      await dialog.waitFor();
      assert.equal(await page.locator('.blue-import-screen').count(), 1, 'opening documentation keeps the workspace mounted');
      assert.equal(await dialog.locator('section').count(), 3);
      assert.match(await dialog.innerText(), /GLiNER/);
      assert.match(await dialog.innerText(), /Qwen/);
      assert.match(await dialog.innerText(), /VodoooFilms/);
      assert.match(await dialog.locator('section strong').first().innerText(), lang === 'es' ? /revisar manualmente/ : /manually review/);
      const mark = dialog.locator('.isotype svg');
      assert.equal(await mark.getAttribute('viewBox'), '406.5 321.5 211 381', 'canonical door proportions');
      assert.equal(await dialog.locator('.isotype').innerText(), '', 'isotype only, no wordmark');
      assert(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth), 'page fits viewport');
      await page.screenshot({ path: `artifacts/documentation/${lang}-${mobile ? 'mobile' : 'desktop'}.png` });
      assert.equal(await dialog.getByRole('button', { name: /GitHub/ }).count(), 0, 'Documentation has no GitHub button');
      await dialog.getByRole('button', { name: lang === 'es' ? 'Invítame un café · PayPal' : 'Buy me a coffee · PayPal', exact: true }).click();
      assert.deepEqual(await page.evaluate(() => window.__supportCalls), [{ lang }], 'support passes only the UI language');
      const close = dialog.getByRole('button', { name: lang === 'es' ? 'Cerrar documentación' : 'Close documentation', exact: true });
      await close.click();
      await dialog.waitFor({ state: 'hidden' });
      await page.waitForFunction(() => document.activeElement === document.querySelector('.settings-menu summary'));
      await settings.click();
      await toggle.click();
      await dialog.waitFor();
      if (mobile) {
        await page.setViewportSize({ width: 844, height: 390 });
        await dialog.locator('footer').scrollIntoViewIfNeeded();
        const box = await close.boundingBox();
        assert(box && box.y >= 0 && box.y + box.height <= 390, 'close stays accessible in landscape after scrolling');
      }
      await page.keyboard.press('Escape');
      await dialog.waitFor({ state: 'hidden' });
      assert.deepEqual(errors, []);
      await page.close();
    }
  }
  console.log('Documentation: ES/EN, desktop/Android, isotype, menu placement, manual-review copy, no GitHub button, support link, close, Escape and focus passed.');
} finally { await browser.close(); }
