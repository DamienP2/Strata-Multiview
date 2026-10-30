import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const server = process.env.CG_SERVER ?? 'http://localhost:8080';

export default defineConfig({
  plugins: [react()],
  build: {
    // `/assets` is reserved for user media served by the engine.
    assetsDir: 'static',
    rolldownOptions: {
      output: {
        // The libraries in a chunk of their own: they change less often than
        // the app, so browsers keep them across updates. The pages loaded
        // when opened (React.lazy in App.tsx) get their own chunks as usual.
        codeSplitting: {
          groups: [
            { name: 'react', test: /node_modules[\\/](react|react-dom|scheduler|zustand|use-sync-external-store)[\\/]/ },
            { name: 'icons', test: /node_modules[\\/]lucide-react[\\/]/ },
          ],
        },
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': server,
      '/assets': server,
      '/fonts': server,
      '/ws': { target: server.replace(/^http/, 'ws'), ws: true },
    },
  },
});
