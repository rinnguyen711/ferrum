# First-run setup

A fresh Rustapi server has no users. Before you can call the admin API, you
create the first admin account, then log in to get a token. This page assumes
you have a server running from [Installation](installation.md).

## Create the first admin

`POST /auth/setup` creates the initial admin. It needs no authentication — but
it only works while the users table is empty. Once any user exists, it returns
`409 Conflict`, so this is a one-time call.

The password must be at least 8 characters.

```sh
curl -X POST http://localhost:8080/auth/setup \
  -H 'Content-Type: application/json' \
  -d '{ "email": "admin@example.com", "password": "change-me-please" }'
```

```json
{
  "id": "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90",
  "email": "admin@example.com",
  "roles": ["admin"]
}
```

The new account gets the `admin` role.

## Log in to get a token

Setup creates the account but does not return a token. Log in to get one:

```sh
curl -X POST http://localhost:8080/auth/login \
  -H 'Content-Type: application/json' \
  -d '{ "email": "admin@example.com", "password": "change-me-please" }'
```

```json
{ "token": "<jwt>", "expires_at": 1781000000 }
```

`token` is a JWT; `expires_at` is its expiry as a Unix timestamp. When the token
expires, log in again.

## Send authenticated requests

Pass the token in an `Authorization: Bearer` header on every admin request. Save
it to a shell variable so you can reuse it:

```sh
TOKEN=<token from /auth/login>

curl http://localhost:8080/admin/content-types \
  -H "Authorization: Bearer $TOKEN"
```

A request without a valid token is rejected.

## Next steps

You're authenticated. Now define a content type and create an entry in
[Your first content type](first-content-type.md).
