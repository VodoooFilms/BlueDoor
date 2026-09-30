import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  root: import.meta.dirname,
  publicDir: "../public",
  plugins: [svelte()],
  server: { port: 5198, strictPort: true },
});