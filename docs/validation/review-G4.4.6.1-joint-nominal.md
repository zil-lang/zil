# G4.4.6.1 joint nominal / Copy review

Review input: integration `e0710d47c2e27c38df37ff3de439c547a9e58d4d` plus the `96a1ad3e5a3fddde83d48e3caa941d07e12fcd2b` Types field patch. Review worktree merge: `bccf3e225030e5fb0afa8913d910d7efc2629c9b`. This is a static, scoped review; the integration owner is running the exact frozen tests separately.

## Blocking finding

`Wrap<T>{value:T}` with `T=Phantom<'s,i32,42>` reaches `zil-types/src/aggregates.rs` `aggregate_member`: the new `stored::pure_value` check admits the actual `BorrowedStorage` child at lines 2258–2260, and lines 2266–2284 emit `AddressField` followed by `ReadAddress` with `Address<BorrowedStorage>` as its input. `zil-analyze/src/stored_flow.rs` lines 2875–2885 explicitly require `inner.is_copy_value()` for `ReadAddress`; `zil-verify/src/lib.rs` lines 2384–2392 repeats this requirement. `zil-ir/src/lib.rs` lines 599–628 makes `BorrowedStorage` non-Copy. The real physical field source positive therefore cannot pass Analyze/Verify on these commits. A fix must be narrow to a proven physical pure-value boundary and independently checked by Analyze and Verify; making all `BorrowedStorage` Copy would erase the intended address/ownership boundary.

## Static checks and limits

The Expand producer resolves a qualified Type actual through its real `stored_nominal` application and complete local class vector (`generic_stored_actuals.rs`); it selects substituted fields by their original generic formal ordinal, including grouped and tuple occurrences (`stored_types.rs` lines 627–775). The produced outer rows use real child declaration/argument ordinals and `TypeArgument` paths (`stored_types.rs` lines 897–999). Three separate nominal walkers reconstruct direct life rows before Type argument recursion from full actuals, select a unique complete child instance, and check each child-local class mapping (`zil-types`, `zil-analyze`, `zil-verify` `stored/nominal.rs`). Their retained scans inspect actual Record/Tuple fields, reject address/ownership leaves at a qualified physical boundary, and require full class coverage. These observations do not close the blocker above.

The public raw/Wire representation has no original field-to-Type-formal recipe. Its consumers check internally consistent physical types and class relations; they cannot certify which of two identical physical child types came from distinct source formals. That source relation is checked during Expand. Compound qualified Type actuals remain explicitly unsupported. No heavy test or link was run from this review worktree.
