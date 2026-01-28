//! S3 Mock服务器
//!
//! 提供模拟S3服务器的工具，用于集成测试
//!
//! 注意：当前有一个完整的TypeScript实现位于web/tests/e2e/s3_archive.spec.ts
//! 这个Rust版本用于后端集成测试

use axum::{Router, routing::get, response::Response};
use http::{StatusCode, header};
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use crate::TestError;

/// S3对象元数据
#[derive(Serialize, Clone)]
pub struct S3Object {
    /// 对象键
    pub key: String,
    /// 最后修改时间
    pub last_modified: String,
    /// ETag
    pub etag: String,
    /// 对象大小（字节）
    pub size: u64,
    /// 存储类型
    pub storage_class: String,
}

/// S3模拟服务器配置
#[derive(Clone)]
pub struct MockS3Config {
    /// 服务器监听地址
    pub address: String,
    /// 服务器监听端口
    pub port: u16,
    /// 存储桶名称
    pub bucket: String,
    /// 模拟的对象列表
    pub objects: Vec<S3Object>,
}

impl Default for MockS3Config {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".to_string(),
            port: crate::constants::S3_PORT_START,
            bucket: "test-bucket".to_string(),
            objects: vec![
                S3Object {
                    key: "logs/app1.log".to_string(),
                    last_modified: "2024-01-01T00:00:00.000Z".to_string(),
                    etag: "\"deadbeef\"".to_string(),
                    size: 1024,
                    storage_class: "STANDARD".to_string(),
                },
                S3Object {
                    key: "logs/app2.log".to_string(),
                    last_modified: "2024-01-02T00:00:00.000Z".to_string(),
                    etag: "\"cafebabe\"".to_string(),
                    size: 2048,
                    storage_class: "STANDARD".to_string(),
                },
            ],
        }
    }
}

/// S3模拟服务器实例
pub struct MockS3Server {
    /// 服务器任务句柄
    pub task: JoinHandle<()>,
    /// 服务器地址
    pub address: SocketAddr,
    /// 服务器配置
    pub config: MockS3Config,
}

impl MockS3Server {
    /// 启动模拟S3服务器
    pub async fn start(config: MockS3Config) -> Result<Self, TestError> {
        let address = format!("{}:{}", config.address, config.port);

        // 创建简单的S3模拟服务器
        // 注意：这是一个简化版本，完整的S3 API模拟在前端测试中实现
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/xml")
                        .body(axum::body::Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner>
    <ID>test-owner</ID>
    <DisplayName>Test Owner</DisplayName>
  </Owner>
  <Buckets>
    <Bucket>
      <Name>test-bucket</Name>
      <CreationDate>2024-01-01T00:00:00.000Z</CreationDate>
    </Bucket>
  </Buckets>
</ListAllMyBucketsResult>"#))
                        .unwrap()
                }),
            );

        let listener = TcpListener::bind(&address).await
            .map_err(|e| TestError::Network(format!("绑定端口失败: {}", e)))?;

        let bound_address = listener.local_addr()
            .map_err(|e| TestError::Network(format!("获取本地地址失败: {}", e)))?;

        let task = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .expect("模拟S3服务器启动失败");
        });

        // 等待服务器启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(Self {
            task,
            address: bound_address,
            config,
        })
    }

    /// 获取服务器基础URL
    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// 获取服务器端点（用于AWS SDK配置）
    pub fn endpoint(&self) -> String {
        self.base_url()
    }

    /// 停止服务器
    pub async fn stop(self) -> Result<(), TestError> {
        self.task.abort();

        // 等待任务完成
        match self.task.await {
            Ok(_) => Ok(()),
            Err(e) if e.is_cancelled() => Ok(()),
            Err(e) => Err(TestError::Other(format!("停止服务器失败: {}", e))),
        }
    }
}

/// 启动模拟S3服务器（简化版本）
pub async fn start_mock_s3_server(port: u16) -> Result<MockS3Server, TestError> {
    let config = MockS3Config {
        port,
        ..Default::default()
    };

    MockS3Server::start(config).await
}

/// 生成S3 ListObjectsV2响应XML
pub fn generate_list_objects_v2_xml(
    bucket: &str,
    objects: &[S3Object],
    prefix: Option<&str>,
    max_keys: Option<usize>,
    is_truncated: bool,
) -> String {
    let prefix = prefix.unwrap_or("");
    let max_keys = max_keys.unwrap_or(1000);
    let key_count = objects.len().min(max_keys);

    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>{}</Name>
  <Prefix>{}</Prefix>
  <KeyCount>{}</KeyCount>
  <MaxKeys>{}</MaxKeys>
  <IsTruncated>{}</IsTruncated>"#,
        bucket,
        prefix,
        key_count,
        max_keys,
        is_truncated
    );

    for object in objects.iter().take(max_keys) {
        xml.push_str(&format!(
            r#"
  <Contents>
    <Key>{}</Key>
    <LastModified>{}</LastModified>
    <ETag>{}</ETag>
    <Size>{}</Size>
    <StorageClass>{}</StorageClass>
  </Contents>"#,
            object.key, object.last_modified, object.etag, object.size, object.storage_class
        ));
    }

    xml.push_str("\n</ListBucketResult>");
    xml
}