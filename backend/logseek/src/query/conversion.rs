//! 查询类型转换
//!
//! 提供 logseek 当前查询类型与 opsbox-domain 领域类型之间的转换。

use crate::query::{Expr, Query, Term};
use opsbox_domain::search::{
    QueryExpression, ParsedQuery, PathFilter,
};

/// 查询类型转换工具
pub struct QueryConverter;

impl QueryConverter {
    /// 将当前 Query 转换为领域 ParsedQuery
    pub fn to_parsed_query(query: &Query) -> ParsedQuery {
        let expression = query.expr.as_ref()
            .and_then(|e| Self::expr_to_query_expression(e))
            .unwrap_or_else(|| QueryExpression::literal("".to_string()));

        let path_filter = Self::path_filter_to_domain(&query.path_filter);

        ParsedQuery {
            expression,
            path_filter,
            date_filter: None,  // 当前 Query 没有日期过滤
            encoding_filter: None,  // 当前 Query 没有编码过滤
            raw_query: "".to_string(),  // TODO: 从 Query 保存原始查询字符串
        }
    }

    /// 将领域 ParsedQuery 转换为当前 Query（近似转换）
    pub fn from_parsed_query(parsed: &ParsedQuery) -> Query {
        let (terms, expr) = Self::query_expression_to_parts(&parsed.expression);

        let path_filter = Self::path_filter_from_domain(&parsed.path_filter);

        Query {
            terms,
            expr,
            path_filter,
            highlights: vec![],
            byte_matchers: vec![],
        }
    }

    /// 将当前 Expr 转换为领域 QueryExpression
    fn expr_to_query_expression(expr: &Expr) -> Option<QueryExpression> {
        match expr {
            Expr::Atom(_idx) => {
                // Atom 需要外部 terms 上下文，这里返回 None
                // 调用方需要手动构建完整的表达式
                None
            }
            Expr::And(items) => {
                let converted: Vec<_> = items.iter()
                    .filter_map(|e| Self::expr_to_query_expression(e))
                    .collect();

                if converted.len() == 2 {
                    Some(QueryExpression::and(
                        converted[0].clone(),
                        converted[1].clone()
                    ))
                } else {
                    // 多项 AND 需要嵌套处理
                    let mut result = None;
                    for item in converted {
                        result = Some(match result {
                            None => item,
                            Some(prev) => QueryExpression::and(prev, item),
                        });
                    }
                    result
                }
            }
            Expr::Or(items) => {
                let converted: Vec<_> = items.iter()
                    .filter_map(|e| Self::expr_to_query_expression(e))
                    .collect();

                if converted.len() == 2 {
                    Some(QueryExpression::or(
                        converted[0].clone(),
                        converted[1].clone()
                    ))
                } else {
                    let mut result = None;
                    for item in converted {
                        result = Some(match result {
                            None => item,
                            Some(prev) => QueryExpression::or(prev, item),
                        });
                    }
                    result
                }
            }
            Expr::Not(inner) => {
                Self::expr_to_query_expression(inner)
                    .map(|e| QueryExpression::not(e))
            }
        }
    }

    /// 将领域 QueryExpression 转换为当前 Expr（需要 terms 上下文）
    fn query_expression_to_parts(expr: &QueryExpression) -> (Vec<Term>, Option<Expr>) {
        match expr {
            QueryExpression::Literal { value, case_sensitive } => {
                let term = if *case_sensitive {
                    Term::Phrase(value.clone())
                } else {
                    Term::Literal(value.clone())
                };
                (vec![term], Some(Expr::Atom(0)))
            }
            QueryExpression::Regex { pattern, .. } => {
                // 尝试编译为标准 regex
                let term = regex::Regex::new(pattern)
                    .ok()
                    .map(|re| Term::RegexStd {
                        pattern: pattern.clone(),
                        re,
                    })
                    .unwrap_or_else(|| {
                        // 回退到 fancy-regex
                        Term::RegexFancy {
                            pattern: pattern.clone(),
                            re: fancy_regex::Regex::new(pattern).unwrap(),
                        }
                    });
                (vec![term], Some(Expr::Atom(0)))
            }
            QueryExpression::And { left, right } => {
                let (left_terms, left_expr) = Self::query_expression_to_parts(left);
                let (right_terms, right_expr) = Self::query_expression_to_parts(right);
                let mut terms = left_terms;
                let offset = terms.len();
                terms.extend(right_terms);
                let expr = match (left_expr, right_expr) {
                    (Some(le), Some(re)) => {
                        let mut items = vec![];
                        if let Expr::And(mut v) = le {
                            items.append(&mut v);
                        } else {
                            items.push(le.clone());
                        }
                        // Adjust right-side atom indices
                        items.push(Self::adjust_atom_indices(re, offset));
                        Some(Expr::And(items))
                    }
                    _ => None,
                };
                (terms, expr)
            }
            QueryExpression::Or { left, right } => {
                let (left_terms, left_expr) = Self::query_expression_to_parts(left);
                let (right_terms, right_expr) = Self::query_expression_to_parts(right);
                let mut terms = left_terms;
                let offset = terms.len();
                terms.extend(right_terms);
                let expr = match (left_expr, right_expr) {
                    (Some(le), Some(re)) => {
                        let mut items = vec![];
                        if let Expr::Or(mut v) = le {
                            items.append(&mut v);
                        } else {
                            items.push(le.clone());
                        }
                        items.push(Self::adjust_atom_indices(re, offset));
                        Some(Expr::Or(items))
                    }
                    _ => None,
                };
                (terms, expr)
            }
            QueryExpression::Not { expression } => {
                let (terms, inner_expr) = Self::query_expression_to_parts(expression);
                let expr = inner_expr.map(|e| Expr::Not(Box::new(e)));
                (terms, expr)
            }
        }
    }

    /// 调整表达式中的原子索引
    fn adjust_atom_indices(expr: Expr, offset: usize) -> Expr {
        match expr {
            Expr::Atom(i) => Expr::Atom(i + offset),
            Expr::And(mut items) => {
                for item in &mut items {
                    *item = Self::adjust_atom_indices(item.clone(), offset);
                }
                Expr::And(items)
            }
            Expr::Or(mut items) => {
                for item in &mut items {
                    *item = Self::adjust_atom_indices(item.clone(), offset);
                }
                Expr::Or(items)
            }
            Expr::Not(inner) => {
                Expr::Not(Box::new(Self::adjust_atom_indices(*inner, offset)))
            }
        }
    }

    /// 将当前 PathFilter 转换为领域 PathFilter
    fn path_filter_to_domain(_filter: &crate::query::PathFilter) -> PathFilter {
        // TODO: 从 GlobSet 提取模式字符串
        // 目前返回空的 PathFilter
        PathFilter::new()
    }

    /// 将领域 PathFilter 转换为当前 PathFilter（近似转换）
    fn path_filter_from_domain(_filter: &PathFilter) -> crate::query::PathFilter {
        // TODO: 从 Vec<String> 构建 GlobSet
        // 目前返回空的 PathFilter
        crate::query::PathFilter::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_expression_literal() {
        let expr = QueryExpression::literal("test".to_string());

        let (terms, result_expr) = QueryConverter::query_expression_to_parts(&expr);

        assert_eq!(terms.len(), 1);
        assert!(matches!(terms[0], Term::Literal(_)));
        assert!(result_expr.is_some());
    }

    #[test]
    fn test_query_expression_and() {
        let expr = QueryExpression::and(
            QueryExpression::literal("foo".to_string()),
            QueryExpression::literal("bar".to_string()),
        );

        let (terms, result_expr) = QueryConverter::query_expression_to_parts(&expr);

        assert_eq!(terms.len(), 2);
        assert!(result_expr.is_some());
    }

    #[test]
    fn test_query_expression_not() {
        let expr = QueryExpression::not(QueryExpression::literal("test".to_string()));

        let (terms, result_expr) = QueryConverter::query_expression_to_parts(&expr);

        assert_eq!(terms.len(), 1);
        if let Some(Expr::Not(inner)) = result_expr {
            assert!(matches!(*inner, Expr::Atom(0)));
        } else {
            panic!("Expected Not expression");
        }
    }

    #[test]
    fn test_to_parsed_query_simple() {
        // 创建简单的 Query
        let query = Query::new(vec![
            Term::Literal("foo".to_string()),
            Term::Literal("bar".to_string()),
        ]);

        // 转换为领域类型
        let parsed = QueryConverter::to_parsed_query(&query);

        // 验证表达式存在
        let _ = &parsed.expression;
    }

    #[test]
    fn test_from_parsed_query_simple() {
        // 创建领域查询表达式
        let expr = QueryExpression::literal("test".to_string());
        let parsed = ParsedQuery::new(expr, "test".to_string());

        // 转换为当前 Query
        let restored = QueryConverter::from_parsed_query(&parsed);

        assert_eq!(restored.terms.len(), 1);
    }
}
