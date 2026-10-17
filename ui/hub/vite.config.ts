import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  base: '/hub/',
  optimizeDeps: {
    exclude: [
      '@aaif/goose-hub-core',
      '@aaif/goose-hub-core/rest',
      '@aaif/goose-hub-core/types',
      '@aaif/goose-hub-core/url',
      '@aaif/goose-hub-core/cron',
    ],
  },
  server: {
    host: '0.0.0.0',
    port: 5173,
    fs: { allow: ['..'] },
    watch: {
      ignored: ['!**/hub-core/**'],
    },
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
      },
      '/acp': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
      },
      // Requests that stay under the /hub/ mount, as when a reverse proxy
      // only forwards that prefix.
      '/hub/api': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/hub/, ''),
      },
      '/hub/acp': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/hub/, ''),
      },
    },
  }
})
