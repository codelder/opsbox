import { describe, it, expect } from 'vitest';
import { parseOdfi, isArchive, stringifyOdfi } from './odfi';

describe('odfi', () => {
  describe('parseOdfi', () => {
    it('parses local odfi url', () => {
      const url = 'odfi://local/var/log/syslog';
      const parsed = parseOdfi(url);
      expect(parsed).toEqual({
        endpointType: 'local',
        endpointId: 'localhost',
        targetType: 'dir',
        path: 'var/log/syslog',
        entryPath: undefined,
        original: url,
        serverAddr: undefined,
        displayName: 'syslog'
      });
    });

    it('parses agent odfi url', () => {
      const url = 'odfi://web-01@agent/app/logs/error.log';
      const parsed = parseOdfi(url);
      expect(parsed).toEqual({
        endpointType: 'agent',
        endpointId: 'web-01',
        targetType: 'dir',
        path: 'app/logs/error.log',
        entryPath: undefined,
        original: url,
        serverAddr: undefined,
        displayName: 'error.log'
      });
    });

    it('parses s3 odfi archive url', () => {
      const url = 'odfi://prod@s3/logs/2023/10/data.tar.gz?entry=internal/service.log';
      const parsed = parseOdfi(url);
      expect(parsed).toEqual({
        endpointType: 's3',
        endpointId: 'prod:logs',
        targetType: 'archive',
        path: '2023/10/data.tar.gz',
        entryPath: 'internal/service.log',
        original: url,
        serverAddr: undefined,
        displayName: 'service.log'
      });
    });

    it('parses s3 odfi url without profile (fallbacks to empty id)', () => {
      const url = 'odfi://s3/my-bucket/path/to/file';
      const parsed = parseOdfi(url);
      expect(parsed?.endpointId).toBe(':my-bucket');
      expect(parsed?.path).toBe('path/to/file');
    });

    it('parses multi-cluster odfi url', () => {
      const url = 'odfi://web-01@agent.hk-prod:4000/var/log/syslog';
      const parsed = parseOdfi(url);
      expect(parsed?.serverAddr).toBe('hk-prod:4000');
      expect(parsed?.endpointType).toBe('agent');
      expect(parsed?.endpointId).toBe('web-01');
    });

    it('handles encoded paths', () => {
      const url = 'odfi://local/var/log/my%20log.txt';
      const parsed = parseOdfi(url);
      expect(parsed?.path).toBe('var/log/my log.txt');
      expect(parsed?.displayName).toBe('my log.txt');
    });

    it('detects archive', () => {
      expect(isArchive('odfi://prod:logs@s3/file.tar.gz?entry=x')).toBe(true);
      expect(isArchive('odfi://local/file.txt')).toBe(false);
    });

    it('returns null for invalid scheme', () => {
      expect(parseOdfi('file:///var/log')).toBeNull();
    });
  });

  describe('stringifyOdfi', () => {
    it('stringifies local odfi url', () => {
      expect(
        stringifyOdfi({
          endpointId: 'localhost',
          endpointType: 'local',
          path: 'var/log/syslog'
        })
      ).toBe('odfi://local/var/log/syslog');
    });

    it('stringifies agent odfi url with server and port', () => {
      expect(
        stringifyOdfi({
          endpointId: 'web-01',
          endpointType: 'agent',
          serverAddr: 'hk-prod:4000',
          path: 'app.log'
        })
      ).toBe('odfi://web-01@agent.hk-prod:4000/app.log');
    });

    it('stringifies s3 odfi url correctly', () => {
      expect(
        stringifyOdfi({
          endpointId: 'prod:my-bucket',
          endpointType: 's3',
          path: 'logs/2023.log'
        })
      ).toBe('odfi://prod@s3/my-bucket/logs/2023.log');
    });

    it('stringifies s3 odfi archive url', () => {
      expect(
        stringifyOdfi({
          endpointId: 'prod:my-bucket',
          endpointType: 's3',
          path: 'data.tar.gz',
          entryPath: 'access.log'
        })
      ).toBe('odfi://prod@s3/my-bucket/data.tar.gz?entry=access.log');
    });

    it('omits default profile in s3 odfi url', () => {
      expect(
        stringifyOdfi({
          endpointId: 'default:my-bucket',
          endpointType: 's3',
          path: 'logs/error.log'
        })
      ).toBe('odfi://s3/my-bucket/logs/error.log');
    });
  });
});
