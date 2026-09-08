import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import packageJson from "./package.json";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(packageJson.version),
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
