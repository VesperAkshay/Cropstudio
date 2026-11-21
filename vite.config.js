import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  server: {
    strictPort: true,
    port: 1420,
    host: '127.0.0.1',
    // The Rust backend is not part of the frontend graph. Excluding it prevents
    // Chokidar from watching locked Windows build executables under Cargo target.
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: { outDir: 'dist', emptyOutDir: true },
});
