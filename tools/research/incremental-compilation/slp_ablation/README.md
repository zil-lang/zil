# O2 SLP ablation

Read changes/research-o2-slp-ablation.md. This experiment uses exactly commit 43037eb7b108ba864144979204b5c6d18636dac3. It adds no compiler CLI option and does not edit the current production tree.

Static preparation:

    python3 tools/research/incremental-compilation/slp_ablation/prepare.py --repo /workspace/zil --out <new-plan-dir>

The sole codegen change sets LLVMPassBuilderOptionsSetSLPVectorization to zero. Three O2 receipt strings additionally record slp-vectorization=false; the same identity is hashed by the object cache. Original/patched files, exact patch, commit/tree/blob identities and SHA256 receipts are saved. No compilation occurs. Loop vectorization/interleaving, MergeFunctions, verification and publication remain as in the base commit.

In an authorized functional window, use another new directory with --materialize, then build its source/Cargo.toml with an independent CARGO_TARGET_DIR. Source extraction uses Git objects, never current worktree files; SOURCE-ORIGINALS.json binds every unpacked original file before the two-file patch. Keep the reference executable and caches separate.

Compare identical source, inputs, merge policy, closed/default contract, CGU/job counts and target. First validate all three merge policies and opaque-input arithmetic, branch and checked-add bodies; then schedule exclusive compile and native-runtime measurements. A smaller compiler time alone does not demonstrate acceptable generated-code performance. This tool provides no performance result by itself.
