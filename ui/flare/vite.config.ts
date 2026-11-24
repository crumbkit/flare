import { defineConfig } from "vite";

// The reading page's scripts: the reading entry, which colours code and
// draws diagrams, and the comments entry, which draws the comments section.
// flare-server's build script embeds every file in dist/assets and serves it
// at /flare/assets/<file> with an immutable cache, so each name carries a
// content hash. The manifest names both entry files, which the reading page
// loads as module scripts.
export default defineConfig({
  base: "/flare/",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsDir: "assets",
    manifest: true,
    // A source map would double the embedded files.
    sourcemap: false,
    // The polyfill would add an inline script, which the page's policy
    // refuses. Every browser flare supports preloads modules itself.
    modulePreload: { polyfill: false },
    // A test in flare-server fails on any file over 5 MB. This limit only
    // keeps Vite's own warning about Mermaid's large chunks out of the
    // output.
    chunkSizeWarningLimit: 5000,
    rolldownOptions: {
      input: { reading: "src/main.ts", comments: "src/comments.tsx" },
      // TanStack Query marks its modules "use client" for server
      // components. The page has no server components, so the directive
      // means nothing in this bundle and its warning is left out.
      onwarn(warning, warn) {
        if (warning.code !== "MODULE_LEVEL_DIRECTIVE") warn(warning);
      },
    },
  },
});
