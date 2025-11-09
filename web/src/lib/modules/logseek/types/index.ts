/**
 * LogSeek 模块类型定义
 * 集中管理所有 LogSeek 相关的 TypeScript 类型
 */

// ============ 搜索相关类型 ============

/**
 * JSON 行结构（NDJSON 流中的单行）
 */
export interface JsonLine {
  no: number;
  text: string;
}

/**
 * JSON 块结构（包含行号范围和多行内容）
 */
export interface JsonChunk {
  range: [number, number] | { 0: number; 1: number };
  lines: JsonLine[];
}

/**
 * 搜索结果（NDJSON 流中的单个文件结果）
 */
export interface SearchJsonResult {
  /**
   * 文件 URL 标识符
   *
   * 支持多种格式：
   * - 本地文件: `file:///path/to/file.log`
   * - S3 对象（默认配置）: `s3://bucket/path/to/file`
   * - S3 对象（指定配置）: `s3://profile:bucket/path/to/file`
   * - Tar 压缩包内文件: `tar.gz+s3://bucket/archive.tar.gz:logs/app.log`
   * - Agent 远程文件: `agent://server-01/var/log/app.log`
   */
  path: string;
  keywords: string[];
  chunks: JsonChunk[];
  /**
   * 文件编码名称（如 "UTF-8"、"GBK"）
   */
  encoding?: string;
}

/**
 * 搜索请求体
 */
export interface SearchBody {
  q: string; // 查询字符串
}

// ============ 设置相关类型 ============

/**
 * S3 对象存储配置负载（用于 POST 请求）
 * 支持 AWS S3、MinIO、阿里云 OSS 等 S3 兼容存储
 */
export interface S3SettingsPayload {
  endpoint: string;
  bucket: string;
  access_key: string;
  secret_key: string;
}

/**
 * S3 对象存储设置响应（包含连接状态）
 */
export interface S3SettingsResponse extends S3SettingsPayload {
  configured?: boolean;
  connection_error?: string | null;
}

/**
 * S3 Profile 负载（用于 POST 请求）
 *
 * 每个 Profile 包含完整的 S3 访问配置：Endpoint + Bucket + Credentials
 */
export interface S3ProfilePayload {
  profile_name: string;
  endpoint: string;
  bucket: string;
  access_key: string;
  secret_key: string;
}

/**
 * S3 Profile 列表响应
 */
export interface S3ProfileListResponse {
  profiles: S3ProfilePayload[];
}

// ============ LLM 设置相关类型 ============

export type LlmProviderType = 'ollama' | 'openai';

export interface LlmBackendUpsertPayload {
  name: string;
  provider: LlmProviderType;
  base_url: string;
  model: string;
  timeout_secs?: number;
  api_key?: string; // openai
  organization?: string; // openai
  project?: string; // openai
}

export interface LlmBackendListItem {
  name: string;
  provider: LlmProviderType;
  base_url: string;
  model: string;
  timeout_secs: number;
  has_api_key: boolean;
}

export interface LlmBackendListResponse {
  backends: LlmBackendListItem[];
  default: string | null;
}

// ============ 自然语言转查询 ============

/**
 * NL2Q 请求体
 */
export interface NL2QRequest {
  nl: string; // 自然语言文本
}

/**
 * NL2Q 响应
 */
export interface NL2QResponse {
  q: string; // 生成的查询字符串
}

// ============ 文件查看相关类型 ============

/**
 * 查看缓存参数（URL 查询参数）
 */
export interface ViewParams {
  sid: string; // 会话 ID
  /**
   * 文件 URL 标识符（同 SearchJsonResult.path）
   *
   * 支持的格式示例：
   * - `file:///var/log/app.log`
   * - `s3://backupdr/logs/app.log`
   * - `tar.gz+s3://bucket/archive.tar.gz:logs/app.log`
   */
  file: string;
  start: number; // 起始行号（1-based）
  end: number; // 结束行号（包含）
}

/**
 * 查看缓存响应
 */
export interface ViewCacheResponse {
  file: string;
  total: number; // 文件总行数
  start: number;
  end: number;
  keywords: string[];
  lines: JsonLine[];
}

// ============ UI 状态类型 ============

/**
 * 搜索 UI 状态
 */
export interface SearchState {
  query: string;
  results: SearchJsonResult[];
  loading: boolean;
  error: string | null;
  sid: string; // 搜索会话 ID
  hasMore: boolean;
}

/**
 * 设置 UI 状态
 */
export interface SettingsState {
  endpoint: string;
  bucket: string;
  accessKey: string;
  secretKey: string;
  loadingSettings: boolean;
  loadError: string | null;
  saving: boolean;
  saveError: string | null;
  saveSuccess: boolean;
  loadedOnce: boolean;
  connectionError: string | null;
}

/**
 * 文件查看 UI 状态
 */
export interface ViewState {
  file: string;
  sid: string;
  total: number;
  start: number;
  end: number;
  keywords: string[];
  lines: JsonLine[];
  loading: boolean;
  error: string | null;
}

// ============ 搜索事件类型 ============

/**
 * 搜索错误事件（从流中接收）
 */
export interface SearchErrorEvent {
  source: string; // 错误来源
  message: string; // 错误信息
  recoverable: boolean; // 是否可恢复（是否继续搜索其他源）
}

/**
 * 搜索完成事件（从流中接收）
 */
export interface SearchCompleteEvent {
  source: string; // 完成的来源
  elapsed_ms: number; // 耗时（毫秒）
}

// ============ 工具类型 ============

/**
 * API 错误响应（RFC 7807 Problem Details）
 */
export interface ApiProblem {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
}

/**
 * 高亮片段结果
 */
export interface SnippetResult {
  html: string; // 带 <mark> 标签的 HTML
  leftTrunc: boolean; // 左侧是否被截断
  rightTrunc: boolean; // 右侧是否被截断
}

/**
 * 片段选项
 */
export interface SnippetOptions {
  max?: number; // 最大长度
  context?: number; // 关键词周围上下文长度
}
