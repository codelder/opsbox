import type { ResourceItem } from './types';

// TODO: Move base URL to config
const BASE_URL = '/api/v1/explorer'; // Proxy or backend URL

export async function listResources(orl: string): Promise<ResourceItem[]> {
  console.log('[API] listResources called with ORL:', orl);
  const requestBody = { orl: orl };
  console.log('[API] Request body:', JSON.stringify(requestBody));

  const res = await fetch(`${BASE_URL}/list`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify(requestBody)
  });

  console.log('[API] Response status:', res.status, res.statusText);

  if (!res.ok) {
    // 只读取一次响应 body，避免 "body stream already read" 错误
    let message = 'Unknown error';
    const contentType = res.headers.get('content-type') || '';

    if (contentType.includes('application/json')) {
      try {
        const errJson = await res.json();
        message = errJson.detail || errJson.title || JSON.stringify(errJson);
      } catch {
        message = 'Failed to parse error response as JSON';
      }
    } else {
      // 非 JSON 响应，读取为文本
      try {
        message = await res.text();
      } catch {
        message = `HTTP ${res.status}: ${res.statusText}`;
      }
    }
    throw new Error(message);
  }

  const json = await res.json();
  // Backend returns SuccessResponse { data: { items: [] } }
  return json.data?.items || [];
}
