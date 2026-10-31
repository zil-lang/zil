# Frozen stage-5 tool/protocol review

Verdict: **Pass for measurement readiness**, exact `283c661bcb18978fe048770dace4994ade1910d2`. Production remains byte-identical to independently reviewed `801933a9e8f060d44b8305786d01399527aa6137`. Reviewer: `/root/review_incremental_slice1`. This was read-only source/protocol review: no tests or benchmarks rerun, and no performance/product verdict.

The two tools and DISTRIBUTION-PROTOCOL identities are recorded in `tool-protocol-review.json`. The declared five windows contain 4,140 + 1,380 + 960 + 1,008 + 336 = **7,824 measured requests / 652 cells**, each with six observations per arm, plus **28 Rust O0 oracle builds**. The generated corpus has 23 families; real corpus has 14 independent source pairs and 15 declared input vectors per successful native executable.

The prior real-oracle validity gap is fixed. Check/build both depend on a successfully compiled oracle with complete non-error, non-signal outcomes. Measured native outcomes must independently be valid before exact exit/stdout/stderr equality. Legitimate positive application exits remain allowed. The app-specific Rust `--check` argument matches the checked-in wrapper comment and ZIL ordinary main: each checks the exact same 475000738663 result with no ordinary output. Sources are unchanged.

Generated output checks full four-byte integer results. Source edit identities, fresh per-sequence caches, stale executable removal, report Verify=1, failures and null ratios remain enforced. The implementation ablation labels its old ZIL arm `baseline`, not Rust. Closed-executable relinquishes generated symbol exports explicitly and has its own windows. Default budget failures, including the known body/types large cases, remain part of the corpus.

AB/BA order, direct rustc/metadata boundaries, different LLVM versions, source-cache limitations, unchanged OS page cache, real corpus's limited representativeness, and subprocess timing/resource interpretation are explicit. Full outcome tables and adverse results remain required.

Formal commands must pass `--blocks 6` explicitly (script default is 5), avoid `--smoke` and main-window `--source-cache`, and use the real rustc binary. The final data audit will independently check exact counts/identities, native/report controls and statistics; this receipt does not preapprove future measurements. All independent reviewer CPU tests have ended.
