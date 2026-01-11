export type EndpointType = 'Local' | 'S3' | 'Agent';
export type TargetType = 'Dir' | 'Files' | 'Archive';

export interface Orl {
  endpoint_type: EndpointType;
  endpoint_id: string; // "localhost", "profile:bucket", "agent_id"
  target_type: TargetType;
  path: string; // root-relative or absolute
  filter_glob?: string;
}

export type ResourceType = 'file' | 'dir' | 'linkfile' | 'linkdir';

export interface ResourceItem {
  name: string;
  path: string; // Full ORL string for child
  type: ResourceType;
  size?: number | null;
  modified?: number | null;
  has_children?: boolean | null;
  child_count?: number | null;
  hidden_child_count?: number | null;
  mime_type?: string | null;
}

export interface ResourceListRequest {
  orl: string; // Serialized ORL string
}
