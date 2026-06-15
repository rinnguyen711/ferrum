// TS mirrors of the Rust API wire shapes. `kind` strings match the
// serde lowercase rename on `rustapi_core::FieldKind`.

export type ContentTypeKind = "collection" | "single";

export type FieldKind =
  | "string"
  | "text"
  | "integer"
  | "float"
  | "boolean"
  | "datetime"
  | "uuid"
  | "relation"
  | "media"
  | "enum"
  | "json"
  | "email"
  | "url"
  | "slug"
  | "rich_text"
  | "component";

export interface Field {
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  default: unknown;
  max_length?: number;
  kind_meta: Record<string, unknown>;
  _component_fields?: Field[];
}

export interface ContentType {
  id: string;
  name: string;
  display_name: string;
  fields: Field[];
  options?: { draft_publish?: boolean; managed?: boolean; [key: string]: unknown };
  kind: ContentTypeKind;
  created_at: string;
  updated_at: string;
}

export interface NewContentType {
  name: string;
  display_name: string;
  fields: Field[];
  options?: { draft_publish?: boolean };
  kind?: ContentTypeKind;
}

// PATCH /admin/content-types/{name} wire shape — mirrors rustapi_core.
export interface EnumExtension {
  field: string;
  append: string[];
}

export interface PatchContentType {
  display_name?: string;
  add_fields: Field[];
  drop_fields: string[];
  extend_enum_values: EnumExtension[];
  options?: { draft_publish?: boolean };
}

export type Entry = {
  id: string;
  created_at: string;
  updated_at: string;
  published_at?: string | null;
  [field: string]: unknown;
};

export interface ListResponse<T> {
  data: T[];
  meta: { page: number; pageSize: number; total: number };
}

export interface Health {
  status: string;
  version: string;
  db_ms: number;
}

// Relation kind_meta shape (when kind === "relation").
export interface RelationMeta {
  target: string;
  cardinality: string;
  inverse?: string | null;
}

// Enum kind_meta shape (when kind === "enum").
export interface EnumMeta {
  values: string[];
}

export function draftPublishEnabled(ct: ContentType): boolean {
  return ct.options?.draft_publish === true;
}

export function managedType(ct: ContentType): boolean {
  return ct.options?.["managed"] === true;
}

export function relationMeta(f: Field): RelationMeta | null {
  if (f.kind !== "relation") return null;
  const m = f.kind_meta as Partial<RelationMeta>;
  return typeof m.target === "string"
    ? { target: m.target, cardinality: String(m.cardinality), inverse: m.inverse ?? null }
    : null;
}

export function enumValues(f: Field): string[] {
  if (f.kind !== "enum") return [];
  const v = (f.kind_meta as Partial<EnumMeta>).values;
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}

// Media kind_meta shape (when kind === "media").
export interface MediaMeta {
  multiple: boolean;
}

export function mediaMeta(f: Field): MediaMeta | null {
  if (f.kind !== "media") return null;
  const m = f.kind_meta as Partial<MediaMeta>;
  return { multiple: m.multiple === true };
}

// Component kind_meta shape (when kind === "component").
export interface ComponentMeta {
  component: string;
  multiple: boolean;
}

export function componentMeta(f: Field): ComponentMeta | null {
  if (f.kind !== "component") return null;
  const m = f.kind_meta as Partial<ComponentMeta>;
  return typeof m.component === "string"
    ? { component: m.component, multiple: m.multiple === true }
    : null;
}

export interface LoginResponse {
  token: string;
  expires_at: number;
}

export interface SetupStatus {
  setup_required: boolean;
}

export interface User {
  id: string;
  email: string;
  roles: string[];
  confirmed: boolean;
  blocked: boolean;
  created_at: string;
}

export interface NewUser {
  email: string;
  password: string;
  roles: string[];
}

export interface PatchUser {
  email?: string;
  password?: string;
  roles?: string[];
  confirmed?: boolean;
  blocked?: boolean;
}

export interface MediaFolder {
  id: string;
  parent_id: string | null;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface MediaAsset {
  id: string;
  folder_id: string | null;
  file_name: string;
  alt_text: string | null;
  caption: string | null;
  mime_type: string;
  size_bytes: number;
  width: number | null;
  height: number | null;
  original_filename: string;
  created_at: string;
  updated_at: string;
}

export interface NewFolder {
  name: string;
  parent_id?: string | null;
}

export interface PatchFolder {
  name?: string;
  parent_id?: string | null;
}

export interface PatchAsset {
  file_name?: string;
  alt_text?: string;
  caption?: string;
  folder_id?: string | null;
}

export interface MediaProviderField {
  name: string;
  label: string;
  type: string; // "string"
  required: boolean;
  secret: boolean;
}

export interface MediaProviderDescriptor {
  id: string;
  label: string;
  fields: MediaProviderField[];
}

export interface MediaSettings {
  provider: string;
  config: Record<string, string>;
}

export interface Component {
  uid: string;
  display_name: string;
  fields: Field[];
}

export interface NewComponent {
  uid: string;
  display_name: string;
  fields: Field[];
}

export interface UpdateComponent {
  display_name: string;
  fields: Field[];
}

export interface ApiToken {
  id: string;
  name: string;
  description: string;
  scopes: string[];
  expires_at: string | null;
  last_used_at: string | null;
  created_at: string;
}

export interface NewApiToken {
  name: string;
  description?: string;
  scopes: string[];
  expires_at?: string | null;
}

export interface CreatedApiToken extends ApiToken {
  token: string;
}

export interface RolePermission {
  content_type: string;
  action: string;
}

export interface RoleSummary {
  key: string;
  name: string;
  description: string;
  color: string;
  is_system: boolean;
  permission_count: number;
}

export interface Role {
  key: string;
  name: string;
  description: string;
  color: string;
  is_system: boolean;
  permissions: RolePermission[];
}

export interface NewRole {
  key: string;
  name: string;
  description: string;
  color: string;
  permissions: RolePermission[];
}

export interface PatchRole {
  name: string;
  description: string;
  color: string;
  permissions: RolePermission[];
}
