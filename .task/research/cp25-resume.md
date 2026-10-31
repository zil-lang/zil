Doing
# CP25 resumed: measured incremental compilation

Parent: B0 / research/compile-performance. User authorization: 2026-10-31 zil编译6 request to continue compiler performance work, study ZIT/Rust/Blaze, and publish recoverable results. Frozen research baseline e7b492c6e04128e6106bbf29c0d6c7b381556d77; observed main 42ce8081a2aa5722a14fea53f23f819f46685a60. No main write or merge is authorized by this slice's acceptance.

Status: active investigation. CP25-FEASIBILITY.md explicitly contains no performance measurements. Do not relabel CP21/22/23 data as CP25. Restore precise source and existing pinned Rust/LLVM tools, inspect actual function/module boundaries, implement a bounded candidate, preserve original safety/diagnostics/budgets, and run differential correctness before paired complete-request timing. Do not adopt a scheduler-only change as fine-grained incrementality.

Initial write set: this task, docs/research/compile-performance/CP25-RESUME.md, tools/research/compile-performance/cp25-*, .github/workflows/compile6-recovery.yml. Production write set must be added here before implementation. Current environment has no subagent invocation tool: any local or CI parallel jobs are processes, not independent reviewers. Independent review remains NotRun until a separate reviewer exists.

Recovery workflow: contents:read, checkout without persisted credentials, pinned actions reused from existing repository, repository installer with archive verification. Artifacts contain tracked source and toolchain files only, never credentials or provider tokens. Recovery is not a performance test. Source archive and raw commit metadata permit exact tree verification; no claim of recovering all parent history.

Space page editor is unavailable in this tool environment. The original page page_5799fccae05c8191b25b68c26ec7a4ab has NOT been edited. Temporary live record is .task/research/compile6-live.md and the conversation attachment; it is not the requested Space page. Update only this slice's row during active execution and before permission-sensitive actions. No unattended heartbeat or background-work claim.
