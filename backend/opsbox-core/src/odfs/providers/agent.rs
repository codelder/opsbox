use crate::odfs::{OpsEntry, OpsFileSystem, OpsMetadata, OpsPath, OpsRead};
use async_trait::async_trait;
use std::io;
// 假设有一个 AgentClient 能够发送 HTTP 请求
// 这里为了演示，先定义一个简单的 Client trait 依赖，实际项目中应复用 logseek/agent 或 opsbox-core/agent 的客户端
// 由于不能反向依赖 logseek，我们假设 agent-client 逻辑会下沉到 core 或独立 crate
// 这里展示骨架逻辑

pub trait AgentApiClient: Send + Sync {
  // 假设这些方法返回标准 Result
  // 实际实现需要对接 HTTP API
}

#[allow(dead_code)]
pub struct AgentOpsFS {
  agent_id: String,
  base_url: String, // http://<agent-ip>:<port>
  client: reqwest::Client,
}

impl AgentOpsFS {
  pub fn new(agent_id: impl Into<String>, base_url: impl Into<String>) -> Self {
    Self {
      agent_id: agent_id.into(),
      base_url: base_url.into(),
      client: reqwest::Client::new(),
    }
  }

  fn _url(&self, path: &str) -> String {
    format!("{}/api/v1/files{}", self.base_url.trim_end_matches('/'), path)
  }
}

#[async_trait]
impl OpsFileSystem for AgentOpsFS {
  fn name(&self) -> &str {
    "AgentOpsFS"
  }

  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    let _url = format!("{}/metadata?path={}", self.base_url, path.as_str());
    // TODO: 调用 Agent API 获取元数据
    // GET /api/v1/files/metadata?path=/var/log/syslog

    // Mock implementation for skeleton
    Err(io::Error::new(
      io::ErrorKind::Unsupported,
      "AgentOpsFS metadata not implemented yet",
    ))
  }

  async fn read_dir(&self, _path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    // TODO: 调用 Agent API list 目录
    Ok(vec![])
  }

  async fn open_read(&self, _path: &OpsPath) -> io::Result<OpsRead> {
    // GET /api/v1/files/download?path=/var/log/syslog
    /*
    let resp = self.client.get(&self.url("/download"))
        .query(&[("path", path.as_str())])
        .send()
        .await
        .map_err(...)

    let stream = resp.bytes_stream().map_err(...);
    Ok(Box::pin(StreamReader::new(stream)))
    */
    Err(io::Error::new(
      io::ErrorKind::Unsupported,
      "AgentOpsFS open_read not implemented yet",
    ))
  }
}
