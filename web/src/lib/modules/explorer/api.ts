import type { ResourceItem } from './types';

// TODO: Move base URL to config
const BASE_URL = '/api/v1/explorer'; // Proxy or backend URL

export async function listResources(odfi: string): Promise<ResourceItem[]> {
  const res = await fetch(`${BASE_URL}/list`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ odfi })
  });

  if (!res.ok) {
    let message = 'Unknown error';
    try {
      const errJson = await res.json();
      message = errJson.detail || errJson.title || JSON.stringify(errJson);
    } catch {
      message = await res.text();
    }
    throw new Error(message);
  }

  const json = await res.json();
  // Backend returns SuccessResponse { data: { items: [] } }
  return json.data?.items || [];
}
