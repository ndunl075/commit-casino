import tseslint from 'typescript-eslint';
export default tseslint.config({ignores:['**/dist/**','**/target/**','.worktrees/**','.toolchain/**','node_modules/**','playwright-report/**','test-results/**']},...tseslint.configs.recommended,{rules:{'@typescript-eslint/no-explicit-any':'error','@typescript-eslint/no-unused-vars':['error',{argsIgnorePattern:'^_',varsIgnorePattern:'^_'}]}});
