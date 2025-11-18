/**
 * Agent 日志配置 API 封装
 */

import { getAgentApiBase, commonHeaders } from './config';

/**
 * 日志配置响应
 */
export interface LogConfigResponse {
  /** 日志级别 */
  level: string;
  /** 日志保留数量（天） */
  retention_count: number;
  /** 日志目录 */
  log_dir: string;
}

/**
 * 更新日志级别请求
 */
export interface UpdateLogLevelRequest {
  /** 日志级别: "error" | "warn" | "info" | "debug" | "trace" */
  level: string;
}

/**
 * 更新保留数量请求
 */
export interface UpdateRetentionRequest {
  /** 保留数量（天） */
  retention_count: number;
}

/**
 * 通用成功响应
 */
export interface SuccessResponse {
  message: string;
}

/**
 * 获取 Server 日志配置
 */
export async function fetchServerLogConfig(): Promise<LogConfigResponse> {
  const res = await fetch('/api/v1/log/config', {
    headers: { Accept: 'application/json' }
  });
  if (!res.ok) throw new Error(`加载 Server 日志配置失败：HTTP ${res.status}`);
  return await res.json();
}

/**
 * 更新 Server 日志级别
 */
export async function updateServerLogLevel(level: string): Promise<SuccessResponse> {
  const res = await fetch('/api/v1/log/level', {
    method: 'PUT',
    headers: commonHeaders,
    body: JSON.stringify({ level })
  });
  if (!res.ok) throw new Error(`更新 Server 日志级别失败：HTTP ${res.status}`);
  return await res.json();
}

/**
 * 更新 Server 日志保留数量
 */
export async function updateServerLogRetention(retention_count: number): Promise<SuccessResponse> {
  const res = await fetch('/api/v1/log/retention', {
    method: 'PUT',
    headers: commonHeaders,
    body: JSON.stringify({ retention_count })
  });
  if (!res.ok) throw new Error(`更新 Server 日志保留数量失败：HTTP ${res.status}`);
  return await res.json();
}

/**
 * 获取 Agent 日志配置（通过 Server 代理）
 */
export async function fetchAgentLogConfig(agentId: string): Promise<LogConfigResponse> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/log/config`, {
    headers: { Accept: 'application/json' }
  });
  if (!res.ok) {
    if (res.status === 502) {
      throw new Error('Agent 离线或无法连接');
    }
    throw new Error(`加载 Agent 日志配置失败：HTTP ${res.status}`);
  }
  return await res.json();
}

/**
 * 更新 Agent 日志级别（通过 Server 代理）
 */
export async function updateAgentLogLevel(agentId: string, level: string): Promise<SuccessResponse> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/log/level`, {
    method: 'PUT',
    headers: commonHeaders,
    body: JSON.stringify({ level })
  });
  if (!res.ok) {
    if (res.status === 502) {
      throw new Error('Agent 离线或无法连接');
    }
    throw new Error(`更新 Agent 日志级别失败：HTTP ${res.status}`);
  }
  return await res.json();
}

/**
 * 更新 Agent 日志保留数量（通过 Server 代理）
 */
export async function updateAgentLogRetention(agentId: string, retention_count: number): Promise<SuccessResponse> {
  const API_BASE = getAgentApiBase();
  const res = await fetch(`${API_BASE}/${encodeURIComponent(agentId)}/log/retention`, {
    method: 'PUT',
    headers: commonHeaders,
    body: JSON.stringify({ retention_count })
  });
  if (!res.ok) {
    if (res.status === 502) {
      throw new Error('Agent 离线或无法连接');
    }
    throw new Error(`更新 Agent 日志保留数量失败：HTTP ${res.status}`);
  }
  return await res.json();
}
