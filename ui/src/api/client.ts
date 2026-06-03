import { getToken } from "../auth";

/** Per-field validation message, parsed from `details.fields[]`. */
export interface FieldError {
  field: string;
  message?: string;
}

export class ApiError extends Error {
  status: number;
  code: string;
  fieldErrors: FieldError[];
  constructor(status: number, code: string, message: string, fieldErrors: FieldError[] = []) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.fieldErrors = fieldErrors;
  }
}

/** Thrown on 401. Intercepted by the registered onAuthError handler. */
export class AuthError extends ApiError {
  constructor(message = "unauthorized") {
    super(401, "unauthorized", message);
    this.name = "AuthError";
  }
}

let onAuthError: (() => void) | null = null;

/** Router layer registers a handler that clears the key and redirects. */
export function setAuthErrorHandler(fn: () => void): void {
  onAuthError = fn;
}

interface FetchOpts {
  method?: string;
  body?: unknown;
  /** When set, sends this token instead of the stored one. */
  token?: string;
}

export async function apiFetch<T>(path: string, opts: FetchOpts = {}): Promise<T> {
  const token = opts.token ?? getToken();
  const headers: Record<string, string> = { Accept: "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";

  let resp: Response;
  try {
    resp = await fetch(path, {
      method: opts.method ?? "GET",
      headers,
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    });
  } catch {
    throw new ApiError(0, "network", "Can't reach the API.");
  }

  if (resp.status === 401) {
    const err = new AuthError("Invalid or missing credentials.");
    // Only fire the global handler for stored-token requests, not explicit ones.
    if (opts.token === undefined && onAuthError) onAuthError();
    throw err;
  }

  if (resp.status === 204) return undefined as T;

  let payload: unknown = null;
  const text = await resp.text();
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      payload = null;
    }
  }

  if (!resp.ok) {
    // The server serializes per-field errors as `{field, reason}` (see
    // rustapi_core::FieldValidation). Normalize `reason` into `message` so
    // consumers can read a single field.
    type WireField = { field: string; reason?: string; message?: string };
    const env = (payload as { error?: { code?: string; message?: string; details?: { fields?: WireField[] } } } | null)?.error;
    const code = env?.code ?? "error";
    const message = env?.message ?? `Request failed (${resp.status}).`;
    const fieldErrors: FieldError[] = (env?.details?.fields ?? []).map((f) => ({
      field: f.field,
      message: f.reason ?? f.message,
    }));
    throw new ApiError(resp.status, code, message, fieldErrors);
  }

  return payload as T;
}

/** POST multipart FormData. Browser sets Content-Type (with boundary). */
export async function apiUpload<T>(path: string, form: FormData): Promise<T> {
  const token = getToken();
  const headers: Record<string, string> = { Accept: "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  let resp: Response;
  try {
    resp = await fetch(path, { method: "POST", headers, body: form });
  } catch {
    throw new ApiError(0, "network", "Can't reach the API.");
  }

  if (resp.status === 401) {
    if (onAuthError) onAuthError();
    throw new AuthError("Invalid or missing credentials.");
  }
  if (resp.status === 204) return undefined as T;

  let payload: unknown = null;
  const text = await resp.text();
  if (text) { try { payload = JSON.parse(text); } catch { payload = null; } }

  if (!resp.ok) {
    type WireField = { field: string; reason?: string; message?: string };
    const env = (payload as { error?: { code?: string; message?: string; details?: { fields?: WireField[] } } } | null)?.error;
    const code = env?.code ?? "error";
    const message = env?.message ?? `Request failed (${resp.status}).`;
    const fieldErrors: FieldError[] = (env?.details?.fields ?? []).map((f) => ({
      field: f.field, message: f.reason ?? f.message,
    }));
    throw new ApiError(resp.status, code, message, fieldErrors);
  }
  return payload as T;
}

/** Authed GET returning the raw bytes as a Blob (for thumbnails/preview). */
export async function fetchBlob(path: string): Promise<Blob> {
  const token = getToken();
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  let resp: Response;
  try {
    resp = await fetch(path, { headers });
  } catch {
    throw new ApiError(0, "network", "Can't reach the API.");
  }
  if (resp.status === 401) {
    if (onAuthError) onAuthError();
    throw new AuthError("Invalid or missing credentials.");
  }
  if (!resp.ok) throw new ApiError(resp.status, "error", `Request failed (${resp.status}).`);
  return resp.blob();
}
