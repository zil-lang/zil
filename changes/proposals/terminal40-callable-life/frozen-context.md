# Concrete frozen invocation context and migration

Read-only inspected current production `ct_artifact/{read_view.rs,execute.rs,authority.rs,source_index.rs,compiler.rs,compiler_generics.rs}` plus344's scoped WIP. This document proposes code changes; it does not claim they exist or passed tests.

## Actual boundary selected

`FrozenReadView` currently contains SourceIndex, original sources/syntax, shape/impl snapshots, input tokens, limits/target and CtSnapshotRequest. It contains NO complete instantiation context. `Entry::Expression` carries an actual definition symbol and expression, but its `(String,CtTy)` parameters are CT value bindings, not complete Type/Const/Lifetime generic actuals. Therefore none of those fields can be renamed into scope authority.

Add a PRIVATE `FrozenInvocationContext` to Driver's immutable environment at the actual materialization/specialization boundary, before `execute::evaluate_data_context` calls `compiler::compile` and creates its expected artifact. It contains:

- A borrowed original SourceIndex plus a paid binder inventory produced by traversing the actual original AST/Origins rows. Each row resolves a real declaration/member/binder site to full ordered generic parameter kinds and its enclosing lexical owner. Span can reject a diagnostic mismatch but cannot select the row.
- The actual active enclosing declaration/binder rows selected by that traversal. Merely being visible or present elsewhere in SourceIndex does not make another function's lifetime an enclosing scope.
- For an uninstantiated original generic definition: `CtScopeInstance::Definition`. This is an authentic lexical identity, with full original generic parameter ordinals; it is not a vector of made-up generic values.
- For a real specialization: original declaration/binder identity plus ALL ordered actual Type/Const/Lifetime arguments supplied by that specialization operation. A spelling, current generic environment hash, emitted function name or final declaration ordinal is not a substitute. Missing lineage/actuals fails closed.

Use this same immutable object in compiler attribution, FrozenAuthority and FrozenReadView. `SourceIndex::freeze` already assigns actual declaration IDs in traversal order; extend this inventory with actual binder rows. Do not obtain new binder authority by matching `item.span` or by range containment. An ephemeral actual-node-to-row lookup during an immutable traversal is acceptable, but it is not serialized or reused across AST clones.341's original/Aligned lineage ledger supplies genuine moved declaration correspondence; generated/member transformations need their own actual ledger before scope opening.

## Public representation

See `model.rs`: `CtAmbientLifeScope{declaration,binder_site,instance}`; `CtScopeInstance::{Definition,Instantiated(full_actuals)}`; `CtAmbientLifeReference{scope,parameter}` uses the FULL original parameter ordinal. The declaration's real binder inventory supplies kind and count; raw scope rows cannot invent them.

`CtTypeRef.ambient_classes` maps each actual graph-local Fixed owner/class to one ambient reference. This handles physical-only Address metadata as well as phantom nominal arguments. It avoids overloading CtTypeNodeId: CtLifetimeArgument remains a LOCAL graph owner backreference, while ambient scope IDs live in their own artifact table. A graph copy remaps local owner IDs and retains/rebinds ambient table references jointly; it never alpha-freshens the latter.

`CtArtifactProgram.ambient_life_scopes` is the shared table for every graph/schema/constant in that artifact. Scope actual Type graphs use that same namespace. Semantic scope dependencies through actuals must be finite, range/kind checked and cycle/depth/work bounded; local generic definition binders are terminal identities, not a recursive self-instantiation. The raw table order is canonical first actual use in the full artifact traversal. Duplicate scopes with identical complete resolved context canonicalize once; different full actuals or definition owners never collapse.

A standalone graph with no artifact/value context rejects nonempty ambient mappings. Every free Fixed class must have a unique ambient mapping or be connected by the actual enclosing rich signature's class relation. An empty table cannot turn an open Fixed class into a new forall binder. Pure local Invocation BFP quantifiers remain closed without ambient rows.

## Authentication and independent validation

1. Driver constructs the expected ambient table from its private FrozenInvocationContext, paying the full source/binder/actual traversals before copying it into a raw artifact.
2. `FrozenAuthority::authenticate_artifact` already compares to an independently held expected artifact; retain that exact full-context comparison, including all new fields. Add an explicit `CtArtifactAuthority::authenticate_life_scopes` boundary against the same frozen context if scope authentication is separated from full artifact authentication; neither path trusts raw membership or a hash. No producer validation bit is introduced.
3. Verify independently checks the complete scope table, kind/arity/full ordinals, Type actual closure, finite instance dependencies, local physical graph, actual owner ancestry, class coverage and relation consistency. Source authentication does not replace these checks, just as it does not replace CT CFG/type checking today.
4. FrozenReadView gets `invocation_context: &FrozenInvocationContext`; CT argument/type/query services interpret ambient references only against this exact frozen invocation. VM entry must bind the verified artifact scope table to that context. A schema copied from another invocation cannot be interpreted by coincidental scope index0.
5. `EvaluatedData`/`ValueContext` retains the complete authenticated ambient table alongside returned schema graphs. `materialize_value` reifies through the actual enclosing owner/full-ordinal map; it does not choose a lifetime spelling by index alone. The final ordinary source/type/kernel pipeline rechecks all resulting relations and access independently.
6. Semantic cache/Type/schema keys encode resolved stable declaration identity, binder-site identity, mode Definition/Instantiated and complete actual argument identities, including their joint ambient references. Raw ModuleId/NodeId/table ordinal is relocation only. Existing SourceIdentityContext explicit or closed-snapshot namespace supplies the stable source identity according to its already checked contract.

There is no visibility, borrow, construction, Transfer or cleanup authority in these metadata records. Freeze/authentication proves actual provenance; actual roots and runtime permissions remain separate.

## Minimal production change sites

- IR `lib.rs`: SignatureLifeClass/Binding, StorageLifeContract.classes, BFP life and callable-role/Core bound field replacement. Keep closure/Typed/Scalar storage_life as the one common type. `dynamic.rs`: use only rich storage_life for a slot. Existing public interface representations must carry it too.
- IR `ct_artifact.rs`: rich BFP node/constructor, ambient scope/instance/reference/class rows, CtTypeRef.ambient_classes and CtArtifactProgram.ambient_life_scopes. No new lifetime runtime value or physical child edge.
- `wire.rs`, dynamic codecs, fingerprint functions: bounded class-row codecs and joint identity. `ct_wire.rs`: bounded ambient/actual/context codecs and local-vs-ambient relocation. Historical39/CT4 bytes stay frozen; successor domains40/5.
- Driver `source_index.rs` +341 Origins/Aligned producer: real paid binder inventory and actual current scope handle. `materialize.rs` / generic specialization: pass the genuine active scope and complete actual vector to CompileRequest, without Span joins.
- Driver `execute.rs` / `authority.rs` / `read_view.rs`: construct and share the frozen invocation context as above. Preserve remaining-budget flow; no separate fresh allowance.
- CT compiler metadata/generic/schema attribution and scoped normalizer: full rich relation, ambient mapping on every graph copy/remap/key; producer must not dedup graphs before contextual key construction.
- Verify `ct_artifact_scoped_graph.rs` / identities: independent scope/table/class coverage and contextual keys. Existing no-scope paths may retain old fees where actual work is unchanged.
- CT VM/query and Driver `value_context.rs`, `materialize_value.rs`, `type_ref.rs`, `ordinary_type.rs`: retain the context until the returned Type/value is consumed; debit complete dynamic bytes and release them on all paths.
- Types/Analyze/Verify existing borrowed_fp/lending/helper/role/storage code: derive legacy outer-address projection from the common contract, independently verify actual target roots and slot classes; unsupported nonempty class relationships remain rejected until each consumer exists.
- Backend: signature metadata may erase only after independent verification; actual argument/result ABI and borrow runtime representation stay unchanged. No branch trusts a CT class map as actual access permission.

## First actual discriminators

One original generic owner with full order `<T, life a, life b>` must distinguish Phantom<a>/Phantom<b> and preserve two uses of a. Two different source definitions each using full ordinal1 must remain distinct. Same real definition with two different complete generic actual vectors must remain distinct. Raw attempts to select a visible non-enclosing owner, swap ambient rows, substitute wrong parameter kind, drop a mapping, reuse another invocation's table or mutate an actual Type argument must reject independently. Repeated Span and module/source-vector relocation leave only the actual owner ledger relevant. Each has nearest valid controls plus N/N-1/fresh and source -> raw/Wire -> VM -> reification checks.
