# Xarxa Roadmap

Everything that needs to be done to make Xarxa a product with a WOW effect.
Based on architecture analysis, competitor comparison, and product research.

---

## Table of Contents

1. [Critical Architecture Bugs](#1-critical-architecture-bugs)
2. [Durability — real durability](#2-durability--real-durability)
3. [Activity Model — step reuse](#3-activity-model--step-reuse)
4. [CLI — developer experience](#4-cli--developer-experience)
5. [SDKs for languages](#5-sdks-for-languages)
6. [BigData — DataFusion integration](#6-bigdata--datafusion-integration)
7. [Playground — WOW moment](#7-playground--wow-moment)
8. [Monitoring UI](#8-monitoring-ui)
9. [Priorities and order](#9-priorities-and-order)

---

## 1. Critical Architecture Bugs

These things must be fixed before everything else — they block parallel workflow execution.

### 1.1 Write lock blocks all workflows

**Problem:** in `wit_runtime.rs` `execute_wit_function` holds a `write` lock for the entire duration of WASM execution. This means workflows execute strictly one at a time.

```rust
// src/adapters/wasmtime/wit_runtime.rs:165
// NOW — all workflows wait for each other
let mut plugins = self.plugins.write().await;
```

**How to fix:** take a `read` lock only to clone the component, then execute outside the lock.

```rust
// SHOULD BE
let component = {
    let plugins = self.plugins.read().await;
    let plugin = plugins.get(plugin_name)...?;
    plugin.component.clone()  // Component::clone() is cheap — it's an Arc internally
};
// execute outside the lock — in parallel with other workflows
let result = self.execute_wit_component_function(&component, function_name, &params).await?;
```

### 1.2 Synchronous WASM runs on the Tokio thread

**Problem:**

```rust
config.async_support(false); // Synchronous operation for simplicity
```

This blocks a Tokio worker thread for the entire duration of WASM execution. Under load, this degrades the whole server.

**How to fix:** use `tokio::task::spawn_blocking` to run WASM, or enable `async_support(true)` in Wasmtime.

---

## 2. Durability — real durability

Currently Xarxa is called a "Durable Workflow Orchestrator" but there is no real durability — if the server crashes at step 3 of 5, the workflow restarts from the beginning. This is the most important thing to implement.

### 2.1 Event Log in PostgreSQL

Every workflow event is saved to the database:

```sql
CREATE TABLE workflow_events (
    id          BIGSERIAL PRIMARY KEY,
    run_id      BIGINT NOT NULL,
    seq         INT NOT NULL,           -- sequential event number
    event_type  TEXT NOT NULL,          -- STARTED, STEP_COMPLETED, STEP_FAILED, COMPENSATED
    step_name   TEXT,
    input       BYTEA,
    output      BYTEA,
    error       TEXT,
    created_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE(run_id, seq)
);
```

### 2.2 Replay logic

When `continue_workflow` is called, the host passes the full event history into WASM. The SDK inside WASM does not re-execute steps during replay — it only restores the context state.

```text
start_workflow("booking", input)
    → events: [STARTED]
    → execute step "reserve" → success
    → events: [STARTED, STEP_COMPLETED(reserve, output)]
    → server crashed

restart server
continue_workflow(run_id, history)
    → history contains [STARTED, STEP_COMPLETED(reserve)]
    → SDK sees that "reserve" is already completed → skips it
    → executes the next step "charge"
```

### 2.3 Workflow statuses

```text
RUNNING → SLEEPING → RUNNING → COMPLETED
                   ↘ FAILED → COMPENSATING → COMPENSATED
```

- `SLEEPING` — the workflow is waiting for an external event (timer, webhook, human approval)
- `COMPENSATING` — compensations have been triggered after a failure
- Stored in the `workflow_runs` table

---

## 3. Activity Model — step reuse

Currently there is no way to reuse steps across workflows written in different languages. An Activity model is needed.

### 3.1 WIT changes

Add `run-activity` to `host-controller`:

```wit
// wit/orchestrator/world.wit
interface host-controller {
    use engine-types.{run-id};

    send-result: func(id: run-id, data: list<u8>);

    // new — call an activity by name through the host
    run-activity: func(name: string, input: list<u8>) -> result<list<u8>, string>;
}
```

### 3.2 Separate WIT world for activities

```wit
// wit/activity/world.wit — minimal contract for an activity
package xarxa:activity@0.1.0;

world activity {
    export activity-ctrl;
    import custom-http;
    import logger;
}

interface activity-ctrl {
    execute: func(input: list<u8>) -> result<list<u8>, string>;
}
```

### 3.3 Host implementation

```rust
// src/adapters/wasmtime/wit_runtime.rs
impl MainController for HostState {
    fn run_activity(&mut self, name: String, input: Vec<u8>) -> Result<Vec<u8>, String> {
        // host finds the required wasm among loaded plugins and executes it
        self.runtime
            .execute_activity_sync(&name, &input)
            .map_err(|e| e.to_string())
    }
}
```

### 3.4 HTTP endpoint for uploading activities

```text
POST /api/namespaces/{id}/activities
    body: multipart/form-data with .wasm file

→ Activity is available to all workflows in the namespace by name
```

### 3.5 What reuse looks like

```text
send_email.wasm (Python)    charge_card.wasm (Go)
        │                           │
        └─────────────┬─────────────┘
                      │ host router
        ┌─────────────┴─────────────┐
        │                           │
booking.wasm (Rust)         onboarding.wasm (Rust)
  run_activity("send_email")    run_activity("send_email")
  run_activity("charge_card")
```

The language of the activity does not matter — the interface is the same for everyone.

---

## 4. CLI — developer experience

Without a CLI there is no proper DX. Every second of friction means a lost user.

### 4.1 Installation

```bash
curl -fsSL https://install.xarxa.io | sh
# or
brew install xarxa
```

### 4.2 Commands

```bash
# ── Plugins ───────────────────────────────────────────────────────
# Scaffold a new plugin
xarxa plugin init my-workflow
# creates: src/lib.rs, Cargo.toml, xarxa.toml

# Build the plugin into .wasm
xarxa plugin build --out ./plugins

# ── Local run (no server, no auth) ────────────────────────────────
# Run a workflow directly via wasmtime — saves state to ./xarxa.db
xarxa workflow run booking --input '{"flight_id": "FL-123", "user_id": "user-456"}'

# Interactive plugin selection + prompt for input
xarxa workflow run

# Continue a failed/interrupted run (crash recovery)
xarxa workflow run booking --continue <run_id>

# ── Server ────────────────────────────────────────────────────────
# Local server: SQLite + wasmtime + hot-reload, no JWT
xarxa server start

# Production server: PostgreSQL + S3 + JWT auth
xarxa server start   # (without --dev, reads .env / config)

# ── Remote server ─────────────────────────────────────────────────
xarxa workflow run booking --input '...' --server https://api.xarxa.io --token $JWT

# ── TODO ──────────────────────────────────────────────────────────
# xarxa workflow logs <run_id>   — view workflow_events
# xarxa workflow list            — list runs from SQLite/PostgreSQL
# xarxa test --mock "..."        — testing with HTTP mocks
```

### 4.3 Local mode (xarxa dev)

Key feature — works entirely without external dependencies:

- In-memory storage instead of PostgreSQL
- Local directory instead of S3
- File watcher for automatic reload
- Log output directly in the terminal with colors

```text
$ xarxa dev

  Xarxa Dev Mode
  Watching: ./src/**/*.rs
  Runtime: http://localhost:4000

  ✓ booking    loaded  (23ms)

  [12:34:01] POST /api/workflows/booking/start-workflow
  [12:34:01] ▶ reserve_flight    ✓ 8ms
  [12:34:01] ▶ charge_card       ✓ 23ms
  [12:34:01] ▶ send_email        ✓ 45ms
  [12:34:01] ✓ Workflow completed in 76ms

  src/lib.rs changed → rebuilding...
  ✓ booking reloaded (1.2s)
```

### 4.4 xarxa.toml — project config

```toml
[workflow]
name = "booking"
version = "0.1.0"

[build]
target = "wasm32-wasip2"

[dev]
port = 4000
watch = ["src/**/*.rs"]

[mocks]
"https://api.stripe.com" = "fixtures/stripe.json"
"https://api.sendgrid.com" = "fixtures/email.json"
```

---

## 5. SDKs for languages

### 5.1 Rust SDK (already exists, needs improvement)

The current SDK already allows writing:

```rust
Pipeline::new()
    .step("reserve", |ctx| reserve_flight(ctx))
    .compensate("reserve", |ctx| release_flight(ctx))
    .step("charge", |ctx| charge_card(ctx))
    .build()
```

What to add:

- `ctx.run_activity("send_email", &input)` — calling an activity from another .wasm (see 5.1.2)
- `ctx.sleep(Duration::from_secs(3600))` — pause the workflow
- `ctx.wait_for_signal("payment_confirmed")` — wait for an external event
- Typed input/output via proc-macro (see 5.1.1)

### 5.1.1 Proc-macro `#[workflow]` — remove all boilerplate

Currently every workflow file contains ~80 lines of boilerplate (`struct Engine`, `impl Guest`, `match workflow_name` in two places, `export!`). This needs to be removed completely.

**Goal — the final user file:**

```rust
use xarxa_sdk_rs::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct BookingInput {
    flight_id: String,
    user_id: String,
    amount: f64,
}

#[workflow("booking")]          // ← registration + input deserialization
fn booking_flow(input: BookingInput) -> PipelineState {
    let flight_id = input.flight_id.clone();
    let user_id = input.user_id.clone();
    let amount = input.amount;

    Pipeline::new()
        .step("reserve", move |_ctx| {
            logging(format!("Reserving {flight_id}").as_bytes());
            Ok(())
        })
        .compensate("reserve", move |_ctx| {
            logging(b"Releasing reservation");
            Ok(())
        })
        .step("charge", move |_ctx| {
            logging(format!("Charging {amount} for {user_id}").as_bytes());
            Ok(())
        })
        .build()
}

#[workflow("payment")]          // ← new workflow = one attribute
fn payment_flow(input: PaymentInput) -> PipelineState {
    Pipeline::new()
        .step("validate", |ctx| { ... })
        .build()
}

xarxa_entry_point!();           // ← once per file, replaces all Engine boilerplate
```

**What the macro removes:**

| Before | After |
| --- | --- |
| `struct Engine` | gone |
| `impl Guest for Engine` | gone |
| `fn start_workflow` (~40 lines) | gone |
| `fn continue_workflow` (~35 lines) | gone |
| `match workflow_name` in two places | gone |
| `export!(Engine)` | gone |
| **~80 lines of boilerplate** | **`xarxa_entry_point!()`** |

**Crate structure:**

```text
xarxa-sdk-rs/               ← main SDK, re-exports the macro
xarxa-sdk-rs-macros/        ← separate proc-macro crate (required by Rust rules)
  ├── Cargo.toml             [lib] proc-macro = true
  └── src/lib.rs             implementation of #[workflow]
```

**What `#[workflow("booking")]` does under the hood:**

```rust
// User writes:
#[workflow("booking")]
fn booking_flow(input: BookingInput) -> PipelineState { ... }

// Macro expands to:
fn booking_flow(input: BookingInput) -> PipelineState { ... }  // function unchanged

inventory::submit! {
    xarxa_sdk_rs::registry::WorkflowFactory {
        name: "booking",
        build: |raw_bytes: Vec<u8>| {
            let input: BookingInput = serde_json::from_slice(&raw_bytes)
                .expect("Failed to deserialize input for workflow 'booking'");
            booking_flow(input)   // call the original function with typed input
        },
    }
}
```

`xarxa_entry_point!()` iterates over all registered factories via `inventory` — no manual `match workflow_name` needed.

**Dependencies to add to the SDK:**

```toml
# xarxa-sdk-rs/Cargo.toml
[dependencies]
xarxa-sdk-rs-macros = { path = "../xarxa-sdk-rs-macros" }
inventory = "0.3"
serde_json = "1"

# xarxa-sdk-rs-macros/Cargo.toml
[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

### 5.1.2 `ctx.run_activity` — calling an activity from another .wasm *(Nice to Have)*

Allows a workflow to call activities written in **any language** — Go, Python, Rust — by name through the host. The language of the activity is completely transparent to the workflow.

**What it looks like for the user:**

```rust
#[derive(Serialize)]
struct ChargeCardInput { user_id: String, amount: f64 }

#[derive(Deserialize)]
struct ChargeCardOutput { payment_id: String, status: String }

#[workflow("booking")]
fn booking_flow(input: BookingInput) -> PipelineState {
    let user_id = input.user_id.clone();
    let user_id_comp = input.user_id.clone();
    let amount = input.amount;

    Pipeline::new()
        .step("charge", move |ctx| {
            // charge_card.wasm is written in Go — called in one line
            let result: ChargeCardOutput = ctx.run_activity(
                "charge_card",
                &ChargeCardInput { user_id: user_id.clone(), amount },
            )?;
            ctx.set("payment_id", Value::Str(result.payment_id));
            Ok(())
        })
        .compensate("charge", move |ctx| {
            let payment_id = ctx.get_string("payment_id").unwrap_or_default();
            // refund.wasm is written in Python — the same interface
            ctx.run_activity_raw("refund", &json!({ "payment_id": payment_id }))?;
            Ok(())
        })
        .step("notify", move |ctx| {
            let payment_id = ctx.get_string("payment_id").unwrap_or_default();
            // send_email.wasm is written in Python
            ctx.run_activity("send_email", &SendEmailInput {
                to: format!("{user_id_comp}@example.com"),
                subject: "Booking confirmed".into(),
                body: format!("Payment {payment_id} processed"),
            })?;
            Ok(())
        })
        .build()
}
```

**What needs to be added to the SDK (`ctx.run_activity`):**

```rust
// xarxa_sdk_rs/src/context.rs
impl StepContext {
    // typed version — serialize input, deserialize output automatically
    pub fn run_activity<I, O>(&self, name: &str, input: &I) -> Result<O, String>
    where
        I: Serialize,
        O: for<'de> Deserialize<'de>,
    {
        let input_bytes = serde_json::to_vec(input)
            .map_err(|e| format!("Serialize error: {e}"))?;
        // run_activity — WIT binding for host-controller::run-activity
        let result_bytes = run_activity(name, &input_bytes)?;
        serde_json::from_slice(&result_bytes)
            .map_err(|e| format!("Deserialize error: {e}"))
    }

    // raw version — when typing is not needed
    pub fn run_activity_raw(&self, name: &str, input: &serde_json::Value) -> Result<serde_json::Value, String> {
        let bytes = serde_json::to_vec(input).map_err(|e| e.to_string())?;
        let result = run_activity(name, &bytes)?;
        serde_json::from_slice(&result).map_err(|e| e.to_string())
    }
}
```

**Full call diagram:**

```text
booking.wasm (Rust)
  ctx.run_activity("charge_card", &input)
         │
         │  WIT: host-controller::run-activity("charge_card", bytes)
         ▼
    Xarxa Host
    plugins["charge_card"] → charge_card.wasm
         ▼
  charge_card.wasm (Go)     ← language does not matter
    execute(input) → { payment_id, status }
         │
         └──────────────────▶ booking.wasm receives the typed result
```

**What needs to be implemented:**

1. Add `run-activity` to the WIT `host-controller` (see section 3.1)
2. Add `ctx.run_activity` and `ctx.run_activity_raw` to `StepContext` in the SDK
3. A separate WIT world for activities with a single `execute` export

**An activity in Go as an example (`charge_card.wasm`):**

```go
// the only convention — export the function execute(input: list<u8>) -> result<list<u8>, string>
func execute(input []byte) ([]byte, error) {
    var req ChargeCardInput
    json.Unmarshal(input, &req)
    result := ChargeCardOutput{
        PaymentID: fmt.Sprintf("PAY-%s-%d", req.UserID, time.Now().Unix()),
        Status:    "success",
    }
    return json.Marshal(result)
}
```

### 5.2 Go SDK (TinyGo + wasip2)

```go
// go.mod: require github.com/xarxa-systems/xarxa-sdk-go

type BookingInput struct {
    FlightID string `json:"flight_id"`
    UserID   string `json:"user_id"`
    Amount   float64 `json:"amount"`
}

func BookingFlow(ctx *xarxa.Context, input BookingInput) error {
    return xarxa.Pipeline(ctx,
        xarxa.Step("reserve", func(ctx *xarxa.StepContext) error {
            return ctx.Activity("reserve_flight", input.FlightID)
        }),
        xarxa.Compensate("reserve", func(ctx *xarxa.StepContext) error {
            return ctx.Activity("release_flight", input.FlightID)
        }),
        xarxa.Step("charge", func(ctx *xarxa.StepContext) error {
            return ctx.Activity("charge_card", input.UserID, input.Amount)
        }),
    )
}
```

### 5.3 Python SDK (componentize-py)

```python
# pip install xarxa-sdk-py

from xarxa import Pipeline, step, compensate

@step("reserve")
def reserve_flight(ctx):
    ctx.activity("reserve_flight", flight_id=ctx.get("flight_id"))

@compensate("reserve")
def release_flight(ctx):
    ctx.activity("release_flight", flight_id=ctx.get("flight_id"))

booking_flow = Pipeline([reserve_flight, release_flight])
```

### 5.4 Language support matrix

| Language | Compiler | Status | .wasm size |
| --- | --- | --- | --- |
| Rust | `wasm32-wasip2` | First class | ~100KB |
| Go | TinyGo + wasip2 | Planned | ~500KB |
| Python | componentize-py | Planned | ~10MB |
| TypeScript | Javy / StarlingMonkey | Roadmap | ~2MB |

---

## 6. BigData — DataFusion integration

A unique feature that neither Temporal, nor Airflow, nor Windmill have.

### 6.1 Concept

The workflow works with terabytes of data without holding it in WASM memory — data is written to S3 as Parquet through the host, and DataFusion runs SQL queries directly on top of S3.

```text
WASM (256MB limit)                    Host (Rust)
    │                                     │
    │  write_batch(chunk)  ──────────────▶│ S3 multipart upload
    │                                     │ → s3://bucket/runs/42/output.parquet
    │  query("SELECT...")  ──────────────▶│ DataFusion
    │                                     │ → predicate pushdown in S3
    │◀─────── Arrow IPC batches ──────────│
```

### 6.2 WIT interface

```wit
interface data-engine {
    use engine-types.{run-id};

    // Open a write stream to S3
    open-writer: func(
        run-id: run-id,
        artifact-name: string,
        schema-json: list<u8>,
    ) -> result<writer-handle, string>;

    // Write a batch (Arrow IPC format)
    write-batch: func(
        handle: writer-handle,
        data: list<u8>,
    ) -> result<u64, string>;

    // Finish writing, flush to S3
    close-writer: func(handle: writer-handle) -> result<artifact-ref, string>;

    // SQL over artifacts
    query: func(
        run-id: run-id,
        sql: string,
    ) -> result<query-result, string>;

    // Read an artifact from another workflow
    get-artifact: func(
        namespace: string,
        run-id: run-id,
        name: string,
    ) -> result<artifact-ref, string>;

    type writer-handle = u32;

    record artifact-ref {
        s3-key: string,
        row-count: u64,
        size-bytes: u64,
    }

    record query-result {
        schema-json: list<u8>,
        batches: list<list<u8>>,
        row-count: u64,
    }
}

// Add to world orchestrator:
// import data-engine;
```

### 6.3 What it looks like for the user

```rust
// Workflow downloads 1TB from an API and saves it to S3 as Parquet
fn etl_flow() -> PipelineState {
    Pipeline::new()
        .step("download", |ctx| {
            let schema = r#"{"fields": [{"name": "user_id", "type": "Utf8"}, ...]}"#;
            let writer = ctx.data.open_writer("transactions.parquet", schema)?;

            let mut offset = 0u64;
            loop {
                let chunk = ctx.http.get(
                    &format!("https://api.example.com/data?offset={offset}&limit=10000")
                )?;
                if chunk.is_empty() { break; }

                writer.write_json_lines(&chunk)?;  // directly to S3, not into WASM memory
                offset += 10_000;
            }

            let artifact = writer.finish()?;
            ctx.set("artifact", Value::Str(artifact.s3_key));
            Ok(())
        })
        .step("analyze", |ctx| {
            // DataFusion on the host, predicate pushdown in S3
            let result = ctx.data.query("
                SELECT user_id, SUM(amount) as total
                FROM 'transactions.parquet'
                WHERE timestamp > '2024-01-01'
                GROUP BY user_id
                ORDER BY total DESC
                LIMIT 100
            ")?;

            ctx.set("top_users", Value::Any(result.to_json()));
            Ok(())
        })
        .build()
}
```

### 6.4 Dependencies

```toml
# Cargo.toml of the host
datafusion = "35"
object_store = { version = "0.10", features = ["aws"] }
arrow = "52"
parquet = { version = "52", features = ["async"] }
```

### 6.5 Complexity estimate

| Stage | Complexity | Time |
| --- | --- | --- |
| `data-engine` WIT interface | Low | 2 days |
| DataFusion + S3 object store on host | Medium | 1 week |
| Streaming write via multipart S3 upload | Medium | 3-4 days |
| Arrow IPC serialization for WIT | Medium | 3 days |
| SQL query + returning result to WASM | Medium | 3 days |
| **MVP total** | | **~3 weeks** |

---

## 7. Playground — WOW moment

The highest ROI item in the entire roadmap. One convincing demo converts better than any documentation.

### 7.1 Appearance

```text
┌─────────────────────────────────────────────────────────────────┐
│  Examples: [Booking Saga ▼]  [Payment Flow]  [ETL Pipeline]     │
├─────────────────────────────┬───────────────────────────────────┤
│                             │  Execution Log                    │
│  fn booking_flow() {        │                                   │
│    Pipeline::new()          │  ▶ reserve_flight    ✓  8ms      │
│    .step("reserve", ...)    │  ▶ charge_card        ✓ 23ms     │
│    .compensate("reserve")   │  ↻ send_email         running... │
│    .step("charge", ...)     │  ○ send_sms            pending   │
│    .compensate("charge")    │                                   │
│    .step("email", ...)      │  Run ID: wf_abc123               │
│    .step("sms", ...)        │  Status: executing               │
│    .build()                 │  Started: 2s ago                 │
│  }                          │                                   │
│                             │  ──────────────────────────────  │
│                             │  Logs:                           │
│                             │  [12:34:01] Starting booking     │
│                             │  [12:34:01] Reserving FL-123     │
│  [▶ Run Workflow]           │  [12:34:02] Charging card...     │
│  [💥 Fail Current Step]     │                                   │
└─────────────────────────────┴───────────────────────────────────┘
```

### 7.2 The "Fail Current Step" button — the main WOW

The user clicks during execution → the current step fails → compensations are triggered in reverse order → the user can see the Saga rolling back automatically.

```text
Clicked "Fail" on step "charge_card":

  ▶ reserve_flight    ✓ 8ms
  ✗ charge_card       FAILED
  ↩ release_flight    compensating... ✓ 12ms   ← automatically!

  Status: COMPENSATED
  Message: "Workflow failed at 'charge_card', compensation completed"
```

This cannot be explained in words — it has to be shown interactively. That is exactly what sells the Saga pattern.

### 7.3 Backend — what to add

```rust
// New endpoints in src/adapters/http/

// List of ready-made examples
GET /api/playground/examples
→ [{ "id": "booking", "name": "Booking Saga", "description": "..." }, ...]

// Run an example, get run_id
POST /api/playground/run/{example_id}
→ { "run_id": "wf_abc123" }

// SSE stream of execution events
GET /api/playground/stream/{run_id}
→ text/event-stream
   data: {"type": "step_started", "step": "reserve_flight"}
   data: {"type": "step_completed", "step": "reserve_flight", "duration_ms": 8}
   data: {"type": "log", "message": "Reserving FL-123"}

// Fail the current step
POST /api/playground/fail/{run_id}
→ triggers compensations
```

### 7.4 Frontend stack

- **Monaco Editor** — read-only for MVP, code highlighting
- **Server-Sent Events** — event streaming without WebSocket, simpler
- **Step visualization** — plain CSS + JS, no need for D3
- **No React** for MVP — static HTML + vanilla JS

### 7.5 Examples that should be included

| Example | What it demonstrates |
| --- | --- |
| Booking Saga | Compensations, Saga pattern |
| Payment Flow | Context between steps, data types |
| ETL Pipeline | BigData, writing to S3 (once DataFusion is ready) |
| Multi-language | Go activity called from a Rust workflow |

### 7.6 Time estimate

| Stage | Time |
| --- | --- |
| Backend SSE endpoint + precompiled examples | 2-3 days |
| Frontend Monaco + step visualization | 3-4 days |
| "Fail Step" + compensations in UI | 1-2 days |
| Rate limiting + sandbox (separate namespace) | 1 day |
| **MVP total** | **~1.5 weeks** |

---

## 8. Monitoring UI

Needed once the first users appear in production. Without it they are blind.

### 8.1 Minimum set of pages

```text
/dashboard
  └── List of namespaces, overall statistics

/namespaces/{id}/workflows
  └── List of workflows with run counts

/workflows/{id}/runs
  └── Table: run_id, status, start time, duration

/runs/{id}
  └── Step graph + logs + timeline
      ┌─────────────────────────────────────┐
      │  reserve ──▶ charge ──▶ email ──▶ sms│
      │    ✓ 8ms     ✓ 23ms    ✗ FAILED      │
      │                        ↩ compensate  │
      └─────────────────────────────────────┘
```

### 8.2 What to show on the run page

- Step execution timeline (like a Gantt chart)
- Logs with filtering
- Input/Output for each step
- "Retry" button for failed workflows
- "Cancel" button for running workflows

---

## 9. Priorities and order

### Phase 1 — Foundation (4-6 weeks)

Without this the product cannot be shown to anyone.

```text
[ ] 1.1 Fix write lock → parallel workflow execution
[ ] 1.2 Enable async WASM execution
[x] 2.1 Event log — workflow_events table (SQLite locally, PostgreSQL in prod)
[x] 2.2 Checkpoint state blob — host saves state on every send-result
[x] 2.3 Workflow statuses (running, completed, failed, suspended)
[ ] 2.4 Replay logic in SDK — on continue-workflow SDK skips already completed steps
```

### Phase 2 — WOW moment (2-3 weeks)

This is what you show to investors and early users.

```text
[ ] 7.1 Playground MVP (precompiled examples + SSE stream)
[ ] 7.2 "Fail Step" button + compensation visualization
[x] 4.3 Local mode with no dependencies — xarxa workflow run (wasmtime directly)
```

### Phase 3 — Ecosystem (4-6 weeks)

This is what makes the product complete.

```text
[ ] 3.1 run-activity in WIT + host routing
[ ] 3.2 HTTP endpoint for uploading activities
[x] 4.1 CLI — xarxa plugin init, build; xarxa workflow run, --continue; xarxa server start
[ ] 4.2 xarxa workflow logs <run_id> — view events from workflow_events
[ ] 4.3 xarxa workflow list — list all runs from SQLite/PostgreSQL
[x] 5.1.1 proc-macro #[workflow] + xarxa_entry_point!() — boilerplate removed
[ ] 5.1.2 ctx.run_activity — call an activity from another .wasm (nice to have)
[ ] 5.2 Go SDK (TinyGo)
[ ] 8.1 Monitoring UI MVP
```

### Phase 4 — Unique features (6-8 weeks)

This is what nobody else has.

```text
[ ] 6.1 data-engine WIT interface
[ ] 6.2 DataFusion + S3 object store on host
[ ] 6.3 Streaming write (multipart S3 upload)
[ ] 6.4 SQL query from workflow
[ ] 7.5 ETL example in playground
```

---

## Positioning

**Who it is for:** platforms that want to give their users the ability to upload and execute their own code — in an isolated, reliable way, with built-in analytics.

**One sentence:**
> Upload your code. We handle isolation, durability, and compensation. Any language that compiles to WASM.

**Why not Temporal:** Temporal will not let a client upload an arbitrary workflow — there is no isolation. Xarxa solves exactly this through the WASM sandbox.

**Why not Airflow:** Airflow is a DAG scheduler with time-based scheduling. Xarxa is a durable executor for long-running business processes with user-provided code.

**Unoccupied niche:** a multi-tenant workflow platform where code isolation + durability + built-in analytics are first-class requirements.
