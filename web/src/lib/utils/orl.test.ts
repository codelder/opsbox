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

  describe('archive URL encoding', () => {
    it('handles archive entry paths without double encoding', () => {
      // 模拟后端返回的 ORL（entry 值未编码）
      const backendOrl = 'orl://local/tmp/test.gz?entry=/home/bbipadm/logs';

      // 前端使用 encodeURIComponent 编码整个 ORL
      const encoded = encodeURIComponent(backendOrl);

      // 验证：不应该有双重编码
      expect(encoded).not.toContain('%252F');

      // 验证：应该有正确的单次编码
      expect(encoded).toContain('%3Fentry%3D%2F');

      // 测试往返：解码后应该等于原始值
      const decoded = decodeURIComponent(encoded);
      expect(decoded).toBe(backendOrl);
    });

    it('detects double encoding in backend response', () => {
      // 错误的后端返回（entry 值已编码）
      const wrongBackendOrl = 'orl://local/tmp/test.gz?entry=%2Fhome';

      // 前端编码后会产生双重编码
      const encoded = encodeURIComponent(wrongBackendOrl);

      // 验证：存在双重编码
      expect(encoded).toContain('%252F');

      // 这会导致后端无法正确解析
      expect(decodeURIComponent(encoded)).toBe(wrongBackendOrl);
      // 但后端收到的是 ?entry=%2Fhome 而不是 ?entry=/home
    });

    it('correctly parses URL query parameter', () => {
      // 模拟浏览器 URL 中的查询参数
      const url = new URL('http://localhost:5173/explorer?orl=orl%3A%2F%2Flocal%2Ftmp%2Ftest.gz%3Fentry%3D%2Fhome');

      // 获取并验证解码后的 ORL
      const orl = url.searchParams.get('orl');
      expect(orl).toBe('orl://local/tmp/test.gz?entry=/home');

      // 验证可以正确解析
      const parsed = parseOrl(orl!);
      expect(parsed?.path).toBe('tmp/test.gz');
      expect(parsed?.entryPath).toBe('/home');
    });
  });

  describe('parseOrl additional cases', () => {
    it('should parse ORL with archive entry', () => {
      const orl = 'orl://local/var/log/archive.tar.gz?entry=logs/app.log';
      const parsed = parseOrl(orl);

      expect(parsed).not.toBeNull();
      expect(parsed?.endpointType).toBe('local');
      expect(parsed?.endpointId).toBe('localhost');
      expect(parsed?.path).toBe('var/log/archive.tar.gz');
      expect(parsed?.entryPath).toBe('logs/app.log');
      expect(parsed?.targetType).toBe('archive');
    });

    it('should parse S3 ORL', () => {
      const orl = 'orl://myprofile@s3/bucket/path/file.log';
      const parsed = parseOrl(orl);

      expect(parsed).not.toBeNull();
      expect(parsed?.endpointType).toBe('s3');
      expect(parsed?.endpointId).toBe('myprofile');
      expect(parsed?.path).toBe('bucket/path/file.log');
      expect(parsed?.targetType).toBe('dir');
    });

    it('should parse Agent ORL with server address', () => {
      const orl = 'orl://agent-01@agent.192.168.1.100:4001/var/log/app.log';
      const parsed = parseOrl(orl);

      expect(parsed).not.toBeNull();
      expect(parsed?.endpointType).toBe('agent');
      expect(parsed?.endpointId).toBe('agent-01');
      expect(parsed?.serverAddr).toBe('192.168.1.100:4001');
      expect(parsed?.path).toBe('var/log/app.log');
    });
  });

  describe('stringifyOrl additional cases', () => {
    it('should build ORL for local file', () => {
      const orl = stringifyOrl({
        endpointType: 'local',
        endpointId: 'localhost',
        path: '/var/log/test.log'
      });

      expect(orl).toBe('orl://local/var/log/test.log');
    });

    it('should build ORL for S3 resource with profile', () => {
      const orl = stringifyOrl({
        endpointType: 's3',
        endpointId: 'prod-bucket',
        path: 'logs/2024/app.log'
      });

      expect(orl).toBe('orl://prod-bucket@s3/logs/2024/app.log');
    });
  });
});
