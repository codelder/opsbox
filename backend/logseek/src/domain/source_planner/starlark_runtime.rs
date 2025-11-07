use std::{fs, path::PathBuf};

use opsbox_core::SqlitePool;

use crate::{
  api::models::AppError,
  domain::config::SourceConfig,
  domain::source_planner::{DateRange, PlanResult},
};

/// 通过 Starlark 脚本调度的存储源规划运行时
///
/// 约定：
/// - 按 app 名称加载脚本：优先 $HOME/.opsbox/planners/<app>.star；不存在则回退到内置资源 backend/logseek/src/planners/<app>.star
/// - 运行前注入以下全局变量（脚本可直接使用）：
///   - CLEANED_QUERY: str （移除了 dt:/fdt:/tdt: 的查询）
///   - DATE_RANGE: dict {"start": "YYYY-MM-DD", "end": "YYYY-MM-DD"}
///   - TODAY: "YYYY-MM-DD"（北京时区）
///   - DATES: list[dict]，按日期范围展开的每日对象，每项 {"iso": "YYYY-MM-DD", "yyyymmdd": "YYYYMMDD", "next_yyyymmdd": "YYYYMMDD"}
///   - AGENTS: list[dict]，每项 {"id": str, "tags": dict[str,str]}
///   - S3_PROFILES: list[dict]（非敏感字段），每项 {"profile_name": str, "endpoint": str, "bucket": str}
/// - 脚本需导出：
///   - SOURCES: list[dict]  每项为 SourceConfig 形状的字典（type: "s3"|"agent"|"local"）
///   - 可选 CLEANED_QUERY: str 若未覆盖，则沿用全局 CLEANED_QUERY
pub async fn plan_with_starlark(pool: &SqlitePool, app: Option<&str>, query: &str) -> Result<PlanResult, AppError> {
  use chrono::Utc;
  use chrono_tz::Asia::Shanghai;

  let app = app.unwrap_or("bbip");

  // 1) 解析日期并清理查询
  let today = Utc::now().with_timezone(&Shanghai).date_naive();
  let (cleaned_query, range) = parse_date_directives_from_query(query, today);

  // 2) 预取上下文（Agent 列表与 S3 Profiles 列表）
  // 列出在线 Agent 及其标签，供脚本按标签自行筛选
  let agents_info = if let Some(mgr) = agent_manager::get_global_agent_manager() {
    mgr.list_online_agents().await
  } else {
    vec![]
  };
  let s3_profiles = crate::repository::settings::list_s3_profiles(pool)
    .await
    .map_err(AppError::Settings)?;

  // 3) 生成 Starlark 运行时前缀（全局变量定义）
  let mut prefix = String::new();
  prefix.push_str("# 由后端注入的上下文变量，请勿在脚本内重命名\n");
  prefix.push_str(&format!("CLEANED_QUERY = '{}'\n", esc_single(&cleaned_query)));
  prefix.push_str(&format!("TODAY = '{}'\n", today.format("%Y-%m-%d")));
  prefix.push_str(&format!(
    "DATE_RANGE = {{'start': '{}', 'end': '{}'}}\n",
    range.start.format("%Y-%m-%d"),
    range.end.format("%Y-%m-%d")
  ));

  // DATES：按范围展开的每日字典（中立数据，不包含业务语义）
  prefix.push_str("DATES = [\n");
  {
    use chrono::Duration;
    let mut d = range.start;
    while d <= range.end {
      let dp1 = d + Duration::days(1);
      let iso = d.format("%Y-%m-%d");
      let yyyymmdd = d.format("%Y%m%d");
      let next_yyyymmdd = dp1.format("%Y%m%d");
      prefix.push_str(&format!(
        "  {{'iso': '{}', 'yyyymmdd': '{}', 'next_yyyymmdd': '{}'}},\n",
        iso, yyyymmdd, next_yyyymmdd
      ));
      d += Duration::days(1);
    }
  }
  prefix.push_str("]\n");

  // S3_PROFILES（仅非敏感字段）
  prefix.push_str("S3_PROFILES = [\n");
  for p in &s3_profiles {
    prefix.push_str(&format!(
      "  {{'profile_name': '{}', 'endpoint': '{}', 'bucket': '{}'}},\n",
      esc_single(&p.profile_name),
      esc_single(&p.endpoint),
      esc_single(&p.bucket)
    ));
  }
  prefix.push_str("]\n");

  // AGENTS（id + tags）
  prefix.push_str("AGENTS = [\n");
  for a in &agents_info {
    // 构造 tags 映射文本
    let mut tags_buf = String::new();
    tags_buf.push('{');
    let mut first = true;
    for t in &a.tags {
      if !first {
        tags_buf.push(',');
      }
      first = false;
      tags_buf.push_str(&format!("'{}':'{}'", esc_single(&t.key), esc_single(&t.value)));
    }
    tags_buf.push('}');

    prefix.push_str(&format!("  {{'id': '{}', 'tags': {}}},\n", esc_single(&a.id), tags_buf));
  }
  prefix.push_str("]\n");

  // 4) 读取 Starlark 脚本内容（优先 DB，其次用户目录，最后内置回退）
  let script_body = match load_planner_script_from_db(pool, app).await {
    Some(s) => s,
    None => load_planner_script(app).unwrap_or_else(|| builtin_planner_script(app)),
  };
  let script = format!("{}\n{}", prefix, script_body);

  // 5) 运行 Starlark
  let module = starlark::environment::Module::new();
  let ast = starlark::syntax::AstModule::parse(&format!("{}.star", app), script, &starlark::syntax::Dialect::Extended)
    .map_err(|e| AppError::Settings(opsbox_core::AppError::internal(format!("Starlark 脚本解析失败: {}", e))))?;

  let globals = starlark::environment::GlobalsBuilder::standard().build();
  let mut eval = starlark::eval::Evaluator::new(&module);
  eval
    .eval_module(ast, &globals)
    .map_err(|e| AppError::Settings(opsbox_core::AppError::internal(format!("Starlark 脚本执行失败: {}", e))))?;

  // 6) 读取输出变量
  let cleaned_val = module.get("CLEANED_QUERY");
  let cleaned = cleaned_val.map(|v| v.to_str().to_string()).unwrap_or(cleaned_query);

  let sources_val = module
    .get("SOURCES")
    .ok_or_else(|| AppError::Settings(opsbox_core::AppError::internal("Starlark 未导出 SOURCES")))?;

  // 转为 JSON，再转为 SourceConfig
  let list = starlark::values::list::ListRef::from_value(sources_val)
    .ok_or_else(|| AppError::Settings(opsbox_core::AppError::internal("SOURCES 不是列表类型")))?;

  let mut sources: Vec<SourceConfig> = Vec::new();
  for i in 0..list.len() {
    let Some(v) = list.get(i) else {
      continue;
    };
    let mut j = starlark_to_json(*v).map_err(|e| AppError::Settings(opsbox_core::AppError::internal(e)))?;
    normalize_source_tag(&mut j);
    normalize_source_strings(&mut j);
    let cfg: SourceConfig = serde_json::from_value(j).map_err(|e| {
      AppError::Settings(opsbox_core::AppError::internal(format!(
        "解析 SourceConfig 失败: {}",
        e
      )))
    })?;
    log_script_source(i, &cfg);
    sources.push(cfg);
  }

  log::info!("[Planner] 脚本生成来源总数: {}", sources.len());
  Ok(PlanResult {
    sources,
    cleaned_query: cleaned,
  })
}

// ---------------------------- 内部工具 ----------------------------

/// 解析 dt:/fdt:/tdt: 指令，返回（清理后的 q，日期区间）
fn parse_date_directives_from_query(q_raw: &str, today: chrono::NaiveDate) -> (String, DateRange) {
  use chrono::Datelike;
  let mut dt_q: Option<String> = None;
  let mut fdt_q: Option<String> = None;
  let mut tdt_q: Option<String> = None;

  let tokens: Vec<&str> = q_raw.split_whitespace().collect();
  for t in &tokens {
    if let Some(rest) = t.strip_prefix("dt:") {
      if rest.len() == 8 && rest.chars().all(|c| c.is_ascii_digit()) {
        dt_q = Some(rest.to_string());
      }
    } else if let Some(rest) = t.strip_prefix("fdt:") {
      if rest.len() == 8 && rest.chars().all(|c| c.is_ascii_digit()) {
        fdt_q = Some(rest.to_string());
      }
    } else if let Some(rest) = t.strip_prefix("tdt:")
      && rest.len() == 8
      && rest.chars().all(|c| c.is_ascii_digit())
    {
      tdt_q = Some(rest.to_string());
    }
  }

  let today_str = format!("{:04}{:02}{:02}", today.year(), today.month(), today.day());

  let (start_yyyymmdd, end_yyyymmdd) = if let Some(d) = dt_q {
    (d.clone(), d)
  } else {
    match (fdt_q, tdt_q) {
      (Some(s), Some(e)) => (s, e),
      (Some(s), None) => {
        let e = s.clone();
        (s, e)
      }
      (None, Some(e)) => {
        let s = e.clone();
        (s, e)
      }
      (None, None) => (today_str.clone(), today_str.clone()),
    }
  };

  let parse_or_today = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y%m%d").unwrap_or(today);
  let range = DateRange::new(parse_or_today(&start_yyyymmdd), parse_or_today(&end_yyyymmdd));

  let cleaned = tokens
    .into_iter()
    .filter(|t| !(t.starts_with("dt:") || t.starts_with("fdt:") || t.starts_with("tdt:")))
    .collect::<Vec<_>>()
    .join(" ");

  (cleaned, range)
}

/// 从用户目录或内置目录加载脚本
fn load_planner_script(app: &str) -> Option<String> {
  // 用户目录：$HOME/.opsbox/planners/<app>.star
  if let Ok(home) = std::env::var("HOME") {
    let mut p = PathBuf::from(home);
    p.push(".opsbox/planners");
    p.push(format!("{}.star", app));
    if p.exists()
      && let Ok(s) = fs::read_to_string(&p)
    {
      return Some(s);
    }
  }
  None
}

/// 从数据库加载脚本（若存在返回 Some）
async fn load_planner_script_from_db(pool: &SqlitePool, app: &str) -> Option<String> {
  match crate::repository::planners::load_script_text(pool, app).await {
    Ok(opt) => opt,
    Err(e) => {
      log::warn!("从数据库加载 Planner 脚本失败: {}", e);
      None
    }
  }
}

/// 内置脚本（作为回退）
fn builtin_planner_script(app: &str) -> String {
  match app {
    // bbip 的等效脚本：基于 DATES/TODAY/BUCKETS 生成 Agent 与 S3 源，S3_PROFILES 由业务选择
    "bbip" => include_str!("../../planners/bbip.star").to_string(),
    _ => {
      // 默认空实现：只透传 CLEANED_QUERY，SOURCES 为空
      "# 默认空脚本\nSOURCES = []\n".to_string()
    }
  }
}

/// 字符串转义（单引号与反斜杠），用于内联到 Starlark 代码
fn esc_single(s: &str) -> String {
  let s = s.replace('\\', "\\\\");
  s.replace('\'', "\\'")
}

/// 将 Starlark 值转为 serde_json::Value（仅支持脚本生成的字面量结构）
fn starlark_to_json(v: starlark::values::Value) -> Result<serde_json::Value, String> {
  use starlark::values::{ValueLike, dict::DictRef, list::ListRef, none::NoneType, string::StarlarkStr};

  if let Some(b) = v.unpack_bool() {
    return Ok(serde_json::Value::Bool(b));
  }
  if let Some(i) = v.unpack_i32() {
    return Ok(serde_json::Value::Number(i.into()));
  }
  if let Some(s) = v.downcast_ref::<StarlarkStr>() {
    return Ok(serde_json::Value::String(s.to_string()));
  }
  if v.downcast_ref::<NoneType>().is_some() {
    return Ok(serde_json::Value::Null);
  }
  if let Some(l) = ListRef::from_value(v) {
    let mut arr = Vec::with_capacity(l.len());
    for i in 0..l.len() {
      let Some(it) = l.get(i) else {
        continue;
      };
      arr.push(starlark_to_json(*it)?);
    }
    return Ok(serde_json::Value::Array(arr));
  }
  if let Some(d) = DictRef::from_value(v) {
    let mut map = serde_json::Map::new();
    for (k, v) in d.iter() {
      let ks = k.unpack_str().ok_or_else(|| "字典键必须是字符串".to_string())?;
      map.insert(ks.to_string(), starlark_to_json(v)?);
    }
    return Ok(serde_json::Value::Object(map));
  }
  Err(format!("不支持的 Starlark 值类型: {:?}", v))
}

/// 规范化 SourceConfig 的 type 标签，避免用户脚本误写成 '"s3"' 等
fn normalize_source_tag(j: &mut serde_json::Value) {
  use serde_json::Value as V;
  if let V::Object(map) = j
    && let Some(V::String(t)) = map.get_mut("type")
  {
    let trimmed = t.trim();
    let normalized = if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
      trimmed.trim_matches('"').to_ascii_lowercase()
    } else {
      trimmed.to_ascii_lowercase()
    };
    if *t != normalized {
      log::warn!("规范化脚本返回的来源类型: '{}' -> '{}'", t, normalized);
      *t = normalized;
    }
  }
}

/// 去除顶层所有字符串字段的首尾引号（若存在），修复脚本误写例如 '"s3"'
fn normalize_source_strings(j: &mut serde_json::Value) {
  use serde_json::Value as V;
  if let V::Object(map) = j {
    for (_k, v) in map.iter_mut() {
      if let V::String(s) = v {
        let trimmed = s.trim();
        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
          let newv = trimmed.trim_matches('"').to_string();
          log::warn!("规范化脚本返回的字符串: '{}' -> '{}'", s, newv);
          *s = newv;
        }
      }
    }
  }
}

/// 日志输出用户脚本返回的来源（已规范化并成功解析）
fn log_script_source(idx: usize, cfg: &SourceConfig) {
  match cfg {
    SourceConfig::S3 {
      profile,
      bucket,
      prefix,
      pattern,
      key,
    } => {
      log::info!(
        "[Planner] 来源[{}] type=s3 profile={} bucket={} key={} prefix={} pattern={}",
        idx,
        profile,
        bucket.as_deref().unwrap_or("<none>"),
        key.as_deref().unwrap_or("<none>"),
        prefix.as_deref().unwrap_or("<none>"),
        pattern.as_deref().unwrap_or("<none>")
      );
    }
    SourceConfig::Agent {
      agent_id,
      scope_root,
      path_filter_glob,
      scope,
    } => {
      log::info!(
        "[Planner] 来源[{}] type=agent id={} scope_root={} path_glob={} scope={}",
        idx,
        agent_id,
        scope_root.as_deref().unwrap_or("logs"),
        path_filter_glob.as_deref().unwrap_or("<none>"),
        if scope.is_some() { "是" } else { "否" }
      );
    }
    SourceConfig::Local { path, recursive, scope } => {
      log::info!(
        "[Planner] 来源[{}] type=local path={} recursive={} scope={}",
        idx,
        path,
        recursive,
        if scope.is_some() { "yes" } else { "no" }
      );
    }
  }
}
