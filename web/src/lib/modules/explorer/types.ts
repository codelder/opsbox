export type EndpointType = 'Local' | 'S3' | 'Agent';
export type TargetType = 'Dir' | 'Files' | 'Archive'; // Note: Files usually implied as Dir for container or Archive for specific

export interface Odfi {
  endpoint_type: EndpointType;
  endpoint_id: string; // "localhost", "profile:bucket", "agent_id"
  target_type: TargetType;
  path: string; // root-relative or absolute
  filter_glob?: string;
}

export type ResourceType = 'file' | 'dir' | 'linkfile' | 'linkdir';

export interface ResourceItem {
  name: string;
  path: string; // Full ODFI string for child
  type: ResourceType;
  size?: number | null;
  modified?: number | null;
  has_children?: boolean | null;
}

export interface ResourceListRequest {
  odfi: string; // Serialized ODFI string
}
