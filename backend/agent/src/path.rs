//! 路径解析和过滤功能
//!
//! 提供路径解析、白名单校验和路径过滤功能

use globset::{Glob, GlobSet, GlobSetBuilder};
use logseek::domain::config::Target as ConfigTarget;
use std::collections::HashSet;
use std::path::{Path as StdPath, PathBuf};

use crate::config::AgentConfig;

/// 解析 Target 到实际的文件系统路径
pub fn resolve_target_paths(config: &AgentConfig, target: &ConfigTarget) -> Result<Vec<PathBuf>, String> {
  match target {
    ConfigTarget::Dir { path, recursive: _ } => {
      // path 为 "." 表示根目录
      resolve_directory_path(config, path)
    }
    ConfigTarget::Files { paths } => resolve_file_paths(config, paths),
    ConfigTarget::Archive { path, .. } => resolve_targz_path(config, path),
  }
}

/// 解析目录路径（强制白名单校验，禁止越权）
pub fn resolve_directory_path(config: &AgentConfig, relative_path: &str) -> Result<Vec<PathBuf>, String> {
  let mut resolved_paths = Vec::new();
  let canon_roots = canonicalize_roots(&config.search_roots);

  // 1. First, if it's absolute (or looks like one), try as-is
  let rel_as_path = std::path::Path::new(relative_path);

  if rel_as_path.is_absolute()
    && rel_as_path.exists()
    && let Ok(cand_c) = canonicalize_existing(rel_as_path)
    && is_under_any_root(&cand_c, &canon_roots)
  {
    resolved_paths.push(cand_c);
  }

  // 2. If no paths resolved yet, or even if they did, try treating it as relative to roots
  // Strip leading slash if present for relative join
  let normalized_path = relative_path.strip_prefix('/').unwrap_or(relative_path);

  // Only try relative resolution if normalized_path is not empty (or we want to list roots)
  if !normalized_path.is_empty() {
    for root in &config.search_roots {
      let root_path = PathBuf::from(root);
      let full_path = root_path.join(normalized_path);

      if full_path.exists()
        && let Ok(cand_c) = canonicalize_existing(&full_path)
        && let Ok(root_c) = canonicalize_existing(&root_path)
        && cand_c.starts_with(&root_c)
        && !resolved_paths.contains(&cand_c)
      {
        resolved_paths.push(cand_c);
      }

      // 尝试在一级子目录下拼接（兼容原先的"模糊子目录"逻辑）
      if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
          if entry.path().is_dir() {
            let sub_path = entry.path().join(normalized_path);
            if sub_path.exists()
              && let Ok(cand_c) = canonicalize_existing(&sub_path)
              && let Ok(root_c) = canonicalize_existing(&root_path)
              && cand_c.starts_with(&root_c)
              && !resolved_paths.contains(&cand_c)
            {
              resolved_paths.push(cand_c);
            }
          }
        }
      }
    }
  }

  if resolved_paths.is_empty() {
    Err(format!("未找到路径: {}", relative_path))
  } else {
    Ok(resolved_paths)
  }
}

/// 解析文件路径（强制白名单校验，禁止越权）
pub fn resolve_file_paths(config: &AgentConfig, relative_paths: &[String]) -> Result<Vec<PathBuf>, String> {
  let mut resolved_paths = Vec::new();
  let mut resolved_set: HashSet<PathBuf> = HashSet::new();
  let canon_roots = canonicalize_roots(&config.search_roots);

  for p in relative_paths {
    let candidate = PathBuf::from(p);
    if candidate.is_absolute() {
      if candidate.exists() && candidate.is_file() {
        let cand_c = canonicalize_existing(&candidate)?;
        if !is_under_any_root(&cand_c, &canon_roots) {
          return Err(format!("文件路径不在白名单中: {}", cand_c.display()));
        }
        if resolved_set.insert(cand_c.clone()) {
          resolved_paths.push(cand_c);
        }
      }
      continue;
    }

    // 相对路径：逐个根尝试（不再只取第一个命中）
    for root in &config.search_roots {
      let root_path = PathBuf::from(root);
      let full_path = root_path.join(p);
      if full_path.exists() && full_path.is_file() {
        let cand_c = canonicalize_existing(&full_path)?;
        let root_c = canonicalize_existing(&root_path)?;
        if !cand_c.starts_with(&root_c) {
          return Err(format!("文件路径不在白名单中: {}", cand_c.display()));
        }
        if resolved_set.insert(cand_c.clone()) {
          resolved_paths.push(cand_c);
        }
      }
    }
  }

  Ok(resolved_paths)
}

/// 解析归档文件路径（支持 .tar、.tar.gz、.tgz、.gz；强制白名单校验）
pub fn resolve_targz_path(config: &AgentConfig, relative_path: &str) -> Result<Vec<PathBuf>, String> {
  fn is_supported_archive(p: &StdPath) -> bool {
    let lower = p.to_string_lossy().to_lowercase();
    lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz")
  }

  let mut resolved_paths = Vec::new();
  let mut resolved_set: HashSet<PathBuf> = HashSet::new();
  let canon_roots = canonicalize_roots(&config.search_roots);

  // 若传入的是绝对路径，直接检查
  let rel_as_path = PathBuf::from(relative_path);
  if rel_as_path.is_absolute() {
    if rel_as_path.exists() && is_supported_archive(&rel_as_path) {
      let cand_c = canonicalize_existing(&rel_as_path)?;
      if !is_under_any_root(&cand_c, &canon_roots) {
        return Err(format!("归档文件路径不在白名单中: {}", cand_c.display()));
      }
      if resolved_set.insert(cand_c.clone()) {
        resolved_paths.push(cand_c);
      }
    }
  } else {
    // 否则在 search_roots 下拼接查找
    for root in &config.search_roots {
      let root_path = PathBuf::from(root);
      let full_path = root_path.join(relative_path);
      if full_path.exists() && is_supported_archive(&full_path) {
        let cand_c = canonicalize_existing(&full_path)?;
        let root_c = canonicalize_existing(&root_path)?;
        if !cand_c.starts_with(&root_c) {
          return Err(format!("归档文件路径不在白名单中: {}", cand_c.display()));
        }
        if resolved_set.insert(cand_c.clone()) {
          resolved_paths.push(cand_c);
        }
      }
    }
  }

  if resolved_paths.is_empty() {
    Err(format!("未找到归档文件: {}", relative_path))
  } else {
    Ok(resolved_paths)
  }
}

/// 获取可用的子目录列表（用于错误提示）
pub fn get_available_subdirs(config: &AgentConfig) -> Vec<String> {
  let mut subdirs = Vec::new();

  for root in &config.search_roots {
    if let Ok(entries) = std::fs::read_dir(root) {
      for entry in entries.flatten() {
        if entry.path().is_dir()
          && let Some(name) = entry.file_name().to_str()
        {
          subdirs.push(name.to_string());
        }
      }
    }
  }

  subdirs.sort();
  subdirs.dedup();
  subdirs
}

/// 应用路径过滤器
#[allow(dead_code)]
pub fn apply_path_filter(paths: &[PathBuf], filter: &str) -> Result<Vec<PathBuf>, String> {
  let glob = Glob::new(filter).map_err(|e| format!("路径过滤器语法错误: {}", e))?;

  let glob_set = GlobSetBuilder::new()
    .add(glob)
    .build()
    .map_err(|e| format!("构建路径过滤器失败: {}", e))?;

  let mut filtered_paths = Vec::new();

  for path in paths {
    if path.is_file() {
      if glob_set.is_match(path) {
        filtered_paths.push(path.clone());
      }
    } else if path.is_dir() {
      // 递归查找匹配的文件
      find_matching_files(path, &glob_set, &mut filtered_paths)?;
    }
  }

  Ok(filtered_paths)
}

/// 在目录中递归查找匹配的文件
#[allow(dead_code)]
fn find_matching_files(dir: &StdPath, glob_set: &GlobSet, results: &mut Vec<PathBuf>) -> Result<(), String> {
  if let Ok(entries) = std::fs::read_dir(dir) {
    for entry in entries.flatten() {
      let path = entry.path();

      if path.is_file() {
        if glob_set.is_match(&path) {
          results.push(path);
        }
      } else if path.is_dir() {
        find_matching_files(&path, glob_set, results)?;
      }
    }
  }

  Ok(())
}

/// 规范化（canonicalize）已有路径，返回去除符号链接与 .. 的绝对路径
pub fn canonicalize_existing(path: &StdPath) -> Result<PathBuf, String> {
  std::fs::canonicalize(path).map_err(|e| format!("路径规范化失败: {}: {}", path.display(), e))
}

/// 将配置中的 search_roots 规范化（忽略不存在的根）
pub fn canonicalize_roots(roots: &[String]) -> Vec<PathBuf> {
  let mut out = Vec::new();
  for r in roots {
    if let Ok(c) = std::fs::canonicalize(r) {
      out.push(c);
    }
  }
  out
}

/// 判断规范化后的 path 是否位于任一规范化后的根目录之下
pub fn is_under_any_root(path: &StdPath, canon_roots: &[PathBuf]) -> bool {
  canon_roots.iter().any(|root| path.starts_with(root))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  #[test]
  fn test_is_under_any_root_basic() {
    let roots = vec![PathBuf::from("/var/log"), PathBuf::from("/tmp")];
    assert!(is_under_any_root(&PathBuf::from("/var/log/app"), &roots));
    assert!(is_under_any_root(&PathBuf::from("/tmp/file"), &roots));
    assert!(!is_under_any_root(&PathBuf::from("/etc/config"), &roots));
  }

  #[test]
  fn test_is_under_any_root_empty_roots() {
    let roots: Vec<PathBuf> = vec![];
    assert!(!is_under_any_root(&PathBuf::from("/any/path"), &roots));
  }

  #[test]
  fn test_canonicalize_roots_empty() {
    let roots: Vec<String> = vec![];
    let result = canonicalize_roots(&roots);
    assert!(result.is_empty());
  }

  #[test]
  fn test_canonicalize_roots_nonexistent() {
    let roots = vec!["/nonexistent/path/12345".to_string()];
    let result = canonicalize_roots(&roots);
    assert!(result.is_empty());
  }

  #[test]
  fn test_apply_path_filter_with_real_file() {
    // Create temporary files
    let temp_dir = tempfile::tempdir().unwrap();
    let log_file = temp_dir.path().join("app.log");
    let txt_file = temp_dir.path().join("app.txt");
    std::fs::write(&log_file, "log content").unwrap();
    std::fs::write(&txt_file, "text content").unwrap();

    let paths = vec![log_file.clone(), txt_file.clone()];
    let result = apply_path_filter(&paths, "*.log").unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].ends_with("app.log"));
  }

  #[test]
  fn test_apply_path_filter_no_match() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_file = temp_dir.path().join("app.log");
    std::fs::write(&log_file, "log content").unwrap();

    let paths = vec![log_file];
    let result = apply_path_filter(&paths, "*.txt").unwrap();
    assert!(result.is_empty());
  }

  #[test]
  fn test_apply_path_filter_invalid_pattern() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_file = temp_dir.path().join("app.log");
    std::fs::write(&log_file, "log content").unwrap();

    let paths = vec![log_file];
    // Invalid glob pattern
    let result = apply_path_filter(&paths, "[invalid");
    assert!(result.is_err());
  }

  #[test]
  fn test_resolve_target_paths_dir_with_subdir() {
    use crate::config::AgentConfig;
    use std::sync::{Arc, Mutex};

    // 测试 Target::Dir { path: "subdir" } 场景
    // 验证 resolve_target_paths 返回的是完整的解析后路径
    // 这是针对 P1 回归的关键测试
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // 创建目录结构: root/app/logs/
    let app_dir = root.join("app");
    let logs_dir = app_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();

    // 创建配置（只设置必要的字段）
    let config = AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://127.0.0.1:4000".to_string(),
      search_roots: vec![root.to_string_lossy().to_string()],
      listen_port: 4001,
      enable_heartbeat: false,
      heartbeat_interval_secs: 30,
      worker_threads: None,
      log_dir: std::path::PathBuf::from("/tmp"),
      log_retention: 7,
      reload_handle: None,
      current_log_level: Arc::new(Mutex::new("info".to_string())),
    };

    // 测试 1: 使用子目录名 "app" 应该返回完整路径
    // 关键点：返回的路径应该已经是完整的 /path/to/root/app
    // 调用方（search.rs）不应该再拼接 "app"
    let target = ConfigTarget::Dir {
      path: "app".to_string(),
      recursive: true,
    };
    let result = resolve_target_paths(&config, &target).unwrap();
    assert!(!result.is_empty(), "Should resolve 'app' subdirectory");
    // 返回的路径应该是 /path/to/root/app，而不是 /path/to/root
    for resolved_path in &result {
      // 关键断言：解析后的路径应该以 "/app" 结尾
      assert!(
        resolved_path.ends_with("app"),
        "Expected path ending with 'app', got: {:?}",
        resolved_path
      );
      // 验证路径存在
      assert!(
        resolved_path.exists(),
        "Resolved path should exist: {:?}",
        resolved_path
      );
      // 验证是目录
      assert!(
        resolved_path.is_dir(),
        "Resolved path should be a directory: {:?}",
        resolved_path
      );
    }

    // 测试 2: 使用嵌套子目录 "app/logs" 应该返回完整路径
    let target = ConfigTarget::Dir {
      path: "app/logs".to_string(),
      recursive: true,
    };
    let result = resolve_target_paths(&config, &target).unwrap();
    assert!(!result.is_empty(), "Should resolve 'app/logs' subdirectory");
    // 返回的路径应该是 /path/to/root/app/logs
    for resolved_path in &result {
      assert!(
        resolved_path.ends_with("logs"),
        "Expected path ending with 'logs', got: {:?}",
        resolved_path
      );
      // 验证路径存在
      assert!(
        resolved_path.exists(),
        "Resolved path should exist: {:?}",
        resolved_path
      );
    }
  }

  #[test]
  fn test_resolve_target_paths_files() {
    use crate::config::AgentConfig;
    use std::sync::{Arc, Mutex};

    // 测试 Target::Files 场景
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // 创建测试文件
    let file1 = root.join("test1.log");
    let file2 = root.join("test2.log");
    std::fs::write(&file1, "content1").unwrap();
    std::fs::write(&file2, "content2").unwrap();

    let config = AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://127.0.0.1:4000".to_string(),
      search_roots: vec![root.to_string_lossy().to_string()],
      listen_port: 4001,
      enable_heartbeat: false,
      heartbeat_interval_secs: 30,
      worker_threads: None,
      log_dir: std::path::PathBuf::from("/tmp"),
      log_retention: 7,
      reload_handle: None,
      current_log_level: Arc::new(Mutex::new("info".to_string())),
    };

    // 使用绝对路径
    let target = ConfigTarget::Files {
      paths: vec![file1.to_string_lossy().to_string(), file2.to_string_lossy().to_string()],
    };
    let result = resolve_target_paths(&config, &target).unwrap();
    assert_eq!(result.len(), 2);

    // 使用相对路径
    let target = ConfigTarget::Files {
      paths: vec!["test1.log".to_string()],
    };
    let result = resolve_target_paths(&config, &target).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].ends_with("test1.log"));
  }
}
