# Error union construction

The authority for `E!T` is `_zil_design.md` §15.2. Its error and success alternatives retain separate tags even when their complete payload types are equal.

`error_ok::<E,T>(value)` constructs the success alternative of `E!T`; `error_err::<E,T>(value)` constructs its error alternative. Both require exactly two complete type arguments and one value checked against the selected payload type. They are reserved bare primitive names: declarations, parameters, local bindings and resolved import aliases cannot rebind them. These operations lower to the existing `MakeErrorUnion { is_error, .. }`; a spelling does not grant a new ownership, borrowing or cleanup permission.

```zil
fn success(value:i32)->i32!i32 {
    return error_ok::<i32,i32>(value)
}
fn failure(value:i32)->i32!i32 {
    return error_err::<i32,i32>(value)
}
```

Ordinary implicit injection still requires a unique alternative in its checking context. A bare value with both alternatives equal remains ambiguous. When a generic definition has selected one alternative using its actual symbolic payload type, that choice is retained before substitution; an instance in which its two formal alternatives become equal does not guess again or change the tag. This applies to ordinary generic bodies, arrow returns, annotated locals, trait defaults and generic impl recipes. Unused definitions must prove their declared typing obligations.

Only the selected actual payload is constructed, transferred and cleaned. Owning payloads obey ordinary prepared-value, move, active-tag and exactly-once cleanup rules. `.ok(pattern)` and `.err(pattern)` observe the actual tag before reading or consuming a payload. Kernel validation does not infer a tag from payload type equality; reachable wrong-arm reads and missing live obligations remain errors. A verifier may exclude an edge only from its own current, complete, initialized actual-tag proof, invalidating stale observations on relevant transfers, writes and calls. Structure and instruction typing remain checked throughout the original CFG.

The primitive does not expand the admitted payload families, add implicit conversions, or turn unknown/partial content into an empty alternative. CT transport must preserve both full type arguments; evaluation requires the existing CT value and operation rules.
