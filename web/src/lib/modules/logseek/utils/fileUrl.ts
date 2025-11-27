/**
 * FileUrl Utility (Redesigned)
 *
 * Scheme: `ls://<endpoint_type>/<endpoint_id>/<target_type>/<path>?<params>`
 */

export type EndpointType = 'local' | 'agent' | 's3';
export type TargetType = 'dir' | 'archive';

export interface ParsedFileUrl {
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
 * @param urlStr The URL string (e.g. "ls://local/localhost/dir/var/log/syslog")
 */
export function parseFileUrl(urlStr: string): ParsedFileUrl | null {
  try {
    const url = new URL(urlStr);
    if (url.protocol !== 'ls:') return null;

    const endpointType = url.hostname as EndpointType;
    if (!['local', 'agent', 's3'].includes(endpointType)) return null;

    // Path segments: /<endpoint_id>/<target_type>/<path...>
    const segments = url.pathname.split('/').filter((s) => s.length > 0);
    if (segments.length < 2) return null;

    const endpointId = decodeURIComponent(segments[0]);
    const targetTypeStr = segments[1];

    if (targetTypeStr !== 'dir' && targetTypeStr !== 'archive') return null;
    const targetType = targetTypeStr as TargetType;

    // The rest is the path
    const pathSegments = segments.slice(2);
    const path = pathSegments.map(decodeURIComponent).join('/');

    const entryPath = url.searchParams.get('entry') || undefined;

    return {
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
