---
paths: ["src/commands/**"]
---

## Command pattern

Every command follows this shape:

1. `Git::find_repo(repo_arg)` to locate the admin repo
2. `Git::new(admin_dir)` to create the git interface
3. `git.list_worktrees()` → parse with `worktree::parse_porcelain()`
4. Find target in `Vec<Worktree>`, perform action
5. Return `Ok(())` or `Err(message)`
