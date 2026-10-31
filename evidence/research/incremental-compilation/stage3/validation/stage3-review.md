# Independent stage-3 correctness review

Verdict: **Pass**, scoped to `a4bec09fbaff29a353ad21571e183166de0968dc` relative to `240bb37d806be81f82360e626d74af1844e973fc`. Reviewer: `/root/review_incremental_slice1`; reviewer did not modify repository source. This is not performance/product acceptance.

The fast path requires the complete current Types identity and private process-local raw TypedProgram, and starts after current source-cost and artifact-path protections. Full database closure validation and the original ordered query shape precede reuse. The original Analyze and independent Verify path remain mandatory when requested.

- Explicit ProjectSources only: loaded_syntax None, stop_after >= Types, emit_dir/cache_dir None. File-system discovery, early stage stops and requested intermediate/disk-source outputs retain original path.
- Current Driver cancellation/profile, validate_sources module/file uniqueness, source/path/map costs and ArtifactPlan::project physical destination/source protections execute before reuse.
- Current Types key binds ordered project root/module/file/path/source/import-map fingerprint, all 11 Limits fields, build_inputs/rules/schema and target. Native output/entry/backend/optimization options are consumed later on the unchanged finish_typed path.
- Raw ProjectTyped is private process-local immutable state. Database::read checks complete transitive dependency presence, observed fingerprints and active/cyclic state; then successful Types -> CT -> either original target-independent or target-dependent Expand -> ordered module Parse shape is checked. Missing/incompatible metadata falls back; no disk receipt alone authorizes this path.
- Only exact previously successful frontend computation is skipped, so original Parse charging, import graph/CT selection/evaluation, entry contract and type acceptance are reused at the identical logical budget. Distinct quotas take distinct Types keys; deterministic first-error and quota controls compare with fresh.
- Replayed Parse/Expand/CT/Types event order has before/after cancellation checks and only touches existing raw owners. It does not materialize evicted Parse or introduce recency-only owners. Repeated fast requests retain the existing bounded raw working set.
- Analyze retains original query behavior; every request progressing through Verify increments verification_runs and calls independent verify_shared_program_module. Internal raw schema corruption remains E-VERIFY-SCHEMA, not cached verification authority.
- No production modification outside session.rs/project.rs in this commit; the test/proposal/design/task/limited-runner additions remain inside the assigned research scope.

## Independent execution

- New integration target: 5 passed, 0 failed.
- Internal missing-receipt/raw-schema fault control: 1 passed, 0 failed.
- External public-API probe: Types-only to Verify, actual cross-module selection/CT evaluation, each of the 11 logical budget dimensions, exact/N−1 token (724/723), node (4645/4644) and CT-step (27/26) controls, and current hardlink-source alias protection all passed. Each applicable result was compared with fresh status/phase/diagnostics/interfaces/Verify count/event-phase order.

Commands, exact source and evidence hashes, toolchain, outcomes and probe source are in `stage3-review.json`. The review's CPU tests completed before permission to start the next measurement window was communicated. No benchmark was run or speed claim approved.

## Identity and limits

At review start the tree was clean at the reviewed SHA. After the 5-test run ended at 04:33:59 UTC, the constructor appended a sixth real-corpus test at 04:34:29 UTC and rebuilt that test binary. The original five tests and both production files remain identical to the reviewed commit; this receipt does not claim execution of that additional test. Committed and current test-source hashes are separated in the JSON.

No blocking finding. No formal full suite was run. Constructor's 492-test report is not the independent count. Raw capacity remains a count limit rather than a byte/metadata bound. Exact unchanged-input reuse does not provide semantic incrementality for edits, and source-key hashing/private-cache integrity retain their existing assumptions. Prior stage-2 evidence is not rebound to this implementation.
