import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  // Consult https://svelte.dev/docs/kit/integrations
  // for more information about preprocessors
  preprocess: vitePreprocess(),

  kit: {
    // 使用静态适配器并开启 SPA fallback，以便所有路径回退到 index.html
    // 同时将构建产物直接输出到后端的 static 目录（注意：构建会清空该目录）
    adapter: adapter({
      pages: '../backend/api-gateway/static',
      assets: '../backend/api-gateway/static',
      fallback: 'index.html'
    }),
    // 关闭自动预渲染条目，作为单页应用仅输出 fallback
    prerender: { entries: [] }
  }
};

export default config;
