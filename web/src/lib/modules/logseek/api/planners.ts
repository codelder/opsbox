import { getApiBase, commonHeaders } from './config';

export interface PlannerMeta {
  app: string;
  updated_at: number;
}
export interface PlannerGet {
  app: string;
  script: string;
  updated_at: number;
}
export interface PlannerUpsert {
  app: string;
  script: string;
}
export interface PlannerTestPayload {
  app: string;
  q: string;
  script?: string; // 可选的脚本内容（用于测试未保存的脚本）
}
export type Endpoint =
  | { kind: 'local'; root: string }
  | { kind: 'agent'; agent_id: string; subpath: string }
  | { kind: 's3'; profile: string; bucket: string };

export type Target =
  | { type: 'dir'; path: string; recursive?: boolean }
  | { type: 'files'; paths: string[] }
  | { type: 'archive'; path: string };

export interface Source {
  endpoint: Endpoint;
  target: Target;
  filter_glob?: string;
  display_name?: string;
}

export interface PlannerTestResponse {
  cleaned_query: string;
  sources: Source[];
  debug_logs: string[]; // 调试日志（print 函数的输出）
}

export interface PlannerListResponse {
  items: PlannerMeta[];
  default: string | null;
}

export async function listPlanners(): Promise<PlannerListResponse> {
  const res = await fetch(`${getApiBase()}/settings/planners/scripts`, { headers: { Accept: 'application/json' } });
  if (!res.ok) throw new Error(`加载失败：HTTP ${res.status}`);
  return (await res.json()) as PlannerListResponse;
}

export async function getDefaultPlanner(): Promise<string | null> {
  const res = await fetch(`${getApiBase()}/settings/planners/default`, { headers: { Accept: 'application/json' } });
  if (!res.ok) throw new Error(`获取默认规划脚本失败：HTTP ${res.status}`);
  const name: string | null = await res.json();
  return name ?? null;
}

export async function setDefaultPlanner(app: string): Promise<void> {
  const res = await fetch(`${getApiBase()}/settings/planners/default`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify({ app })
  });
  if (!res.ok) {
    let msg = `设置默认规划脚本失败：HTTP ${res.status}`;
    try {
      const p = await res.json();
      msg = p?.detail || p?.title || msg;
    } catch {
      /* ignore */ void 0;
    }
    throw new Error(msg);
  }
}

export async function getPlanner(app: string): Promise<PlannerGet> {
  const res = await fetch(`${getApiBase()}/settings/planners/scripts/${encodeURIComponent(app)}`, {
    headers: { Accept: 'application/json' }
  });
  if (!res.ok) throw new Error(`加载失败：HTTP ${res.status}`);
  return (await res.json()) as PlannerGet;
}

export async function savePlanner(body: PlannerUpsert): Promise<void> {
  const res = await fetch(`${getApiBase()}/settings/planners/scripts`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(body)
  });
  if (!res.ok) {
    let msg = `保存失败：HTTP ${res.status}`;
    try {
      const p = await res.json();
      msg = p?.detail || p?.title || msg;
    } catch {
      /* ignore */ void 0;
    }
    throw new Error(msg);
  }
}

export async function deletePlanner(app: string): Promise<void> {
  const res = await fetch(`${getApiBase()}/settings/planners/scripts/${encodeURIComponent(app)}`, { method: 'DELETE' });
  if (!res.ok) throw new Error(`删除失败：HTTP ${res.status}`);
}

export async function testPlanner(body: PlannerTestPayload): Promise<PlannerTestResponse> {
  const res = await fetch(`${getApiBase()}/settings/planners/test`, {
    method: 'POST',
    headers: commonHeaders,
    body: JSON.stringify(body)
  });
  if (!res.ok) {
    let msg = `测试失败：HTTP ${res.status}`;
    try {
      const p = await res.json();
      msg = p?.detail || p?.title || msg;
    } catch {
      /* ignore */ void 0;
    }
    throw new Error(msg);
  }
  return (await res.json()) as PlannerTestResponse;
}
