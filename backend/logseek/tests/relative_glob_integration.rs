use logseek::domain::config::Target;
use logseek::query::Query;
use logseek::service::entry_stream::EntryStreamProcessor;
use logseek::service::search::SearchProcessor;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_relative_glob_filtering() -> std::io::Result<()> {
  // 1. 创建测试目录结构
  // root/
  //   root.log (应该不匹配 */*.log，只匹配 *.log 或 **)
  //   sub/
  //     target.log (应该匹配 */*.log)
  //   deep/
  //     nested/
  //       deep.log (应该不匹配 */*.log，只匹配 **/*.log)
  let temp_dir = TempDir::new()?;
  let root = temp_dir.path();
  let sub_dir = root.join("sub");
  let deep_dir = root.join("deep").join("nested");

  fs::create_dir_all(&sub_dir).await?;
  fs::create_dir_all(&deep_dir).await?;

  fs::write(root.join("root.log"), "match me").await?;
  fs::write(sub_dir.join("target.log"), "match me").await?;
  fs::write(deep_dir.join("deep.log"), "match me").await?;

  // 2. 准备搜索处理器
  // 查询词 "match"，上下文 0
  let spec = Arc::new(Query::parse_github_like("match").unwrap());
  let processor = Arc::new(SearchProcessor::new(spec, 0));

  // 3. 测试案例 A: filter_glob = "*/*.log"
  // 期望：只匹配 sub/target.log
  // root.log 在根目录，相对路径是 "root.log"，不匹配 "*/*.log"（因为没有目录分隔符）
  // deep/nested/deep.log 相对路径是 "deep/nested/deep.log"，不匹配 "*/*.log"（因为有两个目录分隔符）
  {
    let filter = logseek::query::path_glob_to_filter("*/*.log").unwrap();
    let mut stream_processor = EntryStreamProcessor::new(processor.clone())
      .with_base_path(root.clone())
      .with_extra_path_filter(filter);

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // 构建本地流（递归）
    let path_str = root.to_string_lossy().to_string();
    let mut estream = logseek::service::entry_stream::build_local_entry_stream(
      &path_str,
      Some(Target::Dir {
        path: ".".to_string(),
        recursive: true,
      }),
    )
    .await
    .expect("构建流失败");

    tokio::spawn(async move {
      let _ = stream_processor.process_stream(&mut *estream, tx).await;
    });

    let mut matched_files = Vec::new();
    while let Some(event) = rx.recv().await {
      if let logseek::service::search::SearchEvent::Success(res) = event {
        // 存储相对于 root 的路径以便验证
        let rel_path = std::path::Path::new(&res.path).strip_prefix(root).unwrap();
        matched_files.push(rel_path.to_string_lossy().to_string());
      }
    }

    // 验证结果
    println!("*/*.log matched: {:?}", matched_files);
    assert!(
      matched_files.contains(&"sub/target.log".to_string()),
      "应该匹配 sub/target.log"
    );
    assert!(!matched_files.contains(&"root.log".to_string()), "不应匹配 root.log");
    assert!(
      !matched_files.contains(&"deep/nested/deep.log".to_string()),
      "不应匹配 deep/nested/deep.log"
    );
    assert_eq!(matched_files.len(), 1, "只应该匹配一个文件");
  }

  // 4. 测试案例 B: filter_glob = "**/*.log"
  // 期望：匹配所有 log 文件
  {
    let filter = logseek::query::path_glob_to_filter("**/*.log").unwrap();
    let mut stream_processor = EntryStreamProcessor::new(processor.clone())
      .with_base_path(root.clone())
      .with_extra_path_filter(filter);

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let path_str = root.to_string_lossy().to_string();
    let mut estream = logseek::service::entry_stream::build_local_entry_stream(
      &path_str,
      Some(Target::Dir {
        path: ".".to_string(),
        recursive: true,
      }),
    )
    .await
    .expect("构建流失败");

    tokio::spawn(async move {
      let _ = stream_processor.process_stream(&mut *estream, tx).await;
    });

    let mut matched_files = Vec::new();
    while let Some(event) = rx.recv().await {
      if let logseek::service::search::SearchEvent::Success(res) = event {
        let rel_path = std::path::Path::new(&res.path).strip_prefix(root).unwrap();
        matched_files.push(rel_path.to_string_lossy().to_string());
      }
    }

    println!("**/*.log matched: {:?}", matched_files);
    assert!(matched_files.contains(&"sub/target.log".to_string()));
    assert!(matched_files.contains(&"root.log".to_string()));
    assert!(matched_files.contains(&"deep/nested/deep.log".to_string()));
    assert_eq!(matched_files.len(), 3);
  }

  Ok(())
}
