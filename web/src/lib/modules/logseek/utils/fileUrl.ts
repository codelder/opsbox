/**
 * FileUrl Utility (Redesigned)
 *
 * Scheme: `ls://<endpoint_type>/<endpoint_id>/<target_type>/<path>?<params>`
 */

export type EndpointType = 'local' | 'agent' | 's3';
export type TargetType = 'dir' | 'archive';

export interface ParsedFileUrl {
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
 * Parse a File URL string
 * @param urlStr The URL string (e.g. "ls://web-01@agent.hk-prod/var/log/syslog")
 */
export function parseFileUrl(urlStr: string): ParsedFileUrl | null {
  try {
    const url = new URL(urlStr);
    if (url.protocol !== 'ls:') return null;

    // Parse id from userinfo
    let endpointId = decodeURIComponent(url.username);
    if (url.password) {
      endpointId += ':' + decodeURIComponent(url.password);
    }

    // Parse type and serverAddr from hostname
    const hostParts = url.hostname.split('.');
    const endpointTypeStr = hostParts[0];
    if (!['local', 'agent', 's3'].includes(endpointTypeStr)) return null;
    const endpointType = endpointTypeStr as EndpointType;

    let serverAddr = hostParts.length > 1 ? hostParts.slice(1).join('.') : undefined;

    // Parse path (strip leading slash)
    let path = decodeURIComponent(url.pathname.startsWith('/') ? url.pathname.slice(1) : url.pathname);

    // Special handling for S3: ls://profile@s3/bucket/path
    if (endpointType === 's3') {
      const slashIndex = path.indexOf('/');
      if (slashIndex !== -1) {
        const bucket = path.substring(0, slashIndex);
        endpointId = `${endpointId}:${bucket}`;
        path = path.substring(slashIndex + 1);
      } else if (path.length > 0) {
        endpointId = `${endpointId}:${path}`;
        path = '';
      }
    }

    if (url.port) {
      serverAddr = (serverAddr || '') + ':' + url.port;
    }

    const entryPath = url.searchParams.get('entry') || undefined;

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
    console.error('Failed to parse file URL:', urlStr, e);
    return null;
  }
}

export function getDisplayName(path: string, entryPath?: string): string {
  const target = entryPath || path;
  const parts = target.split('/');
  return parts[parts.length - 1] || target;
}

export function isArchive(urlStr: string): boolean {
  const parsed = parseFileUrl(urlStr);
  return parsed?.targetType === 'archive';
}

/**
 * Get the file type for icon display
 */
export function getFileUrlType(urlStr: string): EndpointType | null {
  const parsed = parseFileUrl(urlStr);
  return parsed ? parsed.endpointType : null;
}
