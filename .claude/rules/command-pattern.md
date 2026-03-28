---
paths: ["src/commands/**"]
---

## Command pattern

Most commands follow a common shape: locate the admin repo, create a `Git` interface, list worktrees, find the target, perform the action. Read any existing command in this directory for the concrete API. Some commands deviate — exceptions are documented in [ARCHITECTURE.md](../../docs/ARCHITECTURE.md).
