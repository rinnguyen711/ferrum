import { apiFetch, apiUpload, fetchBlob } from "./client";
import type {
  ApiToken,
  Component,
  ContentType,
  CreatedApiToken,
  Entry,
  Health,
  ListResponse,
  LoginResponse,
  MediaAsset,
  MediaFolder,
  MediaProviderDescriptor,
  MediaSettings,
  NewApiToken,
  NewComponent,
  NewContentType,
  NewFolder,
  NewRole,
  NewUser,
  PatchAsset,
  PatchContentType,
  PatchFolder,
  PatchRole,
  PatchUser,
  Role,
  RoleSummary,
  SetupStatus,
  UpdateComponent,
  User,
} from "./types";

export function listContentTypes(): Promise<ContentType[]> {
  return apiFetch<ContentType[]>("/admin/content-types");
}

export function getContentType(name: string): Promise<ContentType> {
  return apiFetch<ContentType>(`/admin/content-types/${encodeURIComponent(name)}`);
}

export function createContentType(body: NewContentType): Promise<ContentType> {
  return apiFetch<ContentType>("/admin/content-types", { method: "POST", body });
}

export function patchContentType(
  name: string,
  body: PatchContentType,
): Promise<ContentType> {
  return apiFetch<ContentType>(
    `/admin/content-types/${encodeURIComponent(name)}`,
    { method: "PATCH", body },
  );
}

export function deleteContentType(name: string): Promise<void> {
  return apiFetch<void>(
    `/admin/content-types/${encodeURIComponent(name)}?confirm=true`,
    { method: "DELETE" },
  );
}

interface ListOpts {
  page?: number;
  pageSize?: number;
  sort?: string;
  populate?: string;
  status?: "published" | "draft" | "all";
  /** Pre-built `filters[field][$op]` → value pairs (see FiltersMenu.serializeFilters). */
  filters?: [string, string][];
}

export function listEntries(type: string, opts: ListOpts = {}): Promise<ListResponse<Entry>> {
  const q = new URLSearchParams();
  if (opts.page) q.set("page", String(opts.page));
  if (opts.pageSize) q.set("pageSize", String(opts.pageSize));
  if (opts.sort) q.set("sort", opts.sort);
  if (opts.populate) q.set("populate", opts.populate);
  if (opts.status) q.set("status", opts.status);
  for (const [k, v] of opts.filters ?? []) q.append(k, v);
  const qs = q.toString();
  return apiFetch<ListResponse<Entry>>(`/api/${encodeURIComponent(type)}${qs ? `?${qs}` : ""}`);
}

export function getEntry(type: string, id: string, opts: { populate?: string } = {}): Promise<Entry> {
  const qs = opts.populate ? `?populate=${encodeURIComponent(opts.populate)}` : "";
  return apiFetch<Entry>(`/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}${qs}`);
}

export function createEntry(type: string, body: Record<string, unknown>): Promise<Entry> {
  return apiFetch<Entry>(`/api/${encodeURIComponent(type)}`, { method: "POST", body });
}

export function updateEntry(type: string, id: string, body: Record<string, unknown>): Promise<Entry> {
  return apiFetch<Entry>(`/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}`, {
    method: "PUT",
    body,
  });
}

export function deleteEntry(type: string, id: string): Promise<void> {
  return apiFetch<void>(`/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export function publishEntry(type: string, id: string): Promise<Entry> {
  return apiFetch<Entry>(
    `/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}/publish`,
    { method: "POST" },
  );
}

export function unpublishEntry(type: string, id: string): Promise<Entry> {
  return apiFetch<Entry>(
    `/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}/unpublish`,
    { method: "POST" },
  );
}

export function getHealth(): Promise<Health> {
  return apiFetch<Health>("/healthz");
}

/** First-run check: does the system still need an admin created? */
export function fetchSetupStatus(): Promise<SetupStatus> {
  return apiFetch<SetupStatus>("/auth/setup");
}

/** Create the first admin. Only succeeds on an empty system (else 409). */
export function setup(email: string, password: string): Promise<void> {
  return apiFetch<void>("/auth/setup", { method: "POST", body: { email, password } });
}

/** Exchange credentials for a JWT. */
export function login(email: string, password: string): Promise<LoginResponse> {
  return apiFetch<LoginResponse>("/auth/login", { method: "POST", body: { email, password } });
}

export function listUsers(): Promise<User[]> {
  return apiFetch<User[]>("/admin/users");
}

export function createUser(body: NewUser): Promise<User> {
  return apiFetch<User>("/admin/users", { method: "POST", body });
}

export function updateUser(id: string, body: PatchUser): Promise<User> {
  return apiFetch<User>(`/admin/users/${encodeURIComponent(id)}`, { method: "PATCH", body });
}

export function deleteUser(id: string): Promise<void> {
  return apiFetch<void>(`/admin/users/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export function listRoles(): Promise<RoleSummary[]> {
  return apiFetch<RoleSummary[]>("/admin/roles");
}

export function getRole(key: string): Promise<Role> {
  return apiFetch<Role>(`/admin/roles/${encodeURIComponent(key)}`);
}

export function createRole(body: NewRole): Promise<Role> {
  return apiFetch<Role>("/admin/roles", { method: "POST", body });
}

export function updateRole(key: string, body: PatchRole): Promise<Role> {
  return apiFetch<Role>(`/admin/roles/${encodeURIComponent(key)}`, { method: "PUT", body });
}

export function deleteRole(key: string): Promise<void> {
  return apiFetch<void>(`/admin/roles/${encodeURIComponent(key)}`, { method: "DELETE" });
}

export function listFolders(opts: { parentId?: string | null; all?: boolean } = {}): Promise<MediaFolder[]> {
  if (opts.all) return apiFetch<MediaFolder[]>("/admin/media/folders?scope=all");
  const q = opts.parentId != null ? `?parent_id=${encodeURIComponent(opts.parentId)}` : "";
  return apiFetch<MediaFolder[]>(`/admin/media/folders${q}`);
}

export function createFolder(body: NewFolder): Promise<MediaFolder> {
  return apiFetch<MediaFolder>("/admin/media/folders", { method: "POST", body });
}

export function updateFolder(id: string, body: PatchFolder): Promise<MediaFolder> {
  return apiFetch<MediaFolder>(`/admin/media/folders/${id}`, { method: "PATCH", body });
}

export function deleteFolder(id: string): Promise<void> {
  return apiFetch<void>(`/admin/media/folders/${id}`, { method: "DELETE" });
}

export function listAssets(folderId?: string | null): Promise<MediaAsset[]> {
  const q = folderId != null ? `?folder_id=${encodeURIComponent(folderId)}` : "";
  return apiFetch<MediaAsset[]>(`/admin/media/assets${q}`);
}

export function getAsset(id: string): Promise<MediaAsset> {
  return apiFetch<MediaAsset>(`/admin/media/assets/${id}`);
}

export function updateAsset(id: string, body: PatchAsset): Promise<MediaAsset> {
  return apiFetch<MediaAsset>(`/admin/media/assets/${id}`, { method: "PATCH", body });
}

export function deleteAsset(id: string): Promise<void> {
  return apiFetch<void>(`/admin/media/assets/${id}`, { method: "DELETE" });
}

export function uploadAsset(file: File, folderId?: string | null): Promise<MediaAsset> {
  const form = new FormData();
  form.append("file", file);
  if (folderId != null) form.append("folder_id", folderId);
  return apiUpload<MediaAsset>("/admin/media/assets", form);
}

export function fetchAssetBlob(id: string): Promise<Blob> {
  return fetchBlob(`/admin/media/assets/${id}/raw`);
}

export function listMediaProviders(): Promise<MediaProviderDescriptor[]> {
  return apiFetch<MediaProviderDescriptor[]>("/admin/media/providers");
}

export function getMediaSettings(): Promise<MediaSettings | null> {
  return apiFetch<MediaSettings | null>("/admin/media/settings");
}

export function putMediaSettings(body: MediaSettings): Promise<void> {
  return apiFetch<void>("/admin/media/settings", { method: "PUT", body });
}

export function testMediaSettings(body: MediaSettings): Promise<void> {
  return apiFetch<void>("/admin/media/settings/test", { method: "POST", body });
}

export function listComponents(): Promise<Component[]> {
  return apiFetch<Component[]>("/admin/components");
}
export function getComponent(uid: string): Promise<Component> {
  return apiFetch<Component>(`/admin/components/${encodeURIComponent(uid)}`);
}
export function createComponent(body: NewComponent): Promise<Component> {
  return apiFetch<Component>("/admin/components", { method: "POST", body });
}
export function updateComponent(uid: string, body: UpdateComponent): Promise<Component> {
  return apiFetch<Component>(`/admin/components/${encodeURIComponent(uid)}`, { method: "PUT", body });
}
export function deleteComponent(uid: string): Promise<void> {
  return apiFetch<void>(`/admin/components/${encodeURIComponent(uid)}?confirm=true`, { method: "DELETE" });
}

export function getSingleType(name: string): Promise<Entry | null> {
  return apiFetch<Entry | null>(`/api/single-types/${encodeURIComponent(name)}`);
}

export function putSingleType(name: string, body: Record<string, unknown>): Promise<Entry> {
  return apiFetch<Entry>(`/api/single-types/${encodeURIComponent(name)}`, {
    method: "PUT",
    body,
  });
}

export function listApiTokens(): Promise<ApiToken[]> {
  return apiFetch<ApiToken[]>("/api/admin/tokens");
}

export function createApiToken(body: NewApiToken): Promise<CreatedApiToken> {
  return apiFetch<CreatedApiToken>("/api/admin/tokens", { method: "POST", body });
}

export function updateApiToken(id: string, body: NewApiToken): Promise<ApiToken> {
  return apiFetch<ApiToken>(`/api/admin/tokens/${encodeURIComponent(id)}`, { method: "PATCH", body });
}

export function revokeApiToken(id: string): Promise<void> {
  return apiFetch<void>(`/api/admin/tokens/${encodeURIComponent(id)}`, { method: "DELETE" });
}
