# 211 — allocation/lifetime efficiency, not a new safety checker
Baseline: GitHub main 6aa9b9b2336d5bb754f6c17cd9054da565c5c559.
Restored release CLI SHA256 f825cf7610532f69cdbc13bcf849b20afd5b6b2a7628b71913198b1ef212ffc7.
Research comparisons use the same LLVM17.0.6, Rust1.99.1, source snapshots,
O0, 8 CGUs, 4 workers and four allowed logical CPUs. Keep inherited project
weights 5/15/30/30/20 and request weights 10/35/40/15; weighted absolute
costs before ratios. Retain failures, missing coverage and adverse results.
Each process still runs independent ZIL Verify. No object caching or no-op claim.
After implementation and correctness tests, commit the candidate and freeze binaries,
fixtures, environment and order before primary timing; no concurrent heavy jobs.
Compare native output and final machine/text artifacts. Check cancellation/failure controls.
Independent review and product acceptance are NotRun. Re-read remote main immediately
before packaging. Export only normal descendant history; never force-push.
