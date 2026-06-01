// Rustapi sample content — ported from design/rustapi/data.js.
// "Aurora Journal" — a publishing CMS demo.

export type Status = "published" | "draft" | "review";

export interface Author {
  id: number;
  name: string;
  role: string;
  avatar: string;
  color: string;
  bio: string;
}

export interface Category {
  id: number;
  name: string;
  slug: string;
  color: string;
  count: number;
}

export interface Article {
  id: number;
  title: string;
  slug: string;
  status: Status;
  author: number;
  categories: number[];
  featured: boolean;
  readTime: number;
  updatedAt: string;
  publishedAt: string | null;
  excerpt: string;
  locale: string;
}

export interface FieldDef {
  name: string;
  type: string;
  required: boolean;
  meta: string;
}

export interface ContentType {
  key: string;
  kind: "collection";
  display: string;
  plural: string;
  icon: string;
  fields: FieldDef[];
}

export interface SingleType {
  key: string;
  display: string;
  icon: string;
}

export interface MediaItem {
  id: number;
  name: string;
  w: number;
  h: number;
  size: string;
  hue: number;
  ext: string;
}

export const authors: Author[] = [
  { id: 1, name: "Mara Velez", role: "Editor in chief", avatar: "MV", color: "#C2410C", bio: "Runs the desk. Twelve years in long-form science journalism." },
  { id: 2, name: "Idris Bello", role: "Staff writer", avatar: "IB", color: "#0E7490", bio: "Covers climate, energy, and the people in between." },
  { id: 3, name: "Saoirse Lynch", role: "Contributor", avatar: "SL", color: "#7C3AED", bio: "Essayist. Writes about cities, memory, and maps." },
  { id: 4, name: "Tomas Reier", role: "Photo editor", avatar: "TR", color: "#15803D", bio: "Pictures first, words later." },
];

export const categories: Category[] = [
  { id: 1, name: "Science", slug: "science", color: "#0E7490", count: 18 },
  { id: 2, name: "Climate", slug: "climate", color: "#15803D", count: 24 },
  { id: 3, name: "Culture", slug: "culture", color: "#7C3AED", count: 31 },
  { id: 4, name: "Cities", slug: "cities", color: "#C2410C", count: 12 },
  { id: 5, name: "Interviews", slug: "interviews", color: "#B45309", count: 9 },
];

export const articles: Article[] = [
  { id: 11, title: "The quiet reinvention of the tidal turbine", slug: "tidal-turbine-reinvention", status: "published", author: 2, categories: [2, 1], featured: true, readTime: 9, updatedAt: "2026-05-29T14:22:00", publishedAt: "2026-05-28T09:00:00", excerpt: "A new generation of low-speed rotors is making estuary power viable for the first time.", locale: "en" },
  { id: 12, title: "What a city remembers when its river is gone", slug: "city-remembers-river", status: "published", author: 3, categories: [4, 3], featured: false, readTime: 14, updatedAt: "2026-05-30T11:05:00", publishedAt: "2026-05-27T07:30:00", excerpt: "Walking the buried waterways of four cities that paved over their founding streams.", locale: "en" },
  { id: 13, title: "The lab growing coral in the dark", slug: "coral-in-the-dark", status: "draft", author: 2, categories: [1, 2], featured: false, readTime: 7, updatedAt: "2026-05-31T08:41:00", publishedAt: null, excerpt: "Inside a basement aquarium where bleaching has been reversed — for now.", locale: "en" },
  { id: 14, title: "Forty years of the same weather diary", slug: "weather-diary-forty-years", status: "published", author: 1, categories: [2], featured: false, readTime: 11, updatedAt: "2026-05-26T16:18:00", publishedAt: "2026-05-24T06:00:00", excerpt: "A retired postmaster recorded the sky every morning. The data turned out to matter.", locale: "en" },
  { id: 15, title: "The mapmakers who refuse to draw borders", slug: "mapmakers-no-borders", status: "review", author: 3, categories: [3, 4], featured: false, readTime: 8, updatedAt: "2026-05-30T19:52:00", publishedAt: null, excerpt: "A small cartography collective is redrawing the world without nation-states.", locale: "en" },
  { id: 16, title: "Why your bread tastes different at altitude", slug: "bread-at-altitude", status: "published", author: 1, categories: [3], featured: false, readTime: 5, updatedAt: "2026-05-22T10:11:00", publishedAt: "2026-05-21T08:00:00", excerpt: "Pressure, yeast, and the chemistry of a mountain-town bakery.", locale: "en" },
  { id: 17, title: "Xin Chào Việt Nam An interview with the last lighthouse keeper", slug: "last-lighthouse-keeper", status: "draft", author: 1, categories: [5, 3], featured: false, readTime: 16, updatedAt: "2026-05-31T07:03:00", publishedAt: null, excerpt: "Forty-one years on a rock in the North Atlantic, in his own words.", locale: "en" },
  { id: 18, title: "The return of the night train", slug: "return-of-night-train", status: "published", author: 2, categories: [4, 2], featured: true, readTime: 10, updatedAt: "2026-05-20T13:30:00", publishedAt: "2026-05-19T07:00:00", excerpt: "Europe rebuilt its sleeper network. We rode it for a week to see if it works.", locale: "en" },
  { id: 19, title: "A field guide to urban lichen", slug: "urban-lichen-field-guide", status: "review", author: 4, categories: [1, 4], featured: false, readTime: 6, updatedAt: "2026-05-29T09:14:00", publishedAt: null, excerpt: "The pollution map hiding in plain sight on every old stone wall.", locale: "en" },
  { id: 20, title: "The economics of a free public sauna", slug: "free-public-sauna", status: "published", author: 3, categories: [4, 3], featured: false, readTime: 12, updatedAt: "2026-05-18T15:45:00", publishedAt: "2026-05-17T08:30:00", excerpt: "One northern city bet that warmth should be a commons. The numbers are surprising.", locale: "en" },
];

export const types: Record<string, ContentType> = {
  article: {
    key: "article", kind: "collection", display: "Article", plural: "Articles", icon: "doc",
    fields: [
      { name: "title", type: "Text", required: true, meta: "Short text" },
      { name: "slug", type: "UID", required: true, meta: "Attached to title" },
      { name: "status", type: "Enumeration", required: true, meta: "draft · review · published" },
      { name: "cover", type: "Media", required: false, meta: "Single image" },
      { name: "excerpt", type: "Text", required: false, meta: "Long text" },
      { name: "body", type: "Rich text", required: true, meta: "Markdown / blocks" },
      { name: "author", type: "Relation", required: true, meta: "Article ↔ Author" },
      { name: "categories", type: "Relation", required: false, meta: "Article ↔ many Category" },
      { name: "featured", type: "Boolean", required: false, meta: "Default false" },
      { name: "readTime", type: "Number", required: false, meta: "Integer · minutes" },
      { name: "publishedAt", type: "Datetime", required: false, meta: "" },
    ],
  },
  author: {
    key: "author", kind: "collection", display: "Author", plural: "Authors", icon: "user",
    fields: [
      { name: "name", type: "Text", required: true, meta: "Short text" },
      { name: "role", type: "Text", required: false, meta: "Short text" },
      { name: "avatar", type: "Media", required: false, meta: "Single image" },
      { name: "bio", type: "Text", required: false, meta: "Long text" },
      { name: "articles", type: "Relation", required: false, meta: "Author ↔ many Article" },
    ],
  },
  category: {
    key: "category", kind: "collection", display: "Category", plural: "Categories", icon: "tag",
    fields: [
      { name: "name", type: "Text", required: true, meta: "Short text" },
      { name: "slug", type: "UID", required: true, meta: "Attached to name" },
      { name: "color", type: "Text", required: false, meta: "Hex" },
      { name: "description", type: "Text", required: false, meta: "Long text" },
    ],
  },
};

export const singleTypes: SingleType[] = [
  { key: "homepage", display: "Homepage", icon: "home" },
  { key: "global", display: "Global", icon: "globe" },
];

export const media: MediaItem[] = [
  { id: 1, name: "estuary-dawn.jpg", w: 4096, h: 2731, size: "3.2 MB", hue: 195, ext: "JPG" },
  { id: 2, name: "buried-river-map.png", w: 2400, h: 3000, size: "1.8 MB", hue: 28, ext: "PNG" },
  { id: 3, name: "coral-tank-01.jpg", w: 3000, h: 2000, size: "2.6 MB", hue: 270, ext: "JPG" },
  { id: 4, name: "night-train-window.jpg", w: 3600, h: 2400, size: "4.1 MB", hue: 220, ext: "JPG" },
  { id: 5, name: "lichen-macro.jpg", w: 2800, h: 2800, size: "2.0 MB", hue: 95, ext: "JPG" },
  { id: 6, name: "sauna-steam.jpg", w: 3200, h: 2133, size: "2.9 MB", hue: 18, ext: "JPG" },
  { id: 7, name: "lighthouse-keeper.jpg", w: 2667, h: 4000, size: "3.7 MB", hue: 210, ext: "JPG" },
  { id: 8, name: "mountain-bakery.jpg", w: 3000, h: 2250, size: "2.4 MB", hue: 36, ext: "JPG" },
];

export const RUSTAPI = { authors, categories, articles, types, singleTypes, media };

export function relTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const now = new Date("2026-06-01T12:00:00");
  const mins = Math.round((now.getTime() - d.getTime()) / 60000);
  if (mins < 60) return mins + "m ago";
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return hrs + "h ago";
  const days = Math.round(hrs / 24);
  return days + "d ago";
}
