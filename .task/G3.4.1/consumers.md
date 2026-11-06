# G3.4.1 sealed Option independent consumers

Root delegated this slice on 2026-11-06, main c63001c8e baseline.
Worktree /workspace/zil-G3.4.1-consumers, branch task/G3.4.1-consumers.
Read AGENTS.md, current goal G3.4.1, design fixed-domain paragraph and
RUST-GOALS. Root approved exact sealed Option profile, coordinated with
life producer's matching canonical builtin declaration and shape. Proposal:
changes/G3.4.1-option-address-consumer.md. Types/Analyze/Verify never call
one another's proofs. Public schema unchanged.

Write set: Types stored + aggregates resolver, Analyze/Verify stored_flow
and private nominal/option_address consumers; existing cross_function_borrows
linked fixtures only. Text kernel author's stored_flow changes affect other
functions; small patches are coordinated directly. No heavy builds/new targets.

Initial check: cargo check -p zil-types -p zil-analyze -p zil-verify -j1 --offline;
independent CARGO_TARGET_DIR=/workspace/.zil-env/target-G3.4.1-consumers,
source /workspace/.zil-env/env.sh, incremental0, default limits unchanged.
Earlier metadata-only no-link check passed; full transport check pending.

Rust four goals: exact natural Option borrowed payload expands expression;
permission transport has implementation but full source/raw/Wire/native proof is
pending. Runtime remains inline pointer+tag with no new allocation; no Rust
runtime/memory or compile-cost comparison. No product-level breakthrough claim.
