/**
 * FileUrl 工具测试
 */

import { describe, it, expect } from 'vitest';
import {
  parseFileUrl,
  getDisplayName,
  isArchive,
  getFileUrlType,
  type LocalFileUrl,
  type S3FileUrl,
  type TarEntryFileUrl,
  type AgentFileUrl
} from './fileUrl';

describe('fileUrl utils', () => {
  describe('parseFileUrl', () => {
    it('应该解析本地文件 URL', () => {
      const result = parseFileUrl('file:///var/log/app.log');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('local');
      const localUrl = result as LocalFileUrl;
      expect(localUrl.path).toBe('/var/log/app.log');
      expect(localUrl.displayName).toBe('app.log');
    });

    it('应该解析 S3 文件 URL（无 profile）', () => {
      const result = parseFileUrl('s3://mybucket/logs/app.log');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('s3');
      const s3Url = result as S3FileUrl;
      expect(s3Url.bucket).toBe('mybucket');
      expect(s3Url.key).toBe('logs/app.log');
      expect(s3Url.profile).toBeUndefined();
    });

    it('应该解析 S3 文件 URL（带 profile）', () => {
      const result = parseFileUrl('s3://prod:backupdr/logs/app.log');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('s3');
      const s3Url = result as S3FileUrl;
      expect(s3Url.profile).toBe('prod');
      expect(s3Url.bucket).toBe('backupdr');
      expect(s3Url.key).toBe('logs/app.log');
    });

    it('应该解析 tar.gz 文件 URL', () => {
      const result = parseFileUrl('tar.gz+s3://bucket/archive.tar.gz:logs/app.log');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('tar-entry');
      const tarUrl = result as TarEntryFileUrl;
      expect(tarUrl.compression).toBe('tar.gz');
      expect(tarUrl.baseUrl).toBe('s3://bucket/archive.tar.gz');
      expect(tarUrl.entryPath).toBe('logs/app.log');
    });

    it('应该解析 tar 文件 URL', () => {
      const result = parseFileUrl('tar+file:///var/archive.tar:logs/app.log');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('tar-entry');
      const tarUrl = result as TarEntryFileUrl;
      expect(tarUrl.compression).toBe('tar');
      expect(tarUrl.baseUrl).toBe('file:///var/archive.tar');
      expect(tarUrl.entryPath).toBe('logs/app.log');
    });

    it('应该解析 dir 文件 URL', () => {
      const result = parseFileUrl('dir+file:///root:subdir/file.txt');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('dir-entry');
      expect(result?.displayName).toBe('file.txt');
    });

    it('应该解析 Agent 文件 URL', () => {
      const result = parseFileUrl('agent://host1:8080/var/log/app.log');
      expect(result).not.toBeNull();
      expect(result?.type).toBe('agent');
      const agentUrl = result as AgentFileUrl;
      expect(agentUrl.agentId).toBe('host1:8080');
      expect(agentUrl.path).toBe('/var/log/app.log');
    });

    it('应该返回 null 对于无效 URL', () => {
      expect(parseFileUrl('invalid://url')).toBeNull();
      expect(parseFileUrl('no-scheme')).toBeNull();
      expect(parseFileUrl('')).toBeNull();
    });

    it('应该返回 null 对于格式错误的 tar URL', () => {
      expect(parseFileUrl('tar.gz+missing-colon')).toBeNull();
      expect(parseFileUrl('tar+no-entry')).toBeNull();
    });
  });

  describe('getDisplayName', () => {
    it('应该从本地文件路径提取文件名', () => {
      expect(getDisplayName('file:///var/log/app.log')).toBe('app.log');
      expect(getDisplayName('file:///path/to/file.txt')).toBe('file.txt');
    });

    it('应该从 S3 key 提取文件名', () => {
      expect(getDisplayName('s3://bucket/logs/app.log')).toBe('app.log');
      expect(getDisplayName('s3://prod:bucket/path/to/file')).toBe('file');
    });

    it('应该从 tar entry 提取文件名', () => {
      expect(getDisplayName('tar.gz+s3://bucket/archive.tar.gz:logs/app.log')).toBe('app.log');
      expect(getDisplayName('tar+file:///archive.tar:nested/path/file.txt')).toBe('file.txt');
    });

    it('应该处理无法解析的 URL', () => {
      const result = getDisplayName('some-invalid-url');
      expect(result).toBeDefined();
      expect(typeof result).toBe('string');
    });

    it('应该从 Agent URL 提取文件名', () => {
      expect(getDisplayName('agent://host/var/log/app.log')).toBe('app.log');
    });
  });

  describe('isArchive', () => {
    it('应该识别 tar.gz 文件', () => {
      expect(isArchive('tar.gz+s3://bucket/archive.tar.gz:file.txt')).toBe(true);
    });

    it('应该识别 tar 文件', () => {
      expect(isArchive('tar+file:///archive.tar:file.txt')).toBe(true);
    });

    it('应该返回 false 对于非归档文件', () => {
      expect(isArchive('file:///var/log/app.log')).toBe(false);
      expect(isArchive('s3://bucket/file.txt')).toBe(false);
      expect(isArchive('agent://host/file.log')).toBe(false);
    });
  });

  describe('getFileUrlType', () => {
    it('应该返回正确的文件类型', () => {
      expect(getFileUrlType('file:///path/file.txt')).toBe('local');
      expect(getFileUrlType('s3://bucket/file.txt')).toBe('s3');
      expect(getFileUrlType('tar.gz+s3://bucket/archive:file')).toBe('tar-entry');
      expect(getFileUrlType('agent://host/file')).toBe('agent');
    });

    it('应该返回 null 对于无效 URL', () => {
      expect(getFileUrlType('invalid')).toBeNull();
      expect(getFileUrlType('unknown://scheme')).toBeNull();
    });
  });
});
