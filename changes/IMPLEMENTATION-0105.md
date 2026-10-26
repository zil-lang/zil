# 105 — finite Copy enums through CT execution and ordinary rechecking

Production baseline: `24ca9583d8789cfdfe3ccdbfe3eeff614bf56429`.
The main-tree `.task/impl/105.md` owns task state and review. The implementation
uses the existing enum contract and remains independent of generic-alias task
103. The main task's additional three Types files were authorized before their
changes. This document records author evidence, not independent acceptance.

## Result and implementation boundary

Pure `#(...)` queries can construct finite Copy enums, inspect their tags,
execute checked payload projections, return nested enum/record/array values,
and materialize those values into runtime source. Both explicit unit payloads
and variants without payloads work. Ordinary helper calls, loops, match,
existing generic enum instances, and multiple source modules use this path.

`zil-verify` still performs ordinary independent module verification before its
CT-only capability check. The only newly admitted instructions are the existing
`MakeVariant`, `VariantTag`, and `ReadVariant`. All fields of every variant must
recursively be serializable Copy data, including inactive variants. Foreign
handles, addresses, dynamic storage, callbacks, and closures acquire no CT
execution authority. The private immutable `VerifiedCTProgram` remains the
only VM entry. IR/wire schemas, default quotas, DB, Analyze, LLVM lowering and
product gates are unchanged.

The VM adds an explicit tag plus the initialized active payload. Independently
verified construction fixes the tag, arity, and field types. Loads, stores,
calls, aggregate updates and returns preserve that shape through ordinary
copies. A `ReadVariant` first compares the requested tag with the active tag;
it returns `E-CT-TRAP` before accessing a field on mismatch. Malformed tag,
index, payload type, undefined value or dominance claims fail independent
Verify before the VM receives a wrapper.

## Reification and nominal identity

An enum result becomes an ordinary Core constructor call with a member and a
namespace name, plus its recursively materialized fields. Every expression
node gets a fresh identity from the existing bounded lazy identity cursor.
There is no new Core enum literal, trusted result marker, or wrapper constructor.
The materialized source goes through ordinary Types, Analyze and Verify before
native generation. The compile reports retain those stage executions and one
ordinary verification run for every successful native build.

The constructor namespace carries the existing full nominal identity
`@moduleN:Declaration`. Three existing Types lookup paths share a helper which
looks up the declaration key in the existing map and then compares the complete
stored `Type::Enum.name`. An absent or different owner is `E-UNKNOWN-NAME`; the
lookup never drops the owner and falls back to a local spelling. It does not
scan all declarations or trust that Core came from the CT materializer.

This resolves runtime locals named like the enum in ordinary functions and
closures, preserves distinct same-spelling enums in different modules, and
handles normalized generic instances. Hostile SourceCompiler tests deliberately
return internally valid, wire-decoded CT programs with a forged owner, absent
declaration, or coherent but false payload shape. VM success remains pure data:
ordinary source Types rejects the forged owner/declaration or payload mismatch.

## Budget obligations

For serializable type `T`, the existing scalar and aggregate cell accounting is
extended by:

```
cells(Enum) = 2 + sum(cells(field) for every field of every variant)
```

The two cells represent container and tag. This is a conservative logical
reservation, not a claim about native byte layout. The sum includes all inactive
payload alternatives, uses checked arithmetic, and respects the existing cap
before frame allocation or data copies. Frame, transient-copy, store, call and
return accounting reuse this shape. Construction pays the full reserved shape;
tag inspection pays its instruction step; projection pays the tag check and
the result copy. Unit fields consume cells.

The source adapter bounds borrowed enum declarations before cloning them into
a query. Direct-constructor type hints bound every variant and field; they
charge the known namespace prefix, decimal owner length and declaration text
before formatting the nominal name. CT source reification also charges the
complete result type before cloning it, all three constructor nodes, payload
nodes, depth, text and copied record metadata. Queries share cumulative source
node/work and executed-step budgets. The default quotas were not increased.

The quota domains remain those of the existing APIs: Core and wire decoding
have node limits; already-decoded Typed Verify bounds metadata through its
work/depth budgets. Tests exercise each domain separately instead of adding a
new quota meaning to Typed Verify.

Measured exact boundaries of the retained small programs are:

| Boundary | N accepted | N−1 result |
| --- | ---: | --- |
| Enum helper VM execution, direct and wire | 22 steps | ResourceLimit |
| Same helper VM storage, direct and wire | 13 cells | ResourceLimit |
| Same helper call depth, direct and wire | 2 | ResourceLimit |
| Two actual source enum queries | 38 executed steps | ResourceLimit in Ct |
| One reified `E.N(42)` value | 4 Core nodes | ResourceLimit in Ct |
| Same reification depth | 2 | ResourceLimit in Ct |
| Same reification work, including constructor text | 15 | ResourceLimit in Ct |

The reifier also rejects identity overflow. A separate inactive-payload enum
with two `[i64;32]` alternatives reserves a peak of 136 cells even when its
active variant is empty; a cap of 70 fails. Wide 300-variant and nested 33-enum
raw/wire controls reject at their selected metadata quotas.

## Source consumer and retained verification

`examples/ct_enum` is a three-module program. A pure CT helper loops over an
array of enum values and returns a nested enum/record plan. The native program
then reads an actual byte with `getchar`, selects behavior by the materialized
plan, and writes the transformed byte. Seven inputs, including zero and 255,
and an empty-input error path run at both O0 and O2. The input is observed at
runtime; this is not a constant-output compiler smoke test.

Author checks are bounded to the changed surfaces:

| Check target | Passing test invocations |
| --- | ---: |
| `impl105_ct_enum` | 12 |
| Existing `comptime_programs` | 14 |
| Existing `impl088_ct_collector` | 6 |
| Existing `impl088_ct_preflight` | 12 |
| Existing `impl094_ct_preflight` | 5 |
| CT materializer unit tests | 4 |

The two adapter-including legacy targets each execute their existing included
adapter unit test; the table counts invocations, not distinct tests. Of the
four materializer unit tests, one is new and three preserve earlier identity
cursor controls. The final 105 source run retains 27 compile reports: 22 native
successes and five intentional source rejections, with 22 linked programs and
36 checked native executions. Raw/wire mutations, materialized Core, phase IR,
budget boundaries, compiler commands, execution stdin/stdout/stderr and test
binaries are retained separately. Clippy with warnings denied and formatting
checks pass. No full product gate or performance sampling is claimed here.

## First failures and corrections

All attempts are retained rather than overwritten:

1. `tests-001`: a local-shadowing positive fixture inferred an `i32` literal but
   added it to an `i64` payload. The valid fixture now explicitly annotates the
   local as `i64`. A forged canonical owner was already rejected, but the old
   fallthrough reported Unsupported; exact canonical lookup now gives the
   existing SafeReject `E-UNKNOWN-NAME` diagnostic.
2. `tests-003`: the new wide-metadata test incorrectly expected a node quota on
   already-decoded Typed Verify. The test was corrected to the existing API
   quota domains described above. No production verification limit was changed.
3. `clippy-005`: a needless extra borrow at the shared constructor lookup was
   removed. No warning suppression was added.

The evidence manifest binds every retained file by size and SHA256. The final
test source snapshots and actual test binaries are included. Earliest attempts
retain their available source/phase/diagnostic output; the manifest does not
claim an earlier overwritten test binary is available.

## Four Rust-goal reflection and remaining limits

| Goal | Evidence supplied by 105 | Limit of this slice |
| --- | --- | --- |
| Rust-class safety | Independent ordinary and CT verification, fresh untrusted Core, forged-owner/payload rejection | Evidence for finite Copy enums; no whole-language safety claim |
| Runtime and memory cost | O0/O2 ordinary enum lowering, logical VM cells and exact metadata rejection boundaries | No Rust timing ratio, physical-allocation parity or native heap claim |
| Expression and maintenance | Shared exact-identity helper, existing enum IR and ordinary recheck, replayable source/raw/wire controls | No measured maintenance-cost superiority |
| Compilation cost | Bounded CT evaluation and cumulative source work | No compiler wall-time improvement claim |

Only finite existing serializable Copy enums are admitted. Resource enums,
address-bearing values and foreign execution remain outside the capability set.
The evidence supports this implementation slice; independent review and main
integration are still owned by the parent task.
