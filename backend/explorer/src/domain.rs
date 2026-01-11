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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_serialization() {
        assert_eq!(serde_json::to_string(&ResourceType::File).unwrap(), "\"file\"");
        assert_eq!(serde_json::to_string(&ResourceType::Dir).unwrap(), "\"dir\"");
        assert_eq!(serde_json::to_string(&ResourceType::LinkFile).unwrap(), "\"linkfile\"");
        assert_eq!(serde_json::to_string(&ResourceType::LinkDir).unwrap(), "\"linkdir\"");
    }

    #[test]
    fn test_resource_type_deserialization() {
        assert_eq!(serde_json::from_str::<ResourceType>("\"file\"").unwrap(), ResourceType::File);
        assert_eq!(serde_json::from_str::<ResourceType>("\"dir\"").unwrap(), ResourceType::Dir);
        assert_eq!(serde_json::from_str::<ResourceType>("\"linkfile\"").unwrap(), ResourceType::LinkFile);
        assert_eq!(serde_json::from_str::<ResourceType>("\"linkdir\"").unwrap(), ResourceType::LinkDir);
    }

    #[test]
    fn test_resource_item_serialization() {
        let item = ResourceItem {
            name: "test.log".to_string(),
            path: "/var/log/test.log".to_string(),
            r#type: ResourceType::File,
            size: Some(1024),
            modified: Some(1234567890),
            has_children: None,
            child_count: None,
            hidden_child_count: None,
            mime_type: Some("text/plain".to_string()),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("test.log"));
        assert!(json.contains("\"type\":\"file\""));
        assert!(json.contains("1024"));

        let deserialized: ResourceItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test.log");
        assert_eq!(deserialized.r#type, ResourceType::File);
        assert_eq!(deserialized.size, Some(1024));
    }

    #[test]
    fn test_resource_item_directory() {
        let item = ResourceItem {
            name: "logs".to_string(),
            path: "/var/logs".to_string(),
            r#type: ResourceType::Dir,
            size: None,
            modified: Some(1234567890),
            has_children: Some(true),
            child_count: Some(10),
            hidden_child_count: Some(2),
            mime_type: None,
        };

        assert_eq!(item.r#type, ResourceType::Dir);
        assert_eq!(item.has_children, Some(true));
        assert_eq!(item.child_count, Some(10));
    }

    #[test]
    fn test_resource_type_equality() {
        assert_eq!(ResourceType::File, ResourceType::File);
        assert_ne!(ResourceType::File, ResourceType::Dir);
        assert_ne!(ResourceType::LinkFile, ResourceType::File);
    }
}
