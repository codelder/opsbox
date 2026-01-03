/**
 * API 配置
 * 统一管理 API 基础路径和公共配置
 */

import { env } from '$env/dynamic/public';

/**
 * 获取 API 基础路径
 */
export function getApiBase(): string {
  return env.PUBLIC_API_BASE || '/api/v1/logseek';
}

/**
 * 公共请求头
 */
export const commonHeaders = {
  'Content-Type': 'application/json',
  Accept: 'application/json, application/x-ndjson'
};
