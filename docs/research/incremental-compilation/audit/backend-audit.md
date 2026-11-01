> 2026-10-31 开工只读审计的冻结记录，保留原分支/SHA/当时路径作为来源。本文不是当前实现状态；已修项目和最终实测见 ../SLICE1.md、../SLICE2.md 与 ../RESULTS.md。

# ZIL backend audit, 2026-10-31

Read-only audit of main 21058d0e331de764e3a1b9123c3e1edf5e55a1aa and research/compile7-20261031 4b9ae3826a2d60102dcbb68f01bb76db5ea0532f. No repository source changes or formal full benchmark runs.

## Confirmed object cache invalidation amplification

Main `crates/zil-backend-llvm/src/machine_cgu.rs:33-54` places all nondefinition items into a global prelude, `:152` clones that prelude to every CGU; `object_cache.rs:201-216` hashes all items. Compile7 retains the same behavior at machine_cgu.rs:73-93 and :185-192. Necessary cross-unit function declarations are selected specifically, but the pre-existing foreign declarations and named types are not filtered to each unit's reachable symbol/type closure.

Mechanism probe in `backend-probe/` uses existing debug CLI, Rust 1.99.1 and LLVM 23.1.4, four source modules, requested eight units/two jobs, actual four units:

| request | cache hits / units | native stdout |
|---|---:|---|
| cold | 0/4 | 42 |
| unchanged | 4/4 | 42 |
| c.value body return 12 -> 13 | 3/4 | 43 |
| restore | 4/4 | 42 |
| append unused foreign declaration to c | 0/4 | 42 |

Raw JSON: `cold-valid.json`, `unchanged-valid.json`, `private_edit-valid.json`, `restore-valid.json`, `unused_extern-valid.json`. Final fixture sources in `backend-probe/src`. Each successful run completed current Verify and native execution. The prepartition LLVM text differs only in verified-fingerprint comment and one added `declare i32 @not_used_in_program(i32) nonlazybind`; all original function IDs and bodies remain unchanged. The object-key implementation already omits the diagnostic fingerprint. Thus this probe isolates prelude pollution, not position/ID renumbering. This is a mechanism counterexample, not a performance benchmark. Initial fixture omitted pub and correctly SafeRejected; original failed JSON was retained with names without `-valid`.

First correction experiment: collect transitive referenced symbols and named types per unit; preserve ABI attributes, target, type-layout closure and pointer identity semantics. Run this exact counterexample before/after, then mutations to a referenced foreign signature/type, nested named types, local linkage/address-taking, and cancellation/publication. Do not normalize away ABI or instruction order. Stable DefId and layout/effect summaries remain a separate upstream requirement.

## Cache lookup occurs after global work

Main build.rs:182-187 always reconstructs machine IR and renders full LLVM text before partition/cache lookup. :285 writes text even API backend uses structural model; :366-372 relinks every LLVM API build. Hit path :565-575 rechecks current machine model and copies object to staging. Compile7: build.rs:218-224 still lower/render, :481-493 still link. CP13 closed-executable means internalization/CGU policy, not cached final executable. CP12 query cache retains unverified Typed+Candidate for exact input and resumes fresh Verify; it does not remove current verification, Lower, machine creation or link.

Main driver/session.rs:416-421 clones TypedModule and CandidatePlan into Verify. zil-lower/src/lib.rs:122-130 deep-clones functions/resources/closures into public LIR. backend/emit.rs:351-362 compares LIR contents against VerifiedModule by deep equality. Compile7 still has these at session.rs:461-468 and same lower lines. CP35 is a separate candidate implementing Arc shared raw Verify input, and must not be called absent or already accepted; it does not itself eliminate all downstream traversal.

Consequences: object-cache hit is not delta compilation, and front-end memo hit count alone cannot produce sublinear edit latency. Current full independent check is an intentional safety contract. Keep it while first removing duplicate ownership/encoding. Longer-term independently checked per-item proof receipts or summaries require explicit architecture work; reusing old verification authority or using an object hash as proof is invalid.

Potential sequence: private immutable verified LIR view for normal internal path, keep public raw-LIR validation path; demand-driven diagnostic rendering via explicit artifact contract; verified current-input link-action cache bound to exact object bytes and complete linker/startup/library/tool/flags identity; then stable per-item machine actions driven by front-end dependency results. Link caching alone only improves unchanged builds, not arbitrary edited program linking. Check a cost waterfall and Amdahl bound before choosing each workstream.

## Stable source/ABI identity and CGU granularity

Main machine_cgu.rs:69 names symbols zil.fn.{FunctionId}; :74 groups all same FileId functions; :116 bucket owner is FileId % count. Compile7 corresponding :109, :113-118, :156. Insertion/reordering may perturb numbered identities globally. All source-file functions are joined regardless of dependency locality; common local definitions can merge source groups; arbitrary bucket count changes may invalidate wide swaths of cached code. The existing object cache is useful for private body changes in stable layouts, but its benchmark coverage must include insert/delete/move/unrelated declaration, type changes and generic instance creation rather than only numeric body edits.

Improvement hypothesis: stable module/item identity separate from current dense IDs and spans; semantic ABI/layout/effect/body facets; stable CGU ownership from module/item/SCC identity; reachable declaration closure. Preserve closed-world/O2 optimization quality and measure full native runtime—more CGUs can destroy inlining or enlarge repeated preludes.

## Additional algorithmic risk to validate

zil-llvm-api/src/preflight.rs:53-102 recursively expands NamedType fields using only current DFS stack (cycle detection), with no completed-node memo. :756 repeats from each named type. A machine type DAG T_n={T_(n-1),T_(n-1)} has O(n) unique definitions but can induce O(2^n) visits. Depth 128 does not bound repeated branches. This is source-level algorithmic analysis; no claim that this exact DAG is reachable through current source lowering. Next cheap experiment is direct machine-IR control plus actual-source reachability. A safe memo must preserve cycle errors, maximum depth constraints and accounting; merely caching `valid=true` can hide a deep call path.

## Prior evidence and interpretation

Compile5 (research/compile-performance COMPILE5-RESULT.md) already reports exact cold ratio ZIL/Rust 0.818380, increment mix 1.312799, private/public 1.664037/1.713892 and O2 1.047511, on a bounded five-case mainly scalar corpus. These are prior branch measurements, not this audit's reruns. Compile7 reports CP29-33 only through Verify, not AOT or Rust comparison. CP32 reused 6940/9222 functions in its largest private-edit corpus but overall default mixture was 1.003583 and weighted RSS 1.202026, demonstrating retained representation and fixed full-project work dominate naive memo benefits. Do not compound old 211/212/compile5 percentages or present these as same-code baseline.

Rust four product goals remain unproven by this audit. The concrete new result is cache invalidation amplification with an output-preserving unused declaration, and an architecture path whose gains still require measured evidence.

## Exact probe command and fixtures

Activate `. /workspace/.zil-env/env.sh` in `/workspace/zil`. Build command (executed once per edit):

```sh
/workspace/.zil-env/target/debug/zil build \
  /workspace/.zil-env/research/backend-probe/src/main.zil \
  --entry value --codegen-units 8 --codegen-jobs 2 \
  --object-cache /workspace/.zil-env/research/backend-probe/cache \
  -o /workspace/.zil-env/research/backend-probe/program --json
/workspace/.zil-env/research/backend-probe/program
```

For reproduction use a fresh external cache directory for the cold row. Baseline main.zil is `import a\nimport b\nimport c\nfn value()->i32{return a.value()+b.value()+c.value()}\n`; a.zil/b.zil/c.zil each contain `pub fn value()->i32{return N}\n`, respectively N=10/20/12. The first edit changes only N=12 to 13; restore returns it to 12. Last edit appends `extern "C" fn not_used_in_program(x:i32)->i32;\n` to c.zil. Successful outputs retained in JSON and described above. No performance timing claim is made from these short runs.
