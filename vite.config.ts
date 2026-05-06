import path from 'node:path'

import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import svgr from 'vite-plugin-svgr'

const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  root: 'src',
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  plugins: [
    svgr(),
    react(),
  ],
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 4000,
  },
  resolve: {
    alias: {
      '@': path.resolve('./src'),
      '@root': path.resolve('.'),
      'tauri-plugin-mihomo-api': path.resolve('./src/services/mihomo-api.ts'),
    },
  },
  define: {
    OS_PLATFORM: '"android"',
  },
})
