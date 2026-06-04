import type {
  ContentType, Field, FieldKind, NewContentType, PatchContentType, EnumExtension,
} from "../api/types";
import { enumValues, relationMeta, mediaMeta } from "../api/types";
import type { IconKey } from "../components/icons";

export const KINDS: FieldKind[] = [
  "string", "text", "integer", "float", "boolean", "datetime",
  "relation", "media", "enum", "json", "email", "url", "slug",
];

/** Picker cards — one per user-addable FieldKind, with a friendly label.
 *  `uuid` is server-managed and intentionally excluded. */
export const FIELD_CARDS: { kind: FieldKind; label: string; desc: string; icon: IconKey }[] = [
  { kind: "string",   label: "Short text",  desc: "Small text like a title or name",         icon: "type" },
  { kind: "text",     label: "Long text",   desc: "Multi-line text or description",          icon: "doc" },
  { kind: "email",    label: "Email",       desc: "An email with built-in validation",       icon: "mail" },
  { kind: "slug",     label: "Slug",        desc: "A URL-friendly identifier",               icon: "hash" },
  { kind: "url",      label: "URL",         desc: "A web address",                           icon: "link" },
  { kind: "integer",  label: "Integer",     desc: "Whole numbers",                           icon: "hash" },
  { kind: "float",    label: "Decimal",     desc: "Decimals and floats",                     icon: "hash" },
  { kind: "boolean",  label: "Boolean",     desc: "A yes-or-no toggle",                      icon: "toggle" },
  { kind: "datetime", label: "Datetime",    desc: "A date, time or date-time",               icon: "calendar" },
  { kind: "enum",     label: "Enumeration", desc: "A list of values to pick from",           icon: "layers" },
  { kind: "relation", label: "Relation",    desc: "Link entries across types",               icon: "relation" },
  { kind: "media",    label: "Media",       desc: "Files — images, video, audio, documents", icon: "image" },
  { kind: "json",     label: "JSON",        desc: "Raw, structured JSON data",               icon: "braces" },
];

/** Friendly label for a kind; falls back to the raw kind string. */
export function fieldLabel(kind: FieldKind): string {
  return FIELD_CARDS.find((c) => c.kind === kind)?.label ?? kind;
}

export type Cardinality = "many_to_one" | "one_to_one" | "many_to_many";

export interface DraftField {
  id: string;                    // crypto.randomUUID() — React key, not sent
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  enumValues: string[];          // kind === "enum"
  target: string;                // kind === "relation"
  inverse: string;               // kind === "relation" (optional)
  cardinality: Cardinality;      // kind === "relation"
  mediaMultiple: boolean;        // kind === "media"
  defaultValue: string;          // raw text; "" → null on the wire
  isPrivate: boolean;            // UI-only — not yet persisted by the server
  origin: "existing" | "new";
}

export interface Draft {
  name: string;
  display_name: string;
  fields: DraftField[];
  mode: "new" | "existing";
  serverSnapshot?: ContentType;  // existing only — diff baseline
}

export function blankField(kind: FieldKind = "string"): DraftField {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind,
    required: false,
    unique: false,
    enumValues: [],
    target: "",
    inverse: "",
    cardinality: "many_to_one",
    mediaMultiple: false,
    defaultValue: "",
    isPrivate: false,
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
      cardinality: (rel?.cardinality as Cardinality) ?? "many_to_one",
      mediaMultiple: mediaMeta(f)?.multiple ?? false,
      defaultValue: defaultToText(f.default),
      isPrivate: false,
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

/** Server `default` (any JSON) → editable text for the input box. */
function defaultToText(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "string") return v;
  return JSON.stringify(v);
}

/** Editable text → wire `default`. Empty → null. Numbers/booleans get
 *  typed for numeric/boolean kinds; everything else stays a string. */
function textToDefault(text: string, kind: FieldKind): unknown {
  const t = text.trim();
  if (t === "") return null;
  if (kind === "boolean") return t === "true";
  if (kind === "integer" || kind === "float") {
    const n = Number(t);
    return Number.isFinite(n) ? n : t;
  }
  if (kind === "json") {
    try { return JSON.parse(t); } catch { return t; }
  }
  return t;
}

function draftFieldToField(d: DraftField): Field {
  let kind_meta: Record<string, unknown> = {};
  if (d.kind === "relation") {
    kind_meta = {
      target: d.target,
      cardinality: d.cardinality,
      ...(d.inverse ? { inverse: d.inverse } : {}),
    };
  } else if (d.kind === "enum") {
    kind_meta = { values: d.enumValues };
  } else if (d.kind === "media") {
    kind_meta = { multiple: d.mediaMultiple };
  }
  return {
    name: d.name,
    kind: d.kind,
    required: d.required,
    unique: d.unique,
    default: textToDefault(d.defaultValue, d.kind),
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
    // A new type is only savable once it has at least one field — the server
    // rejects an empty field list, so name alone is not enough to enable Save.
    return draft.fields.length > 0;
  }
  return !isPatchEmpty(diffToPatch(draft));
}
