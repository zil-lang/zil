# G9.3.1 backend implementation

Parent G9.3.1; author `/root/general_text_backend`; base
`967e6e74a546acf21f18acd24e91ce8318361344` (shared schema43).
Dispatched Lower/LLVM and explicit VM consumer handling by root.

Write set: `crates/zil-lower/src/**`, `crates/zil-backend-llvm/src/**`,
`crates/zil-ct/src/program.rs`, this task note, corresponding changes and
focused evidence. Shared IR/schema/fingerprint ownership stays with root.
Contract: `changes/G9.3.1-backend-static-utf8.md` and the shared parent proposal.

Validation pending implementation: existing backend unit target and driver
native target coordinated with parent author; exact Unicode/NUL/empty bytes,
slice lengths, stale raw LIR, private-global CGU/cache identity, current-request
budget controls. No new target directory; no heavy build without root slot.

Rust goals: this slice adds ordinary source/machine expressibility without an
implicit runtime allocation. No Rust runtime/RSS/compile or framework-cost
comparison, complete language safety, or product-level superiority is claimed.
