> This checkout now contains a completed continuation slice. Read `.task/point-to-point-continuation.md` and `tools/research/incremental-demand/continuation-results.md` first. The prior handoff below remains historical provenance.

# Point-to-point compilation continuation

User requests a new Codex Cloud chat to implement this continuation. Stop old-chat measurement work.

Remote repository: https://github.com/zil-lang/zil.git
Research branch: research/incremental-demand-propagation
Historical delivery: c1ae85aa839e6e5c7f7e517ee66362bc839d9017
Measured compiler source: 0601b9786d0fe683b4321f0d086230e5be027602
Actual main baseline last verified: 99035931d358add0eaed6d4535035c38413701dd
Goal branch last verified: ee7fcd1a229799f1146c4dbdee822d6c9f8ff269

Correction: Zil CGU16 versus Rust CGU256 is not a controlled O2 implementation-gap comparison. Withdraw 2.36/1.43 cross-language O2 ratios. Historical same-configuration main versus candidate observations remain separate. Read README correction; results.json preserves historical observations and independent review receipts, not a new corrected benchmark.

Environment observation: old worktrees, binaries and local raw archive were absent; original /workspace/zil and provisioned toolchain remained. Provider still reports the same environment ID and running/connected. Recycling/recreation/remount cause is unknown. Remote source and compact results were restored. Do not claim previous raw archive is available in a new environment.

Confirmed source findings (recheck exact code before implementation):
- driver/session.rs query dependencies all observe PrivateImplementation; raw digest includes complete query key/dependency construction identities. Whole-project edits reenter phases; this is not output-value red-green propagation.
- types/incremental.rs graph only evaluates BodyKey from Context/Signature/RawBody, not CFG/Analyze/Verify/Lower/CGU/Object/Link queries. RawBody currently binds module revision. It returns a complete keys table to whole-program traversal.
- expand/incremental.rs still clones all SourceUnit text while assembling sources (cloned_source_bytes), despite shared Core/SourceMaps.
- CFG hits still clone/rebase typed functions; global inventories/context admission remain.
- semantic/function publication serializes whole products; encode_work_products clones all cached functions before encoding.
- current independent Verify is mandatory. Preserve its authority boundaries; do not silently reuse old Verified capabilities.
- object hits still follow current Lower view, whole machine IR construction, diagnostic LLVM rendering, partitioning, hashing/preflight, object IO and linking. lower_view already borrows functions and bind_lir has pointer equality fast paths: do not claim nonexistent deep copies.

Rust source pin: rustc1.99.1 commit58cf8f9c35b355199727627d07ae34a4e2846d67; Cargo commit5f94df4789f005f9a352888e8355ffc645b7ed0e. LLVM Rust23.1.1 versus Zil23.1.4. Consult official pinned query-system/dep-graph, query serialization, mono partitioning and codegen work-product reuse. Rust also has whole-crate work and affected-CGU rebuild; do not idealize it as only changed-function compilation.

Next work: trace edit→final artifact and classify necessary versus redundant work; implement a concrete smallest valuable slice with complete dependencies/current verification. Rebenchmark main/candidate/Rust in one sequential measurement window, same requested CGU16/O2/jobs1 and cache/history controls, with actual CGU/work-product inventory; CGU flags are not proof of identical partitioning. Six blocks/eight named-parent requests, exact native output and precise diagnostic oracle. No reduced budgets/optimization, no profiling in wall measurements. Keep source, command, binary hashes and negative evidence. Compare only main/Rust, not another active worker.

At handoff no new matched-CGU measurement has completed; new benchmark helper, if present, is WIP and must be independently reviewed before use. Production compiler source unchanged. Offline rebuild first failed for missing rustc_apfloat; locked online retry log at validation/out/runs/matched-build-candidate-r2, not a new source acceptance. New chat must inspect actual available files and build status.

Read AGENTS.md and remote goal/workflow. Separate worktree/target, one heavy window, no reset/clean, no main merge/default enablement. Persist source, scripts and compact evidence in Git early; local out archives alone are not durable cross-environment backups.
