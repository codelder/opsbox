<script lang="ts">
  import noFile from '$lib/assets/nofile.svg';
  import noFileDark from '$lib/assets/nofile-dark.svg';
  import error from '$lib/assets/error.svg';
  import errorDark from '$lib/assets/error-dark.svg';
  import empty from '$lib/assets/empty.svg';
  import emptyDark from '$lib/assets/empty-dark.svg';
  import { ChevronDown } from 'lucide-svelte';
  import { resolve } from '$app/paths';

  /**
   * 搜索空状态组件
   * 显示错误、无结果、等待输入等状态
   */
  interface Props {
    /**
     * 状态类型
     */
    type: 'error' | 'no-results' | 'initial';
    /**
     * 错误消息（仅 error 类型使用）
     */
    errorMessage?: string;
    /**
     * 重试回调（仅 error 类型使用）
     */
    onRetry?: () => void;
  }

  let { type, errorMessage, onRetry }: Props = $props();

  // Svelte 5 类型导出
  export type { Props };
</script>

{#if type === 'error'}
  <div class="mx-auto w-full max-w-6xl px-6 py-12">
    <!-- Outer border container -->
    <div class="rounded-lg border border-border bg-card p-12 md:p-16">
      <div class="flex flex-col items-center gap-12 md:flex-row md:items-center md:gap-16">
        <!-- Illustration -->
        <div class="shrink-0">
          <img src={error} alt="Error" class="w-56 md:w-72 dark:hidden" />
          <img src={errorDark} alt="Error" class="hidden w-56 md:w-72 dark:block" />
        </div>

        <!-- Content -->
        <div class="w-full flex-1 space-y-6">
          <div>
            <h3 class="text-2xl font-semibold text-foreground">搜索出错</h3>
            <p class="mt-2 text-muted-foreground">{errorMessage || '发生未知错误，请稍后重试。'}</p>
          </div>

          <!-- Error Details Box -->
          <div class="rounded-md border border-border bg-background text-sm">
            <!-- Error message -->
            <details class="group border-b border-border last:border-0" open>
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>错误详情</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p class="rounded bg-muted p-3 break-all">{errorMessage || '未知错误'}</p>
              </div>
            </details>

            <!-- Troubleshooting -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>故障排查建议</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>如果问题持续存在，请检查：</p>
                <ul class="ml-2 list-inside list-disc space-y-1">
                  <li>网络连接是否正常</li>
                  <li>日志源服务是否可用（S3、Agent 等）</li>
                  <li>搜索查询语法是否正确</li>
                  <li>系统资源是否充足</li>
                </ul>
              </div>
            </details>

            <!-- Retry action -->
            {#if onRetry}
              <div class="p-4">
                <button
                  class="inline-flex items-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors duration-200 hover:bg-primary/90 focus:ring-2 focus:ring-primary focus:ring-offset-2 focus:outline-none"
                  onclick={onRetry}
                >
                  <svg class="mr-2 h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                    />
                  </svg>
                  重新搜索
                </button>
              </div>
            {/if}
          </div>
        </div>
      </div>
    </div>
  </div>
{:else if type === 'no-results'}
  <div class="mx-auto w-full max-w-6xl px-6 py-12">
    <!-- Outer border container -->
    <div class="rounded-lg border border-border bg-card p-12 md:p-16">
      <div class="flex flex-col items-center gap-12 md:flex-row md:items-center md:gap-16">
        <!-- Illustration -->
        <div class="shrink-0">
          <img src={noFile} alt="No results" class="w-56 md:w-72 dark:hidden" />
          <img src={noFileDark} alt="No results" class="hidden w-56 md:w-72 dark:block" />
        </div>

        <!-- Content -->
        <div class="w-full flex-1 space-y-6">
          <div>
            <h3 class="text-2xl font-semibold text-foreground">您的搜索没有匹配到任何日志</h3>
            <p class="mt-2 text-muted-foreground">您可以尝试以下建议。</p>
          </div>

          <!-- Tips Box -->
          <div class="rounded-md border border-border bg-background text-sm">
            <!-- Search across all sources -->
            <details class="group border-b border-border last:border-0" open>
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>跨所有日志源搜索</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>搜索会自动在所有已配置的日志源中进行，包括：</p>
                <ul class="ml-2 list-inside list-disc space-y-1">
                  <li><strong>S3 云存储</strong> - 存储在 S3 的日志文件和归档</li>
                  <li><strong>远程代理</strong> - 通过 Agent 访问的远程服务器日志</li>
                  <li><strong>本地文件</strong> - 本地文件系统中的日志</li>
                </ul>
              </div>
            </details>

            <!-- App qualifier -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>指定应用标识</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>
                  使用 <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">app:</code> 限定词指定应用标识，系统会根据配置的规划脚本自动选择相应的数据源：
                </p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">指定应用:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">app:myapp error</code>

                  <span class="text-foreground">组合使用:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">app:myapp path:logs/**/*.log timeout</code>
                </div>
                <p class="mt-2 text-xs">
                  规划脚本可在<a href={resolve('/settings')} class="text-primary hover:underline">设置</a>中配置
                </p>
              </div>
            </details>

            <!-- Find a particular file extension -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>按路径或文件类型过滤</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <div class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">匹配特定路径:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">path:logs/**/*.log error</code>

                  <span class="text-foreground">排除某些路径:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">-path:node_modules/ error</code>
                </div>
                <p class="mt-2 text-xs">
                  支持通配符 <code class="rounded bg-muted px-1 py-0.5 font-mono">*</code> 和
                  <code class="rounded bg-muted px-1 py-0.5 font-mono">?</code>
                </p>
              </div>
            </details>

            <!-- Boolean operators -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>布尔运算符</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <div class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">AND（默认）:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">error timeout</code>

                  <span class="text-foreground">OR:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">error OR warning</code>

                  <span class="text-foreground">NOT:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">error -debug</code>

                  <span class="text-foreground">分组:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">(error OR warn) timeout</code>
                </div>
              </div>
            </details>

            <!-- Regular expressions -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>正则表达式</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>
                  使用 <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">/pattern/</code> 语法进行正则匹配：
                </p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">匹配错误码:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">/ERR\d+/</code>

                  <span class="text-foreground">匹配 IP 地址:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs"
                    >/\d{`{1,3}`}\.\d{`{1,3}`}\.\d{`{1,3}`}\.\d{`{1,3}`}/</code
                  >
                </div>
              </div>
            </details>

            <!-- Why wasn't my log found? -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>为什么找不到我的日志？</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>搜索仅限于已配置的日志源。请检查：</p>
                <ul class="ml-2 list-inside list-disc space-y-1">
                  <li>
                    日志源是否已在<a href={resolve('/settings')} class="text-primary hover:underline">设置</a>中配置
                  </li>
                  <li>搜索关键词是否正确（区分大小写）</li>
                  <li>路径过滤器是否过于严格</li>
                  <li>文件编码是否支持（支持 UTF-8、GBK、Big5 等）</li>
                </ul>
              </div>
            </details>
          </div>
        </div>
      </div>
    </div>
  </div>
{:else if type === 'initial'}
  <div class="mx-auto w-full max-w-6xl px-6 py-12">
    <!-- Outer border container -->
    <div class="rounded-lg border border-border bg-card p-12 md:p-16">
      <div class="flex flex-col items-center gap-12 md:flex-row md:items-center md:gap-16">
        <!-- Illustration -->
        <div class="shrink-0">
          <img src={empty} alt="Start searching" class="w-56 md:w-72 dark:hidden" />
          <img src={emptyDark} alt="Start searching" class="hidden w-56 md:w-72 dark:block" />
        </div>

        <!-- Content -->
        <div class="w-full flex-1 space-y-6">
          <div>
            <h3 class="text-2xl font-semibold text-foreground">开始搜索您的日志</h3>
            <p class="mt-2 text-muted-foreground">输入关键词、正则表达式或使用高级筛选语法来查找您需要的内容。</p>
          </div>

          <!-- Syntax Guide Box -->
          <div class="rounded-md border border-border bg-background text-sm">
            <!-- App qualifier -->
            <details class="group border-b border-border last:border-0" open>
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>指定应用标识</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>
                  使用 <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">app:</code> 限定词指定应用标识，系统会根据配置的规划脚本自动选择相应的数据源：
                </p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">指定应用:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">app:myapp error</code>

                  <span class="text-foreground">组合使用:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">app:myapp path:logs/**/*.log timeout</code>
                </div>
                <p class="mt-2 text-xs">
                  规划脚本可在<a href={resolve('/settings')} class="text-primary hover:underline">设置</a>中配置
                </p>
              </div>
            </details>

            <!-- Search across all sources -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>跨所有日志源搜索</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>搜索会自动在所有已配置的日志源中进行，包括：</p>
                <ul class="ml-2 list-inside list-disc space-y-1">
                  <li><strong>S3 云存储</strong> - 存储在 S3 的日志文件和归档</li>
                  <li><strong>远程代理</strong> - 通过 Agent 访问的远程服务器日志</li>
                  <li><strong>本地文件</strong> - 本地文件系统中的日志</li>
                </ul>
              </div>
            </details>

            <!-- Path filter -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>按路径或文件类型过滤</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>
                  使用 <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">path:</code> 限定词来过滤文件路径：
                </p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">匹配特定路径:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">path:logs/**/*.log error</code>

                  <span class="text-foreground">排除某些路径:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">-path:node_modules/ error</code>

                  <span class="text-foreground">匹配多个扩展名:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">{'path:**/*.{log,txt} error'}</code>

                  <span class="text-foreground">递归搜索子目录:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">path:**/logs/** error</code>
                </div>
                <p class="mt-2 text-xs">
                  支持通配符 <code class="rounded bg-muted px-1 py-0.5 font-mono">*</code>（匹配任意字符）和
                  <code class="rounded bg-muted px-1 py-0.5 font-mono">?</code>（匹配单个字符），<code
                    class="rounded bg-muted px-1 py-0.5 font-mono">**</code
                  > 表示递归匹配
                </p>
              </div>
            </details>

            <!-- Boolean operators -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>布尔运算符</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>使用逻辑运算符组合多个搜索条件：</p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">AND（默认）:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">error timeout</code>

                  <span class="text-foreground">OR:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">error OR warning</code>

                  <span class="text-foreground">NOT（使用 -）:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">error -debug</code>

                  <span class="text-foreground">分组:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">(error OR warn) timeout</code>

                  <span class="text-foreground">复杂组合:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">(error OR warn) -debug timeout</code>
                </div>
                <p class="mt-2 text-xs">
                  多个关键词之间默认使用 AND 逻辑，使用 <code class="rounded bg-muted px-1 py-0.5 font-mono">OR</code>
                  表示或关系，使用 <code class="rounded bg-muted px-1 py-0.5 font-mono">-</code> 前缀表示排除
                </p>
              </div>
            </details>

            <!-- Regular expressions -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>正则表达式</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>
                  使用 <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">/pattern/</code> 语法进行正则匹配：
                </p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">匹配错误码:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">/ERR\d+/</code>

                  <span class="text-foreground">匹配 IP 地址:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs"
                    >{'/\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}/'}</code
                  >

                  <span class="text-foreground">匹配时间格式:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">{'/\\d{4}-\\d{2}-\\d{2}/'}</code>

                  <span class="text-foreground">组合使用:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">/ERROR|WARN/ timeout</code>
                </div>
                <p class="mt-2 text-xs">正则表达式使用 JavaScript 正则语法，支持所有标准正则特性</p>
              </div>
            </details>

            <!-- Case sensitivity -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>大小写敏感</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <p>默认情况下，搜索是<strong>区分大小写</strong>的：</p>
                <div class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
                  <span class="text-foreground">区分大小写:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">ERROR</code>
                  <span class="col-span-2 text-xs text-foreground">（只匹配 "ERROR"，不匹配 "error"）</span>

                  <span class="text-foreground">使用正则忽略大小写:</span>
                  <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">/(?i)error/</code>
                </div>
              </div>
            </details>

            <!-- Quick tips -->
            <details class="group border-b border-border last:border-0">
              <summary
                class="flex cursor-pointer items-center justify-between p-4 font-medium select-none hover:bg-muted/50"
              >
                <span>快速提示</span>
                <ChevronDown
                  class="h-4 w-4 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
                />
              </summary>
              <div class="space-y-2 px-4 pt-0 pb-4 text-muted-foreground">
                <ul class="ml-2 list-inside list-disc space-y-1">
                  <li>
                    使用引号来搜索包含空格的短语：<code class="rounded bg-muted px-1 py-0.5 font-mono text-xs"
                      >"connection timeout"</code
                    >
                  </li>
                  <li>
                    组合多个限定词可以精确过滤：<code class="rounded bg-muted px-1 py-0.5 font-mono text-xs"
                      >app:myapp path:logs/**/*.log error</code
                    >
                  </li>
                  <li>使用左侧筛选器可以快速按日志源、路径等筛选结果</li>
                  <li>搜索结果支持高亮显示匹配的关键词</li>
                  <li>点击结果卡片可以查看完整的文件内容</li>
                </ul>
              </div>
            </details>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}
