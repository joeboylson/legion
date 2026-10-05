import path from 'node:path'

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The app only ever loads the built files (no dev server), so Tauri finds
// them in dist/.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': path.resolve(import.meta.dirname, 'src') } },
  // One bundle is fine: the app loads it from disk, never over a network.
  build: { outDir: 'dist', emptyOutDir: true, chunkSizeWarningLimit: 2048 },
  test: { environment: 'node' },
})
