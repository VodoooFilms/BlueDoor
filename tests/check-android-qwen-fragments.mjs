/** Real Android CPU regression: default context, multi-fragment input, no retry amplification. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { connectAndroid } from './android-cdp.mjs';

const adb = `${process.env.ANDROID_HOME || `${process.env.HOME}/Library/Android/sdk`}/platform-tools/adb`;
assert.equal(execFileSync(adb, ['-s', 'emulator-5554', 'shell', 'getprop', 'ro.boot.qemu'], { encoding: 'utf8' }).trim(), '1');
const android = await connectAndroid();
let listener;
try {
  const catalog = (await android.invoke('list_models')).privacy;
  assert(catalog.filter(model => model.id.startsWith('gliner')).every(model => !model.installed), 'Run with GLiNER assets absent');
  listener = await android.evaluate(`window.__qwenMeasured=[];window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'private-analysis-progress',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(event=>window.__qwenMeasured.push(event.payload[2]))})`);
  const text = readFileSync(new URL('./fixtures/android-legal-es.txt', import.meta.url), 'utf8').repeat(6);
  let session = await android.invoke('import_private_text', { text, name: 'long-synthetic.txt' });
  const started = performance.now();
  session = await android.invoke('analyze_private_document', { id: session.id, revision: session.revision, modelId: 'qwen3-fast' });
  const seconds = (performance.now() - started) / 1000;
  const phases = await android.evaluate('window.__qwenMeasured');
  const prefills = phases.filter(phase => phase.startsWith('prefill:') && phase.endsWith(':128')).length;
  const fragmentTokens = {};
  for (const phase of phases) {
    const match = /^(prefill|reading):(\d+)\/\d+:(\d+)$/.exec(phase);
    if (!match) continue;
    const [, kind, fragment, count] = match;
    const entry = fragmentTokens[fragment] ||= { input: 0, generated: 0 };
    const key = kind === 'prefill' ? 'input' : 'generated';
    entry[key] = Math.max(entry[key], Number(count));
  }
  const report = { fragmentTokens, environment: 'Android ARM64 emulator; no GLiNER', chars: text.length, contextTokens: 4096, chunkChars: 1200, overlapChars: 300, responseTokens: 384, seconds, prefills, complete: session.analysisComplete, warnings: session.warnings };
  mkdirSync('artifacts/android-emulator', { recursive: true });
  writeFileSync('artifacts/android-emulator/qwen-fragments.json', JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  assert(session.analysisComplete, 'Every fragment must complete');
  assert.equal(prefills, 4, 'The four source fragments must each be decoded once');
  for (const value of ['Ana Pérez', 'Comercial Lirio', 'ana@example.com', '0912345678']) assert(!session.sanitized.includes(value), 'Private value remains');
} finally {
  if (listener !== undefined) await android.invoke('plugin:event|unlisten', { event: 'private-analysis-progress', eventId: listener }).catch(() => {});
  await android.invoke('unload_privacy_model').catch(() => {});
  android.close();
}
