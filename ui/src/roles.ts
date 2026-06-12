/** Permission verbs, matching backend PERM_VERBS. */
export const PERM_VERBS = ["find", "findOne", "create", "update", "delete", "publish"] as const;
export type PermVerb = (typeof PERM_VERBS)[number];

/** A content type or plugin pseudo-type shown as a row in the permission matrix. */
export interface PermType {
  key: string; // content_type value sent to the API
  label: string;
  icon: string; // Icons key
  verbs: PermVerb[];
}

/** Plugin pseudo content-types that appear in the matrix alongside content types. */
export const PLUGIN_TYPES: PermType[] = [
  {
    key: "plugin::users",
    label: "Users & Permissions",
    icon: "user",
    verbs: ["find", "findOne", "create", "update", "delete"],
  },
  {
    key: "plugin::upload",
    label: "Media Library",
    icon: "image",
    verbs: ["find", "create", "update", "delete"],
  },
];

/** Color swatches offered when creating a role. */
export const ROLE_COLORS = ["#D14D2B", "#2B6CD1", "#52525B", "#2E8B57", "#8B5CF6", "#D98E04"];

export const DEFAULT_ROLE_COLOR = "#52525B";

/** Verbs available for a regular content type. */
export const CONTENT_VERBS: PermVerb[] = ["find", "findOne", "create", "update", "delete", "publish"];
