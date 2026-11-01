import assert from 'node:assert/strict';
import { privacyModel, MOBILE_PRIVACY_MODEL } from '../src/lib/privacy-model.ts';
const saved = new Map();
globalThis.localStorage = { getItem: key => saved.get(key) ?? null, setItem: (key, value) => saved.set(key, value) };
assert.equal(privacyModel(false), 'qwen3-fast');
assert.equal(saved.has('bluedoor-privacy-model'), false, 'initial desktop rendering must not overwrite Android defaults');
assert.equal(privacyModel(true), MOBILE_PRIVACY_MODEL);
for (const id of [MOBILE_PRIVACY_MODEL, 'gliner-multi-pii-fp16', 'qwen3-fast']) {
  saved.clear(); saved.set('bluedoor-privacy-model', id);
  assert.equal(privacyModel(false), id, 'asynchronous platform detection must preserve a valid choice');
  assert.equal(privacyModel(true), id, 'entering Android must preserve an explicit choice');
  assert.equal(saved.get('bluedoor-privacy-model'), id);
}
saved.set('bluedoor-privacy-model', 'removed-model');
assert.equal(privacyModel(true), MOBILE_PRIVACY_MODEL);
console.log('PASS model selection survives asynchronous platform detection and defaults do not overwrite explicit choices');
