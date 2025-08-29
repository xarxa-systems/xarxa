# Xarxa — Durable WASM Workflow Orchestrator

**Xarxa** is an open-source **durable workflow engine** written in Rust, designed
for high performance, reliability, and secure isolation. Inspired by systems
like Temporal, Xarxa brings durable orchestration to a modern **Rust + WASM** runtime.


## Submodule

This project depends on a shared repository called **WIT** (WIT interface definitions).  
We use a Git submodule to ensure every project (`engine`, `sdk`, etc.) is tied to an exact version of the wit contracts.

### Clone the repository
When cloning this repo, make sure to initialize submodules:

```bash
git clone --recursive git@github.com:you/engine.git
# or if already cloned
git submodule update --init --recursive
```

### Update WIT to a specific version

Checkout the desired tag or commit inside the contracts folder:
```bash
cd contracts
git fetch --tags
git checkout v1.2.0
cd ..
git add contracts
git commit -m "update contracts to v1.2.0"
```

### Rollback contracts

If you need to use an older version:
```bash
cd contracts
git checkout v1.0.0   # or a specific commit SHA
cd ..
git add contracts
git commit -m "rollback contracts to v1.0.0"
```

### Sync contracts after pull

If someone else updated the submodule reference:
```bash
git pull
git submodule update --init --recursive
```