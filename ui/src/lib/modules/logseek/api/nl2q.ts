/**
 * NL2Q（自然语言转查询）API 客户端
 * 封装自然语言到查询字符串的转换 API
 */

import type { NL2QRequest, NL2QResponse } from '../types';
import { getApiBase, commonHeaders } from './config';

/**
 * 将自然语言转换为查询字符串
 * @param nl 自然语言文本
 * @returns 生成的查询字符串
 */
export async function convertNaturalLanguage(nl: string): Promise<string> {
	const API_BASE = getApiBase();
	const body: NL2QRequest = { nl };

	const response = await fetch(`${API_BASE}/nl2q`, {
		method: 'POST',
		headers: commonHeaders,
		body: JSON.stringify(body)
	});

	if (!response.ok) {
		throw new Error(`AI 生成失败：HTTP ${response.status}`);
	}

	const data = (await response.json()) as NL2QResponse;
	const query = data?.q?.trim() || '';

	if (!query) {
		throw new Error('AI 返回空结果');
	}

	return query;
}
