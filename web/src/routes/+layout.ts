import type { LayoutLoad } from './$types';

export const ssr = false;
export const csr = true;

export const load: LayoutLoad = async () => {
  // 首页与其他页面默认不强制检查 S3 配置，直接渲染
  return {};
};
