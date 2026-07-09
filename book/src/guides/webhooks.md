# Webhooks

A webhook tells an external service when your content changes. Subscribe a URL
to one or more content events, and Ferrum POSTs a JSON payload to it each time
one fires — durably queued and retried. This guide covers creating webhooks,
the payload shape, verifying signatures, and retries.

## Events

A webhook subscribes to one or more of these events:

| Event | Fires when |
|---|---|
| `entry.created` | An entry is created |
| `entry.updated` | An entry is updated |
| `entry.deleted` | An entry is deleted |
| `entry.published` | An entry is published |
| `entry.unpublished` | An entry is unpublished |

A webhook must subscribe to at least one event; an unknown event name is
rejected `422`.

## Create a webhook

Webhook management lives under `/admin/webhooks` and requires admin permission.
Post a name, a valid URL, the events, and an optional signing `secret`:

```sh
curl -X POST http://localhost:8080/admin/webhooks \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "Rebuild site",
    "url": "https://ci.example.com/hooks/rebuild",
    "events": ["entry.published", "entry.unpublished"],
    "secret": "a-shared-secret"
  }'
```

## The payload

Each delivery is a POST with a JSON body:

```json
{
  "event": "entry.published",
  "createdAt": "2026-06-17T12:00:00Z",
  "model": "article",
  "entry": {
    "id": "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90",
    "title": "Hello world",
    "published_at": "2026-06-17T12:00:00Z"
  }
}
```

`model` is the content type name. `entry` is the full row for most events; for
`entry.deleted` it's just `{ "id": "…" }` (the row is already gone).

## Verify the signature

If the webhook has a `secret`, Ferrum signs each request body with HMAC-SHA256
and sends it in the `x-ferrum-signature` header:

```
x-ferrum-signature: sha256=<hex digest>
```

Recompute the HMAC over the **raw request body** with your shared secret and
compare. A non-matching signature means the request didn't come from your
Ferrum instance — reject it.

## Delivery and retries

Deliveries are queued to the database, then sent by a background worker — so a
slow or down receiver never blocks the API write. Each attempt has a 10-second
timeout. A `2xx` response is success; anything else (non-2xx or a transport
error) is a failure and is retried.

A failed delivery is retried up to **5 attempts** with exponential backoff. The
last error is recorded so you can inspect why a delivery failed.

## Inspect and test

| Method | Path | Does |
|---|---|---|
| `GET` | `/admin/webhooks` | List webhooks. |
| `PATCH` | `/admin/webhooks/{id}` | Update name, URL, events, secret, `enabled`. |
| `DELETE` | `/admin/webhooks/{id}` | Delete the webhook. |
| `GET` | `/admin/webhooks/{id}/deliveries` | Recent delivery attempts and their status. |
| `POST` | `/admin/webhooks/{id}/test` | Queue a `ping` delivery to check wiring. |

A test queues a delivery with `"event": "ping"` and null `model`/`entry` — use
it to confirm your receiver is reachable. Testing a disabled webhook is
rejected. Disable a webhook by setting `enabled: false` rather than deleting it
to pause deliveries without losing the config.

## Where to go next

- [Draft & publish](../concepts/draft-publish.md) — what triggers the publish
  events.
- [Write hooks](write-hooks.md) — change a write in-process, before it commits.
