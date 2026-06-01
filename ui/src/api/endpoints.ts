import { apiFetch } from "./client";
import type { ContentType, Entry, Health, ListResponse, NewContentType } from "./types";

export function listContentTypes(): Promise<ContentType[]> {
  return apiFetch<ContentType[]>("/admin/content-types");
}

export function getContentType(name: string): Promise<ContentType> {
  return apiFetch<ContentType>(`/admin/content-types/${encodeURIComponent(name)}`);
}

export function createContentType(body: NewContentType): Promise<ContentType> {
  return apiFetch<ContentType>("/admin/content-types", { method: "POST", body });
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

/** Probe the gated content-types route with a candidate key. */
export async function checkAuth(key: string): Promise<boolean> {
  try {
    await apiFetch<ContentType[]>("/admin/content-types", { key });
    return true;
  } catch (e) {
    if (e instanceof Error && e.name === "AuthError") return false;
    throw e; // network / 5xx — let caller show "can't reach API"
  }
}
