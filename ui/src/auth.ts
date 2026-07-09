const TOKEN = "ferrum:token";

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN);
  } catch {
    return null;
  }
}

export function setToken(token: string): void {
  try {
    localStorage.setItem(TOKEN, token);
  } catch {
    // ignore quota / privacy-mode errors
  }
}

export function clearToken(): void {
  try {
    localStorage.removeItem(TOKEN);
  } catch {
    // ignore
  }
}

/** JWT claims we care about for display. Never trusted for authz — the server
 * is authoritative; this only drives UI labels. */
export interface Claims {
  sub: string;
  email: string;
  roles: string[];
}

/** Decode (not verify) the JWT payload for display. Returns null if absent or
 * malformed. */
export function getClaims(): Claims | null {
  const token = getToken();
  if (!token) return null;
  const parts = token.split(".");
  if (parts.length !== 3) return null;
  try {
    const json = atob(parts[1].replace(/-/g, "+").replace(/_/g, "/"));
    const obj = JSON.parse(json) as Partial<Claims>;
    if (typeof obj.email !== "string" || !Array.isArray(obj.roles)) return null;
    return { sub: String(obj.sub ?? ""), email: obj.email, roles: obj.roles };
  } catch {
    return null;
  }
}
