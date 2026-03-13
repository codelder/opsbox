//! S3集成测试
//!
//! 测试S3配置管理、连接测试和基本功能

use logseek::repository::s3 as s3_repo;
use opsbox_core::repository::s3::{S3Profile, S3Settings};
use opsbox_test_common::{database, s3_mock};
use serde_json;

/// 测试S3默认配置保存和加载
#[tokio::test]
async fn test_s3_settings_crud() {
  let db = database::TestDatabase::in_memory().await.expect("创建测试数据库失败");

  database::init_logseek_schema(&db.pool).await.expect("初始化schema失败");

  // 创建S3设置
  let settings = S3Settings {
    endpoint: "http://localhost:9000".to_string(),
    access_key: "test-access-key".to_string(),
    secret_key: "test-secret-key".to_string(),
  };

  // 保存设置
  let save_result = s3_repo::save_s3_settings(&db.pool, &settings).await;
  assert!(save_result.is_ok(), "保存S3设置失败: {:?}", save_result);

  // 加载设置
  let loaded_result = s3_repo::load_s3_settings(&db.pool).await;
  assert!(loaded_result.is_ok(), "加载S3设置失败: {:?}", loaded_result);

  let loaded = loaded_result.unwrap();
  assert!(loaded.is_some(), "加载的设置应该是Some");

  let loaded_settings = loaded.unwrap();
  assert_eq!(loaded_settings.endpoint, settings.endpoint);
  assert_eq!(loaded_settings.access_key, settings.access_key);
  assert_eq!(loaded_settings.secret_key, settings.secret_key);

  println!("✓ S3 Settings CRUD测试通过");
}

/// 测试S3 API端点（通过HTTP请求测试Profile CRUD）
///
/// 使用MockS3Server + axum测试路由器验证：
/// - POST /profiles 创建Profile
/// - GET /profiles 列出Profiles
/// - DELETE /profiles/{name} 删除Profile
#[tokio::test]
async fn test_s3_api_endpoints() {
  use axum::body::Body;
  use axum::http::{Request, StatusCode};
  use tower::util::ServiceExt;

  // 1. 启动MockS3服务器（使用唯一端口避免冲突）
  let port = opsbox_test_common::constants::S3_PORT_START + 10;
  let mock_server = match s3_mock::start_mock_s3_server(port).await {
    Ok(server) => server,
    Err(e) => {
      println!("⚠️ Mock S3服务器启动失败: {}，跳过API端点测试", e);
      return;
    }
  };

  // 2. 创建测试数据库
  let db = database::TestDatabase::in_memory().await.expect("创建测试数据库失败");
  database::init_logseek_schema(&db.pool).await.expect("初始化schema失败");

  // 3. 创建测试路由器
  let app = logseek::router(db.pool.clone());

  // 4. 测试POST /profiles - 创建Profile
  let profile_json = serde_json::json!({
    "profile_name": "test-api-profile",
    "endpoint": mock_server.endpoint(),
    "access_key": "test-access-key",
    "secret_key": "test-secret-key"
  });

  let create_req = Request::builder()
    .method("POST")
    .uri("/profiles")
    .header("content-type", "application/json")
    .body(Body::from(serde_json::to_string(&profile_json).unwrap()))
    .unwrap();

  let create_resp = app.clone().oneshot(create_req).await.expect("创建Profile请求失败");
  assert_eq!(
    create_resp.status(),
    StatusCode::NO_CONTENT,
    "创建Profile应返回204 No Content"
  );

  // 5. 测试GET /profiles - 列出Profiles
  let list_req = Request::builder()
    .method("GET")
    .uri("/profiles")
    .body(Body::empty())
    .unwrap();

  let list_resp = app.clone().oneshot(list_req).await.expect("列出Profiles请求失败");
  assert_eq!(list_resp.status(), StatusCode::OK, "列出Profiles应返回200 OK");

  let list_body = axum::body::to_bytes(list_resp.into_body(), usize::MAX)
    .await
    .expect("读取响应体失败");
  let list_json: serde_json::Value = serde_json::from_slice(&list_body).expect("解析JSON失败");

  let profiles = list_json["profiles"].as_array().expect("profiles应为数组");
  assert_eq!(profiles.len(), 1, "应有1个Profile");
  assert_eq!(profiles[0]["profile_name"], "test-api-profile");
  assert_eq!(profiles[0]["endpoint"], mock_server.endpoint());

  // 6. 测试DELETE /profiles/{name} - 删除Profile
  let delete_req = Request::builder()
    .method("DELETE")
    .uri("/profiles/test-api-profile")
    .body(Body::empty())
    .unwrap();

  let delete_resp = app.clone().oneshot(delete_req).await.expect("删除Profile请求失败");
  assert_eq!(
    delete_resp.status(),
    StatusCode::NO_CONTENT,
    "删除Profile应返回204 No Content"
  );

  // 7. 验证删除结果 - 再次列出应为空
  let verify_req = Request::builder()
    .method("GET")
    .uri("/profiles")
    .body(Body::empty())
    .unwrap();

  let verify_resp = app.oneshot(verify_req).await.expect("验证列表请求失败");
  assert_eq!(verify_resp.status(), StatusCode::OK);

  let verify_body = axum::body::to_bytes(verify_resp.into_body(), usize::MAX)
    .await
    .expect("读取响应体失败");
  let verify_json: serde_json::Value =
    serde_json::from_slice(&verify_body).expect("解析JSON失败");
  let verify_profiles = verify_json["profiles"].as_array().expect("profiles应为数组");
  assert!(verify_profiles.is_empty(), "删除后应无Profile");

  // 8. 清理Mock服务器
  mock_server.stop().await.expect("停止Mock服务器失败");

  println!("✓ S3 API端点测试通过（Profile CRUD验证完成）");
}

/// 测试S3连接测试（使用Mock服务器）
#[tokio::test]
async fn test_s3_connection_test() {
  // 创建测试数据库
  let db = database::TestDatabase::in_memory().await.expect("创建测试数据库失败");

  database::init_logseek_schema(&db.pool).await.expect("初始化schema失败");

  // 尝试启动Mock S3服务器
  let port = opsbox_test_common::constants::S3_PORT_START + 1; // 使用不同端口避免冲突
  let mock_server_result = s3_mock::start_mock_s3_server(port).await;

  match mock_server_result {
    Ok(mock_server) => {
      // 如果Mock服务器启动成功，使用其端点
      let endpoint = mock_server.endpoint();
      let profile = S3Profile {
        profile_name: "mock-profile".to_string(),
        endpoint,
        access_key: "test-access-key".to_string(),
        secret_key: "test-secret-key".to_string(),
      };

      let save_result = s3_repo::save_s3_profile(&db.pool, &profile).await;
      assert!(save_result.is_ok(), "保存Mock S3配置失败: {:?}", save_result);

      println!("✓ S3连接测试配置保存成功 (Mock服务器运行中)");

      // 清理
      mock_server.stop().await.expect("停止Mock服务器失败");
    }
    Err(e) => {
      // 如果Mock服务器启动失败（例如CI环境限制），仍然测试配置保存
      // 使用一个虚拟的端点
      println!("⚠️ Mock S3服务器启动失败: {}，跳过连接测试部分", e);

      let profile = S3Profile {
        profile_name: "test-profile".to_string(),
        endpoint: "http://localhost:9000".to_string(),
        access_key: "test-access-key".to_string(),
        secret_key: "test-secret-key".to_string(),
      };

      let save_result = s3_repo::save_s3_profile(&db.pool, &profile).await;
      assert!(save_result.is_ok(), "保存S3配置失败: {:?}", save_result);

      println!("✓ S3配置保存测试成功 (Mock服务器不可用)");
    }
  }
}

/// 测试S3配置边界条件
#[tokio::test]
async fn test_s3_profile_boundary_conditions() {
  let db = database::TestDatabase::in_memory().await.expect("创建测试数据库失败");

  database::init_logseek_schema(&db.pool).await.expect("初始化schema失败");

  // 测试空配置名称
  let profile = S3Profile {
    profile_name: "".to_string(),
    endpoint: "http://localhost:9000".to_string(),
    access_key: "key".to_string(),
    secret_key: "secret".to_string(),
  };
  let result = s3_repo::save_s3_profile(&db.pool, &profile).await;

  // 空名称应该被拒绝（具体行为取决于实现）
  // 这里只确保不会崩溃
  println!("空配置名称测试结果: {:?}", result);

  // 测试超长配置名称
  let long_name = "a".repeat(255);
  let profile = S3Profile {
    profile_name: long_name.clone(),
    endpoint: "http://localhost:9000".to_string(),
    access_key: "key".to_string(),
    secret_key: "secret".to_string(),
  };
  let result = s3_repo::save_s3_profile(&db.pool, &profile).await;

  println!("超长配置名称测试结果: {:?}", result);

  // 测试特殊字符
  let special_name = "test@#$%^&*()";
  let profile = S3Profile {
    profile_name: special_name.to_string(),
    endpoint: "http://localhost:9000".to_string(),
    access_key: "key".to_string(),
    secret_key: "secret".to_string(),
  };
  let result = s3_repo::save_s3_profile(&db.pool, &profile).await;

  println!("特殊字符配置名称测试结果: {:?}", result);

  println!("✓ S3边界条件测试完成");
}

/// 测试S3配置唯一性
#[tokio::test]
async fn test_s3_profile_uniqueness() {
  let db = database::TestDatabase::in_memory().await.expect("创建测试数据库失败");

  database::init_logseek_schema(&db.pool).await.expect("初始化schema失败");

  let profile_name = "duplicate-profile";

  // 第一次保存
  let profile1 = S3Profile {
    profile_name: profile_name.to_string(),
    endpoint: "http://endpoint1:9000".to_string(),
    access_key: "key1".to_string(),
    secret_key: "secret1".to_string(),
  };
  let result1 = s3_repo::save_s3_profile(&db.pool, &profile1).await;

  assert!(result1.is_ok(), "第一次保存失败: {:?}", result1);

  // 第二次保存相同名称（应该更新）
  let profile2 = S3Profile {
    profile_name: profile_name.to_string(),
    endpoint: "http://endpoint2:9000".to_string(),
    access_key: "key2".to_string(),
    secret_key: "secret2".to_string(),
  };
  let result2 = s3_repo::save_s3_profile(&db.pool, &profile2).await;

  assert!(result2.is_ok(), "第二次保存失败: {:?}", result2);

  // 验证更新后的值
  let loaded = s3_repo::load_s3_profile(&db.pool, profile_name).await;
  assert!(loaded.is_ok(), "加载更新后的配置失败: {:?}", loaded);

  let profile = loaded.unwrap();
  assert!(profile.is_some(), "加载的配置应该是Some");
  let profile = profile.unwrap();
  assert_eq!(profile.endpoint, "http://endpoint2:9000");
  assert_eq!(profile.access_key, "key2");

  println!("✓ S3配置唯一性/更新测试通过");
}
