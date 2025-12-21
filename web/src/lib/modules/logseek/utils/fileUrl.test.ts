import { describe, it, expect } from 'vitest';
import { parseFileUrl, isArchive } from './fileUrl';

describe('fileUrl', () => {
  it('parses local dir url', () => {
    const url = 'ls://local/localhost/dir/var/log/syslog';
    const parsed = parseFileUrl(url);
    expect(parsed).toEqual({
      endpointType: 'local',
      endpointId: 'localhost',
      targetType: 'dir',
      path: 'var/log/syslog',
      entryPath: undefined,
      original: url,
      displayName: 'syslog'
    });
  });

  it('parses agent dir url', () => {
    const url = 'ls://agent/web-01/dir/app/logs/error.log';
    const parsed = parseFileUrl(url);
    expect(parsed).toEqual({
      endpointType: 'agent',
      endpointId: 'web-01',
      targetType: 'dir',
      path: 'app/logs/error.log',
      entryPath: undefined,
      original: url,
      displayName: 'error.log'
    });
  });

  it('parses s3 archive url', () => {
    const url = 'ls://s3/prod:logs-bucket/archive/2023/10/data.tar.gz?entry=internal/service.log';
    const parsed = parseFileUrl(url);
    expect(parsed).toEqual({
      endpointType: 's3',
      endpointId: 'prod:logs-bucket',
      targetType: 'archive',
      path: '2023/10/data.tar.gz',
      entryPath: 'internal/service.log',
      original: url,
      displayName: 'service.log'
    });
  });

  it('handles encoded paths', () => {
    const url = 'ls://local/localhost/dir/var/log/my%20log.txt';
    const parsed = parseFileUrl(url);
    expect(parsed?.path).toBe('var/log/my log.txt');
    expect(parsed?.displayName).toBe('my log.txt');
  });

  it('detects archive', () => {
    expect(isArchive('ls://s3/bucket/archive/file.tar.gz')).toBe(true);
    expect(isArchive('ls://local/localhost/dir/file.txt')).toBe(false);
  });

  it('returns null for invalid scheme', () => {
    expect(parseFileUrl('file:///var/log')).toBeNull();
  });
});
