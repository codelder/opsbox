/**
 * 高亮工具函数
 * 提供文本转义、高亮和智能截断功能
 */

import type { SnippetResult, SnippetOptions, KeywordInfo } from '../types';

/**
 * 转义 HTML 特殊字符
 */
export function escapeHtml(s: string): string {
  return s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

/**
 * 转义正则表达式特殊字符
 */
export function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * 高亮关键词（使用带 class 的 mark 标签）
 * @param line 要高亮的文本行
 * @param keywords 带类型信息的关键词列表
 */
export function highlight(line: string, keywords: KeywordInfo[]): string {
  if (!line || line.length === 0) return '';
  if (!keywords || keywords.length === 0) return escapeHtml(line);

  const spans: Array<[number, number]> = [];

  for (const kwInfo of keywords) {
    const kw = kwInfo.text;
    if (!kw || kw.length === 0) continue;

    let re: RegExp | null = null;
    if (kwInfo.type === 'literal') {
      re = new RegExp(escapeRegExp(kw), 'gi');
    } else if (kwInfo.type === 'phrase') {
      re = new RegExp(escapeRegExp(kw), 'g');
    } else if (kwInfo.type === 'regex') {
      try {
        re = new RegExp(kw, 'g');
      } catch {
        re = null;
      }
    }
    if (!re) continue;

    let m: RegExpExecArray | null;
    while ((m = re.exec(line)) !== null) {
      const start = m.index;
      const matchText = m[0] ?? '';
      const end = start + matchText.length;
      if (end <= start) {
        re.lastIndex = Math.min(line.length, start + 1);
        continue;
      }
      spans.push([start, end]);
    }
  }

  if (spans.length === 0) return escapeHtml(line);

  spans.sort((a, b) => (a[0] !== b[0] ? a[0] - b[0] : b[1] - a[1]));
  const merged: Array<[number, number]> = [];
  for (const [s, e] of spans) {
    const last = merged[merged.length - 1];
    if (!last || s > last[1]) {
      merged.push([s, e]);
    } else {
      last[1] = Math.max(last[1], e);
    }
  }

  let out = '';
  let cursor = 0;
  for (const [s, e] of merged) {
    out += escapeHtml(line.slice(cursor, s));
    out += `<mark class="highlight">${escapeHtml(line.slice(s, e))}</mark>`;
    cursor = e;
  }
  out += escapeHtml(line.slice(cursor));
  return out;
}

/**
 * 智能截断长行，优先保留首次命中关键字
 * @param line 原始行文本
 * @param keywords 带类型信息的关键词列表
 * @param opts 选项：max=最大长度，context=关键词周围上下文长度
 */
export function snippet(line: string, keywords: KeywordInfo[], opts: SnippetOptions = {}): SnippetResult {
  const max = opts.max ?? 540;
  const ctx = opts.context ?? 230;

  if (line.length <= max) {
    return { html: highlight(line, keywords), leftTrunc: false, rightTrunc: false };
  }

  let firstIdx = -1;
  let firstLen = 0;

  // 查找首个关键词/正则命中位置
  for (const kwInfo of keywords) {
    const kw = kwInfo.text;
    if (!kw || kw.length === 0) continue;

    let idx = -1;
    let len = 0;

    if (kwInfo.type === 'literal') {
      idx = line.toLowerCase().indexOf(kw.toLowerCase());
      len = kw.length;
    } else if (kwInfo.type === 'phrase') {
      idx = line.indexOf(kw);
      len = kw.length;
    } else if (kwInfo.type === 'regex') {
      try {
        const re = new RegExp(kw);
        const m = re.exec(line);
        if (m && typeof m.index === 'number') {
          idx = m.index;
          len = (m[0] ?? '').length;
        }
      } catch {
        // ignore invalid regex
      }
    }

    if (idx !== -1 && (firstIdx === -1 || idx < firstIdx)) {
      firstIdx = idx;
      firstLen = len;
    }
  }

  let start = 0;
  let end = 0;

  if (firstIdx >= 0) {
    // 以关键词为中心截取
    start = Math.max(0, firstIdx - ctx);
    end = Math.min(line.length, firstIdx + firstLen + ctx);
    if (end - start < max) {
      const deficit = max - (end - start);
      const addLeft = Math.min(start, Math.floor(deficit / 2));
      const addRight = Math.min(line.length - end, deficit - addLeft);
      start -= addLeft;
      end += addRight;
    }
  } else {
    // 无关键词，截取开头
    start = 0;
    end = max;
  }

  // 对齐截取边界，避免从单词中间开始或结束
  if (start > 0 && line[start] !== ' ' && line[start - 1] !== ' ') {
    const prevSpace = line.lastIndexOf(' ', start);
    if (prevSpace >= 0 && start - prevSpace < 20) {
      start = prevSpace;
    }
  }

  const leftTrunc = start > 0;
  const rightTrunc = end < line.length;
  const slice = line.slice(start, end);

  return { html: highlight(slice, keywords), leftTrunc, rightTrunc };
}
