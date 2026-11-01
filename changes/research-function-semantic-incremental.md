# Function-scoped Types work products

Stage 15 proposes a compiler-bound, optional trusted-local cache of successful
scalar function CFG construction, usable by a live CompilationSession and by
`--semantic-cache` across processes. This is a Types computation cache, not a
Verified value or a source-language extension.

The initial reusable domain consists of ordinary scalar function signatures and
bodies with direct ordinary calls, scalar expressions and structured control
flow. Foreign declarations (including ABI/attributes) remain in the declaration
context but are never reused. An unsupported body, such as an unsafe I/O main,
follows the original builder without disabling unrelated eligible scalar helpers.
Resource/template/aggregate headers conservatively disable the whole epoch. The complete
current declaration/import/visibility/FunctionId universe, target and all Limits
are bound independently from each function body. A body key includes its full
structure and relative spans, but not allocation-only Core NodeIds. Current
spans are rebound by the current function origin. Declaration/ID shifts,
unsupported effects, closures, resources, templates, residual CT or aggregate
shapes conservatively fall back. Expand, CT, global signature/effect/visibility
checks, Analyze and independent Verify remain current. Private body edits can
reuse unchanged function CFGs without pretending that the whole Types query was
reused or that general semantic dependency tracking is complete.

A successful builder records its exact changes to all shared closure-state
budget counters. A hit applies those deltas only if they fit today's remaining
allowance. Otherwise the original builder runs, retaining its original first
error and current span. Per-function checks use identical complete Limits.
Only a complete successful Types epoch replaces the prior in-memory entries;
driver cancellation rolls this back, and disk publication occurs only after
successful downstream processing and current artifact guards. Cache corruption
or unsupported work products is an optional miss. Public Wire/schema, default
budgets and language acceptance are unchanged.

Controls must compare edited incremental output and diagnostics with a fresh
compiler for same-length and shifted-span private bodies, cross-module calls,
signature/visibility changes, function-ID shifts, low budget fallback and
unsupported-effect fallback. Persistent receipts remain explicitly trusted local
producer data like the existing semantic cache, with a compiler identity and
bounded integrity envelope; they do not authenticate a malicious writer.
