import { browserType } from './browser.mjs';
import assert from "node:assert/strict";
const browser = await browserType.launch();
try {
  const page = await browser.newPage();
  await page.goto("http://localhost:5198/pcm.html");
  await page.locator("#record").click();
  await page.locator("#result").waitFor({ timeout: 15000 });
  const result = JSON.parse(await page.locator("#result").textContent());
  assert.equal(result.ok, true, JSON.stringify(result));
  console.log(`PASS: ${browserType.name()} AudioWorklet captures direct PCM/WAV`, result);
} finally { await browser.close(); }
