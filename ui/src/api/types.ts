// TS mirrors of the Rust API wire shapes. `kind` strings match the
// serde lowercase rename on `rustapi_core::FieldKind`.

export type FieldKind =
  | "string"
  | "text"
  | "integer"
  | "float"
  | "boolean"
  | "datetime"
  | "uuid"
  | "relation"
  | "enum"
  | "json"
  | "email"
  | "url"
  | "slug";

export interface Field {
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  default: unknown;
  max_length?: number;
  kind_meta: Record<string, unknown>;
}

export interface ContentType {
  id: string;
  name: string;
  display_name: string;
  fields: Field[];
  created_at: string;
  updated_at: string;
}

export type Entry = {
  id: string;
  created_at: string;
  updated_at: string;
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
