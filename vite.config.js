import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig({
  plugins: [
    react(),
    process.env.ANALYZE === 'true' &&
      visualizer({
        filename: 'dist/bundle-report.html',
        gzipSize: true,
        brotliSize: true,
        open: false,
      }),
  ].filter(Boolean),
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: { main: 'index.html', toast: 'toast.html' },
      output: {
        manualChunks: {
          'react-vendor': ['react', 'react-dom'],
        },
      },
    },
  },
});
