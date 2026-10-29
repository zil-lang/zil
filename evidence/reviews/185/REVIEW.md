# Independent review: 185 public lending metadata

**Pass** for schema/transport slice `6f0c77612b504de74e50ee147d124eb303b4ff6a`. Reviewer source `88a83a3d` adds only independent tests. All crate production source trees match the author and earlier `0bb75c0d`; `PRODUCTION-IDENTITY.json` records the exact identities.

This slice publishes two separate `lending_captures` lists on ClosureDefinition and CallableReturnContract. It does not admit lending language execution. The lists bind actual environment positions, remain distinct from hold/call/writes/cleanup, appear in Wire23 and public JSON, and participate in TypedModule V23 and callable-return contract domain2 identity. Current nonempty claims remain closed until independently checked result/loan semantics exist.

## Independent validation

Final clean replay passed **51 tests**: IR31 unit plus7 integration tests, driver author5, raw factory4, retained factory1, and reviewer3. Rustfmt and all-target IR/Verify/Driver Clippy passed. Commands retain exact source commits, environment, stdout/stderr hashes and real exit codes. No full-workspace run occurred.

The reviewer used a source-generated environment containing two captures. Duplicate `[0,0]` and reversed `[1,0]` lists fit its count and index bounds, so their rejection exercises the ordering check rather than an earlier out-of-range condition. Both direct raw verification and independently decoded Wire reject. Well-formed nonempty `[0]`, `[1]` and `[0,1]` remain explicitly Unsupported.

For both record positions, a 17-element claim encoded with normal limits is decoded with max_locals16 and rejected as ResourceLimit. An independent byte mutation then replaces that exact vector count with `u32::MAX`; normal-limit decoding rejects it. Reader code checks count, quota and remaining-byte lower bound before allocating the vector; the test supplies hostile raw bytes rather than relying on a source declaration to enforce limits. Raw verifier checks precede module fingerprinting.

Seven equal-length/source-position combinations produce distinct module identities and roundtrip exactly. Five exported-contract index sequences have distinct contract identities. A real two-module project confirms that verified exported JSON contains the empty lending relation. Public fields carry capture positions unchanged through contract cloning; only closure identities are remapped.

## Preserved first failure

The first `0bb75c0d` author-suite replay passed9 and failed1. Its JSON fixture called the single-file compile_source API, whose report has no project scalar_interfaces. Root changed only that fixture to compile_project with a real imported factory, producing `6f0c7761`. No production semantics or rejection expectation was weakened; both receipts remain. The inherited handoff-integrity failure is also retained without a Pass label.

## Boundaries and Rust goals

This establishes bounded transport, exact identity, visible interface data and a closed independent proof boundary. It does not establish actual environment-backed result lifetimes, mutation invalidation or machine execution for lending closures; those remain task184. Legacy retained factories were replayed, but no new runtime layout or performance result is claimed. There is no matched Rust runtime, memory, maintenance or compiler-cost comparison, and no product-level net breakthrough claim. Full language stays NotSatisfied, product NotRun, and default/product thresholds remain unchanged. Executables and repeated legacy phase output are excluded from this compact evidence; exact omissions are recorded.
