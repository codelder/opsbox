/**
 * ORL Utility (Object Resource Locator)
 *
 * Scheme: `orl://[id]@[type][.serverAddr]/[path]?entry=[entryPath]`
 */

import { parse } from 'uri-js';

export type EndpointType = 'local' | 'agent' | 's3';
export type TargetType = 'dir' | 'archive';

export interface OrlInfo {
  serverAddr?: string;
  endpointType: EndpointType;
  endpointId: string;
  targetType: TargetType;
  path: string;
  entryPath?: string;
  original: string;
  displayName: string;
}

/**
 * 解析 ORL URL 字符串
 * @param urlStr 原始 URL 字符串
 */
export function parseOrl(urlStr: string): OrlInfo | null {
  if (!urlStr) return null;

  try {
    const uri = parse(urlStr);

    if (uri.scheme !== 'orl') return null;

    // Parse id from userinfo
    let endpointId = '';
    if (uri.userinfo) {
      endpointId = decodeURIComponent(uri.userinfo);
    }

    // Parse type and serverAddr from host
    const host = uri.host || '';
    const hostParts = host.split('.');
    const endpointTypeStr = hostParts[0];
    if (!['local', 'agent', 's3'].includes(endpointTypeStr)) return null;
    const endpointType = endpointTypeStr as EndpointType;

    let serverAddr = hostParts.length > 1 ? hostParts.slice(1).join('.') : undefined;

    if (endpointType === 'local' && !endpointId) {
      endpointId = 'localhost';
    }

    // Parse path
    let rawPath = uri.path || '';
    if (rawPath.startsWith('/')) {
      rawPath = rawPath.slice(1);
    }
    const path = decodeURIComponent(rawPath);

    if (uri.port) {
      serverAddr = (serverAddr || '') + ':' + uri.port;
    }

    // Parse Query
    const queryParams = new URLSearchParams(uri.query || '');
    const entryPath = queryParams.get('entry') ? decodeURIComponent(queryParams.get('entry')!) : undefined;

    // Infer targetType
    const targetType: TargetType = entryPath ? 'archive' : 'dir';

    return {
      serverAddr,
      endpointType,
      endpointId,
      targetType,
      path,
      entryPath,
      original: urlStr,
      displayName: getDisplayName(path, entryPath)
    };
  } catch (e) {
    console.error('Failed to parse ORL URL:', urlStr, e);
    return null;
  }
}

/**
 * 构造 ORL URL 字符串
 */
export function stringifyOrl(parts: {
  endpointId: string;
  endpointType: EndpointType;
  serverAddr?: string;
  path: string;
  entryPath?: string;
}): string {
  let id = parts.endpointId;

  let host = parts.endpointType;
  let port: string | undefined;

  if (parts.serverAddr) {
    const addrParts = parts.serverAddr.split(':');
    host += '.' + addrParts[0];
    if (addrParts.length > 1) {
      port = addrParts[1];
    }
  }

  if (parts.endpointType === 'local' && id === 'localhost') {
    id = '';
  }

  const url = new URL(`orl://${host}`);
  url.username = id;
  if (port) url.port = port;

  const finalPath = parts.path;
  url.pathname = finalPath.startsWith('/') ? finalPath : '/' + finalPath;

  if (parts.entryPath) {
    url.searchParams.set('entry', parts.entryPath);
  }

  return url.toString().replace('orl://', 'orl://').replace(/%3A/g, ':'); // Ensure scheme is correct and colons are not encoded
}

export function getDisplayName(path: string, entryPath?: string): string {
  const target = entryPath || path;
  const parts = target.split('/');
  return parts[parts.length - 1] || target;
}

export function isArchive(urlStr: string): boolean {
  const parsed = parseOrl(urlStr);
  return parsed?.targetType === 'archive';
}

/**
 * 获取 ORL 类型用于图标显示
 */
export function getOrlType(urlStr: string): EndpointType | null {
  const parsed = parseOrl(urlStr);
  return parsed ? parsed.endpointType : null;
}
