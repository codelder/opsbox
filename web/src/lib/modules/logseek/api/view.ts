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

  // 确保 file 参数是正确格式的 URL（ls://...）
  // 如果已经是正确格式，直接使用；否则可能需要处理
  const fileParam = file.trim();

  // 使用 URLSearchParams 来正确构建查询参数，避免双重编码
  const params = new URLSearchParams({
    sid: sid,
    file: fileParam, // URLSearchParams 会自动编码
    start: start.toString(),
    end: end.toString()
  });

  const url = `${API_BASE}/view.cache.json?${params.toString()}`;

  const response = await fetch(url, {
    headers: { Accept: 'application/json' }
  });

  if (!response.ok) {
    // 尝试获取更详细的错误信息
    let errorMessage = `加载文件失败：HTTP ${response.status}`;
    try {
      const errorText = await response.text();
      if (errorText) {
        const errorJson = JSON.parse(errorText);
        errorMessage = errorJson.message || errorJson.error || errorMessage;
      }
    } catch {
      // 忽略解析错误
    }
    throw new Error(errorMessage);
  }

  return await response.json();
}

/**
 * 下载完整文件内容
 * @param sid 会话 ID
 * @param file 文件路径
 * @returns 返回 Response 对象，调用者可以处理下载
 */
export async function fetchViewDownload(sid: string, file: string): Promise<Response> {
  const API_BASE = getApiBase();

  // 确保 file 参数是正确格式的 URL（ls://...）
  const fileParam = file.trim();

  // 使用 URLSearchParams 来正确构建查询参数，避免双重编码
  const params = new URLSearchParams({
    sid: sid,
    file: fileParam
  });

  const url = `${API_BASE}/view/download?${params.toString()}`;

  const response = await fetch(url);

  if (!response.ok) {
    // 尝试获取更详细的错误信息
    let errorMessage = `下载文件失败：HTTP ${response.status}`;
    try {
      const errorText = await response.text();
      if (errorText) {
        const errorJson = JSON.parse(errorText);
        errorMessage = errorJson.message || errorJson.error || errorMessage;
      }
    } catch {
      // 忽略解析错误
    }
    throw new Error(errorMessage);
  }

  return response;
}
