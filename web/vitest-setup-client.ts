/// <reference types="@vitest/browser/matchers" />
/// <reference types="@vitest/browser/providers/playwright" />

import { vi } from 'vitest';

// Mock SvelteKit environment modules that aren't available in test environment
vi.mock('$env/dynamic/public', () => ({
  env: {
    PUBLIC_API_BASE: '/api/v1/logseek'
  }
}));

vi.mock('$env/dynamic/private', () => ({
  env: {}
}));
