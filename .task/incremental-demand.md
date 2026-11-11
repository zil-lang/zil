# Incremental producer continuation

User requested a new research branch based on the latest responsive-query
experiment and the prior point-to-point propagation hypothesis.

Base: `ea0ce45741fde0082b6c92173b7480b167c888d4`.
Branch: `research/incremental-demand-propagation`.
Main remains untouched. This is research, not default enablement or main acceptance.

The base profiled scalar 20×20×16 request contained 22 files / 498620 bytes.
Private-edit B (producer enabled, graph disabled) spent 182.856 ms in
materialization, 131.335 ms in producer assembly, 170.726 ms in combined
semantic/function cache publication, and 6.980 ms in actual producer execution.
One profiled block is diagnosis only. Raw data: `out/runs/ea0-profile`.

Implementation:

- Retain immutable CoreModule and SourceMap Arcs through producer assembly.
  Raw transformations explicitly use copy-on-write; owned public boundaries
  unwrap unique allocations. No wire schema/version change.
- Replay successful scalar materializer module receipts bound to sealed parse
  owners. Current global environment admission still executes. The original
  module walk handles insufficient work, temporary bytes, or depth domains.
- Keep Analyze and current independent Verify. The graph receives no credit
  for common representation or producer improvements.
- Split optional publication profiling into semantic candidate and function
  cache costs to identify residual work without altering cache policy.

Validation requirements: actual pointer sharing; fresh/warm full TypedProgram
and diagnostic equality; changed-width/batched edits; current low-budget first
diagnostics; failed epoch recovery; rich fallback; real O2 execution; paired
unprofiled comparisons against the frozen base and A/B/C attribution.

Known initial failures are retained: baseline offline build lacked cached
`rustc_apfloat` and succeeded after ordinary locked dependency download; first
Arc all-target check exposed a double Arc in a unit test and old integration
constructors/mutations. These are not successful test runs.

Only one heavy build or measurement window runs at a time. Source builds use
the detached `/workspace/zil-incremental-validation` tree and its own target.
All new runs have unique directories. No reset/clean or edits in a building tree.
