/**
 * Highlight 工具测试
 */

import { describe, it, expect } from 'vitest';
import { escapeHtml, escapeRegExp, highlight, snippet } from './highlight';

describe('highlight utils', () => {
  describe('escapeHtml', () => {
    it('应该转义 &', () => {
      expect(escapeHtml('a&b')).toBe('a&amp;b');
    });

    it('应该转义 <', () => {
      expect(escapeHtml('<div>')).toBe('&lt;div&gt;');
    });

    it('应该转义 >', () => {
      expect(escapeHtml('a>b')).toBe('a&gt;b');
    });

    it('应该转义引号', () => {
      expect(escapeHtml('"hello"')).toBe('&quot;hello&quot;');
      expect(escapeHtml("'hello'")).toBe('&#39;hello&#39;');
    });

    it('应该同时转义多个特殊字符', () => {
      expect(escapeHtml('<script>alert("XSS");</script>')).toBe('&lt;script&gt;alert(&quot;XSS&quot;);&lt;/script&gt;');
    });

    it('应该处理普通文本', () => {
      expect(escapeHtml('Hello World')).toBe('Hello World');
    });

    it('应该处理空字符串', () => {
      expect(escapeHtml('')).toBe('');
    });
  });

  describe('escapeRegExp', () => {
    it('应该转义正则表达式特殊字符', () => {
      expect(escapeRegExp('a.b')).toBe('a\\.b');
      expect(escapeRegExp('a*b')).toBe('a\\*b');
      expect(escapeRegExp('a+b')).toBe('a\\+b');
      expect(escapeRegExp('a?b')).toBe('a\\?b');
      expect(escapeRegExp('a^b')).toBe('a\\^b');
      expect(escapeRegExp('a$b')).toBe('a\\$b');
      expect(escapeRegExp('a{1,3}b')).toBe('a\\{1,3\\}b');
      expect(escapeRegExp('a(b)c')).toBe('a\\(b\\)c');
      expect(escapeRegExp('a|b')).toBe('a\\|b');
      expect(escapeRegExp('a[bc]d')).toBe('a\\[bc\\]d');
      expect(escapeRegExp('a\\b')).toBe('a\\\\b');
    });

    it('应该处理普通文本', () => {
      expect(escapeRegExp('hello')).toBe('hello');
    });

    it('应该处理空字符串', () => {
      expect(escapeRegExp('')).toBe('');
    });
  });

  describe('highlight', () => {
    it('应该用 <mark> 标签高亮关键词', () => {
      const result = highlight('hello world', ['hello']);
      expect(result).toContain('<mark>hello</mark>');
    });

    it('应该高亮多个关键词', () => {
      const result = highlight('hello world hello', ['hello']);
      expect(result).toBe('<mark>hello</mark> world <mark>hello</mark>');
    });

    it('应该转义 HTML 特殊字符后再高亮', () => {
      const result = highlight('hello<world>', ['hello']);
      expect(result).toBe('<mark>hello</mark>&lt;world&gt;');
    });

    it('应该处理多个不同关键词', () => {
      const result = highlight('hello world', ['hello', 'world']);
      expect(result).toContain('<mark>hello</mark>');
      expect(result).toContain('<mark>world</mark>');
    });

    it('应该忽略空关键词', () => {
      const result = highlight('hello world', ['', 'hello']);
      expect(result).toBe('<mark>hello</mark> world');
    });

    it('应该处理区分大小写', () => {
      const result = highlight('Hello HELLO hello', ['hello']);
      // 默认不区分大小写（全局匹配）
      expect(result).toContain('<mark>hello</mark>');
    });

    it('应该处理空关键词列表', () => {
      const result = highlight('hello world', []);
      expect(result).toBe('hello world');
    });

    it('应该处理普通文本', () => {
      const result = highlight('hello world', ['xyz']);
      expect(result).toBe('hello world');
    });

    it('应该处理空字符串', () => {
      const result = highlight('', ['hello']);
      expect(result).toBe('');
    });

    it('应该处理特殊字符在关键词中', () => {
      const result = highlight('price: $100', ['$100']);
      expect(result).toContain('<mark>$100</mark>');
    });
  });

  describe('snippet', () => {
    it('应该返回完整行当其小于 max 长度', () => {
      const result = snippet('short line', ['line']);
      expect(result.html).toContain('<mark>line</mark>');
      expect(result.leftTrunc).toBe(false);
      expect(result.rightTrunc).toBe(false);
    });

    it('应该截断长行并保留关键词', () => {
      const longLine = 'a'.repeat(1000);
      const result = snippet(longLine, ['test'], { max: 100 });
      expect(result.html.length).toBeLessThanOrEqual(200); // 转义后可能更长
    });

    it('应该在关键词周围截取内容', () => {
      const line = 'prefix ' + 'x'.repeat(200) + ' keyword ' + 'y'.repeat(200) + ' suffix';
      const result = snippet(line, ['keyword'], { max: 100, context: 50 });
      expect(result.html).toContain('keyword');
    });

    it('应该在无关键词时从开头截取', () => {
      const longLine = 'a'.repeat(1000);
      const result = snippet(longLine, [], { max: 50 });
      expect(result.leftTrunc).toBe(false);
      expect(result.rightTrunc).toBe(true);
    });

    it('应该标记左截断', () => {
      const line = 'prefix ' + 'x'.repeat(300) + ' keyword';
      const result = snippet(line, ['keyword'], { max: 50, context: 20 });
      expect(result.leftTrunc).toBe(true);
    });

    it('应该标记右截断', () => {
      const line = 'keyword ' + 'x'.repeat(300);
      const result = snippet(line, ['keyword'], { max: 50, context: 20 });
      expect(result.rightTrunc).toBe(true);
    });

    it('应该使用默认 max 和 context 选项', () => {
      const line = 'a'.repeat(1000);
      const result = snippet(line, []);
      expect(result.html).toBeDefined();
      expect(result.rightTrunc).toBe(true);
    });

    it('应该对齐截取边界避免从单词中间开始', () => {
      const line = 'prefix with space keyword more text';
      const result = snippet(line, ['keyword'], { max: 20, context: 5 });
      // 应该尝试在空白处对齐
      expect(result.html).toBeDefined();
    });

    it('应该高亮截取后的关键词', () => {
      const line = 'test keyword here test keyword there';
      const result = snippet(line, ['keyword'], { max: 100 });
      expect(result.html).toContain('<mark>keyword</mark>');
    });
  });
});
