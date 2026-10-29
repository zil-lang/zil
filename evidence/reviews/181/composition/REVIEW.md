# Independent 181 composition and projection review

**Pass**, bound to production `83174688e249c8e5723ebaffd11cde3024b601b6`. The same production is also present in `8fffa2e56befa9fb3d22db3d7dd1e5ee4d7e80da`; its two legacy-test migrations received a separate replay. `PRODUCTION-IDENTITY.json` records all crate source-tree identities and exact non-evidence differences. The reviewer changed no production code.

## Finding and correction

The first candidate `e7896c54` was rejected. An unused generic `Holder<T>` helper could read `holder.value` twice into `(T,T)` because every Member base was changed to borrowing mode. This bypassed the ordinary generic value-consumption rule. The original rejected report and command receipts remain unchanged.

Root corrected the production at `83174688`: only a direct or grouped local with an actual Address type gets the projection reborrow. Ordinary Value/Package expressions retain their prior consumption rules. The projected field after dereference must still have an actual finite Copy type. The original Holder input and three nearest variants now reject. Seven Address/Managed/unknown-T boundaries stay closed. Exclusive read/write/read and later caller use remain valid. An independent O0/O2 native test increments an explicit counter in an address-returning helper, proving each projection base is evaluated once.

## Actual results

The frozen independent test source `7f318403` passed 53 tests: impl177_state_composition (11), impl178_generic_store (15), impl180_store_mut (19), impl181_composition (4), and the reviewer projection groups (4). After merging the unchanged-production legacy migrations, `719311c8` passed another 14 tests: impl126_helper_sequences (5) and review177_composition (9). No tests were ignored or filtered in these final runs.

The actual four-module composed consumer passed 48 Linux native executions and 48 Win64 executions under Wine compatibility. Each platform includes 30 allocation-failure injections at O0/O2. All instrumented runs ended with zero live allocations and allocated count equal to frees. The compiler SHA-256 was `2a6558f7bce22f51c0d2b9b226641896819bb5fa6292af02a33ef3d2e734890f`. These are compatibility runs, not a native Windows-host claim. The independent projection side-effect test adds two Linux native executions.

The focused build, rustfmt check and all-target Expand/Driver Clippy check passed on clean `7f318403`. Their exact commands, environment, status, stdout/stderr hashes and real exit codes are retained. The two later legacy files have their own test replay; the earlier quality commands are not relabeled as having run after that merge.

## Test migration review

Both migrated legacy files preserve their original language inputs. A cyclic callback loop still rejects as Unsupported, now at Types after the expanded generic definition check. Store descriptor give has an explicit safe rejection. Distinct State callback, Store and Copy fields now satisfy the intended 179 precision; whole-parent destruction, unsupported give and same-resource writes remain guarded. The raw/Wire sibling case is valid, with a new nearest early-whole-parent-cleanup mutation rejecting. No blanket negative-test deletion or unknown-raw permission was introduced.

## Preserved failures and limits

The initial rejection, its passing author suite, the reviewer counterexample, and all intermediate run receipts remain visible. The first Wine initialization failed because the sandbox blocked Unix sockets; a new-prefix initialization then failed with official stub DLL/kernel32 setup. Both failures preceded the consumer run and retain their actual exit codes. An independent copy of the restored official prefix plus explicit WINEDLLPATH allowed the real Win64 binaries to execute; no author prefix was mutated. The inherited handoff-integrity failure is preserved without a Pass claim.

This scoped review does not run the full workspace or product gates and does not certify the full language. The separate 178 callable-helper regression repair has its own review and is not included in this production identity. Executables, object files, Wine prefixes and repeated phase dumps are omitted from this compact evidence; their hashes or original run locations are recorded. One representative Linux phase chain and the Win64 verified/interface/LLVM chain remain inspectable.

## Rust goals

Finite projection safety and composed ownership have direct source, raw/Wire, native and allocation-failure evidence, and independent review caught a real definition-time error before acceptance. Four modules now reuse the generic Store and State callback facilities. No matched Rust runtime, peak-memory, maintenance-cost or compilation-cost comparison was performed in this review. These are reproducible language improvements, not proof of a product-level Rust advantage or a new general safety theorem. Full language remains NotSatisfied; product remains NotRun; no threshold changed.

## Final author source binding

Final author source `2f42c86cbe75d5d542bb46dee28fc92e354a257f` retains the exact reviewed projection and composition implementation. Its only production difference from `83174688` is the concrete-resource callable-helper correction from `6b2a700a`; the exact added/deleted source lines were compared. All other crate source trees, public schemas, specification and Cargo configuration are unchanged. Root independently reviewed that helper correction (reported source `47949d71`, fresh-target 27 groups Pass). Because this reviewer authored that helper, this record does not self-certify it. The earlier 67-test and 96-consumer observations retain their true commits; the final combined consumer replays are recorded separately by the 181 author.
