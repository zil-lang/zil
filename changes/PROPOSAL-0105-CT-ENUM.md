# 105 — verified finite Copy enums in compile-time programs

Production baseline: `24ca9583d8789cfdfe3ccdbfe3eeff614bf56429`. The main-tree
`.task/impl/105.md` is authoritative. This is independent of 103.

## Closed capability and data contract

The existing ordinary enum operations `MakeVariant`, `VariantTag` and checked
`ReadVariant` enter the CT CFG whitelist only after ordinary independent Verify
has reconstructed type, initialization, SSA, payload arity and tag/index bounds.
Every field of every variant must recursively be an existing serializable Copy
value: unit, bool, supported integers, tuples, records, fixed arrays and now
finite enums. An inactive variant cannot hide an address, foreign handle,
resource, callback or closure capability. No new IR/wire tag or default quota is
introduced. The immutable private `VerifiedCTProgram` remains the only VM entry.

The VM data representation adds `{ variant: u32, fields: Vec<Value> }`: one
explicit tag and only the initialized active payload. This is serializable data,
not a native address or verified wrapper. Constructor and projection payload
shapes come from the independently verified program. A checked projection of a
different active tag returns `E-CT-TRAP` before reading a field, matching runtime
checked projection semantics. Raw and wire candidates are reverified; changing
a cached fingerprint does not authorize malformed tags or payloads.

## Work, storage and reification

An enum's reserved VM cell shape is two cells (container and tag) plus the sum of
the recursively reserved fields of **all** variants. This conservative static
shape is computed with checked arithmetic and the existing cap before frames or
aggregate copies are allocated. It bounds all active payload choices and does
not let a wide inactive variant disappear from the quota. Existing aggregate
clone/store/call/return charging uses this shape. `MakeVariant` pays that shape;
`VariantTag` pays its instruction step; `ReadVariant` pays the tag check and its
projected result shape. Unit payloads count as cells. Existing transient-copy
and total live-frame reservations still apply.

Reification makes ordinary untrusted Core: a constructor call, member selection,
namespace name, and recursively materialized active fields, all with fresh
NodeIds. All three constructor nodes, payload nodes, nesting depth, nominal/tag
text and any copied record/type metadata are charged before allocation. Source
queries share the original step/node/work budgets across the module. No
reification result constructs a Verified wrapper; Types, Analyze and Verify
check the resulting program again before LLVM/native execution.

The namespace uses the full existing nominal name `@moduleN:Declaration`, which
source identifiers cannot spell. The three existing ordinary enum-constructor
lookup paths resolve its declaration key in their existing map and then require
the resulting `Type::Enum.name` to equal the complete supplied name. They do not
scan all types, invent a declaration, drop the owner, or bypass type/visibility
checking. This prevents a runtime local with the same source spelling from
capturing a reified constant. A forged owner or unknown declaration is rejected.
The root approved these three Types files in the main task before implementation.

## Required evidence and exclusions

Source tests cover empty/unit/compound payloads, nested enums in records/arrays,
ordinary pure helpers and match/loop control, same-spelling local values and
cross-module declarations, and a real multi-module dynamic-input consumer at
O0/O2. Raw/wire tests distinguish malformed candidates rejected by Verify from
a well-formed wrong-active-tag projection rejected by VM execution. Negative
capability controls include forbidden types hidden in inactive variants.

Exact N/N-1 checks cover execution steps, live cells and fresh Core node/depth
reification; bounded deep/wide shapes and repeated queries are retained. Source,
phase IR, command, stdout/stderr and test-binary identities are preserved,
including the first failed attempts. Product gates and Rust performance claims
are outside this functional slice; the four-goal reflection records that limit.
