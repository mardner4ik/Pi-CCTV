import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const backend = process.env.PICCTV_BACKEND || 'http://localhost:8080'

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: '../static',
    emptyOutDir: true,
    cssCodeSplit: false,
    modulePreload: false,
    rollupOptions: {
      output: {
        entryFileNames: 'app.js',
        chunkFileNames: 'app-[name].js',
        assetFileNames: (info) => (info.names?.some((n) => n.endsWith('.css')) ? 'app.css' : 'assets/[name][extname]'),
      },
    },
  },
  server: {
    proxy: {
      '/api': backend,
      '/stream': backend,
      '/snapshot': backend,
    },
  },
})
