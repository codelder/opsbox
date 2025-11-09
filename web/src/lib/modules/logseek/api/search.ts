/**
 * 搜索 API 客户端
 * 封装搜索相关的 API 调用
 */

import type { SearchBody } from '../types';
import { getApiBase, commonHeaders } from './config';

/**
 * 开始流式搜索（返回 ReadableStream）
 * @param query 查询字符串
 * @returns Response 对象，包含 NDJSON 流和会话 ID（响应头 X-Logseek-SID）
 */
export async function startSearch(query: string): Promise<Response> {
  // 兼容旧名，已统一走 /search.ndjson
  const API_BASE = getApiBase();
  const body: SearchBody = { q: query };

  const response = await fetch(`${API_BASE}/search.ndjson`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(body)
  });

  if (!response.ok) {
    throw new Error(`搜索请求失败：HTTP ${response.status}`);
  }

  return response;
}

/**
 * 从响应中提取会话 ID
 */
export function extractSessionId(response: Response): string {
  return response.headers.get('X-Logseek-SID') || '';
}

/**
 * 开始搜索（多存储源并行搜索，返回 ReadableStream）
 *
 * 搜索会同时搜索所有配置的存储源（S3、Agent、本地文件等），
 * 并将结果合并返回。存储源配置在后端管理。
 *
 * @param query 查询字符串
 * @returns Response 对象，包含 NDJSON 流和会话 ID（响应头 X-Logseek-SID）
 */
export async function startUnifiedSearch(query: string): Promise<Response> {
  const API_BASE = getApiBase();
  const body: SearchBody = { q: query };

  const response = await fetch(`${API_BASE}/search.ndjson`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(body)
  });

  if (!response.ok) {
    // 尝试解析错误详情（Problem Details 格式）
    let errorMessage = `搜索请求失败：HTTP ${response.status}`;
    try {
      const contentType = response.headers.get('content-type');
      if (contentType && contentType.includes('application/problem+json')) {
        const problem = await response.json();
        errorMessage = problem.detail || problem.title || errorMessage;
      } else if (contentType && contentType.includes('application/json')) {
        const json = await response.json();
        errorMessage = json.detail || json.title || json.message || errorMessage;
      }
    } catch {
      // 如果解析失败，使用默认错误消息
    }
    throw new Error(errorMessage);
  }

  return response;
}
