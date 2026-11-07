# Structural POD Store elements

POD `StoreElement` qualification applies to the complete value representation.
The root may be an existing supported scalar, finite Record, Tuple, or fixed
Array. Tuple fields and array elements recursively use the existing POD field
rules. The complete layout must have positive size and supported alignment;
zero-sized roots, overflowing layouts and recursive inline types reject.
Array length is part of the complete type and must be known and positive.

Qualification proves the declared Copy and StoreElement obligations at each
symbolic field. `Copy` alone does not establish POD layout or positive size;
`StoreElement` alone may include owning values and does not establish Copy.
Unresolved array size or layout obligations require an actual declared proof
and cannot be supplied by a later convenient instantiation. Existing owning
Package element qualification remains a separate responsibility rule.

The canonical Store resource is derived from the complete actual element type.
New Tuple and Array roots use `@builtin:OwnedStore<tuple:HEX>` and
`@builtin:OwnedStore<array:HEX>`, where HEX is the lowercase complete
`Type::fingerprint()` digest. Existing Record/scalar names remain unchanged.
Each checker charges the complete bounded type before hashing, then checks
exact resource payload equality and rejects duplicate canonical names. A
digest only locates a candidate; it does not equate different actual types.
Tuple order, array length, scalar kind and every nominal field identity remain
in equality, fingerprints, helper keys and ABI. Equal byte size cannot equate
a tuple, array or nominal Record. Ordinary aliases resolve to their actual
types. Shared and exclusive element windows keep the actual Store owner and
existing parent-loan rules; indices do not invent static disjointness.

POD elements have initialized value bytes without individual cleanup debt.
The Store retains its real allocation and initialized prefix, with exact
allocation failure, reserve, push/pop, move and destruction behavior. Address,
description, function/callback, owning and borrowed-storage leaves do not enter
this profile through their Copy property. Their independent qualifications
remain necessary.

Types, Analyze and Verify independently derive qualification, canonical
identity, layout and actual window sources, including unused definitions and
raw/Wire inputs. Lower and both native backends use the checked recursive POD
layout. New traversal, hashing, layout and copy work is prepaid within the
existing request limits. This uses the existing complete Tuple/Array and
OwnedStore representations; it requires no new public schema field.

Authority: `_zil_design.md` §§8.1–8.2. Acceptance includes real Source libraries,
both backends at O0/O2, allocation/cleanup controls, equal-layout type swaps,
zero-size/length and overflow neighbors, and fresh budget boundaries.
