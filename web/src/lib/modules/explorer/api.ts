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
    const err = await res.text();
    throw new Error(`Failed to list resources: ${err}`);
  }

  const json = await res.json();
  // Backend returns SuccessResponse { data: { items: [] } }
  return json.data?.items || [];
}
