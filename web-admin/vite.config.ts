import fs from 'node:fs';
import { defineConfig } from 'vitest/config';
import { loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const httpsEnabled = (env.VITE_HTTPS_ENABLED ?? 'false').trim().toLowerCase() === 'true';
  const certFile = (env.VITE_HTTPS_CERT_FILE ?? '').trim();
  const keyFile = (env.VITE_HTTPS_KEY_FILE ?? '').trim();
  const proxyTarget = (env.VITE_ADMIN_API_PROXY_TARGET ?? 'http://localhost:19000').trim();

  return {
    plugins: [react()],
    server: {
      port: 19001,
      host: '0.0.0.0',
      https:
        httpsEnabled && certFile && keyFile
          ? {
              cert: fs.readFileSync(certFile),
              key: fs.readFileSync(keyFile),
            }
          : undefined,
      proxy: {
        '/api': {
          target: proxyTarget,
          changeOrigin: true,
          secure: false,
        },
      },
    },
    test: {
      environment: 'jsdom',
      setupFiles: './src/setupTests.ts',
      globals: true,
    },
  };
});
