# Roles & permissions

Roles decide what a user can do. Every user holds one or more roles; each role
grants a set of permissions over your content types. This guide covers the
built-in roles, how to define custom ones, and how permissions are enforced.

Roles apply to **users**. API tokens carry their own
[content scopes](api-tokens.md#scopes) instead.

## Built-in roles

Three system roles ship and cannot be edited or deleted:

| Role | Grants |
|---|---|
| `admin` | Full access — content, schema, users, settings. |
| `editor` | Read, write, and delete any content. |
| `viewer` | Read any content. |

Only `admin` can manage schema, users, roles, and tokens. `editor` and `viewer`
are content-only.

## Permissions

A permission is a pair: a **content type** and an **action verb**. The verbs map
to the underlying access an action needs:

| Verb | Enforced as | Covers |
|---|---|---|
| `find` | read | List entries |
| `findOne` | read | Read one entry |
| `create` | write | Create an entry |
| `update` | write | Update an entry |
| `publish` | write | Publish/unpublish an entry |
| `delete` | delete | Delete an entry |

A custom role's permissions are an explicit list — a role grants only the
content types and verbs you give it. Permissions can also target the built-in
plugin types `plugin::users` and `plugin::upload`.

## Create a custom role

Role management lives under `/admin/roles` and requires admin permission. A
role needs a kebab-case `key`, a `name`, and a list of permissions:

```sh
curl -X POST http://localhost:8080/admin/roles \
  -H 'Content-Type: application/json' \
  -d '{
    "key": "blog-author",
    "name": "Blog Author",
    "description": "Writes and publishes articles, reads everything else",
    "color": "#52525B",
    "permissions": [
      { "content_type": "article", "action": "create" },
      { "content_type": "article", "action": "update" },
      { "content_type": "article", "action": "publish" },
      { "content_type": "author",  "action": "find" }
    ]
  }'
```

The `key` must be kebab-case (`a-z`, `0-9`, `-`), at most 64 characters, no
leading or trailing dash. Each permission's `content_type` must be a real type
(or a plugin type) and each `action` a known verb — otherwise the request is
rejected `422`.

## Manage roles

| Method | Path | Does |
|---|---|---|
| `GET` | `/admin/roles` | List roles with a permission count. |
| `POST` | `/admin/roles` | Create a custom role. |
| `GET` | `/admin/roles/{key}` | Read a role and its full permission list. |
| `PUT` | `/admin/roles/{key}` | Replace a custom role's name, color, permissions. |
| `DELETE` | `/admin/roles/{key}` | Delete a custom role. |

System roles are locked: a `PUT` or `DELETE` on `admin`, `editor`, or `viewer`
returns `403 Forbidden`. Update and delete replace the whole permission set, so
send the complete list each time.

## How enforcement works

Permissions are held in an in-memory registry, refreshed whenever a role
changes — so authorization never hits the database on the request path. When a
request arrives, the server checks the principal's roles against the action and
target content type, and returns `403 Forbidden` if none grant it.

## Where to go next

- [API tokens](api-tokens.md) — content scopes for non-interactive access.
- [Content types](../concepts/content-types.md) — the types a permission targets.
- [Draft & publish](../concepts/draft-publish.md) — what the `publish` verb
  controls.
