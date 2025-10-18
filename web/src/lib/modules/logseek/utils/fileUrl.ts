/**
 * FileUrl 工具
 *
 * 提供统一文件 URL 的解析和处理功能
 * 对应后端的 FileUrl 枚举
 */

/**
 * 文件 URL 类型
 */
export type FileUrlType = 'local' | 's3' | 'tar-entry' | 'dir-entry' | 'agent';

/**
 * 解析后的文件 URL 信息
 */
export interface ParsedFileUrl {
  type: FileUrlType;
  original: string;
  displayName: string;
}

/**
 * 解析后的本地文件 URL
 */
export interface LocalFileUrl extends ParsedFileUrl {
  type: 'local';
  path: string;
}

/**
 * 解析后的 S3 文件 URL
 */
export interface S3FileUrl extends ParsedFileUrl {
  type: 's3';
  profile?: string; // 配置名称（undefined 表示使用默认配置）
  bucket: string;
  key: string;
}

/**
 * 解析后的 Tar 归档文件 URL
 */
export interface TarEntryFileUrl extends ParsedFileUrl {
  type: 'tar-entry';
  compression: 'tar' | 'tar.gz';
  baseUrl: string; // 基础文件 URL
  entryPath: string; // tar 包内路径
}

export interface DirEntryFileUrl extends ParsedFileUrl {
  type: 'dir-entry';
  baseUrl: string; // 例如 file:///root
  entryPath: string; // 相对路径
}

/**
 * 解析后的 Agent 文件 URL
 */
export interface AgentFileUrl extends ParsedFileUrl {
  type: 'agent';
  agentId: string;
  path: string;
}

/**
 * 所有可能的解析结果
 */
export type AnyParsedFileUrl = LocalFileUrl | S3FileUrl | TarEntryFileUrl | DirEntryFileUrl | AgentFileUrl;

/**
 * 解析文件 URL
 *
 * @param url 文件 URL 字符串
 * @returns 解析后的文件 URL 对象，解析失败返回 null
 *
 * @example
 * ```ts
 * const url1 = parseFileUrl('file:///var/log/app.log');
 * // { type: 'local', path: '/var/log/app.log', displayName: 'app.log', ... }
 *
 * const url2 = parseFileUrl('s3://prod:backupdr/logs/app.log');
 * // { type: 's3', profile: 'prod', bucket: 'backupdr', key: 'logs/app.log', ... }
 *
 * const url3 = parseFileUrl('tar.gz+s3://bucket/archive.tar.gz:logs/app.log');
 * // { type: 'tar-entry', compression: 'tar.gz', baseUrl: 's3://...', entryPath: 'logs/app.log', ... }
 * ```
 */
export function parseFileUrl(url: string): AnyParsedFileUrl | null {
  try {
    const displayName = getDisplayName(url);

    // 处理 tar+<base>:<entry> 或 tar.gz+<base>:<entry>
    if (url.startsWith('tar.gz+')) {
      const afterScheme = url.substring(7); // 'tar.gz+'.length
      const colonIndex = afterScheme.lastIndexOf(':');
      if (colonIndex === -1) return null;

      const baseUrl = afterScheme.substring(0, colonIndex);
      const entryPath = afterScheme.substring(colonIndex + 1);

      return {
        type: 'tar-entry',
        compression: 'tar.gz',
        baseUrl,
        entryPath,
        original: url,
        displayName
      };
    }

    if (url.startsWith('tar+')) {
      const afterScheme = url.substring(4); // 'tar+'.length
      const colonIndex = afterScheme.lastIndexOf(':');
      if (colonIndex === -1) return null;

      const baseUrl = afterScheme.substring(0, colonIndex);
      const entryPath = afterScheme.substring(colonIndex + 1);

      return {
        type: 'tar-entry',
        compression: 'tar',
        baseUrl,
        entryPath,
        original: url,
        displayName
      };
    }

    // 处理 dir+<base>:<entry>
    if (url.startsWith('dir+')) {
      const after = url.substring(4);
      const colonIndex = after.lastIndexOf(':');
      if (colonIndex === -1) return null;
      const baseUrl = after.substring(0, colonIndex);
      const entryPath = after.substring(colonIndex + 1);
      return {
        type: 'dir-entry',
        baseUrl,
        entryPath,
        original: url,
        displayName
      };
    }

    // 处理标准 scheme://... 格式
    const schemeEnd = url.indexOf('://');
    if (schemeEnd === -1) return null;

    const scheme = url.substring(0, schemeEnd);
    const afterScheme = url.substring(schemeEnd + 3);

    switch (scheme) {
      case 'file':
        return {
          type: 'local',
          path: afterScheme,
          original: url,
          displayName
        };

      case 's3': {
        // 注意：JS 的 String.split('/', 2) 与 Rust 的 splitn(2, '/') 语义不同。
        // 这里需要“第一次分隔”，后半段保留所有斜杠。
        const firstSlash = afterScheme.indexOf('/');
        if (firstSlash === -1) return null;
        const bucketPart = afterScheme.substring(0, firstSlash);
        const key = afterScheme.substring(firstSlash + 1);

        // 检查是否包含 profile (格式: profile:bucket)
        const colonIndex = bucketPart.indexOf(':');
        let profile: string | undefined;
        let bucket: string;

        if (colonIndex !== -1) {
          profile = bucketPart.substring(0, colonIndex);
          bucket = bucketPart.substring(colonIndex + 1);
        } else {
          bucket = bucketPart;
        }

        return {
          type: 's3',
          profile,
          bucket,
          key,
          original: url,
          displayName
        };
      }

      case 'agent': {
        // 仅在第一个斜杠处分隔，后半段保留完整路径
        const firstSlash = afterScheme.indexOf('/');
        const agentId = firstSlash === -1 ? afterScheme : afterScheme.substring(0, firstSlash);
        const path = firstSlash === -1 ? '/' : afterScheme.substring(firstSlash);

        return {
          type: 'agent',
          agentId,
          path,
          original: url,
          displayName
        };
      }

      default:
        return null;
    }
  } catch (e) {
    console.error('Failed to parse file URL:', url, e);
    return null;
  }
}

/**
 * 获取文件 URL 的显示名称（文件名部分）
 *
 * @param url 文件 URL 字符串
 * @returns 文件名
 */
export function getDisplayName(url: string): string {
  // 移除 tar+ 或 tar.gz+ 前缀
  let cleanUrl = url;
  if (url.startsWith('tar.gz+')) {
    const colonIndex = url.lastIndexOf(':');
    if (colonIndex !== -1) {
      cleanUrl = url.substring(colonIndex + 1);
    }
  } else if (url.startsWith('tar+')) {
    const colonIndex = url.lastIndexOf(':');
    if (colonIndex !== -1) {
      cleanUrl = url.substring(colonIndex + 1);
    }
  } else if (url.includes('://')) {
    const schemeEnd = url.indexOf('://');
    cleanUrl = url.substring(schemeEnd + 3);
  }

  // 获取路径的最后一部分
  const parts = cleanUrl.split('/').filter((p) => p.length > 0);
  return parts.length > 0 ? parts[parts.length - 1] : url;
}

/**
 * 判断文件 URL 是否为归档文件（tar/tar.gz）
 */
export function isArchive(url: string): boolean {
  return url.startsWith('tar+') || url.startsWith('tar.gz+');
}

/**
 * 获取文件 URL 的类型
 */
export function getFileUrlType(url: string): FileUrlType | null {
  const parsed = parseFileUrl(url);
  return parsed ? parsed.type : null;
}
