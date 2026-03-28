---
paths: ["src/commands/**", "tests/**"]
---

## CLI output conventions

- **stdout**: data only — must be parseable by scripts
- **stderr**: lowercase message (no period, no tool-name prefix)
- **Compound clauses**: joined with `,` — `"cannot X, skipping Y"`
- **Errors**: lowercase, no period, "cannot" not "failed to", actionable ("use --force")
- **Exit codes**: 0 success, 1 error, 2 usage (clap)
