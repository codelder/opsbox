import { describe, it, expect } from 'vitest';
import { parseFileUrl, isArchive } from './fileUrl';

describe('fileUrl', () => {
  it('parses local file url', () => {
    const url = 'ls://localhost@local/var/log/syslog';
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
    const url = 'ls://prod:logs@s3/2023/10/data.tar.gz?entry=internal/service.log';
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

  it('parses multi-cluster url', () => {
    const url = 'ls://web-01@agent.hk-prod:4000/var/log/syslog';
    const parsed = parseFileUrl(url);
    expect(parsed?.serverAddr).toBe('hk-prod:4000');
    expect(parsed?.endpointType).toBe('agent');
    expect(parsed?.endpointId).toBe('web-01');
  });

  it('handles encoded paths', () => {
    const url = 'ls://localhost@local/var/log/my%20log.txt';
    const parsed = parseFileUrl(url);
    expect(parsed?.path).toBe('var/log/my log.txt');
    expect(parsed?.displayName).toBe('my log.txt');
  });

  it('detects archive', () => {
    expect(isArchive('ls://prod:logs@s3/file.tar.gz?entry=x')).toBe(true);
    expect(isArchive('ls://localhost@local/file.txt')).toBe(false);
  });

  it('returns null for invalid scheme', () => {
    expect(parseFileUrl('file:///var/log')).toBeNull();
  });
});
