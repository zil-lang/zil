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

The frozen earlier limits and their source identities remain in exact historical records. A concrete `U=bool` specialization may be valid while an ambiguous `U=i32` instance must reject at Expand or Types with its actual diagnostic. Potential future equality alone does not make a symbolic declaration invalid; neither may a concrete ambiguous payload bypass the eventual check. Existing StoreElement/Unit and Copy callable error-return controls preserve their behavior.

These inputs exercise natural binding and call composition without granting new runtime allocation, public IR, ABI or default limits.
There is no new Rust runtime/RSS or compile-time comparison, and complete
language safety and ZUI product goals remain outside this limited evidence.
