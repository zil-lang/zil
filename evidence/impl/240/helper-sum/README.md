These receipts bind the author frontend fix to source `32d92fd7d09954c349695926baca5b5723779809`. Root common P `d393f47db2a1e3da23d1d52c875fb142c193ef14` exposed the second-give metadata loss; the independent used/unused original sources and real Expand failures are preserved under `original-P-failures`. The first helper managed-pattern failure and subsequent Types gate failure are retained separately. No input or expected result was changed to turn these failures into rejections.

The production change preserves `manages` from the actual non-generic Enum declaration and records an owned Name binding. New local bindings inherit ownership only from an actual validated `give` expression (including grouped/conditional transfers). Bare descriptions do not acquire ownership. Store extraction and forwarding preserve Incoming provenance. The helper data gate accepts only the specified ordinary Sum/Product-with-Sum Value position; F signatures/results/transfer, State, capture, array and StoreElement gates remain separate.

`240-relay-tests01.log` is actual public Parse/Expand/Types testing (front 11, helper 7). It includes used/unused transfer, grouped give, repeated give and descriptor-alias rejection and Incoming Store relay. `240-relay-clippy.log` is the limited Expand/Types all-target check. These are not evidence of independent Analyze/Verify/native closure: the P full-source helper was independently blocked by the kernels' old extra-parameter gates and returned to their authors.

Budget measurements use the same saved Rust harness and source for each implementation, `max_analysis_steps` only, all other limits default. Both iterations require exact N success, N−1 ResourceLimit and identical complete Core/Typed after a failed call and fresh parse/Core retry. The new Sum parameter validation prepays before classification at both `lib.rs` signature validation and `helpers.rs` template installation; the used source traverses both sites. The old admitted parameter path returns before either new classifier. Public thresholds exercise the whole request, rather than claiming either private pool alone determines the minimum.

| Source | Expand N | Types N | Old observation |
| --- | ---: | ---: | --- |
| Explicit give-resource callback parameter | 389 | 544 | 389 / 544 |
| Ordinary Package read-only callback parameter | 430 | 711 | 430 / 711 |
| Managed Enum field direct give | 598 | 2020 | Previously rejected |
| Managed Enum field give relay | 640 | 2141 | Previously rejected |

The old controls were observed at clean readonly `541f21bbe8cff775cb7341aae3652759a0a65c8d` in `/workspace/zil-238-regression`; this is not a claim that the whole tree is Q. Root previously established its base/ir/syntax/expand/Cargo dependency source matches Q for the Expand comparison. Types control costs above are a direct comparison of these two observation commits. No default limit or product threshold was raised. The rejected Package field-transfer setup in `/tmp/240-helper-budget-current.log` is not used as a legal legacy control; the archived canonical controls are the saved `.zil` inputs and successful `current02`/Q logs.

Replay (same environment, isolated targets, one job):

```
. /workspace/.zil-env/env.sh
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/workspace/.zil-env/target-240
cargo run --manifest-path /tmp/impl240-helper-budget/current/Cargo.toml --offline -- /tmp/impl240-helper-budget /tmp/impl240-helper-budget/receipts-current old-give-param old-package-field sum-direct sum-relay
export CARGO_TARGET_DIR=/workspace/.zil-env/target-240-q-budget
cargo run --manifest-path /tmp/impl240-helper-budget/q/Cargo.toml --offline -- /tmp/impl240-helper-budget /tmp/impl240-helper-budget/receipts-q old-give-param old-package-field
```

Rust goals: this repair closes a finite ownership-expression metadata gap while retaining actual move/descriptor checks. It introduces no runtime representation, allocation or synchronization. Natural helper forwarding is improved; whole-framework maintenance advantage, Rust runtime/RSS parity, compile-speed advantage and the four-goal product conjunction were not measured or established. The repeatable improvement here is a previously rejected legal frontend composition, with bounded new work and two unchanged legacy budget controls.
