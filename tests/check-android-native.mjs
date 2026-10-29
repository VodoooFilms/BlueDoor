/** Real JNI/Rust/llama/PDF QA on an isolated Android debug installation. */
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { connectAndroid } from './android-cdp.mjs';
const android = await connectAndroid(Number(process.env.BLUEDOOR_ANDROID_CDP || 9223));
const report = { environment: 'Android emulator/debug, CPU; not Redmi measurements', checks: [] };
const started = performance.now();
try {
  assert.equal(await android.invoke('app_platform'), 'android');
  await android.evaluate(`localStorage.setItem('bluedoor-welcome-seen','true');document.querySelector('.welcome .close')?.click()`);
  const catalog = await android.invoke('list_models');
  assert(catalog.privacy.find(m => m.id === 'qwen3-fast')?.installed, 'Install verified Qwen before QA');
  await android.invoke('configure_mobile_analysis', { contextTokens: 4096, chunkChars: 2000 });
  const original = 'Ana Perez presenta una demanda. Email: ana@example.com. Telefono: 0912345678.';
  let session = await android.invoke('import_private_text', { text: original, name: 'Sintetico.txt' });
  await assert.rejects(android.invoke('validate_private_export', { id: session.id, revision: session.revision, text: original, title: 'Documento protegido' }));
  report.checks.push('unreviewed original blocked');
  session = await android.invoke('analyze_private_document', { id: session.id, revision: session.revision, modelId: 'qwen3-fast' });
  assert(session.analyzed && session.analysisComplete, 'Qwen must complete every fragment');
  for (const value of ['Ana Perez', 'ana@example.com', '0912345678']) {
    assert(session.entities.some(e => e.original.includes(value)), `Qwen missed synthetic ${value}`);
    assert(!session.sanitized.includes(value));
  }
  session = await android.invoke('review_private_document', { id: session.id, revision: session.revision, entities: session.entities, confirm: true });
  assert(session.reviewed);
  const destination = await android.invoke('prepare_share_export', { defaultName: 'synthetic-protected.pdf' });
  const pdf = await android.invoke('generate_pdf', { title: 'Documento protegido', segments: [], language: 'es', model: 'local', duration: 0, destPath: destination, text: session.sanitized, html: null, speakerNames: null, font: 'system', size: 12, align: 'left', private: { id: session.id, revision: session.revision } });
  report.pdf = pdf;
  report.checks.push('real Qwen CPU with 4096 context; native review and PDF writer');
  const approvedRevision = session.revision;
  session = await android.invoke('update_private_document', { id: session.id, revision: session.revision, text: session.sanitized + ' Texto editado.' });
  assert(!session.reviewed && session.revision > approvedRevision);
  await assert.rejects(android.invoke('validate_private_export', { id: session.id, revision: approvedRevision, text: session.sanitized, title: 'Documento protegido' }));
  report.checks.push('edits invalidate previous approval');
  session = await android.invoke('import_private_text', { text: 'Luis Soto comparece en una diligencia nueva. Contacto luis@example.com. Identificacion 0923456789.', name: 'Cancelacion sintetica.txt' });
  const analysis = android.invoke('analyze_private_document', { id: session.id, revision: session.revision, modelId: 'qwen3-fast' });
  // Register rejection before cancellation so Node does not report it unhandled.
  const cancelled = analysis.then(() => ({ completed: true }), error => ({ error: String(error) }));
  await new Promise(r => setTimeout(r, 500));
  await android.invoke('cancel_private_analysis');
  assert((await cancelled).error?.includes('cancelado'));
  session = await android.invoke('get_privacy_session');
  assert(!session.reviewed && !session.analyzed);
  report.checks.push('generation cancellation leaves unapproved session');
  await android.invoke('unload_privacy_model');
  report.checks.push('model unload');
  report.performance = await android.invoke('get_local_performance');
  report.seconds = (performance.now() - started) / 1000;
  mkdirSync('artifacts/android-emulator', { recursive: true });
  const adb = join(process.env.ANDROID_HOME || join(homedir(), 'Library/Android/sdk'), 'platform-tools/adb');
  writeFileSync('artifacts/android-emulator/protected.pdf', execFileSync(adb, ['exec-out', 'run-as', 'com.bluedoor.desktop', 'cat', report.pdf], { maxBuffer: 16 * 1024 * 1024 }));
  writeFileSync('artifacts/android-emulator/native-workflow.json', JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally { android.close(); }
