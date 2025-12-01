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
  let out = escapeHtml(line);

  for (const kwInfo of keywords) {
    const kw = kwInfo.text;
    if (!kw || kw.length === 0) continue;

    const escapedKw = escapeRegExp(kw);
    // Literal: 不区分大小写，Phrase: 区分大小写
    const flags = kwInfo.type === 'literal' ? 'gi' : 'g';
    const re = new RegExp(escapedKw, flags);
    out = out.replace(re, (m) => `<mark class="highlight">${escapeHtml(m)}</mark>`);
  }
  return out;
}

/**
 * 智能截断长行，优先保留首次命中关键字
 * @param line 原始行文本
 * @param keywords 带类型信息的关键词列表
 * @param opts 选项：max=最大长度，context=关键词周围上下文长度
 */
export function snippet(
  line: string,
  keywords: KeywordInfo[],
  opts: SnippetOptions = {}
): SnippetResult {
  const max = opts.max ?? 540;
  const ctx = opts.context ?? 230;

  if (line.length <= max) {
    return { html: highlight(line, keywords), leftTrunc: false, rightTrunc: false };
  }

  const kws = keywords.map((k) => k.text).filter((k) => k && k.length > 0);

  let firstIdx = -1;
  let firstLen = 0;

  // 查找首个关键词位置（不区分大小写查找）
  for (const kw of kws) {
    const idx = line.toLowerCase().indexOf(kw.toLowerCase());
    if (idx !== -1 && (firstIdx === -1 || idx < firstIdx)) {
      firstIdx = idx;
      firstLen = kw.length;
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
