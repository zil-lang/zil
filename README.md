# Source history archive

This branch preserves source ref tips and staged/working-tree snapshots as commit parents. It is not accepted source or a test result.

Restore a preserved commit with `git branch NAME SHA`; inspect manifest.json for exact source refs, worktrees, exclusions, and WIP commits. Older ref names remain in prior manifests via `git log --first-parent archive/source-history`. Push and verify this archive ref before removing any old source ref.

## Compiler multi-edit continuation, 2026-11-11

[Results, exact sources, scripts and complete evidence](research/compiler-multi-edit-20261111/README.md). Both candidates are rejected; unchanged-main concurrency is diagnostic evidence. Preservation and branch retirement do not imply production acceptance.
