import { defineConfig } from 'vitest/config';
export default defineConfig({test:{include:['packages/**/*.test.ts','tests/contracts/**/*.test.ts'],exclude:['.worktrees/**'],testTimeout:10000}});
