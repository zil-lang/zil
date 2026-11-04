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
