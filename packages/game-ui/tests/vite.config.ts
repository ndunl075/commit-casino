import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
// No React refresh preamble: the fixture exercises native-style script-src 'self'.
export default defineConfig({ root: fileURLToPath(new URL('.', import.meta.url)), server: { host: '127.0.0.1', port: Number(process.env.CASINO_UI_TEST_PORT ?? 4176), strictPort: true, headers: { 'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; connect-src 'self' ws://127.0.0.1:*; object-src 'none'; frame-src 'none'; base-uri 'self'" }, fs: { allow: [fileURLToPath(new URL('../../../../', import.meta.url))] } } });
