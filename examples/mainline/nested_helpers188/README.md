# Finite nested helpers with an owning State

The four ZIL modules exercise a real high order library path: `state.run` borrows
the callback field and calls `helpers.append`, which calls `append_leaf`, which
invokes the actual environment. A second path uses `peek` and `read` to return a
shared view from a different owned environment. Both factories retain a
ByteBuffer and a Drop marker. The State updates its independent Copy counter
while the callback's result view is live.

The executable reads one digit, 0–3. It prints `A`, the selected prefix of `BCD`,
then `HAZ`. `H` is the State callback's marker and `Z` is the outer snapshot's
marker. Invalid input exits 9 without output. The allocation runner has an
explicit oracle for every allocation failure, including the exact live buffer
count, frees, prefix output and exit status.

```sh
python3 tools/language/188/consumer.py --repo . --cli /absolute/path/to/zil --out /fresh/output
```

The runner compiles O0 and O2, requires all four loaded modules and one independent
verification, and checks the two factory interfaces and four actual instantiated
helper contracts. It executes 38 Linux cases, including 20 real allocator
failures, and retains original command receipts. Executables and LLVM output are
temporary test products and are excluded from the source delivery.

This slice admits one actual callback parameter with a matching declared kind
through a finite helper dependency DAG. Ordinary branches, zero calls, ordered
multiple calls, error returns and defensive Trap prefixes compose without a new
schema or runtime ABI. FnOnce forwarding still moves one responsibility even
when the child never invokes the callback. The verifier reconstructs callee
bodies and actual parameter origins before using their trace claims; the same
ClosureId does not make two runtime instances interchangeable.

The existing owning State loop is separate from a callback helper's finite
contract. A cyclic callback helper body or a recursive helper dependency remains
Unsupported. Mixed bound kinds, multiple callback parameters, unknown or mixed
environment origins, retained borrowed environments and general lifetime/HRTB
contracts are not implemented here. Explicit Address callback arguments belong
to the separate 189 slice. This source does not depend on the in progress 187
local borrowed ByteBuffer support.

## Rust goals for this slice

The source and raw/Wire checks establish finite origin, loan, trace and cleanup
properties, and actual allocator failures exercise the dynamic consumer. They do
not establish whole language safety. The implementation preserves the existing
inline environment and ordinary call ABI, but no equivalent Rust runtime or
memory benchmark was run. Nested helpers remove a concrete library composition
restriction; this is an implementation advance, without a measured maintenance
cost advantage. Compile speed is a separate workstream, and no comparative
compile performance result is claimed. The four product goals and a repeatable
product level net breakthrough remain unproven; full language NotSatisfied and
product NotRun are unchanged.
