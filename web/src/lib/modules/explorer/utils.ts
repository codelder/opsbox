import type { ResourceItem } from './types';

export const TEXT_EXTS = new Set([
  'txt',
  'log',
  'md',
  'json',
  'yaml',
  'yml',
  'sh',
  'bash',
  'zsh',
  'py',
  'js',
  'ts',
  'rs',
  'c',
  'cpp',
  'h',
  'hpp',
  'go',
  'java',
  'tsx',
  'jsx',
  'xml',
  'html',
  'css',
  'sql',
  'toml',
  'gitconfig',
  'env',
  'config',
  'csv'
]);

export function isTextFile(item: ResourceItem): boolean {
  if (item.mime_type) {
    if (
      item.mime_type.startsWith('text/') ||
      item.mime_type === 'application/json' ||
      item.mime_type === 'application/javascript' ||
      item.mime_type === 'application/xml' ||
      item.mime_type.includes('script')
    ) {
      return true;
    }
    if (
      item.mime_type.includes('executable') ||
      item.mime_type.includes('mach-binary') ||
      item.mime_type.includes('elf')
    ) {
      return false;
    }
    return false;
  }

  const lastDotIndex = item.name.lastIndexOf('.');
  if (lastDotIndex === -1) {
    const name = item.name.toLowerCase();
    return ['makefile', 'dockerfile', 'readme', 'license', 'ignore'].includes(name);
  }

  const ext = item.name.slice(lastDotIndex + 1).toLowerCase();
  return TEXT_EXTS.has(ext);
}

export function isImageFile(item: ResourceItem): boolean {
  if (item.mime_type) {
    return item.mime_type.startsWith('image/');
  }
  const lastDotIndex = item.name.lastIndexOf('.');
  if (lastDotIndex === -1) return false;
  const ext = item.name.slice(lastDotIndex + 1).toLowerCase();
  return ['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff'].includes(ext);
}

export function isArchiveFile(item: ResourceItem): boolean {
  if (item.mime_type) {
    return (
      item.mime_type.includes('archive') ||
      item.mime_type.includes('zip') ||
      item.mime_type.includes('tar') ||
      item.mime_type === 'application/x-7z-compressed' ||
      item.mime_type === 'application/x-rar-compressed'
    );
  }
  const lastDotIndex = item.name.lastIndexOf('.');
  if (lastDotIndex === -1) return false;
  const ext = item.name.slice(lastDotIndex + 1).toLowerCase();
  return ['zip', 'tar', 'gz', 'bz2', 'xz', '7z', 'rar', 'jar', 'war'].includes(ext);
}

export function truncateMiddle(str: string, maxVisualWidth: number = 40, tailChars: number = 7): string {
  let visualWidth = 0;
  for (let i = 0; i < str.length; i++) {
    visualWidth += str.charCodeAt(i) > 255 ? 2 : 1;
  }

  if (visualWidth <= maxVisualWidth) return str;

  const ellipsis = '...';
  const tailStr = str.slice(-tailChars);

  let tailWidth = 0;
  for (let i = 0; i < tailStr.length; i++) {
    tailWidth += tailStr.charCodeAt(i) > 255 ? 2 : 1;
  }

  const availableHeadWidth = maxVisualWidth - tailWidth - 3;
  if (availableHeadWidth <= 0) return '...' + tailStr;

  let headStr = '';
  let headWidth = 0;
  for (let i = 0; i < str.length - tailChars; i++) {
    const charWidth = str.charCodeAt(i) > 255 ? 2 : 1;
    if (headWidth + charWidth > availableHeadWidth) break;
    headStr += str[i];
    headWidth += charWidth;
  }

  return headStr + ellipsis + tailStr;
}
