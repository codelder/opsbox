/**
 * 搜索 API 客户端
 * 封装搜索相关的 API 调用
 */

import type { SearchBody } from '../types';
import { getApiBase, commonHeaders } from './config';

/**
 * 开始流式搜索（返回 ReadableStream）
 * @param query 查询字符串
 * @returns Response 对象，包含 NDJSON 流和会话 ID（响应头 X-Logsearch-SID）
 */
export async function startSearch(query: string): Promise<Response> {
	const API_BASE = getApiBase();
	const body: SearchBody = { q: query };

	const response = await fetch(`${API_BASE}/stream.ndjson`, {
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
	return response.headers.get('X-Logsearch-SID') || '';
}
