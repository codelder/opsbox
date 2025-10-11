/**
 * 文件查看 API 客户端
 * 封装文件查看缓存相关的 API 调用
 */

import type { ViewCacheResponse } from '../types';
import { getApiBase } from './config';

/**
 * 获取文件行范围（从缓存）
 * @param sid 会话 ID
 * @param file 文件路径
 * @param start 起始行号（1-based）
 * @param end 结束行号（包含）
 */
export async function fetchViewCache(
  sid: string,
  file: string,
  start: number,
  end: number
): Promise<ViewCacheResponse> {
  const API_BASE = getApiBase();
  const url = `${API_BASE}/view.cache.json?sid=${encodeURIComponent(sid)}&file=${encodeURIComponent(file)}&start=${start}&end=${end}`;

  const response = await fetch(url, {
    headers: { Accept: 'application/json' }
  });

  if (!response.ok) {
    throw new Error(`加载文件失败：HTTP ${response.status}`);
  }

  return await response.json();
}
