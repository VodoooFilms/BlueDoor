/** Start isolated app/harness servers, run the UI suite, then release both ports. */
import { createServer } from "vite";
import { spawn } from "node:child_process";
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

async function run(file, language = "es") {
  if (interrupted) throw new Error("UI tests interrupted");
  console.log(`\nRunning ${file} (${language})`);
  await new Promise((resolve, reject) => {
    child = spawn(process.execPath, [`tests/${file}`], {
      cwd: root,
      env: { ...process.env, BELEGAL_TEST_LANG: language },
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      child = undefined;
      if (code === 0) resolve();
      else reject(new Error(`${file} failed (${signal || code})`));
    });
  });
}

try {
  for (const config of ["vite.config.ts", "tests/vite.config.ts"]) {
    const server = await createServer({ configFile: `${root}${config}` });
    servers.push(server);
    await server.listen();
  }
  if (process.argv.includes("--layout")) {
    for (const test of ["check-layout.mjs", "check-layout-rich.mjs", "check-alias-groups.mjs"]) await run(test);
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
