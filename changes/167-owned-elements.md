# 167: finite homogeneous owning arrays

## Normative basis and scope

This slice follows `_zil_design.md` ZIL-MODES (ordinary non-Copy Values move),
ZIL-ORIGIN (only stable precise One locations admit strong updates),
ZIL-CLEANUP/ZIL-PACKAGE (prepare before publish, actual remaining obligations,
fixed aggregate field order), and the existing `[T; N]`, `[a, b]`, `a[i]` syntax.
The production baseline is `45dbb920d444ab54a307785d068c934926cfb90c`;
dispatch is `4ba11e1`. It does not change the language's source syntax.

The first vertical slice admits nonempty fixed arrays `[P; N]`, where `P` is an
existing finite Package with Copy fields and explicit Managed resource fields.
Package is already an ordinary owning Value, so the element category is unique
in local inference, parameters, and function results. A resource name `R` still
denotes its established Desc mode; `[R; N]` is not silently changed into an array
of Managed heads. A Package wrapper can own either a user resource or ByteBuffer.

Array construction evaluates elements once, left to right. Each successfully
prepared element has a real temporary carrier until an atomic complete-array
publication. A later recoverable error cleans prepared temporaries in reverse
successful construction order. Complete array cleanup visits indices in
increasing order, and each Package uses its existing declaration-order cleanup.

Whole arrays move through ordinary Value operations and complete SSA/function
boundaries. Local arrays may temporarily contain moved-out elements. An element
move requires a live shell and exactly that live element; an array move requires
every element live. Refill evaluates and stages its RHS before conditionally
cleaning the old element and committing the new responsibility. Partial arrays
remain local and clean only the elements whose responsibility they still hold.

## Exact element operations

The first slice accepts a statically fixed integer index of a named local array.
`a[0]` transfers a whole Package element. `a[0].copy_field` reads a Copy value;
`a[0].managed_field` borrows the actual inline resource head as Desc. Existing
resource method rules then apply to that description. `a[0] = value` requires a
mutable array shell and matching complete Package. `give a[0]` is not an alias
for ordinary Value movement, and direct partial movement of a Managed field out
of an array element is not added. The caller can first take the complete Package.

Descriptions and borrowed ByteBuffer windows retain their actual carrier root.
Element take/replacement, whole-array movement, and cleanup must reject a
conflicting later use. Conservative whole-array window roots are permitted;
different index spellings do not manufacture separation proofs.

Dynamic indices, slices of owning elements, nested owning arrays, arrays captured
by closures, and dynamically allocated generic element containers remain outside
this first slice. Their acceptance needs its own actual location/cardinality,
environment, or explicit allocation protocol. Unsupported paths are not counted
as completed features. Literal out-of-range and negative indices are rejected.

## Public untrusted representation requested from 169

The existing `Type::Array { element, length }` remains the type and stays non-Copy.
Root owns the shared IR/Wire/schema changes. The requested operations are:

- `MakeOwnedArray { elements }`: consumes a complete, typed homogeneous list.
- `ProjectOwnedArrayField { local, index, field }`: Copy field read or actual
  Managed-field description, determined by the independently checked element type.
- `TakeOwnedArrayElement { local, index }`: moves one complete live Package.
- `DropOwnedArrayElement { local, index }`: conditionally retires its responsibility.
- `StoreOwnedArrayElement { local, index, value }`: commits to an empty mutable slot.
- `DropOwnedArray { local }`: conditionally clears the shell and each live element.

All indices are raw untrusted integers, not proofs. Types and Analyze produce
diagnostics; Verify independently validates types, bounds, initialization,
provenance, nonduplication, and responsibility conservation. Existing Copy-array
Load/Extract/Update operations must not admit these owning values.

Length and element-field products are charged before materializing per-element
state, flags, or code. Tiny raw type metadata cannot cause unbounded enumeration.
The LLVM representation stays inline, with explicit local shell/element flags;
no implicit heap box, reference count, GC, or global responsibility table is added.

## Required evidence

Real source tests cover construction, complete parameter/return moves, field
reads, element take/refill, preparation errors, replacement errors, and ordered
unique cleanup at O0 and O2. Closest source rejections cover duplicate moves,
incomplete whole moves, invalid indices, immutable refill, forbidden `give`, and
old descriptions/windows after responsibility transition. Independent raw/wire
mutations must reject duplicated element debt, forged slot indices/types, skipped
cleanup/replacement debt, and attempts to use Copy operations on owning arrays.

Evidence is recorded under `run-logs/167`; Git retains only this proposal, source,
tests, and compact receipts. Full-language, dynamic-container, performance, and
product acceptance are not implied by this slice.
