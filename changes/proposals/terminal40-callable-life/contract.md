# Proposed shared callable relation successor

This proposal is external data design only. Existing 39 View/closed Task source and kernel work continues. A public bump/transport roundtrip is not source or safety acceptance.

## Why 39 cannot encode the required type

`FunctionLifeContract` labels only outer Address parameters/results. In `for<a> fn(&'a Tag<'a>)->i32`, a qualified Tag can retain its own local class0, but BorrowedFunctionPointer has no relation connecting that class to its invocation0. `for<a,b> fn(&'a Tag<'b>, &'b i32)->i32` and the variant whose Tag uses a must have different complete identities. A BorrowedStorage wrapper around the whole function pointer is outside admitted runtime/storage contexts and cannot substitute for a real signature mapping. The CT4 graph may describe an owner backreference that has no lossless runtime representation; the bridge must fail closed until the successor is implemented.

## Concrete shared fields

`model.rs` extends the existing StorageLifeContract with `classes: Vec<SignatureLifeClassBinding>`. Actual borrowed positions remain in `bindings`; nested semantic class selections are a different row type. Replace the flat BFP/CoreCallableParameter/CallableRole life fields with this SAME complete contract. Existing Core/Typed/ClosureDefinition/ScalarFunction storage_life fields use the extended structure. DynSlot uses storage_life as the sole serialized relation and no independent invocation_life field. Retained port bindings, actual access/effect projections, Hold roots, and loans remain separate and unchanged.

A class row selects a real qualified type by StorageBase plus exact LifeTypeProjection path, then one class of its independently validated local domain. For parameter `Address<Tag<'a>>`, path `[Pointee]` selects Tag's class0. The corresponding outer Address still has a normal binding at Parameter0/empty path. Both rows can map to Invocation0. Changing the Tag row to Invocation1 changes identity and must be checked independently of the outer-address label.

The resolver checks every actual constructor and declaration inventory edge; no name parsing, raw ID equality, assumed GEP, implicit pointer traversal, or class count alone establishes a selected type. Qualified storage, opaque retained carriers and a nested callable's FREE Fixed domain are possible selected boundaries. A nested callable's own Invocation binders are a lexical barrier and cannot be selected as the outer signature's classes. Its free Fixed classes can be related by an explicit outer class mapping; a nested `for` is not freshened piecemeal.

No two spellings for one mapping: in the successor, old `ContractProjection::RetainedClass` entries in StorageLifeContract.bindings are noncanonical and migrate to classes. RetainedPort remains an actual typed abstract leaf selector where otherwise allowed. No permission consumer accepts SignatureLifeClass as StoragePosition. A phantom-only mapping has no target, owner, loan, result provenance, read/write/cleanup authority, or access effect.

## Scope, complete identity, and calls

1. Validate all actual parameter/capture/result types and declaration contexts, then enumerate their borrowed positions and qualified type-class boundaries. Every required position/class occurs once; duplicate, missing, out-of-range, malformed or contradictory rows reject. Stop at a qualified boundary and use its independently checked local classes; do not invent a second flattened class relation for its children.
2. Canonical signature visitation is Parameter order, actual Capture order, then Result. At an outer Address, visit its direct borrow relation before its pointee semantic class boundary. Inside a qualified type, the existing nominal-metadata-first class numbering remains authoritative. Grounding is determined before assigning dense Fixed/Invocation numbers; first occurrence is measured over the full joint stream, not separate physical and metadata-only lists. Empty/unreferenced tails reject.
3. Fixed class mappings are invariant. They are grounded by actual retained parameter/capture/environment classes or the complete enclosing type context, not a result-only row. Invocation binders are fresh logical variables per actual call. If an Invocation variable is constrained by an incoming retained class, unify with that EXACT actual class; do not shorten, intersect, rename or freshen the actual stored class. Mapping a captured Fixed environment class to a fresh Invocation variable is invalid. Explicit forall and free Fixed domains remain distinct.
4. A thin pointer has no runtime environment. Its free Fixed type classes require an actual enclosing semantic context; a context-free pointer cannot invent them. A complete polymorphic thin signature can quantify its Invocation classes. Neither phantom nor code metadata supplies a borrowed runtime target.
5. Target conversion checks complete normalized relation and actual full signature, body return roots, mode, ownership, cleanup, CallKind/effects, resume/thread constraints. Different closure roles that share ClosureId still use their declared relation separately. No pointer construction-time object can discharge a later call's argument loans.
6. Call checking first matches semantic class equalities, then substitutes actual target/slot/parent facts independently. A class match never proves alias separation. Returned borrowed values need actual proved roots; CopyShared versus Reborrow carrier/epoch retention remains the common stored-borrow rule. Fixed environment dependencies remain live independently of input-derived returns.
7. Legacy flat FunctionLifeContract is an exact computed projection only. If retained in transitional Core diagnostics, check it against the rich relation before lowering; it cannot override or supplement the rich contract. Runtime and interface identity serialize only the rich authoritative contract.

## CT5 and source-scope context

BFP graph nodes/constructors preserve the exact same contract. Physical parameter/result children, nested qualified classes, and owner backreferences are all retained. Owner references are semantic edges and are separately checked, never physical DAG edges. A BFP owns its Invocation domain; its free Fixed domain must remain related to the actual enclosing context. No independent alpha-freshening of nested Fixed arguments or node-only validity cache.

There is a second concrete loss in CT4: separate local Fixed0 graphs for `Phantom<'a>` and `Phantom<'b>` can collide even though both came from distinct actual binders of one source scope. `model.rs` therefore uses one artifact-wide ambient scope inventory and per-graph `ambient_classes` mappings. This refines the first draft's owner enum: existing CtLifetimeArgument remains a graph-local owner reference; CtAmbientLifeReference is a separate typed source-scope namespace. A graph Fixed class maps once to an authentic source scope/full parameter ordinal, so physical-only borrowed classes are represented too, not only nominal Lifetime arguments.

The actual frozen invocation/source view must authenticate each ambient declaration, lexical binder site, complete generic-kind table, full parameter ordinal and complete actual instantiation vector. A raw table or hash alone cannot authenticate a scope. Same artifact graphs use the SAME inventory; a standalone graph without that context rejects nonempty ambient_classes. Schema life_context and TypeInContext still preserve joint actual argument relationships. A free Fixed class cannot silently become a fresh graph binder; it needs an authenticated ambient mapping or an actual enclosing rich callable relation.

The concrete freeze point and production migration are specified in `frozen-context.md`. No currently existing FrozenReadView field already carries generic instantiation authority: a private FrozenInvocationContext must be created from the actual source/Origins traversal and real specialization operation, then borrowed by BOTH FrozenAuthority and FrozenReadView. Full resolved declaration/binder/actual context enters identity; raw IDs, free strings, Span and unchecked hashes do not.

## Version and bounded consumers

Use a fresh runtime schema/Wire40, TypedModuleV40 and TypeIdentityV40. CallableParameterContract's separately serialized changed field requires its next domain version (currently5 ->6). Any changed independent callable/interface domain also advances; specifically the new STORAGE-LIFE-CONTRACT-1 domain must advance to2 when classes change its bytes, and LIFE-INVENTORY-V39 advances toV40 when complete callable type encodings inside the inventory change. Unchanged historical encoders remain frozen. CT BFP contract/owner/context bytes change, therefore CT artifact/schema/program/provider/dependency/function/value domains advance4 ->5. Interface/export/cache keys must include complete class rows and resolved contextual types.

Every row/vector/path/string/table lookup, full type traversal, clone, sort/comparison, remap, fixed point and contextual key must prepay from the caller's same remaining work/depth/node balance. Bounded codecs validate width/depth before growth. Consumers not yet supporting a nonempty class map or ambient scope explicitly reject it; empty defaults do not erase fields. Metadata transport must not reopen foreign/Suspend/static pointer or ownership profiles.

## Required retained controls

- Same/other Tag class in an otherwise identical two-input BFP; alpha renaming equal, swapped relation distinct; full raw/Wire/CT roundtrip.
- Parameter ordering, phantom Type arguments, two nested callable quantifiers, and nested free Fixed versus nested Invocation boundaries.
- One ClosureId under two different declared rich roles, actual call window/return source controls and wrong-capture substitution.
- Phantom-only signature classes cannot create return roots, permissions or a runtime holder.
- Two actual source ambient binders each locally Fixed0 remain distinct; same ambient binder shared twice stays equal; relocation and repeated Span do not alter identity; wrong sibling/source scope rejects.
- Source -> Core -> both kernels -> both native backends O0/O2 for the exact Code fixture once representable; N/N-1/fresh long/deep class paths and shared-context growth controls.
