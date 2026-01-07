use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
  File,
  Dir,
  LinkFile,
  LinkDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceItem {
  pub name: String,
  pub path: String, // Full ODFI path or relative path? ODFI path seems better for frontend.
  pub r#type: ResourceType,
  pub size: Option<u64>,
  pub modified: Option<i64>, // Unix timestamp
  pub has_children: Option<bool>,
  pub child_count: Option<u64>,
  pub hidden_child_count: Option<u64>,
  pub mime_type: Option<String>,
}
