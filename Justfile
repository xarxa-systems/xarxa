# List available commands
default:
    @just --list

# ── Install ───────────────────────────────────────────────────────────────────

# Build and install xarxa binary to ~/.cargo/bin
install:
    SQLX_OFFLINE=true cargo install --path . --locked
    @echo "✓  installed: $(which xarxa)"

# Remove local SQLite database (clears all runs and events)
clean:
    rm -f ./xarxa.db
    @echo "✓  xarxa.db removed"

# ── Plugin ────────────────────────────────────────────────────────────────────

# Scaffold a new plugin project
# Usage: just init my-booking
init NAME:
    xarxa plugin init {{NAME}}

# Build the booking example and copy to plugins/
build-booking:
    cd examples/booking-workflow && xarxa plugin build --out ../../plugins

# ── Local run (no server, no auth) ───────────────────────────────────────────

# Run a workflow directly via wasmtime (interactive: picks plugin + prompts input)
run:
    xarxa workflow run --pretty

# Run booking workflow with preset input (wasmtime, no server)
run-booking:
    xarxa workflow run booking \
      --input '{"flight_id":"AA123","user_id":"user1","amount":99.0}' \
      --pretty

# ── Server ────────────────────────────────────────────────────────────────────

# Start local server (SQLite, no JWT, hot-reloads plugins/)
serve:
    xarxa server start

# Send a workflow request to the local server with custom JSON input
# Usage: just post booking '{"flight_id":"AA123","user_id":"user1","amount":99.0}'
post PLUGIN INPUT:
    #!/usr/bin/env bash
    curl -s -X POST http://localhost:4000/api/workflows/{{PLUGIN}}/start-workflow \
      -H "Content-Type: application/json" \
      --data-binary "{\"workflow_name\":\"{{PLUGIN}}\",\"input\":$(echo '{{INPUT}}' | jq -Rs .)}" | jq .

# List recent workflow runs from the local server
runs:
    curl -s http://localhost:4000/api/runs | jq .

# ── Remote workflow (against a running server) ────────────────────────────────

# Run booking on a remote server
# Usage: just run-remote SERVER=http://prod:4000 TOKEN=<jwt>
run-remote SERVER TOKEN:
    xarxa workflow run booking \
      --input '{"flight_id":"AA123","user_id":"user1","amount":99.0}' \
      --server {{SERVER}} \
      --token {{TOKEN}} \
      --pretty
