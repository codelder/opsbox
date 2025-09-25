export const ssr = false; // 中文注释：仅在客户端渲染，确保 onMount 与浏览器 API 可用
export const csr = true;  // 中文注释：启用客户端运行时与事件处理
export const prerender = false; // 中文注释：禁止预渲染该页面（依赖运行时参数 sid/file）