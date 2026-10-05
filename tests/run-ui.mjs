/** Start isolated app/harness servers, run the UI suite, then release both ports. */
import { createServer } from "vite";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const servers = [];
let child;
let interrupted = false;
function interrupt() {
  interrupted = true;
  child?.kill("SIGTERM");
}
process.on("SIGINT", interrupt);
process.on("SIGTERM", interrupt);

async function runCommand(command, args, env = process.env) {
  if (interrupted) throw new Error("UI tests interrupted");
  await new Promise((resolve, reject) => {
    child = spawn(command, args, { cwd: root, env, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      child = undefined;
      if (code === 0) resolve();
      else reject(new Error(`${command} failed (${signal || code})`));
    });
  });
}
async function run(file, language = "es") {
  console.log(`\nRunning ${file} (${language})`);
  await runCommand(process.execPath, [`tests/${file}`], { ...process.env, BELEGAL_TEST_LANG: language });
}

try {
  mkdirSync(`${root}artifacts`, { recursive: true });
  const layoutInputs = ["layout-document.pdf.preview.json", "layout-document.pdf.plan.json",
    "rich-edited.preview.json", "text-blocks.json", "paragraph-before.preview.json",
    "paragraph-edited.preview.json", "paragraph-blocks.json"];
  if (process.argv.includes("--layout") && layoutInputs.some(file => !existsSync(`${root}artifacts/layout-workflow/${file}`))) {
    console.log("Regenerating native layout test inputs after cleanup...");
    await runCommand("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "layout::tests"]);
  }
  for (const config of ["vite.config.ts", "tests/vite.config.ts"]) {
    const server = await createServer({ configFile: `${root}${config}` });
    servers.push(server);
    await server.listen();
  }
  if (process.argv.includes("--privacy")) {
    for (const language of ["es", "en"]) await run("check-private-ai.mjs", language);
  } else if (process.argv.includes("--workspace")) {
    for (const language of ["es", "en"]) await run("check-workspace.mjs", language);
    for (const test of ["check-alias-groups.mjs", "check-editor.mjs"]) await run(test);
  } else if (process.argv.includes("--layout")) {
    for (const test of ["check-layout.mjs", "check-layout-rich.mjs", "check-layout-paragraph.mjs", "check-alias-groups.mjs"]) await run(test);
  } else {
    for (const language of ["es", "en"]) await run("check-private-ai.mjs", language);
    for (const test of [
      "check-model-setup.mjs", "check-welcome.mjs", "check-editor.mjs",
      "check-history.mjs", "check-conversation.mjs", "check-pcm.mjs",
    ]) await run(test);
  }
  console.log("\nAll requested UI workflows passed.");
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  await Promise.allSettled(servers.map(server => server.close()));
  process.off("SIGINT", interrupt);
  process.off("SIGTERM", interrupt);
}
