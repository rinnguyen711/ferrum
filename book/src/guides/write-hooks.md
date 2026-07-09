# Write hooks

A write hook runs your own Rust code on the content write path — before an entry
is validated and saved, and after it commits. Use it to inject derived fields,
enforce custom rules, or react to a write in-process. Unlike a
[webhook](webhooks.md), a write hook runs synchronously inside the request, so
it can change or reject the write.

Write hooks are a **code-level extension point**: you implement a trait and wire
it into the server, then rebuild the binary. There is no API to register one at
runtime.

## The `WriteHook` trait

A hook implements `WriteHook`, which has two methods — both with no-op defaults,
so you override only what you need:

```rust
use ferrum_http::{WriteContext, WriteHook};
use ferrum_core::Error;
use serde_json::{Map, Value};

pub struct MyHook;

#[async_trait::async_trait]
impl WriteHook for MyHook {
    async fn before_write(
        &self,
        ctx: &WriteContext<'_>,
        mut body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        // Runs after authz + JSON parse, before schema validation.
        // Add, remove, or rewrite fields — or return Err to reject.
        Ok(body)
    }

    async fn after_write(
        &self,
        ctx: &WriteContext<'_>,
        record: &Value,
    ) -> Result<(), Error> {
        // Runs after the write commits, with the final saved record.
        Ok(())
    }
}
```

The `WriteContext` tells you which write is happening:

- `ctx.content_type` — the type name, e.g. `"article"`. Dispatch on this to
  scope a hook to one type.
- `ctx.operation` — `WriteOp::Create` or `WriteOp::Update`.
- `ctx.principal` — who is making the request.

## `before_write` — change or reject

`before_write` runs **after authz and JSON parsing, before schema validation**.
Whatever body you return is then validated against the schema, so any value you
inject must satisfy the field rules.

This hook fills a `slug` from the `title` on create:

```rust
async fn before_write(
    &self,
    ctx: &WriteContext<'_>,
    mut body: Map<String, Value>,
) -> Result<Map<String, Value>, Error> {
    if ctx.content_type == "article" && !body.contains_key("slug") {
        if let Some(Value::String(title)) = body.get("title") {
            let slug = title.to_lowercase().replace(' ', "-");
            body.insert("slug".into(), Value::String(slug));
        }
    }
    Ok(body)
}
```

Return `Err` to reject the request — a validation error surfaces as a `422`:

```rust
return Err(Error::Validation(
    ferrum_core::ValidationErrors::field("title", "must not be empty"),
));
```

## `after_write` — react to a commit

`after_write` runs **after the write is durable**, with the final saved record.
The write has already committed, so returning `Err` produces an error response
but does **not** roll back. Prefer `Error::Internal` here — a 4xx variant would
wrongly tell the client the request was rejected even though it persisted.

For fire-and-forget fan-out — notifying external systems, busting a cache — use
the event sink that backs [webhooks](webhooks.md) instead; it's built for
side effects that shouldn't affect the response.

## Wire it in

The server holds one hook in its `AppState`. The default is `NoopHook`. Swap in
yours where the state is built:

```rust
let state = AppState {
    // …
    hooks: std::sync::Arc::new(MyHook),
    // …
};
```

A single hook handles every type; branch on `ctx.content_type` inside it to
apply per-type logic.

## Where to go next

- [Webhooks](webhooks.md) — asynchronous, out-of-process notifications.
- [Fields & field kinds](../concepts/fields.md) — what an injected value must
  satisfy.
