# rustapi v1 Implementation Plans

Six sequential plans implementing the [v1 design spec](../specs/2026-05-28-rustapi-core-design.md).
Run them in order — each assumes the previous have been completed and committed.

| # | Plan | Goal |
|---|------|------|
| 00 | [Workspace bootstrap](2026-05-28-00-workspace-bootstrap.md) | Cargo workspace, toolchain, empty crate skeletons |
| 01 | [`rustapi-core`](2026-05-28-01-core-crate.md) | Domain types + validation (pure, no I/O) |
| 02 | [`rustapi-sql`](2026-05-28-02-sql-crate.md) | DDL + DML SQL builders (no sqlx) |
| 03 | [`rustapi-schema`](2026-05-28-03-schema-crate.md) | SchemaRegistry, SchemaService, internal migrations |
| 04 | [`rustapi-http`](2026-05-28-04-http-crate.md) | axum router, handlers, middleware |
| 05 | [`rustapi` binary + integration tests](2026-05-28-05-bin-and-integration.md) | main, testcontainers integration coverage |

## How to use

Pick an execution mode:

- **Subagent-driven (recommended)**: dispatch one subagent per task, review between tasks. Use `superpowers:subagent-driven-development`.
- **Inline**: execute tasks in the current session with batched checkpoints. Use `superpowers:executing-plans`.

Each task has 2–5 minute steps with full code, expected output, and a commit step.
