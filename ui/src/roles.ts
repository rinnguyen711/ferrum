/** Mirrors the backend role→permission map (rustapi_core::role_allows). Display
 * only; the server is authoritative. */
export interface Role {
  key: string;
  name: string;
  color: string;
  desc: string;
}

export const ROLES: Role[] = [
  { key: "admin", name: "Admin", color: "#D14D2B", desc: "Full access to content, schema, and users." },
  { key: "editor", name: "Editor", color: "#2B6CD1", desc: "Read and write content entries." },
  { key: "viewer", name: "Viewer", color: "#52525B", desc: "Read-only access to content." },
];

export function roleOf(key: string): Role {
  return ROLES.find((r) => r.key === key) ?? { key, name: key, color: "#52525B", desc: "Unknown role." };
}

/** Capability matrix per role, derived from role_allows for display. Order
 * matches CAPS below. */
export const CAPS = ["Read content", "Write content", "Read schema", "Write schema", "Manage users"];

export function capsFor(key: string): boolean[] {
  switch (key) {
    case "admin":
      return [true, true, true, true, true];
    case "editor":
      return [true, true, false, false, false];
    case "viewer":
      return [true, false, false, false, false];
    default:
      return [false, false, false, false, false];
  }
}
