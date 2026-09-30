import { webkit } from "playwright";

const BASE = "http://localhost:5198/";

function fail(msg) {
  console.error("FAIL: " + msg);
  process.exit(1);
}
function ok(msg) {
  console.log("ok: " + msg);
}

const browser = await webkit.launch();
const page = await browser.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push("pageerror: " + e.message));
page.on("console", (m) => {
  if (m.type() === "error") errors.push("console: " + m.text());
});

await page.goto(BASE, { waitUntil: "networkidle" });

const editor = page.locator(".rich-editor");
await editor.waitFor({ state: "visible", timeout: 10000 });

const ce = await editor.evaluate((el) => el.contentEditable);
if (ce !== "true") fail("contentEditable != true, got " + ce);
ok("contentEditable=true");

await editor.click();
await page.keyboard.type(" HOLA");
const text1 = (await editor.textContent()) || "";
if (!text1.includes("HOLA")) fail("no se pudo TIPEAR sin correccion. text=" + text1);
ok("tipar sin correccion OK: " + text1.slice(0, 60));

await editor.click();
await page.keyboard.type(" MAS");
const text3 = (await editor.textContent()) || "";
if (!text3.includes("MAS"))
  fail("no se pudo TIPEAR después de editar: " + text3);
ok("tipar después de editar OK: " + text3.slice(0, 80));

await page.keyboard.press(process.platform === "darwin" ? "Meta+a" : "Control+a");
const bold = page.locator(".tb-styles .tb-btn").first();
await bold.click();
await page.waitForTimeout(100);
const htmlAfterBold = await editor.innerHTML();
const hasBold = /<(b|strong)>|<span[^>]*font-weight:\s*bold/i.test(htmlAfterBold);
if (!hasBold)
  fail("negrita no aplicada sobre la seleccion. html=" + htmlAfterBold.slice(0, 200));
ok("negrita sobre la seleccion OK");

const saveBtn = page.locator(".save-btn");
await saveBtn.click();
await page.waitForTimeout(100);
const lastSave = await page.evaluate(() => window.__lastSave ?? null);
if (!lastSave) fail("no se disparo onSave");
if (typeof lastSave.text !== "string" || !lastSave.text.length)
  fail("onSave text vacio");
if (!lastSave.html || !String(lastSave.html).includes("MAS"))
  fail("onSave html sin el contenido editado: " + JSON.stringify(lastSave.html));
ok("grabar OK, html guardado (" + String(lastSave.html).length + " chars)");

// Undo: deshace el último paso escrito
await editor.click();
await page.keyboard.type(" ZZZ");
await page.waitForTimeout(80);
let txt = (await editor.textContent()) || "";
if (!txt.includes("ZZZ")) fail("no se pudo TIPEAR antes de undo: " + txt);
const undoBtn = page.locator(".head-actions .head-btn", { hasText: "Deshacer" });
await undoBtn.click();
await page.waitForTimeout(80);
txt = (await editor.textContent()) || "";
if (txt.includes("ZZZ")) fail("undo no revirtio el ultimo paso: " + txt);
ok("undo revierte el ultimo paso");

// Reset: vuelve al texto que salió de la transcripción
const resetBtn = page.locator(".head-actions .head-btn", { hasText: "Restablecer" });
await resetBtn.click();
await page.waitForTimeout(80);
txt = (await editor.textContent()) || "";
if (!txt.includes("Hola, esta es una prueba de transcripcion."))
  fail("reset no volvio a la transcripcion original: " + txt);
ok("reset vuelve a la transcripcion original");

if (errors.length) {
  console.warn("WARN — errores JS capturados:");
  for (const e of errors.slice(0, 5)) console.warn("  " + e);
}
console.log("\nRESULTADO: TODO OK");
await browser.close();