import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const csr = true;

// 缓存配置检查结果，避免重复请求
let configChecked = false;
let isConfigured = false;

export const load: LayoutLoad = async ({ fetch, url }) => {
  // 设置页面不需要检查
  if (url.pathname.startsWith('/settings')) {
    return {};
  }

  // 如果已经检查过且配置正确，直接返回
  if (configChecked && isConfigured) {
    return {};
  }

  // 只在首次加载时检查配置
  if (!configChecked) {
    try {
      const res = await fetch('/api/v1/logseek/settings/s3', { cache: 'no-store' });
      if (res.ok) {
        const data = await res.json();
        configChecked = true;
        isConfigured = data?.configured || false;

        if (isConfigured) {
          return {};
        }
      }
    } catch (err) {
      console.error('检查 S3 配置失败:', err);
      configChecked = true;
      isConfigured = false;
    }
  }

  // 配置未完成，跳转到设置页面
  throw redirect(307, '/settings');
};
