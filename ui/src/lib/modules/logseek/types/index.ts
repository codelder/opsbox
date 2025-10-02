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
  path: string;
  keywords: string[];
  chunks: JsonChunk[];
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
  file: string; // 文件路径
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
