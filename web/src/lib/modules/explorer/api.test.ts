/**
 * Explorer API 测试
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { listResources } from './api';
import type { ResourceItem } from './types';

describe('Explorer API Client', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('buildListRequest', () => {
    it('should build list request with ORL', async () => {
      const mockItems: ResourceItem[] = [
        {
          name: 'test.log',
          path: 'orl://local/var/log/test.log',
          type: 'file',
          size: 1024,
          modified: 1234567890
        }
      ];

      const mockResponse = {
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        json: vi.fn().mockResolvedValueOnce({ data: { items: mockItems } })
      } as unknown as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const orl = 'orl://local/var/log';
      const result = await listResources(orl);

      expect(globalThis.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/api/v1/explorer/list'),
        expect.objectContaining({
          method: 'POST',
          headers: expect.objectContaining({
            'Content-Type': 'application/json'
          }),
          body: JSON.stringify({ orl })
        })
      );

      expect(result).toEqual(mockItems);
    });
  });

  describe('parseListResponse', () => {
    it('should parse file list response correctly', async () => {
      const mockItems: ResourceItem[] = [
        {
          name: 'app.log',
          path: 'orl://local/var/log/app.log',
          type: 'file',
          size: 2048,
          modified: 1234567890,
          mime_type: 'text/plain'
        },
        {
          name: 'nginx',
          path: 'orl://local/var/log/nginx',
          type: 'dir',
          has_children: true,
          child_count: 5
        }
      ];

      const mockResponse = {
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        json: vi.fn().mockResolvedValueOnce({ data: { items: mockItems } })
      } as unknown as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const result = await listResources('orl://local/var/log');

      expect(result).toHaveLength(2);
      expect(result[0].name).toBe('app.log');
      expect(result[0].type).toBe('file');
      expect(result[1].name).toBe('nginx');
      expect(result[1].type).toBe('dir');
    });

    it('should handle empty items array', async () => {
      const mockResponse = {
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        json: vi.fn().mockResolvedValueOnce({ data: { items: [] } })
      } as unknown as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      const result = await listResources('orl://local/empty');

      expect(result).toEqual([]);
    });
  });

  describe('buildDownloadUrl', () => {
    it('should construct download URL from ORL correctly', () => {
      const orl = 'orl://local/var/log/test.log';
      const encodedOrl = encodeURIComponent(orl);
      const downloadUrl = `/api/v1/explorer/download?orl=${encodedOrl}`;

      expect(downloadUrl).toContain('/api/v1/explorer/download');
      expect(downloadUrl).toContain('orl=');
      expect(decodeURIComponent(downloadUrl.split('orl=')[1])).toBe(orl);
    });

    it('should handle special characters in ORL', () => {
      const orl = 'orl://local/var/log/my file.log';
      const encodedOrl = encodeURIComponent(orl);
      const downloadUrl = `/api/v1/explorer/download?orl=${encodedOrl}`;

      expect(downloadUrl).toContain('/api/v1/explorer/download');
      expect(decodeURIComponent(downloadUrl.split('orl=')[1])).toBe(orl);
    });

    it('should handle archive entry paths in ORL', () => {
      const orl = 'orl://prod@s3/bucket/archive.tar.gz?entry=logs/app.log';
      const encodedOrl = encodeURIComponent(orl);
      const downloadUrl = `/api/v1/explorer/download?orl=${encodedOrl}`;

      expect(downloadUrl).toContain('/api/v1/explorer/download');
      expect(decodeURIComponent(downloadUrl.split('orl=')[1])).toBe(orl);
      expect(orl).toContain('?entry=');
    });
  });

  describe('error handling', () => {
    it('should handle non-JSON error responses', async () => {
      const mockResponse = {
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers({ 'Content-Type': 'text/plain' }),
        json: vi.fn(),
        text: vi.fn().mockResolvedValueOnce('Server Error')
      } as unknown as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      await expect(listResources('orl://local/test')).rejects.toThrow();
    });

    it('should handle JSON error responses', async () => {
      const mockResponse = {
        ok: false,
        status: 404,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        json: vi.fn().mockResolvedValueOnce({ detail: 'Resource not found' })
      } as unknown as Response;

      globalThis.fetch = vi.fn().mockResolvedValueOnce(mockResponse);

      await expect(listResources('orl://local/nonexistent')).rejects.toThrow('Resource not found');
    });
  });
});
