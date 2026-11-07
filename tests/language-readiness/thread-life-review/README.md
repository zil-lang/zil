# Thread, life and origin review inputs

`expectations.json` binds the Source matrix; `baseline.json` and `baseline-controls.json` are frozen observations, not current acceptance. The baseline controls remove life annotations or substitute the old scalar child lane to check fixture setup. `thread-nested-writer-setup-invalid.zil` preserves the initially immutable callback binding; the valid writer control uses `var inner`.

The finite Source controls cover parent and child borrowing, generic life origins, helper return roots, retained captures and actual Store responsibility. A generic alias without a declared life relation remains an incomplete profile, not a successful negative. A shared pointer held by one invocation prevents transferring its Store owner during that invocation; safe helper/capture retention controls show accepted preconditions. Nonowning Store-capture probes that stop at Unsupported are not safety proof.

`raw.rs` and `thread-provenance-raw.rs` are standalone independent harnesses using real Source-derived TypedIR and Wire mutations with recomputed untrusted fingerprints. Analyze candidate acceptance never substitutes for independent Verify rejection. Raw return-type mismatch, mutable-return helper origin, producer traversal, and budget N/N−1/fresh boundaries are separate controls. The pthread observer checks child-thread identity, pointer identity, delayed child read until parent cleanup, and normal/business/create-failure/join-abort outcomes; the observer itself grants no worker or transfer authority.

To replay native cases with an exact compiler, use a fresh output directory and the backend/optimization matrix; bind each command and binary to its reviewed source commit:

```sh
/absolute/path/to/zil build SOURCE --entry main \
  --backend BACKEND --opt OPT --emit-dir TEMP/phases -o TEMP/program --json
TEMP/program
```

The [shared-life review](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review337-astra.json) and [origin review](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review339-astra.json) preserve exact candidate SHA, source and native counts, backend reports, raw/Wire results, original failures and unmeasured boundaries. They do not combine into a full-workspace or Rust product result.
