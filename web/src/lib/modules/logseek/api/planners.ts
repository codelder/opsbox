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
}
export type SourceConfig =
  | { type: 'agent'; agent_id: string; scope_root?: string; path_filter_glob?: string }
  | { type: 's3'; profile: string; bucket?: string; prefix?: string; pattern?: string; key?: string }
  | { type: 'local'; path: string; recursive?: boolean };

export interface PlannerTestResponse {
  cleaned_query: string;
  sources: SourceConfig[];
}

export async function listPlanners(): Promise<PlannerMeta[]> {
  const res = await fetch(`${getApiBase()}/settings/planners/scripts`, { headers: { Accept: 'application/json' } });
  if (!res.ok) throw new Error(`加载失败：HTTP ${res.status}`);
  const data = await res.json();
  return (data?.items ?? []) as PlannerMeta[];
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
