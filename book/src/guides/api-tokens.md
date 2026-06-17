# API tokens

An API token gives a script, service, or CI job programmatic access to the
content API — no interactive login. Each token carries a fixed set of
[content scopes](#scopes) that bound what it can do. This guide shows how to
create, use, and revoke them.

## Authenticate with a token

Send the token as a bearer credential in the `Authorization` header:

```sh
curl http://localhost:8080/api/article \
  -H 'Authorization: Bearer rat_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'
```

The same header carries either a user JWT or an API token — the server tells
them apart by shape. API tokens always start with the `rat_` prefix.

## Create a token

Token management lives under `/api/admin/tokens` and requires admin
(user-management) permission. Post a name, a non-empty `scopes` list, and an
optional `expires_at`:

```sh
curl -X POST http://localhost:8080/api/admin/tokens \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "CI deploy",
    "description": "Publishes articles from the build pipeline",
    "scopes": ["content:read", "content:write:article"],
    "expires_at": "2027-01-01T00:00:00Z"
  }'
```

The response includes the raw token **once**:

```json
{
  "token": "rat_8f3c1a5e2b1f4d8a9b6e7c2f1d4a8e90...",
  "id": "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90",
  "name": "CI deploy",
  "description": "Publishes articles from the build pipeline",
  "scopes": ["content:read", "content:write:article"],
  "expires_at": "2027-01-01T00:00:00Z",
  "last_used_at": null,
  "created_at": "2026-06-17T12:00:00Z"
}
```

Copy the `token` value now — only its SHA-256 hash is stored, so the raw token
is never shown again. Lose it and you create a new one.

## Scopes

A token may only hold **content** scopes. Schema and user management are not
available to API tokens. A scope is `content:<action>` optionally narrowed to a
single content type:

| Scope | Grants |
|---|---|
| `content:read` | Read any content type |
| `content:write` | Create/update any content type |
| `content:delete` | Delete any content type |
| `content:read:article` | Read only the `article` type |
| `content:write:article` | Write only the `article` type |
| `content:delete:article` | Delete only the `article` type |

Every scope must be a valid `content:*` scope, and the list can't be empty —
otherwise create and update are rejected `422`.

## Manage tokens

| Method | Path | Does |
|---|---|---|
| `GET` | `/api/admin/tokens` | List tokens (metadata only, never the raw value). |
| `POST` | `/api/admin/tokens` | Create a token, returning the raw value once. |
| `PATCH` | `/api/admin/tokens/{id}` | Update name, description, scopes, expiry. |
| `DELETE` | `/api/admin/tokens/{id}` | Revoke the token immediately. |

Listing returns each token's `last_used_at` so you can spot dormant tokens.
Revoking deletes the token — the next request using it gets `401 Unauthorized`.

## Expiry

A token with an `expires_at` in the past is rejected at auth time with `401`,
the same as a revoked or unknown token. Leave `expires_at` unset for a
non-expiring token, or set it to force rotation.

## Where to go next

- [Roles & permissions](roles.md) — how scopes and roles map to actions.
- [REST API](../reference/rest-api.md) — the content endpoints a token reaches.
