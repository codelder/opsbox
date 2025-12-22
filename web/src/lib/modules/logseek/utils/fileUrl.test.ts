import { describe, it, expect } from 'vitest';
import { parseFileUrl, isArchive, stringifyFileUrl } from './fileUrl';

describe('fileUrl', () => {
  it('parses local file url', () => {
    const url = 'ls://local/var/log/syslog';
    const parsed = parseFileUrl(url);
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

  it('parses agent file url', () => {
    const url = 'ls://web-01@agent/app/logs/error.log';
    const parsed = parseFileUrl(url);
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

  it('parses s3 archive url', () => {
    const url = 'ls://prod@s3/logs/2023/10/data.tar.gz?entry=internal/service.log';
    const parsed = parseFileUrl(url);
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

  it('parses s3 url without profile (fallbacks to empty id)', () => {
    const url = 'ls://s3/my-bucket/path/to/file';
    const parsed = parseFileUrl(url);
    expect(parsed?.endpointId).toBe(':my-bucket');
    expect(parsed?.path).toBe('path/to/file');
  });

  it('parses multi-cluster url', () => {
    const url = 'ls://web-01@agent.hk-prod:4000/var/log/syslog';
    const parsed = parseFileUrl(url);
    expect(parsed?.serverAddr).toBe('hk-prod:4000');
    expect(parsed?.endpointType).toBe('agent');
    expect(parsed?.endpointId).toBe('web-01');
  });

  it('handles encoded paths', () => {
    const url = 'ls://local/var/log/my%20log.txt';
    const parsed = parseFileUrl(url);
    expect(parsed?.path).toBe('var/log/my log.txt');
    expect(parsed?.displayName).toBe('my log.txt');
  });

  it('detects archive', () => {
    expect(isArchive('ls://prod:logs@s3/file.tar.gz?entry=x')).toBe(true);
    expect(isArchive('ls://local/file.txt')).toBe(false);
  });

  it('returns null for invalid scheme', () => {
    expect(parseFileUrl('file:///var/log')).toBeNull();
  });

  describe('stringifyFileUrl', () => {
    it('stringifies local url', () => {
      expect(
        stringifyFileUrl({
          endpointId: 'localhost',
          endpointType: 'local',
          path: 'var/log/syslog'
        })
      ).toBe('ls://local/var/log/syslog');
    });

    it('stringifies agent url with server and port', () => {
      expect(
        stringifyFileUrl({
          endpointId: 'web-01',
          endpointType: 'agent',
          serverAddr: 'hk-prod:4000',
          path: 'app.log'
        })
      ).toBe('ls://web-01@agent.hk-prod:4000/app.log');
    });

    it('stringifies s3 url correctly', () => {
      expect(
        stringifyFileUrl({
          endpointId: 'prod:my-bucket',
          endpointType: 's3',
          path: 'logs/2023.log'
        })
      ).toBe('ls://prod@s3/my-bucket/logs/2023.log');
    });

    it('stringifies s3 archive url', () => {
      expect(
        stringifyFileUrl({
          endpointId: 'prod:my-bucket',
          endpointType: 's3',
          path: 'data.tar.gz',
          entryPath: 'access.log'
        })
      ).toBe('ls://prod@s3/my-bucket/data.tar.gz?entry=access.log');
    });

    it('omits default profile in s3 url', () => {
      expect(
        stringifyFileUrl({
          endpointId: 'default:my-bucket',
          endpointType: 's3',
          path: 'logs/error.log'
        })
      ).toBe('ls://s3/my-bucket/logs/error.log');
    });
  });
});
