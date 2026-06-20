import { apiFetch } from "./client";

export interface Locale {
  code: string;
  name: string;
  is_default: boolean;
  position: number;
}

export interface UpsertLocaleBody {
  code: string;
  name: string;
  position?: number;
  is_default?: boolean;
}

/** GET /admin/locales → { data: Locale[] }. */
export function listLocales(): Promise<Locale[]> {
  return apiFetch<{ data: Locale[] }>("/admin/locales").then((r) => r.data);
}

export function upsertLocale(body: UpsertLocaleBody): Promise<Locale> {
  return apiFetch<Locale>("/admin/locales", { method: "POST", body });
}

export function deleteLocale(code: string): Promise<void> {
  return apiFetch<void>(`/admin/locales/${encodeURIComponent(code)}`, { method: "DELETE" });
}
