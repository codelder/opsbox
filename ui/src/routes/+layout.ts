import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const csr = true;

export const load: LayoutLoad = async ({ fetch, url }) => {
  if (url.pathname.startsWith('/settings')) {
    return {};
  }

  try {
    const res = await fetch('/api/v1/logseek/settings/minio', { cache: 'no-store' });
    if (res.ok) {
      const data = await res.json();
      if (data?.configured) {
        return {};
      }
    }
  } catch {
    // swallow fetch error and redirect to settings below
  }

  throw redirect(307, '/settings');
};
