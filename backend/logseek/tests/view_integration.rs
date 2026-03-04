use axum::extract::State;
use axum::{
  Json, Router,
  body::Body,
  extract::Query,
  routing::{get, post},
};
use logseek::api::models::ViewParams;
use logseek::routes::view::{view_cache_json, view_raw_file};
use opsbox_core::SqlitePool;
use serde_json::json;
use tokio::net::TcpListener;

/// 集成测试中的响应体最大读取大小（1MB）
const TEST_MAX_BODY_SIZE: usize = 1024 * 1024;

// Define common structures locally if needed or use from crate
#[derive(serde::Deserialize)]
struct RawFileParams {
  path: String,
}

/// Spawns a mock agent server and returns its address
async fn spawn_mock_agent() -> (String, u16) {
  let app = Router::new()
    .route(
      "/api/v1/search",
      post(|Json(body): Json<serde_json::Value>| async move {
        // Body is AgentSearchRequest
        let _task_id = body.get("task_id").and_then(|v| v.as_str()).unwrap_or("unknown");

        // Mock response must match SearchEvent serialized format: {"type": "result", "data": {...}}
        let lines = vec!["agent line 1".to_string(), "agent line 2".to_string()];
        let result_data = json!({
            "path": "/var/log/app.log", // Matches expectations or mocked path
            "lines": lines,
            "merged": [[0, 1]], // Assume match on both lines or simplified ranges like (start, end) tuples
            "encoding": "UTF-8"
        });

        let event = json!({
            "type": "result",
            "data": result_data
        });

        let body_str = format!("{}\n", event);

        axum::response::Response::builder()
          .header("content-type", "application/x-ndjson")
          .body(Body::from(body_str))
          .unwrap()
      }),
    )
    .route(
      "/api/v1/file_raw",
      get(|Query(params): Query<RawFileParams>| async move {
        if params.path.ends_with(".png") {
          axum::response::Response::builder()
            .header("content-type", "image/png")
            .body(Body::from(vec![0x89, 0x50, 0x4E, 0x47])) // Fake PNG header
            .unwrap()
        } else {
          axum::response::Response::builder()
            .header("content-type", "text/plain")
            .body(Body::from("raw content"))
            .unwrap()
        }
      }),
    );

  // Bind to random port
  let listener = TcpListener::bind("127.0.0.1:0")
    .await
    .expect("Failed to bind mock agent");
  let addr = listener.local_addr().unwrap();

  tokio::spawn(async move {
    axum::serve(listener, app).await.unwrap();
  });

  (addr.ip().to_string(), addr.port())
}

async fn setup_db_with_agent(pool: &SqlitePool, agent_id: &str, host: &str, port: u16) {
  // Insert agent into database
  // Table schema assumed based on common knowledge of opsbox
  // agents (id TEXT PRIMARY KEY, name TEXT, version TEXT, hostname TEXT, status TEXT, last_heartbeat INTEGER, tags TEXT)

  let tags_json = json!([
      {"key": "host", "value": host},
      {"key": "listen_port", "value": port.to_string()}
  ])
  .to_string();

  let now = chrono::Utc::now().timestamp();

  sqlx::query(
    "INSERT INTO agents (id, name, version, hostname, status, last_heartbeat, tags) VALUES (?, ?, ?, ?, ?, ?, ?)",
  )
  .bind(agent_id)
  .bind("Mock Agent")
  .bind("1.0.0")
  .bind("localhost")
  .bind("Online")
  .bind(now)
  .bind(tags_json)
  .execute(pool)
  .await
  .expect("Failed to insert agent");
}

#[tokio::test]
#[cfg_attr(not(feature = "network-tests"), ignore)]
async fn test_view_cache_json_agent_integration() {
  // 运行时检查：如果网络不可用则跳过
  if !logseek::test_utils::is_network_binding_available() {
    eprintln!("⚠️  跳过测试：网络绑定不可用（沙箱或受限环境）");
    return;
  }

  let (host, port) = spawn_mock_agent().await;
  let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

  // Run migration (usually needed for in-memory db)
  // We assume backend/migrations exists or logseek::lib handles it?
  // logseek::lib::init_db usually does this?
  // But since we are integrating, we might need manual migration or reuse existing testing setup.
  // OpsBox usually has a migrate function.
  // Let's try to run a simple create table if migration fails/not available
  // But better: use sqlx::migrate!(".") if possible.
  // Or copy the schema creation.

  // Create agents table minimal schema with search_roots
  sqlx::query(
    r#"
        DROP TABLE IF EXISTS agents;
        DROP TABLE IF EXISTS agent_tags;
        CREATE TABLE agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            hostname TEXT NOT NULL,
            status TEXT NOT NULL,
            last_heartbeat INTEGER NOT NULL,
            tags TEXT NOT NULL,
            search_roots TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS agent_tags (
            agent_id TEXT NOT NULL,
            tag_key TEXT NOT NULL,
            tag_value TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (agent_id, tag_key)
        );
    "#,
  )
  .execute(&pool)
  .await
  .unwrap();

  let agent_id = "root";
  setup_db_with_agent(&pool, agent_id, &host, port).await;

  // Insert tags manually for now to ensure it works
  sqlx::query("INSERT INTO agent_tags (agent_id, tag_key, tag_value) VALUES (?, ?, ?)")
    .bind(agent_id)
    .bind("host")
    .bind(&host)
    .execute(&pool)
    .await
    .unwrap();

  sqlx::query("INSERT INTO agent_tags (agent_id, tag_key, tag_value) VALUES (?, ?, ?)")
    .bind(agent_id)
    .bind("listen_port")
    .bind(port.to_string())
    .execute(&pool)
    .await
    .unwrap();

  // Call view_cache_json
  // orl://id@type/path
  let file_url = format!("orl://{}@agent/var/log/app.log", agent_id);

  let params = ViewParams {
    sid: "sid-agent-view".to_string(),
    file: file_url,
    start: None,
    end: None,
  };

  let resp = view_cache_json(State(pool.clone()), Query(params)).await.unwrap();

  // Verify response
  assert_eq!(resp.status(), axum::http::StatusCode::OK);

  let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

  // Mock returns "agent line 1", "agent line 2"
  assert_eq!(json["total"], 2);
  assert_eq!(json["lines"][0]["text"], "agent line 1");
  assert_eq!(json["lines"][1]["text"], "agent line 2");
}

#[tokio::test]
#[cfg_attr(not(feature = "network-tests"), ignore)]
async fn test_view_raw_file_agent_integration() {
  // 运行时检查：如果网络不可用则跳过
  if !logseek::test_utils::is_network_binding_available() {
    eprintln!("⚠️  跳过测试：网络绑定不可用（沙箱或受限环境）");
    return;
  }

  let (host, port) = spawn_mock_agent().await;
  let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

  // Create agents table minimal schema with search_roots
  sqlx::query(
    r#"
        DROP TABLE IF EXISTS agents;
        DROP TABLE IF EXISTS agent_tags;
        CREATE TABLE agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            hostname TEXT NOT NULL,
            status TEXT NOT NULL,
            last_heartbeat INTEGER NOT NULL,
            tags TEXT NOT NULL,
            search_roots TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS agent_tags (
            agent_id TEXT NOT NULL,
            tag_key TEXT NOT NULL,
            tag_value TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (agent_id, tag_key)
        );
    "#,
  )
  .execute(&pool)
  .await
  .unwrap();

  let agent_id = "root";
  // Modified setup_db_with_agent needed? Or just insert tags manually here since setup_db_with_agent is shared
  // Let's modify setup_db_with_agent to insert tags into table locally here or update the function?
  // I'll update the function setup_db_with_agent in a separate edit or assume I update it.
  // For now, I'll allow setup_db_with_agent to run (it inserts into agents) and then I insert into agent_tags manually.
  setup_db_with_agent(&pool, agent_id, &host, port).await;

  // Insert tags manually for now to ensure it works
  sqlx::query("INSERT INTO agent_tags (agent_id, tag_key, tag_value) VALUES (?, ?, ?)")
    .bind(agent_id)
    .bind("host")
    .bind(&host)
    .execute(&pool)
    .await
    .unwrap();

  sqlx::query("INSERT INTO agent_tags (agent_id, tag_key, tag_value) VALUES (?, ?, ?)")
    .bind(agent_id)
    .bind("listen_port")
    .bind(port.to_string())
    .execute(&pool)
    .await
    .unwrap();

  // orl://id@type/path
  let file_url = format!("orl://{}@agent/var/img/icon.png", agent_id);
  let params = ViewParams {
    sid: "sid-agent-raw".to_string(),
    file: file_url,
    start: None,
    end: None,
  };

  let resp = view_raw_file(State(pool.clone()), Query(params)).await.unwrap();

  assert_eq!(resp.status(), axum::http::StatusCode::OK);
  assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");

  let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
  assert_eq!(body_bytes, vec![0x89, 0x50, 0x4E, 0x47]);
}
