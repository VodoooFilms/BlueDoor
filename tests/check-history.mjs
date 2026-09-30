import { webkit } from "playwright";
import assert from "node:assert/strict";

const browser = await webkit.launch();
try {
  for (const failedLoad of [true, false]) {
    const page = await browser.newPage();
    await page.addInitScript(({ failedLoad }) => {
      localStorage.setItem("belegal-local-demo-welcome-seen", "true");
      window.__saveCalls = 0;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd) => {
          if (cmd === "check_installation") return;
          if (cmd === "load_transcripts") {
            if (failedLoad) throw new Error("History cannot be read; original preserved");
            return [];
          }
          if (cmd === "save_transcripts") {
            window.__saveCalls += 1;
            throw new Error("Disk write failed");
          }
          if (cmd === "get_privacy_session") return null;
          if (cmd === "plugin:event|listen") return 1;
          if (cmd === "plugin:event|unlisten") return;
          if (cmd === "list_models") return { whisper: [{ installed: true }], parakeet: [], diarization: [], privacy: [{ installed: false }] };
          if (cmd === "get_active_llm") return "qwen15";
          if (cmd === "get_model_path") return "/mock/model.bin";
          if (cmd === "check_model_exists") return true;
          throw new Error(`Unexpected command ${cmd}`);
        },
        transformCallback: () => 0,
      };
    }, { failedLoad });
    await page.goto("http://localhost:5198/history.html");
    await page.locator("#recordToggle").click();
    const alert = page.locator(failedLoad ? ".boot-detail" : ".storage-error").first();
    await alert.waitFor();
    const message = await alert.textContent();
    if (failedLoad) {
      assert.match(message, /History cannot be read/);
      assert.equal(await page.locator(".recorder").count(), 0);
      assert.equal(await page.evaluate(() => window.__saveCalls), 0);
    } else {
      assert.match(message, /Disk write failed/);
      assert.equal(await page.evaluate(() => window.__saveCalls), 1);
    }
    await page.close();
  }
  console.log("PASS: load failure blocks overwrite/new work; save failure stays visible.");
} finally {
  await browser.close();
}
