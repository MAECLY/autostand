// @ts-check
import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

const src = fileURLToPath(new URL("./src", import.meta.url));
const designSystem = fileURLToPath(new URL("../../design-system", import.meta.url));

export default defineConfig({
  site: "https://maecly.github.io",
  base: "/autostand",
  // Marketing page: prerender everything, ship HTML with React only where a
  // section is genuinely interactive.
  output: "static",
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
    resolve: {
      alias: { "@": src, "@design-system": designSystem },
      // design-system components are imported from source in a sibling package.
      dedupe: ["react", "react-dom"],
    },
  },
});
