import { describe, it, expect } from 'vitest';
import { parseOrl, isArchive, stringifyOrl } from './orl';

describe('orl', () => {
  describe('parseOrl', () => {
    it('parses local orl url', () => {
      const url = 'orl://local/var/log/syslog';
      const parsed = parseOrl(url);
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

    it('parses agent orl url', () => {
      const url = 'orl://web-01@agent/app/logs/error.log';
      const parsed = parseOrl(url);
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

    it('parses s3 orl archive url', () => {
      const url = 'orl://prod:logs@s3/2023/10/data.tar.gz?entry=internal/service.log';
      const parsed = parseOrl(url);
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

    it('parses multi-cluster orl url', () => {
      const url = 'orl://web-01@agent.hk-prod:4000/var/log/syslog';
      const parsed = parseOrl(url);
      expect(parsed?.serverAddr).toBe('hk-prod:4000');
      expect(parsed?.endpointType).toBe('agent');
      expect(parsed?.endpointId).toBe('web-01');
    });

    it('handles encoded paths', () => {
      const url = 'orl://local/var/log/my%20log.txt';
      const parsed = parseOrl(url);
      expect(parsed?.path).toBe('var/log/my log.txt');
      expect(parsed?.displayName).toBe('my log.txt');
    });

    it('detects archive', () => {
      expect(isArchive('orl://prod:logs@s3/file.tar.gz?entry=x')).toBe(true);
      expect(isArchive('orl://local/file.txt')).toBe(false);
    });

    it('returns null for invalid scheme', () => {
      expect(parseOrl('file:///var/log')).toBeNull();
    });
  });

  describe('stringifyOrl', () => {
    it('stringifies local orl url', () => {
      expect(
        stringifyOrl({
          endpointId: 'localhost',
          endpointType: 'local',
          path: 'var/log/syslog'
        })
      ).toBe('orl://local/var/log/syslog');
    });

    it('stringifies agent orl url with server and port', () => {
      expect(
        stringifyOrl({
          endpointId: 'web-01',
          endpointType: 'agent',
          serverAddr: 'hk-prod:4000',
          path: 'app.log'
        })
      ).toBe('orl://web-01@agent.hk-prod:4000/app.log');
    });

    it('stringifies s3 orl url correctly', () => {
      expect(
        stringifyOrl({
          endpointId: 'prod:my-bucket',
          endpointType: 's3',
          path: 'logs/2023.log'
        })
      ).toBe('orl://prod:my-bucket@s3/logs/2023.log');
    });

    it('stringifies s3 orl archive url', () => {
      expect(
        stringifyOrl({
          endpointId: 'prod:my-bucket',
          endpointType: 's3',
          path: 'data.tar.gz',
          entryPath: 'access.log'
        })
      ).toBe('orl://prod:my-bucket@s3/data.tar.gz?entry=access.log');
    });

    it('encodes spaces in path when stringifying', () => {
      expect(
        stringifyOrl({
          endpointId: 'localhost',
          endpointType: 'local',
          path: 'var/log/my log.txt'
        })
      ).toBe('orl://local/var/log/my%20log.txt');
    });
  });
});
