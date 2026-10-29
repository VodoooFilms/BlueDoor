/** Bundled OCR over actual Android SAF/JNI, using only synthetic test images. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { connectAndroid } from './android-cdp.mjs';
const a = await connectAndroid();
try {
  for (const extension of ['png', 'pdf']) {
    const pending = a.invoke('import_private_document').then(value => ({ value }), error => ({ error: String(error) }));
    await new Promise(r => setTimeout(r, 750));
    execFileSync('python3', ['tests/android-pick-file.py', `bluedoor-synthetic-scan.${extension}`], { stdio: 'inherit' });
    const { value: session, error } = await pending;
    assert(!error && session, error || 'Picker cancelled');
    for (const text of ['Maria Perez', '0912345678', 'USD 1500']) assert(session.original.includes(text));
    assert(!session.reviewed && !session.analysisComplete);
    mkdirSync('artifacts/android-emulator', { recursive: true });
    writeFileSync(`artifacts/android-emulator/ocr-${extension}.json`, JSON.stringify({ passed: true, bundledOfflineOCR: true, input: extension, reviewed: false }, null, 2));
  }
  console.log('PASS native Android SAF + bundled OCR: image and scanned PDF');
} finally { a.close(); }
