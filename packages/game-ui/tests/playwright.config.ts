import { defineConfig } from '@playwright/test';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const vite = join(dirname(require.resolve('vite/package.json')), 'bin/vite.js');
const config = fileURLToPath(new URL('./vite.config.ts', import.meta.url));
const origin = `http://127.0.0.1:${process.env.CASINO_UI_TEST_PORT ?? '4176'}`;
export default defineConfig({ testDir: '.', testMatch: '*.spec.ts', fullyParallel: true, use: { baseURL: origin, viewport: { width: 1080, height: 760 } }, webServer: { command: `node "${vite}" --config "${config}"`, url: origin, reuseExistingServer: false } });
