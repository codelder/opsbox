/**
 * Agent 模块类型定义
 * 映射后端 Agent Manager 的 API 数据结构
 */

export interface AgentTag {
  key: string;
  value: string;
}

export type AgentStatus = { type: 'Online' } | { type: 'Busy'; tasks: number } | { type: 'Offline' };

export interface AgentInfo {
  id: string;
  name: string;
  version: string;
  hostname: string;
  tags: AgentTag[];
  search_roots: string[];
  last_heartbeat: number; // Unix 秒
  status: AgentStatus;
}

export interface AgentListResponse {
  agents: AgentInfo[];
  total: number;
}

export interface TagListResponse {
  tags: string[]; // key=value 字符串
  total: number;
}
