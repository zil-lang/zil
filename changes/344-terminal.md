# 344: bounded source inference and machine reliability

## Stack failure and proposed change

The preserved legal `large-copy-enum-error-borrow-capture-var.zil` completes
Parse/Core but aborts on a 2 MiB API thread during the CT closure-constraint
prepass, before Types/Analyze/Verify. An alternate-signal-stack abort trace of
the retained 338 rlib resolves `infer_source_closures -> prepare ->
Solver::expr_raw`, repeated through `expr_expected/expr/projected_base` for the
13 array projections. The unoptimized x64 `expr_raw` frame reserves
`0x1e568` bytes (124264): every projection retains storage for unrelated large
call/closure/member branches.

Split recursive expression variants into separate private helpers. Keep the
node/work charges, expression-index increment, write-place restriction and
evaluation order in the original dispatcher, at the same position and exactly
once. Each helper retains only its own variant's temporary values. Do not raise
thread stack sizes or language limits, introduce worker threads, skip inference,
or change public AST/IR/Wire and verified authority boundaries.

The acceptance change is reliability: the original invalid `let f` source must
return its ordinary Types `E-CLOSURE-MUTABILITY` diagnostic, and the legal `var f`
sibling must reach independent Verify on the default 2 MiB API stack. Their
source bytes remain unchanged. Neither input currently supplies evidence that
Verify itself was entered before the abort.

## Required observations

Run both inputs in subprocesses using a default Rust worker thread so a renewed
stack abort cannot terminate the Rust test harness. Preserve Parse/Core stop
controls, complete legal Verify and raw Types/Analyze/Verify artifacts, and
Clang/API O0/O2 native execution. Add a deeper nearest legal projection case and
an actual low-limit failure; budgets retain their existing meaning. Broader
large callable/helper/foreign and floating/anonymous-union obligations remain
open until real source and machine evidence closes them.

## Deeper legal control

After the first split, both preserved sources return normally at 2 MiB. A new
20-layer length-one array closure then reaches Types and exposes the equivalent
large-frame recursion in `closures::InferBuilder::expr_raw`. Apply the same
per-variant split there; keep its `visit` charge and all graph work unchanged.
This is a second reproduced failure, not an assumption about the first abort.

A 32-layer ladder-initialized projection then exposes cumulative Types
`Builder::expr_raw/aggregate` frames (expr_raw alone: 0xb3e8 = 46056 bytes).
Keep scalar/operator arms available to 340; split only Call/If/Sequence there,
and the six aggregate branches into private helpers. Retain the original
post-dispatch expected-type check and early-return behavior exactly.

## Floating-point terminal semantics

Root froze IEEE binary32/binary64 with raw-bit constants and exact operand
types, unsuffixed float defaults to f64, contextual/suffixed f32/f64 literals,
checked finite literal overflow, gradual underflow, signed zero and normal
IEEE infinity/NaN arithmetic. Comparison is ordered except `!=` includes
unordered operands; no fast math, reassociation or contraction. Numeric
conversion with a floating endpoint has a separate checked operation; existing
integer-only conversion invariants remain. Float-to-integer requires a finite,
integral mathematical value in range and guards before LLVM fptosi/fptoui.
Finite conversion overflow to infinity rejects/traps, with underflow allowed.

The machine event model gains explicit float types and raw-bit constants,
floating arithmetic/predicates and validated numeric casts, consumed identically
by textual LLVM and the LLVM API. Raw bits use constant bitcasts rather than
host-decimal serialization. Nearest counterexamples include mixed numeric types,
non-numeric casts, fractional/NaN/infinite/out-of-range integer conversions and
integer machine instructions applied to float types. Actual CT/native O0/O2
agreement and edge values are required before declaring this capability closed.

Float source parsing debits `bytes² + 64` work before underscore normalization
or software decimal conversion. Independent review found that reconstructing
NaNs with arithmetic would lose raw payload bits from a constant-returning CT
artifact. Reification therefore reserves 26 work units, 26 temporary bytes and
one generated node before emitting generated-only `Literal::Float` spelling
`@bits:f32:XXXXXXXX` or `@bits:f64:XXXXXXXXXXXXXXXX`, using exactly 8/16 canonical
lowercase hex digits. Source lexing cannot produce that prefix. Every generated
AST remains untrusted: Expand prepays its full byte length before validating and
emits the existing Core raw Constant directly; the artifact compiler uses the
same fixed-byte charge and no decimal conversion. Malformed width/case/hex and
format mismatches reject. Same-format conversions preserve every raw bit;
true CT arithmetic retains its specified canonical quiet NaN behavior.
The lower storage table gains the two natural float layouts. Ordinary C scalar parameters/results and borrowed POD leaves admit
f32/f64 with matching independent Verify layouts; ownership, escape and
callback contracts retain their existing meaning.

The real generic call `same(1.0 + 2.0f32)` exposed an early f64 default
before the explicit RHS format. Float metadata hints now inspect an explicit
other-operand format before defaulting an unsuffixed floating literal, including
grouped/negated literals. Group/unary metadata inspection debits actual node
work; value evaluation order remains left-to-right.

## Unsigned composition

The parent integrates complete UInt340 production and public shapes into the
reviewed float candidate. Preserve disjoint inference domains (U64=256,
USIZE=512, F32=1024, F64=2048) and both target-width CT ledgers. Floating numeric
conversion accepts the full unsigned mathematical range, uses unsigned LLVM
casts, and checks exact 2^64 / target-width powers of two before fptoui. CT
conversion reads the target pointer width rather than treating every usize as
u64. The nearest legal f64 below 2^64, exact 2^63, rounded u64::MAX to f64,
exact 2^64 rejection and 32-bit usize bounds must execute on the composed code.
This does not change integer-only conversion or default budget semantics.

The stronger real `Carrier.Value(Mixed{word:u64,amount:f64}, usize)` source
reaches Verify1 and Lower, then codegen reports `non-Copy enum payload physical
layout`. The Enum physical-layout and scalar payload-store consumers omitted
U64/Usize. Add their existing natural x64 8/8 representation to those private
tables; actual Type identity and ordinary tag/payload behavior remain unchanged.
The mixed source is retained as the machine regression, rather than claiming a
scalar-only conversion test covers aggregate storage.

## Anonymous structural tagged sums

Represent `A|B` using existing raw `Type::Enum` with reserved family marker
`@anonymous-sum:v1`, and variants `Member0`, `Member1`, ... containing exactly
one complete resolved member Type each. The marker is not a compressed identity:
Eq/Ord/Wire/interface keys retain all payload Types. Resolve aliases/nominal and
phantom identities first, flatten directly nested marked sums, prepay complete
shape comparison/sorting work, sort actual Type values and reject duplicate
members. Independent Analyze/Verify validate the canonical shape and strict
full-Type ordering after bounded raw-type admission. Different member sets with
the same marker must remain different cache/type/layout candidates.

Only an already determined expected sum admits injection of a unique exact
actual member Type. Infer an unconstrained literal normally before comparing;
never try numeric conversions or backtrack over members. Equal sum values keep
their tag. Lower injection to the existing ordinary MakeVariant and retain all
Copy/owned active-payload, source, borrow, move and cleanup checks. Both ordinary
and closure/source inference prepasses obey the same exact-type relation.

`TypeName(binding)` selects the full resolved member identity; imported names
and aliases naming tuple/array/generic members are ordinary type resolution.
Private alias-pattern metadata can carry the full Type into existing Core
Variant data; it grants no safety authority. Coverage and Copy/owned observation
and unpacking consume the selected real variant index. No public runtime opcode
or raw union reinterpretation is introduced.

Type-valued CT uses root-approved `CtTypeNode::AnonymousSum(Vec<TypeNodeId>)`
(tag15) and `CtTypeConstructor::AnonymousSum` (tag11). Raw Type DAGs and generated
AST remain untrusted: independently check arity, bounded full identities,
flatten/canonical order and duplicates. Complete Type reification returns an
ordinary AST Union; scalar/Copy CT values reify a full Core Ascribed sum with
only the active payload, letting ordinary injection recreate the tag. Do not
resolve a structural sum through a nominal namespace or index it by marker name.

Required real controls include reordered/nested/aliased sums, nominal and phantom
members with equal layouts, exact literal injection/rejection, exhaustive and
wildcard patterns, CT/Wire/cache identity, ordinary closures/helpers, large
Copy and active owned cleanup/borrow behavior, nearest raw canonical violations,
N/N−1/fresh bounds and Clang/API O0/O2. Unused invalid definitions reject; no
implicit heap/RC/lock, larger budgets or raw Type permissions are introduced.

### Follow-up: every generic structural key pays its own work

Independent review of f01e61bcc replayed all source/CT/native controls but found
that alias normalization prepayment did not cover later generic-instance and
coverage key construction. The recursive Union renderer could flatten, copy
and compare complete long keys without debiting those operations at each call.
Thread the existing expansion Budget through bounded unification/key entry
points. For requests whose existing AST walk recorded anonymous sums, compute
an allocation-free bounded key layout, including nested constructor keys,
flattened member counts, copied string bytes and worst-case full comparisons,
and reserve that work before rendering or sorting. No-Union requests retain
the original path and fees. Alias sorting remains a separate paid operation;
it is not a transferable credit for a later key construction. Retain real
long-common-prefix members nested inside generic/tuple/sum types and exact
warm/fresh N/N-1 controls, alongside inherited float/UInt budget neighbors.

### CT artifact value sums and nominal arguments (schema 3)

Root publishes the schema3 contract separately from the reviewed Type-stage
candidate. Add structural CtTy/CtValue AnonymousSum forms and exact injection
with the existing CT tag/field consumers. Descriptors preserve every inactive
member and select active payload by complete identity, never a schema-table ID
or runtime-layout coincidence. Source inference defaults uncontextual literals
normally; no numeric trial injection is introduced.

Both nominal schema tables retain ordered Type and Scalar instance arguments,
including phantom arguments. Source production binds the actual declaration and
all arguments; independent CT admission checks well-formed pure arguments,
acyclic identity graphs, canonical sums and unique full nominal instance keys.
The execution/value validator checks the active descriptor and payload; each
AST/runtime bridge reconstructs the full sum and maps by complete active member
Type. Raw type/value metadata supplies no nominal, syntax or runtime authority.

Schema/domain3 explicitly invalidates previous v2 artifacts and values. Preserve
v2 frozen bytes as rejection controls rather than changing their contents or
calling a header edit an authenticated migration. Charge constructor/schema
argument walks, complete-key comparisons, every generated descriptor/copy and
materialization on the passed request ledgers before allocation; generated AST
still goes through ordinary Types/Analyze/Verify. Keep the completed Type-stage
receipts separate from these initially incomplete value consumers.

CT3 raw admission bounds all nested value-type metadata before calling frozen
artifact authentication, whose equality comparison must not recurse through an
unbounded candidate. Move the existing shape checks before authentication
without duplicating their old node charges; newly represented schema arguments
and anonymous descriptors add paid identity work. Full nominal keys include the
symbol and every ordered Type/Scalar argument. Repeated full instances and
identity cycles reject independently; lexicographic comparisons and key buffers
are paid before copying or comparing. Ordinary zero-argument, non-sum shapes
retain their existing shape-walk charges.

The root corrected CT3 Type schema arguments to CtTypeRef, since a phantom
ordinary fn pointer is legal metadata while its pointer value cannot execute in
CT. Normalize every argument graph through the bounded constructor graph model;
admission independently requires canonical postorder structure and full keys,
including all signature and eventual BFP lifetime relations. New metadata-only
bindings must remain separate from serializable CtTy data bindings. No pointer
CT value, fake nominal schema, or free lifetime spelling becomes authority.

A retained actual CT-source negative exposes another pending producer defect:
Left.V matched by Right.V currently selects the variant by its final name. The
source matcher must resolve the complete owner instance before selecting a tag;
nearest Left.V and mismatched Right.V sources stay separate. This checkpoint
contains raw VM/metadata consumers; the source Sum/value/reification bridge and
phantom callable source positives are still unfinished.
## Integrated scalar, associated and borrowed-code identity

The combined341/342/344 tree prepays complete anonymous-member keys for typed
scalar const arguments, internal MetaEval carriers and symbolic/radix array
lengths without allocating during layout. Associated equation lookup passes the
same Budget; its later substitution renderer is separately prepaid. Requests
without an anonymous sum retain their existing key charges. The CT canonical
borrowed-code key preserves the full parameter/result graph and joint lifetime
contract, including its remap and temporary accounting. Combined actual source
controls cover const specialization, associated projections and a borrowed code
member round-tripped through a computed Type, with native O0/O2 on both backends.

## Combined deep-expression host-frame repair

After the associated-constant composition bfe682bfb, the original default2MiB
worker and a large value-order source crashed again. A separate alternate signal
stack for diagnostics (no change to the ordinary worker stack) localized Expand
expr_expected at88,296 bytes, with Budget::ty_context12,872 bytes. Splitting
expression variants and associated lookup temporaries reduced the dispatcher
frame; the next trace exposed Types::expr_raw26,792-byte frames during nested
owned-array construction. Its remaining scalar/name variants are now private
helpers with the original final-check and early-return ordering intact.
The original106 aggregate tests pass including the exact2MiB source and four
native backend/optimization runs; original budget checks remain unchanged.
A wider ordinary/generic nested-call probe still finds an additional32-call
stack failure in the call-planning helper. It is preserved and under repair;
this checkpoint does not claim all bounded source shapes are stack-safe.

Integrated recursive-call follow-up: preserve evaluation/constraint/charging
order while moving pre/post-call work out of the recursive argument frame, and
split Budget expression variant handling. The original aggregate target passes
106/0 on the default test-thread stack, and composed Registration passes7/0.
The author Expand-only probes accept bounded nested ordinary/generic/conversion
calls through124, with parser ResourceLimit at126/128; the full-driver replay is
still required and is not inferred from those Expand-only probes.

Full-driver nested-call probe after5632 exposed later Types and CT walker host
frames; Expand-only success did not prove the complete pipeline. The new
mechanical extraction is being validated on the original default-stack probes.

The independent Vec identity reference now includes Registration32/opcodes111–113.
Its five new Tuple/Place instruction probes are kept separate from the original
95-operation V31 anchor input; the historical pinned anchors remain unchanged.
The first reference replay failed because those additions had changed that input;
after restoring the original case set and testing additions separately,12/0 pass.
All-target llvm-api Clippy passed before this reference-fixture-only adjustment;
Expand lint-equivalent extraction replay passes the complete package.

Final integrated stack repair: unchanged original aggregates106/0; the retained
nested-call test additionally runs five source families at32/64/96/112/124 on a
default Rust worker stack and gets Parse ResourceLimit at126. The plain96 source
also checks exact max_nodes2348/2347 and max_call_depth197/196 on fresh and warm
sessions. No stack override, source truncation or default quota increase is used.
All-target llvm-api Clippy passes, with the final added test separately linted.
The author independently relinked30 child probes and replayed the copied original
2MiB/raw/Wire/N2520/native4 control; compact source-bound receipt is retained.
These bounded shapes establish this regression fix, not a theorem for all input.


### CT3完整名义实参的公共元数据修正

普通phantom函数指针参数可形成合法类型，但CtSchemaArgument::Type(CtTy)不能表达其完整身份。根集成方将尚未验收的CT3合同修正为Type(CtTypeRef)，复用同一规范有界TypeDAG；Scalar编码不变。实际CT字段和值继续只接受CtTy纯值，不生成运行时指针。两种元数据表示不并存，声明实例的全部有序参数、关系、图规范及来源须由独立核验证；当前生产CT2与历史v2字节不改写为新v3。344随后实现元数据绑定和可序列化值绑定的区分并保留phantom FP正反例。

### CT3 source-value WIP checkpoint after root37 composition

The retained `artifact-values.zil` now executes the verified CT artifact lane,
reifies both active nominal members with the complete anonymous Sum type, and
passes ordinary Verify plus Clang/LLVM API O0/O2 native execution with result42.
Generated-only AST Ascribed nodes carry both the whole sum and each active
payload's full type; they enter the ordinary Expand/Types/Analyze/Verify path.
The source enum matcher independently resolves the complete owner instance:
nearest Left.V passes and the retained wrong-owner Right.V candidate rejects
E-CT-SOURCE. The exact two-test command and output are recorded in
`/tmp/344-ct3-source-bridge-second.log`; this is a construction checkpoint, not
acceptance of all CT3 source metadata. Phantom fn/BFP argument binding, complete
visitor/accounting controls, and final composition replay remain unfinished.

CT3 closed nominal production now stores complete canonical Type graphs in its
instance cache and separates them from pure field-value types. Actual source
controls retain phantom fn and higher-rank borrowed-fn arguments through CT
execution and ordinary native compilation; the latter exposed and corrects a
missing complete higher-rank generic key and Copy-code-pointer case. The key
records the canonical joint invocation relation, preserving alpha equivalence
and different input/result relationships. Its planner prepays binder-name
lookup, remap storage and full output before allocation; old requests without
higher-rank keys or anonymous sums retain their key fees. Signature addresses
remain metadata, and physical CT pointer fields still reject.

The next source checkpoint adds generic enum tuple/unit reification with full
owner arguments and active payload mapping. Four CT3 aggregate tests pass with
native controls (Sum42, phantom fn7, phantom borrowed-fn42, generic enum42),
while the existing generic CT compiler target passes16 focused regressions.
Source reification pays complete generated type and clone traversal, including
const expressions; intermediate Type graphs release their own temporary charge.
Generic CT function specialization of metadata-only arguments, scalar nominal
arguments, and the final independent raw/budget/source review remain pending.

### CT3 full metadata specialization checkpoint before common39

Concrete generic CT functions now specialize/cache/queue complete canonical Type
graphs, while their definition checks keep separate private symbolic Copy models.
Explicit and inferred phantom fn/BFP arguments, returned Type values and an actual
ordinary borrowed-pointer call retain the joint relation. Copy qualification of
metadata visits genuine nominal definitions without constructing pointer fields
or inventing schema IDs. Vacuous higher-rank metadata normalizes to plain code.
Closed nominal arguments retain ordered Type/Scalar arguments and const array
lengths. Closed alias targets normalize scalars in their declared domain before
aliases disappear; record constructors transport the same typed const carriers.
The newly paid scalar and graph normalization work is on the existing ledgers.

Before frozen artifact cloning, a borrowed planner accounts recursive field/value
types, complete schema arguments, function/instruction vectors, BFP arrays,
template tokens/owner types, input tokens and their actual bytes. This replaces
coarse clone estimates; its additional traversal and clone storage are new work,
with no default quota increase. Reified intermediate graphs release their own
charge, and generated AST/type copies remain paid through ordinary checking.

The exact current targeted run is source9/0, including40 actual Clang/API O0/O2
native executions across10 positive programs. A new distinct-nominal control
forces CT declaration order versus runtime member order to differ, scores the
branches differently, and also passes after source declaration reordering.
Alpha/reordered life names pass; wrong result relation, free life, physical CT
pointer data, wrong scalar domain/member and wrong enum owner reject. Existing
CT generic16/0 retains exact fresh work/node/byte/instance/depth and cold/warm
controls. This remains WIP: common39/CT4 scoped metadata identity and all consumers,
owned Sum cleanup, final source budget/cache/default-stack composition and
independent acceptance are not inferred from this checkpoint.
