//! Search 上下文值对象
//!
//! 查询相关的值对象定义。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 正则表达式引擎类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegexEngine {
    /// Rust regex crate (默认)
    Rust,
    /// PCRE2 (需要外部库支持)
    PCRE2,
}

/// 查询表达式
///
/// 表示解析后的查询条件，支持逻辑组合和嵌套。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueryExpression {
    /// 字面量匹配
    Literal {
        value: String,
        case_sensitive: bool,
    },

    /// 正则表达式匹配
    Regex {
        pattern: String,
        engine: RegexEngine,
    },

    /// 逻辑与
    And {
        left: Box<QueryExpression>,
        right: Box<QueryExpression>,
    },

    /// 逻辑或
    Or {
        left: Box<QueryExpression>,
        right: Box<QueryExpression>,
    },

    /// 逻辑非
    Not {
        expression: Box<QueryExpression>,
    },
}

impl QueryExpression {
    /// 创建字面量匹配表达式
    pub fn literal(value: String) -> Self {
        Self::Literal {
            value,
            case_sensitive: false,
        }
    }

    /// 创建大小写敏感的字面量匹配表达式
    pub fn literal_case_sensitive(value: String) -> Self {
        Self::Literal {
            value,
            case_sensitive: true,
        }
    }

    /// 创建正则表达式匹配
    pub fn regex(pattern: String) -> Self {
        Self::Regex {
            pattern,
            engine: RegexEngine::Rust,
        }
    }

    /// 创建逻辑与表达式
    pub fn and(left: QueryExpression, right: QueryExpression) -> Self {
        Self::And {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// 创建逻辑或表达式
    pub fn or(left: QueryExpression, right: QueryExpression) -> Self {
        Self::Or {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// 创建逻辑非表达式
    pub fn not(expression: QueryExpression) -> Self {
        Self::Not {
            expression: Box::new(expression),
        }
    }

    /// 获取表达式的字符串表示（用于调试）
    pub fn to_debug_string(&self) -> String {
        match self {
            Self::Literal { value, case_sensitive } => {
                if *case_sensitive {
                    format!("\"{}\"", value)
                } else {
                    format!("\"{}\"i", value)
                }
            }
            Self::Regex { pattern, .. } => format!("/{}/", pattern),
            Self::And { left, right } => {
                format!("({} AND {})", left.to_debug_string(), right.to_debug_string())
            }
            Self::Or { left, right } => {
                format!("({} OR {})", left.to_debug_string(), right.to_debug_string())
            }
            Self::Not { expression } => {
                format!("(NOT {})", expression.to_debug_string())
            }
        }
    }
}

/// 路径过滤器
///
/// 用于过滤匹配的文件路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFilter {
    /// 包含的 glob 模式
    pub include_patterns: Vec<String>,
    /// 排除的 glob 模式
    pub exclude_patterns: Vec<String>,
}

impl PathFilter {
    /// 创建新的路径过滤器
    pub fn new() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
        }
    }

    /// 添加包含模式
    pub fn include(mut self, pattern: String) -> Self {
        self.include_patterns.push(pattern);
        self
    }

    /// 添加排除模式
    pub fn exclude(mut self, pattern: String) -> Self {
        self.exclude_patterns.push(pattern);
        self
    }

    /// 判断路径是否匹配过滤器
    pub fn matches(&self, path: &str) -> bool {
        // 检查排除模式（优先）
        for pattern in &self.exclude_patterns {
            if self.matches_glob(path, pattern) {
                return false;
            }
        }

        // 如果没有包含模式，则接受所有路径
        if self.include_patterns.is_empty() {
            return true;
        }

        // 检查包含模式
        for pattern in &self.include_patterns {
            if self.matches_glob(path, pattern) {
                return true;
            }
        }

        false
    }

    /// 简单的 glob 匹配（支持 * 和 **）
    fn matches_glob(&self, path: &str, pattern: &str) -> bool {
        // 简化实现：将 glob 转换为正则表达式
        let regex_pattern = pattern
            .replace('.', r"\.")
            .replace("**", ".*")
            .replace('*', "[^/]*")
            .replace('?', ".");

        // 简单的字符串匹配（生产环境应使用 regex crate）
        if regex_pattern.contains(".*") || regex_pattern.contains("[^/]") {
            // 简化处理：如果模式是前缀匹配
            if regex_pattern.ends_with(".*") {
                let prefix = &regex_pattern[..regex_pattern.len() - 2];
                return path.starts_with(prefix);
            }
            // 其他情况简化为包含匹配
            path.contains(&regex_pattern.replace("[^/]*", "").replace(r"\.", "."))
        } else {
            path == regex_pattern
        }
    }
}

impl Default for PathFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// 日期范围
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    /// 开始日期（ISO 8601 格式）
    pub start: String,
    /// 结束日期（ISO 8601 格式）
    pub end: String,
}

impl DateRange {
    /// 创建新的日期范围
    pub fn new(start: String, end: String) -> Self {
        Self { start, end }
    }

    /// 判断给定日期是否在范围内
    pub fn contains(&self, date: &str) -> bool {
        date >= &self.start && date <= &self.end
    }
}

/// 编码过滤器
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodingFilter {
    /// 允许的编码列表（空表示所有）
    pub allowed_encodings: HashSet<String>,
}

impl EncodingFilter {
    /// 创建新的编码过滤器
    pub fn new() -> Self {
        Self {
            allowed_encodings: HashSet::new(),
        }
    }

    /// 添加允许的编码
    pub fn allow(mut self, encoding: String) -> Self {
        self.allowed_encodings.insert(encoding);
        self
    }

    /// 判断编码是否被允许
    pub fn is_allowed(&self, encoding: &str) -> bool {
        self.allowed_encodings.is_empty() || self.allowed_encodings.contains(encoding)
    }
}

impl Default for EncodingFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析后的查询
///
/// 包含查询条件、路径过滤器、日期过滤器等。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// 查询表达式
    pub expression: QueryExpression,
    /// 路径过滤器
    pub path_filter: PathFilter,
    /// 日期过滤器（可选）
    pub date_filter: Option<DateRange>,
    /// 编码过滤器（可选）
    pub encoding_filter: Option<EncodingFilter>,
    /// 原始查询字符串（用于调试）
    pub raw_query: String,
}

impl ParsedQuery {
    /// 创建新的解析查询
    pub fn new(expression: QueryExpression, raw_query: String) -> Self {
        Self {
            expression,
            path_filter: PathFilter::new(),
            date_filter: None,
            encoding_filter: None,
            raw_query,
        }
    }

    /// 设置路径过滤器
    pub fn with_path_filter(mut self, filter: PathFilter) -> Self {
        self.path_filter = filter;
        self
    }

    /// 设置日期过滤器
    pub fn with_date_filter(mut self, filter: DateRange) -> Self {
        self.date_filter = Some(filter);
        self
    }

    /// 设置编码过滤器
    pub fn with_encoding_filter(mut self, filter: EncodingFilter) -> Self {
        self.encoding_filter = Some(filter);
        self
    }

    /// 获取查询的字符串表示
    pub fn to_string(&self) -> String {
        self.raw_query.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_expression_literal() {
        let expr = QueryExpression::literal("error".to_string());
        assert_eq!(
            expr.to_debug_string(),
            "\"error\"i"
        );

        let expr = QueryExpression::literal_case_sensitive("Error".to_string());
        assert_eq!(
            expr.to_debug_string(),
            "\"Error\""
        );
    }

    #[test]
    fn test_query_expression_regex() {
        let expr = QueryExpression::regex(r"\d+\.\d+\.\d+\.\d+".to_string());
        assert_eq!(
            expr.to_debug_string(),
            r"/\d+\.\d+\.\d+\.\d+/"
        );
    }

    #[test]
    fn test_query_expression_and() {
        let left = QueryExpression::literal("error".to_string());
        let right = QueryExpression::literal("fatal".to_string());
        let expr = QueryExpression::and(left, right);
        assert_eq!(
            expr.to_debug_string(),
            "(\"error\"i AND \"fatal\"i)"
        );
    }

    #[test]
    fn test_query_expression_or() {
        let left = QueryExpression::literal("error".to_string());
        let right = QueryExpression::literal("warn".to_string());
        let expr = QueryExpression::or(left, right);
        assert_eq!(
            expr.to_debug_string(),
            "(\"error\"i OR \"warn\"i)"
        );
    }

    #[test]
    fn test_query_expression_not() {
        let expr = QueryExpression::not(QueryExpression::literal("debug".to_string()));
        assert_eq!(
            expr.to_debug_string(),
            "(NOT \"debug\"i)"
        );
    }

    #[test]
    fn test_path_filter_new() {
        let filter = PathFilter::new();
        assert!(filter.include_patterns.is_empty());
        assert!(filter.exclude_patterns.is_empty());
        assert!(filter.matches("/any/path"));
    }

    #[test]
    fn test_path_filter_include() {
        let filter = PathFilter::new()
            .include("*.log".to_string());
        assert!(filter.matches("/var/log/app.log"));
        assert!(!filter.matches("/var/log/app.txt"));
    }

    #[test]
    fn test_path_filter_exclude() {
        let filter = PathFilter::new()
            .exclude("*.tmp".to_string());
        assert!(!filter.matches("/var/log/app.tmp"));
        assert!(filter.matches("/var/log/app.log"));
    }

    #[test]
    fn test_date_range_contains() {
        let range = DateRange::new("2023-01-01".to_string(), "2023-12-31".to_string());
        assert!(range.contains("2023-06-15"));
        assert!(!range.contains("2024-01-01"));
    }

    #[test]
    fn test_encoding_filter_allow() {
        let filter = EncodingFilter::new()
            .allow("utf-8".to_string())
            .allow("gbk".to_string());

        assert!(filter.is_allowed("utf-8"));
        assert!(filter.is_allowed("gbk"));
        assert!(!filter.is_allowed("ascii"));
    }

    #[test]
    fn test_encoding_filter_default() {
        let filter = EncodingFilter::new();
        assert!(filter.is_allowed("any-encoding"));
    }

    #[test]
    fn test_parsed_query_new() {
        let expr = QueryExpression::literal("error".to_string());
        let query = ParsedQuery::new(expr, "error".to_string());

        assert_eq!(query.raw_query, "error");
        assert!(query.path_filter.include_patterns.is_empty());
        assert!(query.date_filter.is_none());
        assert!(query.encoding_filter.is_none());
    }

    #[test]
    fn test_parsed_query_with_filters() {
        let expr = QueryExpression::literal("error".to_string());
        let query = ParsedQuery::new(expr, "error".to_string())
            .with_path_filter(PathFilter::new().include("*.log".to_string()))
            .with_date_filter(DateRange::new("2023-01-01".to_string(), "2023-12-31".to_string()))
            .with_encoding_filter(EncodingFilter::new().allow("utf-8".to_string()));

        assert_eq!(query.path_filter.include_patterns.len(), 1);
        assert!(query.date_filter.is_some());
        assert!(query.encoding_filter.is_some());
    }

    #[test]
    fn test_query_expression_complex() {
        let expr = QueryExpression::and(
            QueryExpression::or(
                QueryExpression::literal("error".to_string()),
                QueryExpression::literal("fatal".to_string()),
            ),
            QueryExpression::not(QueryExpression::literal("debug".to_string())),
        );

        assert_eq!(
            expr.to_debug_string(),
            "((\"error\"i OR \"fatal\"i) AND (NOT \"debug\"i))"
        );
    }
}
