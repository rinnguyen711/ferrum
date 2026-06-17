# Draft & publish

By default every entry is live the moment you save it. Turn on **draft &
publish** for a [content type](content-types.md) and its entries gain a
lifecycle: they start as drafts, stay invisible to the public API until you
publish them, and can be pulled back to draft later.

## Turning it on

Draft & publish is a per-type option, off by default. Enable it by patching the
type's `options`:

```sh
curl -X PATCH http://localhost:8080/admin/content-types/article \
  -H 'Content-Type: application/json' \
  -d '{ "options": { "draft_publish": true } }'
```

Once on, every entry of that type carries a publish state.

## Draft vs published

Publish state is tracked by a single timestamp, `published_at`:

- `published_at` is **NULL** → the entry is a **draft**.
- `published_at` holds a **time** → the entry is **published**.

A newly created entry starts as a draft. You don't set `published_at` yourself —
the publish endpoints manage it.

## Publishing and unpublishing

Two endpoints flip the state of one entry:

```sh
# Publish: stamp published_at = now()
curl -X POST http://localhost:8080/api/article/{id}/publish

# Unpublish: clear published_at back to NULL
curl -X POST http://localhost:8080/api/article/{id}/unpublish
```

Both require write permission on the type. Calling them on a type that doesn't
have draft & publish enabled is rejected, and so is calling them on a
[single type](single-types.md) (which has its own route).

## How publish state affects reads

When draft & publish is on, **list and read default to published only** — the
public, unauthenticated view never sees drafts. A `status` query parameter on
list requests widens that:

| `status` | Returns |
|---|---|
| (omitted) | Published entries only (the default) |
| `draft` | Draft entries only |
| `all` | Both drafts and published |

```sh
# Public: published articles
curl http://localhost:8080/api/article

# Editorial: everything, including drafts
curl http://localhost:8080/api/article?status=all
```

When draft & publish is **off**, every entry is always returned — there is no
publish state to filter on, and `status` is ignored.

## Where to go next

- [Content types](content-types.md) — where the `draft_publish` option lives.
- [Single types](single-types.md) — single-entry types and how they differ.
- [REST API](../reference/rest-api.md) — the full list/read surface and
  parameters.
