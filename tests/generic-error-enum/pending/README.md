# Preserved generic error-union context inputs

Both files preserve the original bytes rejected at Expand on `369bd222`.
Candidate `fb1a95ea` promotes them to positive controls in the existing Expand
target. Separate consumer tests exercise native derivatives that expose the
union result and preserve argument evaluation and cleanup order.

- `unique-binding-value.zil`: an explicit `Choice<i32>!i32` binding should supply
  the unique enum context and inject `Choice.None`.
- `unique-call-argument.zil`: the concrete sink parameter supplies the same unique
  context for ordinary static function argument matching.

The declaration checker accepts an identical complete union directly and checks
declared capabilities and known type relations, including definite `T!T`
ambiguity. Within the existing finite profiles, ordinary type arguments are
specialized before concrete payload injection must match exactly one side.
Potential future equality alone does not reject a symbolic declaration.

For a callee's missing type variables, existing read-only expression hints first
collect ordinary argument constraints, then any still-needed whole-union
constraints. Payload alternatives never infer those variables. Real argument
checks and runtime evaluation retain source order. Actual callable payload types
come from their declared signatures, including `sink(f(), witness())`.

The fixed Expand limits below all pass at N, reject with ResourceLimit at N−1,
and produce identical Core on fresh parsing:

| Input | Steps | Nodes |
| --- | ---: | ---: |
| Original binding | 880 | 301 |
| Original argument | 871 | 290 |
| Independent witness constraint | 1440 | 457 |

The prior return 624/242, scalar 288/144, owned 554/279, local 558/281 and enum
336/126 controls retain their limits. Author checks passed: Expand generics
14, generic defer 3, enum budgets 2, and driver generics with LLVM API enabled 8;
targeted Clippy and formatting passed. Consumer machine results and independent
review are reported separately; full integration gates belong to the root.

The `99de375e` integration failed two unchanged legacy tests: StoreElement/Unit
and Copy+callable error returns. The new universal distinctness requirement had
overridden the existing specialization profiles. Repair `12b6af4f` removes that
guard and its unused type-key observer without a profile whitelist. Original
Store target 15/0 and Copy callable target 19/0 pass; their sources and cleanup
oracles are unchanged. New regressions separately check unused declarations,
valid `U=bool` instances and ambiguous `U=i32` instances. Concrete ambiguity is
still rejected at Expand or Types, with the exact expected diagnostic in each
case. The earlier potential-equality rejection was an overly strict requirement,
not evidence of bad runtime code bypassing the concrete check.

This slice improves natural binding/call composition without new runtime
allocation, public IR, ABI or default limits.
There is no new Rust runtime/RSS or compile-time comparison, and complete
language safety and ZUI product goals remain outside this limited evidence.
