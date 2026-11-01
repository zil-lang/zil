# Independent Stage12 review

**Scoped code, control, statistics and finite-artifact review: Pass.** Production source is `743c9699eda7b5ef79b0759702cb48a17acfb83d`, parent `f15cf54a43a162f0c13fc11b8ddb26c344fc1315`. The measured baseline is Stage11 delivery `38a7d5a088ecd291255907dfca32e37785b8292c`, production `082efcfc0f8e0408844a22af8bcc34d29c0d9df4`. This is not full-language, full-workspace CI, industrial-workload or product-gate approval.

The non-author reviewer and delegated non-author reviewer read the implementation, existing control logs, measurement records and saved native artifacts. They did not build the compiler, run tests or workloads, invoke native inspection tools, or create measurement samples. Offline calculations used existing records only. This review file is the lead reviewer's sole authorized write; subsequent explicit authorization allowed the delegated reviewer to save a read-only byte-inspection script and its receipt for reproducibility. Final delivery/archive/link binding is a separate check after the packet is complete.

## Code and trust boundary

- Buffered patch fingerprints preserve every skeleton/body Hash input and its order. `digest_reset` flushes the live suffix, clears its used length and replaces the complete SHA state with `Default == new`; `finish` only reads a clone. Function names borrow from the same immutable Module already held by Snapshot. Record capacity uses the previously checked exact length, `140 + sum(36 + 4 * edges)`, without changing bytes, checksum, qualification or the 2 MiB bound.
- The three Analyze/Verify type walks retain the root in an Option before the unchanged LIFO stack. Root/child visitation, attached Analyze state, charges, depth limits, target checks and first errors are unchanged. Analyze and Verify remain independent implementations.
- Prepared scalar shape resolution checks the entire header domain before returning a speculative body error. Its accepted domain excludes nominal declarations, binders, receivers and owning rewrites. Every accepted type/block/expression charge, span and traversal order matches the old Resolver, including shared remaining work across modules. Unsupported shapes restart the original resolver with its original allowance. Prepared ownership remains immutable and bound to the exact Limits; changed inference/CT/limits take the original path. This is current-request allocation avoidance, not persistent Types incrementality.
- Expand's String and borrowed-str lookups preserve identical names, shadowing, privacy errors and canonical-name charges. Restore's omitted lookup previously returned None before any charge or lookup; Self handling, actual restoration, generated imports and recursive traversal remain ordered as before. The four emit changes move single-use owned values into the same events; argument/Unit/environment ordering, fallible expressions and ABI are unchanged.
- Semantic v2 stores the actual adopted raw TypedProgram/CandidatePlan pair atomically using the existing bounded public Wire codec. Private magic, namespace and directory change; public Wire does not. Full project source/path/ID/import-map identity, installed compiler identity, target, build inputs, rules and all Limits remain in the key. Oversized or unsupported optional records only skip caching.
- CachedProgram enters the single existing Analyze query position with Reused disposition and the current Types snapshot dependency, without modifying the AnalysisSession epoch. Current independent Verify still runs and is counted on every attempt. Rejected cached candidates clear imported raw state and rerun source, retaining attempted events and Verify counts; cancellation and downstream native/link failures are not retried. Publication remains after complete success and cancellation/artifact checks. Retrieval for persistence checks the exact Analyze key, live database snapshot and its unique Types dependency key/kind/observed fingerprint before taking the raw Candidate.

## Bounded validation evidence

The original `f15cf54a43` controls passed: 10 object-cache, 5 patch, 12 Analyze budget, 2 Verify, 4 scalar-shape, 6 frontend-preparation and 1 real-source merged-export control, **40 distinct tests**. Their unchanged source ranges remain bound to the final production commit.

Final follow-up logs and command receipts record **19 distinct tests actually executed and passed**: 4 semantic-cache unit controls, 7 source/cache integration controls, 5 module/alias/privacy/budget controls and 3 dynamic/indirect/closure native controls. The reviewer read every effective stdout/stderr and the command receipt, including `--features zil-driver/llvm-api`. Related all-targets Clippy, fmt and diff checks completed successfully. Total scoped controls are 59, not a full-workspace run.

The first follow-up runner omitted the LLVM API feature: three selected test files ran zero tests and Clippy failed on a conditional import. Its unchanged records under `controls-without-api-feature` are retained and **are not counted as successful effective validation**. Corrected commands were rerun.

The new false-claim control preserves Typed and both Candidate identities, but adds an invalid initialized-out local to a function with no locals. It therefore reaches the independent initialization-fact check, rather than merely a fingerprint mismatch. Evidence asserts Analyze Reused then Executed, two current Verify attempts, source reconstruction, no active query and exact repaired pair. The existing forged-Typed control now also retains a matching candidate module fingerprint, preserving substantive rejection. Existing limit/target/source separation, corruption, cancellation and link-once controls remain relevant.

## Final measurement identities and recomputation

The final `selected-target`, `selected-guards` and `selected-closed-guard` manifests all bind the final production source and an empty production working diff. Existing artifacts were independently read and hashed:

| Artifact | Bytes | SHA256 |
|---|---:|---|
| Candidate compiler | 8,339,928 | `725aaa5558107d421c81518c4b97fe4734b7057d7be4f94157ac9e2f04118baf` |
| Baseline compiler | 8,350,040 | `e871f7da42854e29ec4a3118c711e9546cd48ebbb63b712c950930bf0d5d8594` |
| Rust compiler | 21,139,856 | `5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4` |
| LLVM DSO | 72,097,832 | `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98` |

The unchanged wait4 runner SHA is `7216f6a1c7f14d6d37af0e4fe6deffd96fb35c16e9150785660a7a2609481ef5`. Rust is the recorded 1.99.1 commit `58cf8f9c35b355199727627d07ae34a4e2846d67` / LLVM23.1.1; ZIL uses the recorded research LLVM23.1.4 DSO.

An independent calculation checked the complete expected tuple set, uniqueness, exit/timeout/valid/native fields, all 15 cell medians and paired geometric means/ranges, and every CPU/RSS summary. Results match the supplied analysis/resources files. There are **63/63 valid build/native observations, 42 current ZIL Verify runs, 21 Rust observations and 7 separate cache histories**. Target default has three histories; each other case/contract has one. Cold, warm and edit within a history are dependent observations. Rust observations are not shared between contracts/windows. The final windows contain 21 candidate/baseline pairs, not 63 independent speed samples.

| Target default request | Candidate/baseline GM | Paired range | Candidate/Rust GM |
|---|---:|---:|---:|
| cold | 0.920970696 | 0.811333–1.036051 | 0.935653362 |
| warm | 0.832646538 | 0.752309–0.933088 | 0.639911287 |
| edit | 0.949282240 | 0.861040–1.100751 | 1.005902274 |

All three target warm pairs improve. Cold and edit each contain one adverse pair; cold's first baseline is visibly slower than its other two observations. Target edit/Rust pairs are 0.896861, 0.995090 and 1.140459; its candidate/Rust medians are 503.443/493.138 ms. This supports near parity in this workload, not stable superiority. Target CPU ratios are 0.929256/0.807123/0.943519. Edit RSS medians are 198.875→199.996 MiB, warm 135.793→135.445 MiB. wait4 process-maximum RSS is not simultaneous whole-process-tree memory.

Adverse guards remain visible: single8192 cold +0.596%; opaque warm +8.504% and edit +3.454% versus baseline. Opaque warm is 390.168 ms against Rust204.101 ms, ratio1.911639; its cold/edit are much faster than Rust in these individual observations. There are three adverse cell point estimates and five adverse individual pairs versus baseline. Thirteen of fifteen cell point estimates are faster than Rust; neither this fraction nor single-history guards establish general or stable wins/regressions.

All seven candidate warm stdout reports independently show exactly Types Reused, Analyze Reused and current Verify Executed, with one verification run. The final measurement includes real runtime input 7 for opaque; correctness controls cover other behavior separately and are not additional performance repetitions. Default/closed contracts, retained-symbol differences, compiler version differences and explicit O2/incremental/cache configuration must accompany Rust comparisons.

The earlier `final-*` directories are **separate `f15cf54a43` intermediate evidence**, also 63 valid observations. Their target cold/warm/edit ratios are 1.042088/1.033897/0.999465 and show no reliable overall gain. They must not be pooled with `selected-*`. Private profile timings are one-sample diagnostic intervals, including nested and cross-unit timings, not an ablation or complete-request speed proof.

## Finite native quality

Saved quality records cover seven edited pairs with no operational errors. The comparison tool's changes only select/report requested windows; its bounded comparison algorithm is unchanged. All fourteen referenced ELF byte lengths and hashes match the actual saved artifacts. Canonical global symbol sets, kinds and sizes agree; there are no effective-body/size differences and every reported size delta versus Stage11 is zero.

The single8192 pair retains **146 raw instruction-label differences**: the existing single-jump thunks name different private retired symbols whose names contain compiler-dependent action keys. Their addresses, sizes, terminal addresses and effective bodies match; they are not reported as raw-text equality. Stage11's existing retired-body jump cost has not been removed. Reviewer-side read-only ELF section inspection additionally found all seven `.text` payload pairs byte-identical. Opaque and closed entire ELFs are byte-identical; the other five pairs differ only in `.strtab` section payload, with no size growth.

This byte comparison is reproducible from archived `quality/section_byte_inspection.py` (SHA256 `c9260f93292f4b89883a7e4a695434555f142f04f51262f09979ce208ce9800d`) and `quality/section_byte_inspection.json` (SHA256 `69b69a5af8011b50a18a4538300dbec5b1c7e43cdeda25eeedf253432b03549b`). The receipt binds all fourteen input ELF hashes, per-side `.text` hashes/offsets/lengths, the script and full quality-results SHA `e0597f73d552a4262affeb1932df7ef996b953726db4b4df418220a7dc0c3b56`. Whole-file byte comparison confirms every changed byte in the five nonidentical pairs lies inside the matching `.strtab` ranges: there are zero changed header, gap or other-section bytes. Only the Python byte reader was executed to save this receipt, with no new native inspection or measurement.

The disassembly comparator still leaves indirect-call entries explicitly uncompared: two functions for each even/skew/single default pair, one opaque function (`zil.fn.2051`, including indirect getchar/putchar calls), and closed main. Comparable counts are 8201/8203 even/skew, 8193/8195 single, 2050/2051 opaque and 0/1 closed. Exact saved text-byte evidence is finite-artifact evidence; it does not turn these comparator exclusions into general runtime, ABI, address, locality or memory-performance proofs.

## Conclusion and remaining scope

The clearest repeated gain is the target's warm request. Editing remains near Rust, while opaque warm construction/key/preflight work is a concrete unresolved gap even with all eight machine objects hit. The next experiment should isolate that current machine work before selecting another reuse mechanism; these records do not identify which individual Stage12 change caused the opaque regressions. Cross-edit Types incrementality, broader current-version check/O0 coverage, industrial projects and the four-goal product claim remain unproved. Historical graph-cut/zit results are not newly attributable to this patch.

No old successful CI run is counted as final-source CI. Historical handoff completeness Fail remains; main, environment publishing, budgets, Verify authority, public Wire, Cargo.lock and product gates were not upgraded by this review. Final report/archive/delivery binding must retain these limits and all negative records.

## Final packet check

**Packet review: Pass**, bound to production `743c9699eda7b5ef79b0759702cb48a17acfb83d`. The crate diff against that source is empty. The final report, index/task summary, provenance and this review retain matching identities, counts, paired estimates, negative observations and scope limits; no material claim or numeric error was found. All ten report links resolve locally, including the linked Rust-reading section.

The archive was independently hashed and enumerated: **1,732 files, 13,348,773 bytes**, SHA256 `2d1ac2abdba013189422c6b25f5825ae2b843ef57ffa3a12421187986978fa6f`. All six windows' archived rows and metadata are byte-identical to the reviewed original records; their public metadata copies are also identical. The intermediate `final-*` windows, failed `controls-without-api-feature` receipts/Clippy log, effective follow-up controls, full native quality record, private-target label receipt and reproducible section-byte script/receipt are present and match their originals. No object/semantic cache, private profile-source tree, compiler build target or baseline-binary directory is included.

Every provenance-bound current tool and binary size/SHA matches its actual file. The public native-quality projection matches every retained full-record field, all fourteen canonical-list counts/digests, and full-record SHA `e0597f73d552a4262affeb1932df7ef996b953726db4b4df418220a7dc0c3b56`. The public section-byte receipt is byte-identical to the archived reviewer receipt. The report correctly retains 146 raw private-label differences, zero size growth, seven exact `.text` pairs, two exact ELF pairs and the comparator's unexamined indirect-call entries.

The historical CI records bind only Stage11 delivery `38a7d5a088ecd291255907dfca32e37785b8292c` (official41290206784 and research41290201058, both completed/success). They do not approve the forthcoming delivery commit. This packet check added no build, test, workload or timing run; final Git delivery identity and its new CI status remain a subsequent receipt.

### Actual native-input attachment

**Attachment review: Pass.** `native-elves.tar.gz` is 829,261 bytes with SHA256 `656ffa4839afa227df65f2fc76fbad92b9aae38072123afb637d94a3890ae9e9`, matching the added provenance entry. Its fourteen file paths exactly match the seven reviewed baseline/candidate pairs relative to the Stage12 root; every extracted ELF length and SHA matches the full quality record. The report links this attachment, supplying the actual byte-reader inputs in addition to scripts and receipts. The original raw archive remains byte-identical at SHA256 `2d1ac2abdba013189422c6b25f5825ae2b843ef57ffa3a12421187986978fa6f`. Only archive bytes were read; no native program or new measurement was executed.
