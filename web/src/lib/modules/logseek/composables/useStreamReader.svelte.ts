/**
 * 流式读取 Composable
 * 提供 NDJSON 流分批读取的可复用逻辑
 */

import type { SearchJsonResult, SearchErrorEvent, SearchCompleteEvent, SearchFinishedEvent } from '../types';

/**
 * 搜索事件联合类型
 */
export type SearchEvent = SearchErrorEvent | SearchCompleteEvent | SearchFinishedEvent;

/**
 * 流式读取器状态 and 方法
 */
export function useStreamReader() {
  let reader = $state<ReadableStreamDefaultReader<Uint8Array> | null>(null);
  let decoder = $state<TextDecoder | null>(null);
  let buffer = $state('');

  /**
   * 初始化流读取器
   */
  function initReader(response: Response) {
    reader = response.body?.getReader() || null;
    decoder = new TextDecoder();
    buffer = '';
  }

  /**
   * 读取一批数据（最多 maxItems 条）
   * @param maxItems 最多读取多少条结果事件
   * @param onResult 处理搜索结果
   * @param onError 处理流错误
   * @param onEvent 处理错误/完成/全局完成事件
   */
  async function readBatch(
    maxItems: number = 20,
    onResult: (result: SearchJsonResult) => void,
    onError: (error: string) => void,
    onEvent?: (event: SearchEvent) => void
  ): Promise<{ hasMore: boolean; produced: number }> {
    if (!reader || !decoder) {
      return { hasMore: false, produced: 0 };
    }

    let produced = 0;

    /**
     * 解析单行并分发，返回是否产生了结果(result)
     */
    const parseAndDispatch = (line: string): boolean => {
      const trimmed = line.trim();
      if (!trimmed) return false;
      try {
        const obj = JSON.parse(trimmed);
        if (obj.type === 'result') {
          onResult(obj.data as SearchJsonResult);
          return true;
        } else if (obj.type === 'error' || obj.type === 'complete' || obj.type === 'finished') {
          onEvent?.(obj);
        } else {
          console.warn('未知的搜索事件类型：', obj.type);
        }
      } catch (e) {
        console.error('解析 NDJSON 行失败：', e, trimmed);
      }
      return false;
    };

    /**
     * 消费缓冲区内所有以 \n 结尾的完整行
     */
    const consumeCompleteLines = () => {
      while (produced < maxItems) {
        const nl = buffer.indexOf('\n');
        if (nl === -1) break;
        const line = buffer.slice(0, nl);
        buffer = buffer.slice(nl + 1);
        if (parseAndDispatch(line)) {
          produced++;
        }
      }
    };

    try {
      while (produced < maxItems && reader) {
        // 1) 优先消费缓冲区
        consumeCompleteLines();

        if (produced >= maxItems) break;

        // 2) 缓冲区不够，从网络读取
        const { done, value } = await reader.read();

        if (done) {
          // 流结束：
          // 1. 最后调用一次 decode（不带 stream: true），冲刷掉可能残留在解码器内的半个字符数据
          buffer += decoder.decode();

          // 2. 如果缓冲区还有剩下的文字（即使末尾没有换行符），补一个换行符确保其能被处理
          if (buffer.length > 0) {
            if (!buffer.endsWith('\n')) {
              buffer += '\n';
            }
            consumeCompleteLines();
          }
          buffer = '';
          return { hasMore: false, produced };
        }

        // 解码并追加到缓冲区
        buffer += decoder.decode(value, { stream: true });
      }

      return { hasMore: true, produced };
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { name?: string; message?: string }) : {};
      // 如果是手动中止请求，不视作错误，只停止读取
      if (err.name === 'AbortError') return { hasMore: false, produced };

      onError(err.message || '搜索过程中发生未知错误');
      reader = null; // 发生致命错误，销毁读取器
      return { hasMore: false, produced };
    }
  }

  /**
   * 清理读取器数据
   */
  function cleanup() {
    reader = null;
    decoder = null;
    buffer = '';
  }

  return {
    get reader() {
      return reader;
    },
    initReader,
    readBatch,
    cleanup
  };
}
