/**
 * 流式读取 Composable
 * 提供 NDJSON 流分批读取的可复用逻辑
 */

import type { SearchJsonResult, SearchErrorEvent, SearchCompleteEvent } from '../types';

/**
 * 流式读取器状态和方法
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
   * @param onEvent 处理错误/完成事件
   */
  async function readBatch(
    maxItems: number = 20,
    onResult: (result: SearchJsonResult) => void,
    onError: (error: string) => void,
    onEvent?: (event: SearchErrorEvent | SearchCompleteEvent) => void
  ): Promise<{ hasMore: boolean; produced: number }> {
    if (!reader || !decoder) {
      return { hasMore: false, produced: 0 };
    }

    let produced = 0;

    try {
      while (produced < maxItems && reader) {
        // 1) 先消费缓冲区中已有的完整行
        while (produced < maxItems) {
          const nl = buffer.indexOf('\n');
          if (nl === -1) break;

          const line = buffer.slice(0, nl);
          buffer = buffer.slice(nl + 1);
          const trimmed = line.trim();
          if (!trimmed) continue;

          try {
            const obj = JSON.parse(trimmed) as { type: string; data: unknown };

            if (obj.type === 'result') {
              // Success 事件：提取数据并作为搜索结果
              const result = obj.data as SearchJsonResult;
              onResult(result);
              produced += 1;
            } else if (obj.type === 'error') {
              // Error 事件：通知事件处理器
              const errorEvent = { ...(obj.data as object), type: 'error' } as SearchErrorEvent;
              onEvent?.(errorEvent);
            } else if (obj.type === 'complete') {
              // Complete 事件：通知事件处理器
              const completeEvent = { ...(obj.data as object), type: 'complete' } as SearchCompleteEvent;
              onEvent?.(completeEvent);
            } else {
              console.warn('未知的搜索事件类型：', obj.type);
            }
          } catch (e) {
            console.error('解析 NDJSON 行失败：', e, trimmed);
          }
        }

        if (produced >= maxItems) break;

        // 2) 读取更多字节补充缓冲区
        const { done, value } = await reader.read();
        if (done) {
          // 流结束：尽最大努力消费缓冲区剩余内容
          const rest = buffer;
          buffer = '';
          if (rest) {
            const parts = rest.split('\n');
            for (let i = 0; i < parts.length && produced < maxItems; i++) {
              const trimmed = parts[i].trim();
              if (!trimmed) continue;
              try {
                const obj = JSON.parse(trimmed) as { type: string; data: unknown };

                if (obj.type === 'result') {
                  const result = obj.data as SearchJsonResult;
                  onResult(result);
                  produced += 1;
                } else if (obj.type === 'error') {
                  const errorEvent = { ...(obj.data as object), type: 'error' } as SearchErrorEvent;
                  onEvent?.(errorEvent);
                } else if (obj.type === 'complete') {
                  const completeEvent = { ...(obj.data as object), type: 'complete' } as SearchCompleteEvent;
                  onEvent?.(completeEvent);
                } else {
                  console.warn('未知的搜索事件类型：', obj.type);
                }
              } catch (e) {
                console.error('解析 NDJSON 尾段失败：', e, trimmed);
              }
            }
          }
          return { hasMore: false, produced };
        }

        buffer += decoder.decode(value, { stream: true });
      }

      return { hasMore: true, produced };
    } catch (e: unknown) {
      const err = e && typeof e === 'object' ? (e as { name?: string; message?: string }) : {};
      if (err.name === 'AbortError') return { hasMore: false, produced };
      onError(err.message || '搜索过程中发生未知错误');
      reader = null;
      return { hasMore: false, produced };
    }
  }

  /**
   * 清理读取器
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
