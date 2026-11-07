# Frozen public interface review fixtures

`lexical_primitive_bindings_public_edge_controls.rs` contains seven independent public-interface checks for actual transport, fingerprints, budgets, names and declared semantic stubs. Two checks preserve a historical Unsupported boundary at the reviewed implementation; their diagnostic text is not a permanent language rule:

- `raw_and_wire_transport_never_bypass_declared_semantic_stubs_or_local_preflight`
- `package_address_predicate_does_not_widen_copy_pointees_or_verify_authority`

The first five groups also live in the active `crates/zil-driver/tests/public_lexical_bindings.rs`. This frozen directory is outside Cargo automatic test discovery. To reproduce all seven, use exact reviewed commit `061e22536784f4e568dfbcebbb8ca2086cdc54a7` and a fresh target:

```sh
git worktree add /tmp/zil-public-interface-review 061e22536784f4e568dfbcebbb8ca2086cdc54a7
(cd /tmp/zil-public-interface-review && \
  CARGO_TARGET_DIR=/tmp/zil-public-interface-review-target \
  cargo test -p zil-driver --test public_lexical_bindings --offline)
```

This fixed review does not establish current implementation acceptance. Original failure and repair receipts are available in that commit's `evidence/reviews/177/public/`.
