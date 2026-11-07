# Independent 336–339 review inputs

These inputs were prepared independently of the implementations. The source
expectations were fixed against the frozen `dde8be301` profiles before candidate
execution. They are review cases, not new language specifications.

`expectations.json` contains the original source matrix. `baseline.json` records
the actual older B CLI results. `baseline-controls/` deliberately removes life
annotations or replaces only the child thread lane with the old scalar lane;
those altered controls check fixture setup and existing behavior, not the new
capabilities. Their results are in `baseline-controls.json`. The initial nested
writer used an immutable callback binding; both original invalid inputs remain
as `thread-nested-writer-setup-invalid.zil`. The corrected case uses `var inner`.

337 is independently reviewed at final commit
`97fb6e7686a01a1715a639d494c45048773fc6e1`. Its production and tests are identical
to the executed `51788f2b95d22394967ef102ad68b8976135596c`; only the author's two
result documents differ. `candidate337.json` records four positive, five safety
negative and one Unsupported source result. The unchanged no-life unused generic
alias gap is a separate KnownIncomplete observation and is excluded from passing
coverage. `native337.json` records two original positive inputs across both
backends and O0/O2, eight actual builds/executions, each printing `0` and exiting
zero. API runs used the builder with verification and no fallback.

The exact source commands are retained in `candidate337.json`. Native commands
used the same candidate CLI and this form with a fresh task temporary directory:

```sh
source /workspace/.zil-env/env.sh
/workspace/.zil-env/target337/debug/zil build SOURCE --entry main \
  --backend BACKEND --opt OPT --emit-dir TEMP/phases -o TEMP/program --json
TEMP/program
```

`raw.rs` is a standalone independent Rust test harness, not a new Cargo target.
It starts from the retained real ZIL sources, then mutates actual TypedIR operands
or contracts while retaining a correctly fingerprinted untrusted candidate.
Exact `rustc` arguments, linked rlib hashes and the three successful 337 test
results are in `raw337.json`. Both raw and Wire forms are tested. The two 339
tests are prepared but were filtered out on the 337-only candidate. A raw return
type mismatch deliberately records Analyze candidate success followed by Verify
rejection; Analyze is not claimed to duplicate complete raw instruction typing.

The 337 review conclusion is in `https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review337-astra.json`. That
receipt's tested harness remains unchanged at review commit `55c40de63` and in
the current `raw.rs`; later 339 attacks are in a separate `raw339.rs` that reuses
the two prepared tests once.

339 is independently reviewed at final commit
`7423b4946e3787aa2a60764b473cde8307919bb7`. Production and tests are identical to
executed `4d7c8b020c595f745a25c060bac7242532548e63`; only three result documents
differ. The original 582312 candidate's failure receipts remain unchanged:
`boundary339-initial.json` records ordinary parent `identity(&x)` rejected by an
overly narrow producer walk, and `helper-return339-initial.json` records a lost
Analyze exclusive root after a mutable-return helper. Verify rejected the latter
unsafe source throughout; it was never executed natively. Completed independent
Verify facts and complete plain mutable-address origin propagation close those
findings on the original source bytes.

`candidate339.json` contains 20 source cases: ten Success, seven safety rejects,
one worker-profile reject, and two explicit Unsupported origin cases. The
additional Analyze-only command rechecks the original helper-return conflict;
it is not another source count. Earlier nonowning Store-capture probes stop at
existing Unsupported boundaries and are not counted as safety proof. A supported
`give Store` helper instead rejects actual owner transfer while its shared
pointer remains required by the same invocation. Safe helper/capture retention
cases establish the corresponding accepted preconditions.

`raw339.json` records three source-derived raw/Wire attacks and one exact budget
test. Actual new-source boundaries are Types 557, Analyze 6544 and Verify 20261;
N-1 returns ResourceLimit and fresh N succeeds. All linked rlib hashes remained
unchanged across the standalone build/run. Three older 337 tests were filtered.

`native339.json` separates the existing author's pthread observer replay
(four builds, sixteen executions) from three independently authored source
consumers (twelve builds/executions). Both backends and O0/O2 pass; API O2's
observer build uses two actual CGUs. The observer verifies real child-thread and
pointer identity, delays child reads until parent body cleanup, and exercises
normal/business/create-failure/join-abort outcomes. It does not implement the ZIL
worker or grant transfer authority. Exact commands, source and artifact hashes,
backend reports and process observations are retained compactly.

The 339 conclusion is in `https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review339-astra.json`. Complementary
resource-lender/caller precondition review `b9177c458` is referenced separately;
its counts are not added here. No full-workspace result or Rust product
comparison is claimed. Original sources and compact receipts are retained;
temporary machine artifacts were hashed and removed.
