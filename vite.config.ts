import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error type error without @types/node package
import { readFileSync } from "node:fs";
const host = process.env.TAURI_DEV_HOST;

/** The website demo (`npm run build:demo`, published by Cloudflare Pages from `main`): it opens in a
 *  frame on the author's home page only, stays out of search engines, and gets the app icon. */
function demoSite(): Plugin {
  const headers = [
    "/*",
    "  Content-Security-Policy: frame-ancestors 'self' https://szymonbakonis.pl https://szymonbakonis.pages.dev",
    "  X-Robots-Tag: noindex",
    "",
  ].join("\n");
  return {
    name: "demo-site",
    apply: "build",
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "_headers", source: headers });
      this.emitFile({ type: "asset", fileName: "favicon.svg", source: readFileSync("src-tauri/icons/source.svg", "utf8") });
    },
    transformIndexHtml: () => [
      { tag: "link", attrs: { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" }, injectTo: "head" },
      { tag: "meta", attrs: { name: "robots", content: "noindex" }, injectTo: "head" },
    ],
  };
}

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
  plugins: [react(), mode === "demo" && demoSite()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
