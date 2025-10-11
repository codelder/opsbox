import prettier from 'eslint-config-prettier';
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';
import svelteConfig from './svelte.config.js';

// 参照父目录 .gitignore，使用 Flat Config 的 ignores（方案 B）
export default ts.config(
  {
    ignores: [
      // 通用日志与依赖
      'node_modules/**',
      '**/node_modules/**',
      'logs/**',
      '*.log',
      // SvelteKit / Vite 相关
      '.svelte-kit/**',
      '.vite/**',
      '.output/**',
      '.vercel/**',
      '.netlify/**',
      '.wrangler/**',
      'build/**',
      'dist/**',
      'coverage/**',
      'playwright-report/**',
      '.playwright/**',
      'test-results/**',
      'vite.config.js.timestamp-*',
      'vite.config.ts.timestamp-*'
    ]
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs.recommended,
  prettier,
  ...svelte.configs.prettier,
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node }
    },
    rules: {
      // typescript-eslint 建议在 TS 项目中关闭 no-undef
      'no-undef': 'off'
    }
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        parser: ts.parser,
        svelteConfig
      }
    },
    rules: {
      // 允许使用 {@html}（我们已做关键词转义，且来源受控）
      'svelte/no-at-html-tags': 'off'
    }
  }
);
