# 查询语法与 Planner 预处理说明

**文档版本**: v1.1  
**最后更新**: 2026-03-20

这份文档面向两类读者：

- 想理解 `q` 查询串当前语义的开发者
- 想给 LLM / Planner / 自动化脚本提供正确上下文的人

## 接口入口

主要搜索接口：

- `POST /api/v1/logseek/search.ndjson`

查看接口：

- `GET /api/v1/logseek/view.cache.json`

请求体：

```json
{
  "q": "error path:**/*.log app:billing dt:20260319",
  "context": 3
}
```

字段：

- `q`: 查询字符串
- `context`: 上下文行数，默认 `3`

## 查询处理的两阶段

当前实现不是“原样把 q 扔给解析器”，而是两阶段：

### 第一阶段：限定词预处理

由 `parse_query_qualifiers()` 处理：

- `app:`
- `encoding:`
- `path:`
- `-path:`

由 planner runtime 处理：

- `dt:`
- `fdt:`
- `tdt:`

### 第二阶段：内容查询解析

清理后的查询文本再进入：

- `Query::parse_github_like()`

这意味着：

- `path:` 不是最终布尔表达式的一部分，而是被提取成附加路径过滤器
- `app:` 只用于选择 planner 脚本
- `encoding:` 只用于强制解码
- `dt:/fdt:/tdt:` 只给 planner 用来生成日期范围来源

## 支持的限定词

### `app:<name>`

用于选择 planner 脚本：

```text
app:billing error timeout
```

如果未显式提供 `app:`，系统会尝试使用默认 planner。

### `encoding:<name>`

用于强制指定文件编码：

```text
encoding:GBK 错误
```

### `path:<pattern>`

附加包含过滤器，可重复出现：

```text
path:src/** path:test/** error
```

### `-path:<pattern>`

附加排除过滤器，可重复出现：

```text
-path:**/vendor/** error
```

### `dt:/fdt:/tdt:`

由 planner runtime 解析：

- `dt:20260320`
- `fdt:20260318 tdt:20260320`

这些令牌会从 `CLEANED_QUERY` 中剥离，转为日期范围上下文。

## 内容查询语法

解析器支持 GitHub-like 布尔语法。

### 字面量

```text
error
timeout
```

当前实现：

- `Literal` 默认**不区分大小写**

因此：

- `error` 可以匹配 `ERROR`
- `warn` 可以匹配 `Warn`

### 短语

```text
"Connection reset"
```

当前实现：

- `Phrase` 按子串匹配
- **区分大小写**

### 正则

```text
/ERR\d{3}/
/(?i)timeout/
```

当前实现同时支持：

- `regex::Regex`
- `fancy_regex::Regex`

因此 look-around 一类高级语法在部分场景也可用。

### 布尔运算

支持：

- `AND`
- `OR`
- `-` 前缀表示否定
- 括号 `()`

示例：

```text
error AND timeout
(error OR warn) -debug
```

规则：

- 相邻词默认是 AND
- `OR` 必须大写；小写 `or` 会按普通字面量处理
- `AND` 必须大写；小写 `and` 会按普通字面量处理
- 当前实现不支持 `NOT` 关键字，只支持 `-`
- 优先级：否定 > `AND` > `OR`

## 路径过滤语义

### glob 与 contains 的区别

对于 `path:` / `-path:`：

- 如果模式包含 `* ? [ ]`，走 strict glob
- 否则按路径 contains 匹配

### glob 规则

- `*` 不跨目录
- `?` 不跨目录
- `**` 才跨目录

示例：

```text
path:*.log
path:**/*.log
-path:**/vendor/**
```

## 高亮与返回结构

搜索结果以 NDJSON 返回。

当前结果结构中的 `keywords` 是带类型的高亮信息，不再只是字符串数组。

示例：

```json
{
  "type": "result",
  "data": {
    "path": "orl://local/var/log/app.log",
    "keywords": [
      { "type": "literal", "text": "error" },
      { "type": "regex", "text": "ERR\\d+" }
    ],
    "chunks": [
      {
        "range": [10, 12],
        "lines": [
          { "no": 10, "text": "ERROR timeout" }
        ]
      }
    ],
    "encoding": "UTF-8"
  }
}
```

## Planner 运行时上下文

Starlark planner 当前会注入：

- `CLEANED_QUERY`
- `TODAY`
- `DATE_RANGE`
- `DATES`
- `AGENTS`
- `S3_PROFILES`

其中：

- `AGENTS` 只包含在线 Agent（90 秒心跳窗口）
- `S3_PROFILES` 只有非敏感字段：
  - `profile_name`
  - `endpoint`

注意：

- `S3_PROFILES` 不包含 `bucket`
- bucket 需要在 ORL 中明确指定

## 当前常见示例

### 普通内容检索

```text
error timeout
```

### 带路径过滤

```text
error path:**/*.log -path:**/vendor/**
```

### 指定 planner

```text
app:billing error dt:20260320
```

### 指定编码

```text
encoding:GBK 订单失败
```

### 组合示例

```text
app:billing encoding:UTF-8 (error OR warn) path:**/*.log -path:**/archive/** fdt:20260318 tdt:20260320
```

## 易错点

- `OR` 必须大写
- `path:` 冒号后不能插空格
- `path:*.log` 只匹配当前目录，不匹配深层目录
- `path:**/*.log` 才能匹配任意深度
- `Literal` 当前不区分大小写，而 `Phrase` 区分大小写
- `dt:/fdt:/tdt:` 不直接参与文本匹配，只参与 planner 来源生成

## 适合给 LLM 的简短结论

如果你给模型提供这套查询规则，最重要的是告诉它：

1. `app:`、`encoding:`、`path:`、`-path:`、`dt:` 等属于限定词，不是普通内容词。
2. 内容表达式支持布尔查询、短语和正则。
3. `Literal` 默认不区分大小写。
4. `path:` 支持 strict glob，`**` 才跨目录。
5. 搜索结果中的 `path` 是 ORL，不是普通本地路径。
