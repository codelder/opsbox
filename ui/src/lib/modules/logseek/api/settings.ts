/**
 * 设置 API 客户端
 * 封装 MinIO 设置相关的 API 调用
 */

import type { MinioSettingsPayload, MinioSettingsResponse } from '../types';
import { getApiBase, commonHeaders } from './config';

/**
 * 获取 MinIO 设置
 */
export async function fetchMinioSettings(): Promise<MinioSettingsResponse> {
	const API_BASE = getApiBase();
	const response = await fetch(`${API_BASE}/settings/minio`, {
		headers: { Accept: 'application/json' }
	});

	if (!response.ok) {
		throw new Error(`加载设置失败：HTTP ${response.status}`);
	}

	return await response.json();
}

/**
 * 保存 MinIO 设置
 */
export async function saveMinioSettings(
	settings: MinioSettingsPayload
): Promise<void> {
	const API_BASE = getApiBase();
	const response = await fetch(`${API_BASE}/settings/minio`, {
		method: 'POST',
		headers: commonHeaders,
		body: JSON.stringify(settings)
	});

	if (!response.ok) {
		// 尝试解析 RFC 7807 Problem Details
		let message = `保存失败：HTTP ${response.status}`;
		try {
			const problem = await response.json();
			message = problem?.detail || problem?.title || message;
		} catch {
			// 忽略 JSON 解析错误，保留默认消息
		}
		throw new Error(message);
	}

	// 后端返回 204 No Content，无需解析响应体
}
