import type { ContentType, Entry, Field } from "./api/types";

/** Human "x ago" rendering of an rfc3339 timestamp. */
export function relTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const now = new Date();
  const mins = Math.round((now.getTime() - d.getTime()) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return mins + "m ago";
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return hrs + "h ago";
  const days = Math.round(hrs / 24);
  return days + "d ago";
}

/** Short uuid for compact display (first segment). */
export function shortId(id: string): string {
  return id.split("-")[0] ?? id;
}

/**
 * Pick a human label for a related entry. Heuristic: the target schema's
 * first string/text field value; fall back to a short uuid. The related
 * value may be a populated object (preferred) or a raw uuid string.
 */
export function relationLabel(
  value: unknown,
  targetSchema: ContentType | undefined,
): string {
  if (value == null) return "—";
  if (typeof value === "string") return shortId(value); // un-populated FK
  if (typeof value === "object") {
    const obj = value as Entry;
    const labelField = targetSchema?.fields.find(
      (f: Field) => f.kind === "string" || f.kind === "text",
    );
    if (labelField) {
      const v = obj[labelField.name];
      if (typeof v === "string" && v.length > 0) return v;
    }
    if (typeof obj.id === "string") return shortId(obj.id);
  }
  return "—";
}
