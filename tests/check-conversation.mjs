import { browserType } from './browser.mjs';
import assert from 'node:assert/strict';

const browser = await browserType.launch();
const page = await browser.newPage({ viewport: { width: 1080, height: 740 } });
// These fixtures assert Spanish UI labels; do not depend on the app default.
await page.addInitScript(() => localStorage.setItem('bluedoor-language', 'es'));
const errors = [];
page.on('pageerror', e => errors.push(e.message));
const fixture = {
  id: 'conversation-layout', title: 'Conversación',
  text: 'Hablante 1: Hola, Sol.\nUna segunda línea.\nHablante 2: Hola, Antonio.',
  segments: [
    { start: 0, end: 2, text: 'Hola, Sol.', speaker: 1 },
    { start: 2, end: 4, text: 'Una segunda línea.', speaker: 1 },
    { start: 4, end: 6, text: 'Hola, Antonio.', speaker: 2 },
  ],
  speakerNames: { 1: 'Antonio', 2: 'Sol' }, duration: 6,
  language: 'es', model: 'parakeet', audioSource: 'recording',
};
async function load(transcript) {
  await page.addInitScript(value => { window.__transcriptFixture = value; }, transcript);
  await page.goto('http://localhost:5198/');
  await page.locator('.rich-editor').waitFor();
}
const editor = page.locator('.rich-editor');
await load(fixture);
assert.deepEqual(await editor.locator(':scope > div').allTextContents(),
  ['Antonio:', 'Hola, Sol.', 'Una segunda línea.', 'Sol:', 'Hola, Antonio.']);
assert.equal(await editor.locator('strong').count(), 2);
assert.equal(await editor.locator('div').nth(2).evaluate(el => getComputedStyle(el).paddingLeft), '48px');
// New paragraphs inherit the body indentation in WKWebView/WebKit.
await editor.evaluate(el => {
  const range = document.createRange();
  range.selectNodeContents(el.lastElementChild); range.collapse(false);
  getSelection().removeAllRanges(); getSelection().addRange(range); el.focus();
});
await page.keyboard.press('Enter');
await page.keyboard.type('Texto adicional.');
assert.equal(await editor.locator('div').last().evaluate(el => getComputedStyle(el).paddingLeft), '48px');
await page.locator('.speaker-field input').first().fill('Ana');
await page.locator('.speaker-field input').first().blur();
assert.equal(await editor.locator('strong').first().textContent(), 'Ana:');
assert.ok((await editor.textContent()).includes('Hola, Antonio.'));
await page.locator('.save-btn').click();
const saved = await page.evaluate(() => window.__lastSave);
assert.ok(saved.html.includes('padding-left: 48px'));
assert.ok(saved.text.includes('Ana:\nHola, Sol.'));
await load({ ...fixture, corrected: saved.text, correctedHtml: saved.html, speakerNames: saved.speakerNames });
assert.equal(await editor.innerHTML(), saved.html);
// Existing rich text is reorganized without losing emphasis or words.
await load({ ...fixture, corrected: 'Antonio: Hola, Sol.\nSol: Hola.',
  correctedHtml: '<div>Antonio: Hola, <i>Sol</i>.</div><div>Sol: Hola.</div>' });
assert.equal(await editor.locator('i').textContent(), 'Sol');
assert.equal(await editor.locator('strong').count(), 2);
const reset = page.locator('.head-btn', { hasText: 'Restablecer' });
assert.equal(await reset.isDisabled(), true, 'loaded document has no manual edits');
await editor.click();
await page.keyboard.type(' Edición manual.');
assert.equal(await reset.isEnabled(), true, 'manual editing enables reset');
await reset.click();
assert.equal(await reset.isDisabled(), true, 'reset establishes a clean baseline');
assert.equal(await editor.locator('strong').count(), 2);
assert.ok((await editor.textContent()).includes('Una segunda línea.'));
await page.screenshot({ path: '/tmp/bluedoor-conversation.png' });
await load({ ...fixture, segments: [{ start: 0, end: 1, text: 'Texto sencillo.' }], text: 'Texto sencillo.' });
assert.equal(await editor.innerHTML(), '<div>Texto sencillo.</div>');
assert.deepEqual(errors, []);
await browser.close();
console.log('OK: layout, continuation, rename, save/reopen, rich-text migration, reset, plain transcript');
