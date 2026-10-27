/** Match Windows WebView2 while preserving the existing WebKit lanes. */
import { chromium, webkit } from "playwright";

const name = process.env.BLUEDOOR_TEST_BROWSER || (process.platform === "win32" ? "chromium" : "webkit");
if (!["chromium", "webkit"].includes(name)) throw new Error(`Unknown QA browser: ${name}`);
export const browserType = name === "chromium" ? chromium : webkit;
