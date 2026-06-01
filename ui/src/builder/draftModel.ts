import type {
  ContentType, Field, FieldKind, NewContentType, PatchContentType, EnumExtension,
} from "../api/types";
import { enumValues, relationMeta } from "../api/types";

export const KINDS: FieldKind[] = [
  "string", "text", "integer", "float", "boolean", "datetime",
  "relation", "enum", "json", "email", "url", "slug",
];

export interface DraftField {
  id: string;                    // crypto.randomUUID() — React key, not sent
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  enumValues: string[];          // kind === "enum"
  target: string;                // kind === "relation"
  inverse: string;               // kind === "relation" (optional)
  origin: "existing" | "new";
}

export interface Draft {
  name: string;
  display_name: string;
  fields: DraftField[];
  mode: "new" | "existing";
  serverSnapshot?: ContentType;  // existing only — diff baseline
}

export function blankField(): DraftField {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind: "string",
    required: false,
    unique: false,
    enumValues: [],
    target: "",
    inverse: "",
    origin: "new",
  };
}

/** "Blog Post" -> "blog_post". Loose mirror of server ^[a-z][a-z0-9_]{0,62}$. */
export function deriveApiId(display: string): string {
  let s = display
    .toLowerCase()
    .trim()
    .replace(/[\s-]+/g, "_")
    .replace(/[^a-z0-9_]/g, "")
    .replace(/_+/g, "_")
    .replace(/^_+/, "");
  s = s.replace(/^[0-9]+/, "");
  return s.slice(0, 63);
}

export function newDraft(name: string, display_name: string): Draft {
  return { name, display_name, fields: [], mode: "new" };
}

export function seedFromContentType(ct: ContentType): Draft {
  const fields: DraftField[] = ct.fields.map((f) => {
    const rel = relationMeta(f);
    return {
      id: crypto.randomUUID(),
      name: f.name,
      kind: f.kind,
      required: f.required,
      unique: f.unique,
      enumValues: enumValues(f),
      target: rel?.target ?? "",
      inverse: rel?.inverse ?? "",
      origin: "existing",
    };
  });
  return {
    name: ct.name,
    display_name: ct.display_name,
    fields,
    mode: "existing",
    serverSnapshot: ct,
  };
}

function draftFieldToField(d: DraftField): Field {
  let kind_meta: Record<string, unknown> = {};
  if (d.kind === "relation") {
    kind_meta = {
      target: d.target,
      cardinality: "many_to_one",
      ...(d.inverse ? { inverse: d.inverse } : {}),
    };
  } else if (d.kind === "enum") {
    kind_meta = { values: d.enumValues };
  }
  return {
    name: d.name,
    kind: d.kind,
    required: d.required,
    unique: d.unique,
    default: null,
    kind_meta,
  };
}

export function toNewContentType(draft: Draft): NewContentType {
  return {
    name: draft.name,
    display_name: draft.display_name,
    fields: draft.fields.map(draftFieldToField),
  };
}

export function diffToPatch(draft: Draft): PatchContentType {
  const snap = draft.serverSnapshot;
  const patch: PatchContentType = {
    add_fields: [],
    drop_fields: [],
    extend_enum_values: [],
  };
  if (!snap) return patch;

  if (draft.display_name !== snap.display_name) {
    patch.display_name = draft.display_name;
  }

  for (const d of draft.fields) {
    if (d.origin === "new") patch.add_fields.push(draftFieldToField(d));
  }

  const draftNames = new Set(draft.fields.map((d) => d.name));
  for (const f of snap.fields) {
    if (!draftNames.has(f.name)) patch.drop_fields.push(f.name);
  }

  for (const d of draft.fields) {
    if (d.origin !== "existing" || d.kind !== "enum") continue;
    const orig = snap.fields.find((f) => f.name === d.name);
    if (!orig) continue;
    const before = new Set(enumValues(orig));
    const append = d.enumValues.filter((v) => !before.has(v));
    if (append.length) patch.extend_enum_values.push({ field: d.name, append });
  }

  return patch;
}

export function isPatchEmpty(p: PatchContentType): boolean {
  return (
    p.display_name === undefined &&
    p.add_fields.length === 0 &&
    p.drop_fields.length === 0 &&
    p.extend_enum_values.length === 0
  );
}

export function isDirty(draft: Draft | null): boolean {
  if (!draft) return false;
  if (draft.mode === "new") {
    return draft.name.trim() !== "" || draft.fields.length > 0;
  }
  return !isPatchEmpty(diffToPatch(draft));
}
