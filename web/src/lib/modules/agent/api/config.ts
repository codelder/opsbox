/**
 * Agent API 配置
 */

import { env } from '$env/dynamic/public';

/**
 * 获取 Agent API 基础路径
 * 优先使用 PUBLIC_AGENTS_API_BASE，其次默认 '/api/v1/agents'
 */
export function getAgentApiBase(): string {
  return env.PUBLIC_AGENTS_API_BASE || '/api/v1/agents';
}

/**
 * 公共请求头
 */
export const commonHeaders = {
  'Content-Type': 'application/json',
  Accept: 'application/json'
};
