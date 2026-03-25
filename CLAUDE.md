# Claude working rules for this repo

## Always do before starting work

1. **Read ROADMAP.md** — check which phase we're in and what's marked `[ ]` vs `[x]`. Don't build things that conflict with the roadmap direction.
2. **Run `SQLX_OFFLINE=true cargo check`** after any Rust change. Zero errors before calling anything done.
3. **Check git status** if context is unclear — recent commits reveal what changed last session.

## Architecture rules

- **Two persistence layers, never mixed:**
  - `SqlitePool` — local mode only (`xarxa server start`, `xarxa workflow run`)
  - `PgPool` — production only (`run_server()` in `main.rs`)
- **`WitPluginRuntime` stays dependency-free** — no SQLite, no PostgreSQL imports. Persistence hooks go through the `on_result` callback pattern.
- **Hexagonal architecture:** `src/core/` has no infra imports. `src/adapters/` depends on `src/core/`, never the other way.
- **SQLite migrations live in `migrations/sqlite/`**, embedded via `sqlx::migrate!("./migrations/sqlite")`. PostgreSQL migrations live in `migrations/`.

## CLI structure

```
xarxa workflow run [PLUGIN]     — wasmtime directly, saves to ./xarxa.db
xarxa workflow run --continue   — reads state blob from SQLite, resumes
xarxa server start              — SQLite + wasmtime + hot-reload, no JWT
xarxa plugin init / build       — scaffold and compile plugins
```

`--server <url>` on `workflow run` routes to remote HTTP (future feature).

## SDK / WIT rules

- `wit-bindgen::generate!` must expand in the **user's crate**, not in the SDK library. It lives inside `xarxa_entry_point!()` proc-macro.
- WIT world is `xarxa:api@0.3.1`, world name `orchestrator`, path `../../wit/orchestrator`.
- Plugin target: `wasm32-wasip2`. Build with `xarxa plugin build` or `cargo build --target wasm32-wasip2`.

## Durability model

- `workflow_runs.state` = opaque blob, updated on every `send-result` call from plugin.
- `workflow_events` = append-only log, one row per `send-result`, for audit/debug.
- Recovery: read `state` from SQLite → pass to `continue-workflow` → plugin replays internally.
- The host never interprets state contents — that's the plugin's job.

## What NOT to do

- Don't add `--dev` flags — local mode is the default, `--server` is opt-in.
- Don't use `Extension` in axum — use `State<T>` + `.with_state()`.
- Don't mock the DB in tests — use real SQLite (in-memory: `sqlite::memory:`) or testcontainers for PostgreSQL.
- Don't translate code examples, variable names, or function names — only prose comments.
- Don't commit without running `cargo check` first.
