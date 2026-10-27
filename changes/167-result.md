# 167 result and remaining boundary

The functional candidate is `02040876a7f56f14b2058b91f6277b9db86ec393`.
The exact-source receipts in `evidence/impl/167/final.json` record 33 specialized
checks, 36 real O0/O2 executions, 21 raw and 21 serialized rejection checks,
and two focused verifier tests for the old Array rejection migration and its
remaining container neighbors. Selected Clippy and formatting checks pass.

The first Package commit is `a710be769a34ac37182ced84cbaa5b44a99f7734`.
The separate capture boundary guard is
`a030e7bf72de65376574be4a53d147f73cd82d7b`. The callback increment depends on
168's stable factory identities and recursive concrete environments; its source
implementations were `6a7ce70` and `a3bf919` and are retained as separate history.
Root owns the schema19 and Wire operation definitions.

## What is executable

Nonempty fixed homogeneous Package arrays construct left to right, move as
complete values across functions/modules, read Copy fields or borrow Managed
fields, transfer a literal indexed element, replace or refill that slot, and
clean only the still-live elements. Complete array cleanup is ascending index;
Package fields retain declaration order. Failure during preparation cleans
completed temporaries in reverse initialization order. Failed replacement leaves
the old element responsible until its ordinary cleanup.

Same-factory self-contained concrete Closure arrays now support construction,
whole movement, element movement/replacement, and borrowing the real inline
callback environment. Shared and mutable calls use existing callable contracts;
owned nested environments use 168's recursive cleanup functions and flags. A
resource-free closure still moves uniquely into its array slot: the verifier's
separate move lane rejects repeated SSA input, a Load masquerading as transfer,
a repeated TakeLocal, and reuse of an extracted SSA after its store. It does not
invent resource cleanup obligations for those environments or change the older
ordinary closure Copy profile outside the array lane.

The exact original 159 `array-of-capturing-callbacks` source is included and
passes O0/O2. Separate callback tests actually mutate independent counters and
exercise nested owned resources; take/refill/borrow produces the observed `ACB`
cleanup sequence. Array borrows conservatively refer to the whole array, so a
live element address or ByteBuffer window prevents overlapping replacement or
movement. Array capture is explicitly Unsupported in Types and Verify.

## Scope and follow-up

The original 159 `[R;2]` resource probe remains unclosed: `R` keeps its established
Desc meaning, and this slice does not silently turn the element into Managed.
A Package wrapper supplies the explicit owner category. Dynamic indices, dynamic
owning containers, nested owning arrays, owning slices and array captures remain
outside this slice. Separate opaque factory identities remain distinct even
when forwarding exposes the same machine closure ID internally.

Review found that expanded environment work must include the full width of Copy
leaf types. The present candidate accounts for array length, direct Package
fields, repeated Closure occurrences, and huge declared lengths, but those
checks alone do not settle moderate-length arrays of very wide leaf payloads.
The coordinated 168 follow-up owns the complete-width budget correction, including
this slice's Types helper; final integration must include and revalidate it.
This functional result is not a blanket budget or complete-language acceptance.

## Rust goal reflection

| Goal | Evidence and limit |
|---|---|
| Comparable safety | Independent raw/Wire checks exercise real slot responsibility, stale addresses, bounds, identity and cleanup. General language safety, external contracts and remaining owning profiles are not proved. |
| Runtime and memory cost | Payloads and flags are inline and native cleanup is observed. The container adds no implicit heap allocation; comparative runtime, RSS and tail latency were not measured. |
| Expression and maintenance | Existing array, assignment and borrow syntax now composes retained callbacks and Package owners. Same-task maintenance cost against Rust is unmeasured. |
| Compilation cost | No compile-speed work or benchmark belongs to this slice. Host target regeneration resolves a toolchain metadata failure and is not a ZIL speed result. |
| Substantive advance | The exact previous handler-array gap has executable positive evidence. A product-level net advantage over Rust remains unproven. |

The product thresholds and historic handoff registry were not changed. Full
workspace and combined consumer acceptance belong to root's exact final source.
