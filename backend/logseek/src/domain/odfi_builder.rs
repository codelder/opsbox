use crate::domain::config::{Endpoint, Source, Target};
use opsbox_core::odfi::{EndpointType, Odfi, TargetType, join_root_path, normalize_path_segment};

/// Entry source type (kept for compatibility with existing logic if needed, but mainly for mapping)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntrySourceType {
  #[default]
  File,
  Tar,
  TarGz,
  Gz,
}

/// Build Odfi from Source and relative path
pub fn build_odfi_for_result(source: &Source, rel_path: &str) -> Option<(Odfi, String)> {
  build_odfi_for_result_with_source_type(source, rel_path, EntrySourceType::default())
}

pub fn build_odfi_for_result_with_source_type(
  source: &Source,
  rel_path: &str,
  source_type: EntrySourceType,
) -> Option<(Odfi, String)> {
  build_odfi_for_result_with_source_type_and_archive_path(source, rel_path, source_type, None)
}

/// Build Odfi from Source and relative path, with optional archive path override.
pub fn build_odfi_for_result_with_archive_path(
  source: &Source,
  rel_path: &str,
  archive_path_override: Option<&str>,
) -> Option<(Odfi, String)> {
  build_odfi_for_result_with_source_type_and_archive_path(
    source,
    rel_path,
    EntrySourceType::default(),
    archive_path_override,
  )
}

fn build_odfi_for_result_with_source_type_and_archive_path(
  source: &Source,
  rel_path: &str,
  _source_type: EntrySourceType,
  archive_path_override: Option<&str>,
) -> Option<(Odfi, String)> {
  let (endpoint_type, endpoint_id, base_path) = match &source.endpoint {
    Endpoint::Local { root } => (EndpointType::Local, "localhost".to_string(), root.clone()),
    Endpoint::Agent { agent_id, subpath } => (EndpointType::Agent, agent_id.clone(), subpath.clone()),
    Endpoint::S3 { profile, bucket } => {
      let id = format!("{}:{}", profile, bucket);
      (EndpointType::S3, id, String::new())
    }
  };

  match &source.target {
    Target::Dir { path, .. } => {
      let full_path = if path == "." {
        base_path
      } else {
        join_root_path(&base_path, path)
      };

      let mut final_path = join_root_path(&full_path, rel_path);

      if endpoint_type == EndpointType::Local || endpoint_type == EndpointType::Agent {
        final_path = normalize_path_segment(&final_path);
      }

      let mut odfi = Odfi::new(endpoint_type, endpoint_id, TargetType::Dir, final_path, None);
      if let Some(tuning) = crate::utils::tuning::get()
        && let Some(sid) = &tuning.server_id
      {
        odfi = odfi.with_server_addr(sid);
      }
      Some((odfi.clone(), odfi.to_string()))
    }
    Target::Files { .. } => {
      let mut final_path = join_root_path(&base_path, rel_path);

      if endpoint_type == EndpointType::Local || endpoint_type == EndpointType::Agent {
        final_path = normalize_path_segment(&final_path);
      }

      let mut odfi = Odfi::new(endpoint_type, endpoint_id, TargetType::Dir, final_path, None);
      if let Some(tuning) = crate::utils::tuning::get()
        && let Some(sid) = &tuning.server_id
      {
        odfi = odfi.with_server_addr(sid);
      }
      Some((odfi.clone(), odfi.to_string()))
    }
    Target::Archive { path } => {
      let mut archive_path = if let Some(override_path) = archive_path_override
        && endpoint_type != EndpointType::S3
      {
        override_path.to_string()
      } else if endpoint_type == EndpointType::S3 {
        path.clone()
      } else {
        join_root_path(&base_path, path)
      };

      if endpoint_type == EndpointType::Local || endpoint_type == EndpointType::Agent {
        archive_path = normalize_path_segment(&archive_path);
      }

      let mut odfi = Odfi::new(
        endpoint_type,
        endpoint_id,
        TargetType::Archive,
        archive_path,
        Some(normalize_path_segment(rel_path)),
      );
      if let Some(tuning) = crate::utils::tuning::get()
        && let Some(sid) = &tuning.server_id
      {
        odfi = odfi.with_server_addr(sid);
      }
      Some((odfi.clone(), odfi.to_string()))
    }
  }
}
