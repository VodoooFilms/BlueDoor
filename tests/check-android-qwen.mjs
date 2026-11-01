/** Real Android regression for the legal fixture that made Qwen repeat prompt examples. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { connectAndroid } from './android-cdp.mjs';

const adb = `${process.env.ANDROID_HOME || `${process.env.HOME}/Library/Android/sdk`}/platform-tools/adb`;
assert.equal(execFileSync(adb, ['-s', 'emulator-5554', 'shell', 'getprop', 'ro.boot.qemu'], { encoding: 'utf8' }).trim(), '1');
const android = await connectAndroid();
const text = readFileSync(new URL('./fixtures/android-legal-es.txt', import.meta.url), 'utf8');
const report = { environment: 'Android ARM64 emulator; not a physical phone', runs: [] };
let progressListener;
try {
  progressListener = await android.evaluate(`window.__qwenProgress=[];window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'private-analysis-progress',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(event=>window.__qwenProgress.push(event.payload[2]))})`);
  const catalog = (await android.invoke('list_models')).privacy;
  assert.equal(catalog.find(model => model.id === 'qwen3-fast')?.size, '397 MB');
  const standalone = process.env.BLUEDOOR_QA_QWEN_ONLY === '1';
  if (standalone) {
    assert(catalog.filter(model => model.id.startsWith('gliner')).every(model => !model.installed), 'Remove GLiNER test assets before the standalone regression');
  }
  report.withoutGliner = standalone;
  const models = standalone ? ['qwen3-fast', 'qwen3-fast'] : ['gliner-pii-small-int8', 'qwen3-fast', 'qwen3-fast', 'gliner-multi-pii-fp16'];
  for (const modelId of models) {
    let session = await android.invoke('import_private_text', { text, name: 'synthetic-legal.txt' });
    await android.evaluate("window.__qwenProgress=[]");
    const started = performance.now();
    session = await android.invoke('analyze_private_document', { id: session.id, revision: session.revision, modelId });
    assert(session.analyzed && session.analysisComplete, `${modelId}: ${session.warnings.join('; ')}`);
    for (const value of ['Ana Pérez', 'Comercial Lirio', 'ana@example.com', '0912345678']) {
      assert(!session.sanitized.includes(value), `${modelId} left a private value`);
    }
    assert(session.sanitized.includes('USD 1500'));
    const phases = await android.evaluate("window.__qwenProgress");
    const prefills = phases.filter(phase => phase.startsWith('prefill:') && phase.endsWith(':128')).length;
    if (modelId === 'qwen3-fast') {
      assert(prefills <= 1, 'Qwen must not decode the short fixture again for a retry');
      if (!report.runs.some(run => run.modelId === modelId)) assert.equal(prefills, 1, 'First Qwen run must perform real inference');
    }
    report.runs.push({ modelId, prefills, seconds: (performance.now() - started) / 1000, complete: session.analysisComplete, entities: session.entities.length });
  }
  mkdirSync('artifacts/android-emulator', { recursive: true });
  writeFileSync(`artifacts/android-emulator/qwen-${standalone ? 'standalone' : 'three-models'}-regression.json`, JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally {
  if (progressListener !== undefined) await android.invoke('plugin:event|unlisten', { event: 'private-analysis-progress', eventId: progressListener }).catch(() => {});
  await android.invoke('unload_privacy_model').catch(() => {});
  android.close();
}
