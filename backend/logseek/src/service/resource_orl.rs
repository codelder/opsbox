use opsbox_core::dfs::archive::infer_archive_from_path;
use opsbox_core::dfs::{ArchiveContext, Location, Resource, ResourcePath, build_orl_from_resource};

/// 将搜索结果路径转换为完整 ORL。
///
/// 规则：
/// - 若原资源已有归档上下文，则只更新归档 `entry`。
/// - 若是 Agent 资源且原路径是归档文件（按扩展名推断），为结果补上归档上下文。
/// - 否则按普通文件/目录路径拼接构造新资源。
pub fn build_search_result_orl(resource: &Resource, result_path: &str) -> String {
  if resource.archive_context.is_some() {
    let mut result_resource = resource.clone();
    if let Some(ref mut result_ctx) = result_resource.archive_context {
      result_ctx.inner_path = ResourcePath::parse(result_path);
    }
    return build_orl_from_resource(&result_resource);
  }

  let is_agent = matches!(resource.endpoint.location, Location::Remote { .. });
  if is_agent && infer_archive_from_path(&resource.primary_path.to_string()).is_some() {
    let mut result_resource = resource.clone();
    result_resource.archive_context = Some(ArchiveContext::from_path_str(result_path, None));
    return build_orl_from_resource(&result_resource);
  }

  let full_path = if result_path.starts_with('/') {
    result_path.to_string()
  } else {
    let base = resource.primary_path.to_string();
    let base = base.trim_end_matches('/');
    format!("{}/{}", base, result_path.trim_start_matches('/'))
  };

  let result_resource = Resource::new(resource.endpoint.clone(), ResourcePath::parse(&full_path), None);
  build_orl_from_resource(&result_resource)
}

#[cfg(test)]
mod tests {
  use super::*;
  use opsbox_core::dfs::OrlParser;

  #[test]
  fn should_update_entry_for_archive_resource() {
    let resource = OrlParser::parse("orl://local/tmp/app.tar.gz?entry=/old.log").expect("parse archive orl");
    let result_orl = build_search_result_orl(&resource, "/new.log");
    let parsed = OrlParser::parse(&result_orl).expect("parse result orl");

    let inner = parsed
      .archive_context
      .as_ref()
      .map(|c| c.inner_path.to_string())
      .expect("archive context expected");
    assert_eq!(inner, "/new.log");
  }

  #[test]
  fn should_infer_agent_archive_by_extension() {
    let resource = OrlParser::parse("orl://agent-1@agent/var/log/app.tar.gz").expect("parse agent orl");
    let result_orl = build_search_result_orl(&resource, "/app.log");
    let parsed = OrlParser::parse(&result_orl).expect("parse result orl");

    assert!(parsed.archive_context.is_some(), "agent archive context expected");
    let inner = parsed
      .archive_context
      .as_ref()
      .map(|c| c.inner_path.to_string())
      .expect("archive context expected");
    assert_eq!(inner, "/app.log");
  }

  #[test]
  fn should_join_relative_path_for_regular_resource() {
    let resource = OrlParser::parse("orl://local/tmp/app.log").expect("parse local orl");
    let result_orl = build_search_result_orl(&resource, "part-1.log");
    let parsed = OrlParser::parse(&result_orl).expect("parse result orl");

    assert!(parsed.archive_context.is_none());
    assert_eq!(parsed.primary_path.to_string(), "/tmp/app.log/part-1.log");
  }
}
