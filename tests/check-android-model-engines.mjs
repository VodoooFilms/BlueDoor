/** Synthetic JNI regression: receipt, screen timeout, both CPU engines and safe export. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { connectAndroid } from './android-cdp.mjs';
const adb = `${process.env.ANDROID_HOME || `${process.env.HOME}/Library/Android/sdk`}/platform-tools/adb`;
const run = (...args) => execFileSync(adb, ['-s', 'emulator-5554', ...args], { encoding: 'utf8' }).trim();
assert.equal(run('shell', 'getprop', 'ro.boot.qemu'), '1', 'Only our synthetic emulator is permitted');
const android = await connectAndroid();
const report = { environment: 'Android 15 ARM64 emulator; not Redmi', checks: [], timings: {}, progress: {} };
const original = 'COMPROBANTE DE TRANSFERENCIA\nMaria Perez\nBANCO ANDINO\nCEDULA 0912345678\nEmail maria@example.com\nPROCESO 17230-2025-01234\nORDENANTE\nCARLOS ANDRES MUNOZ\nBENEFICIARIO: ANA LUCIA VERA\nTOTAL USD 1500';
const oldTimeout = run('shell', 'settings', 'get', 'system', 'screen_off_timeout');
try {
  run('shell', 'svc', 'wifi', 'disable'); run('shell', 'svc', 'data', 'disable');
  run('shell', 'input', 'keyevent', 'KEYCODE_WAKEUP'); run('shell', 'wm', 'dismiss-keyguard');
  await android.evaluate(`window.__qaPhases=[];window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'private-analysis-progress',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(event=>window.__qaPhases.push(event.payload[2]))})`);
  for (const modelId of (process.env.BLUEDOOR_ANDROID_QA_MODELS || 'qwen3-fast,gliner-pii-small-int8').split(',')) {
    const catalog = await android.invoke('list_models');
    assert(catalog.privacy.find(model => model.id === modelId)?.installed, `${modelId} must already be installed`);
    let session = await android.invoke('import_private_text', { text: original, name: 'recibo-sintetico.txt' });
    run('shell', 'settings', 'put', 'system', 'screen_off_timeout', '2000');
    run('shell', 'input', 'keyevent', 'KEYCODE_WAKEUP');
    await android.evaluate('window.__qaPhases=[]');
    const started = performance.now();
    const analysis = android.invoke('analyze_private_document', { id: session.id, revision: session.revision, modelId });
    const pending = analysis.then(value => ({ value }), error => ({ error }));
    await new Promise(resolve => setTimeout(resolve, 700));
    const windows = run('shell', 'dumpsys', 'window', 'windows');
    if (modelId === 'qwen3-fast') assert(windows.includes('KEEP_SCREEN_ON'), 'Qwen analysis must keep the display awake');
    const result = await pending;
    if (result.error) throw result.error;
    session = result.value;
    report.progress[modelId] = await android.evaluate('window.__qaPhases');
    if(modelId === 'qwen3-fast') assert(report.progress[modelId].some(phase=>phase.startsWith('prefill:')), 'Qwen must report input preparation, not a frozen zero output count');
    else assert(report.progress[modelId].some(phase=>phase.startsWith('ner:')), 'GLiNER must report entity detection');
    report.timings[modelId] = (performance.now() - started) / 1000;
    assert(session.analyzed && session.analysisComplete && !session.reviewed);
    for (const value of ['Maria Perez', 'BANCO ANDINO', '0912345678', 'maria@example.com', '17230-2025-01234', 'CARLOS ANDRES MUNOZ', 'ANA LUCIA VERA']) {
      assert(session.entities.some(entity => entity.original.includes(value)), `${modelId} missed synthetic entity ${value}`);
      assert(!session.sanitized.includes(value));
    }
    session = await android.invoke('review_private_document', { id: session.id, revision: session.revision, entities: session.entities, confirm: true });
    assert(session.reviewed);
    const destination = await android.invoke('prepare_share_export', { defaultName: 'receipt-protected.pdf' });
    await android.invoke('generate_pdf', { title: 'Documento protegido', segments: [], language: 'es', model: 'local', duration: 0, destPath: destination, text: session.sanitized, html: null, speakerNames: null, font: 'system', size: 12, align: 'left', private: { id: session.id, revision: session.revision } });
    report.checks.push(`${modelId}: offline receipt, display awake, complete map, approval and native PDF`);
    await android.invoke('unload_privacy_model');
    run('shell', 'settings', 'put', 'system', 'screen_off_timeout', oldTimeout);
  }
  report.performance = await android.invoke('get_local_performance');
  writeFileSync('artifacts/android-emulator/model-engines-v3.json', JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally {
  run('shell', 'settings', 'put', 'system', 'screen_off_timeout', oldTimeout);
  android.close();
}
