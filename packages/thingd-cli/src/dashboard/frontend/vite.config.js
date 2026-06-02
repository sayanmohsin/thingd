import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: "../public",
    emptyOutDir: true,
  },
});
