import { apiFetch } from "./client";
import type {
  ContentType,
  Entry,
  Health,
  ListResponse,
  LoginResponse,
  NewContentType,
  PatchContentType,
  SetupStatus,
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
}

export function listEntries(type: string, opts: ListOpts = {}): Promise<ListResponse<Entry>> {
  const q = new URLSearchParams();
  if (opts.page) q.set("page", String(opts.page));
  if (opts.pageSize) q.set("pageSize", String(opts.pageSize));
  if (opts.sort) q.set("sort", opts.sort);
  if (opts.populate) q.set("populate", opts.populate);
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
