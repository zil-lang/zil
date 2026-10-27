# 177 supplemental compatibility review at f7110edf

Reviewer: /root/callback_field176. Read-only production review; no repository source changes.
Candidate: f7110edf43281cc6202ab7b5e127870deb48b7d1.
Candidate CLI SHA-256: c67a01a95c1e8225a5ce78956c6c40de8c968a9c27ddf2344eb17d5845ef1bc0,
identical to the root consumer04 receipt.
Previous CLI: cc6508f7491db70452a006873c814063afe54c88 (174 consumer01),
SHA-256 18fa8ec6ebe8b763291c47db8ab0097c3db44dacbee71720386003b5a498c579,
verified against the original author receipt before using it.

## Confirmed regressions

Each source below is actually instantiated. The previous CLI accepts all three
through Verify; f711 rejects at Expand. Their nongeneric controls also pass on f711.
Exact complete sources and raw CLI outputs are retained in the named directories.

Shared model:

```zil
resource R { n:i32 }
package P { manages r:R; n:i32 }
fn make()->P => P{r:R{n:1},n:41}
```

1. `grouped_copy_field`:

```zil
fn read<T>(p:P)->i32 { let n=(p).n; return n+p.n }
fn main()->i32 => read::<i32>(make())-82
```

f711: SafeReject / Expand / E-GENERIC-ARGUMENT,
"generic value used after move; Copy cannot be inferred from a concrete instance".
`package_local` recognizes only ExprKind::Name, so the grouped Copy field access
falls through the value expression path and consumes the whole Package.

2. `array_copy_field`:

```zil
fn read<T>(p:[P;1])->i32 { return p[0].n+p[0].n }
fn main()->i32 => read::<i32>([make()])-82
```

f711: Unsupported / Expand / E-UNSUPPORTED,
"operation on an unconstrained type parameter requires a declared bound".
The Member path first asks Index to produce a whole Copy P, although ordinary
Types already implements this actual local + fixed index Copy field projection
as ProjectOwnedArrayField without taking the owner.

3. `array_take`:

```zil
fn first<T>(p:[P;1])->P => p[0]
fn main()->i32 { let p=first::<i32>([make()]); return p.n-41 }
```

f711: the same Unsupported diagnostic as case 2. Ordinary Types already implements
the actual owning-local fixed-slot take as TakeOwnedArrayElement. Rejecting copied
borrowed elements is necessary, but this owned local operation is different.

## Narrow correction principles

Keep Package non-Copy and recursive array/tuple classification. Correct expression
classification for Copy field projections and owned-slot movement instead of
making Package Copy again. Normalize grouped local identity consistently: if
package_local starts accepting ExprKind::Group, managed_field_transfer must not
retain its later ExprKind::Name/unreachable assumption for the unnormalized object.

For array Copy projections, check the actual field type at an admitted owning
local + fixed index. For array take, admit only the actual owning local and track
slot move/refill/whole-array transfer; never infer ownership from borrowed-array
shape. Leave dynamic indices, general nested owners and reference escapes closed.

## Compatibility checks that passed

`copy_alias_array`, `mutable_parameter_reborrow`, and
`shared_parameter_projection` all pass Verify on the previous and candidate CLIs.
They cover an alias to a Copy record array, repeated use after passing a mutable
slice to a written reference parameter, and shared Copy record projection through
a reference-returning ordinary helper. Transparent aliases are normalized before
validate_parametric, so they do not bypass the real Package identity branch.

The tuple branch already recursed before f711. It now notices actual Package
leaves; non-Copy tuples are still rejected by ordinary Types, so no existing valid
owned-tuple path was identified. Array recursion newly closes concrete mutable
reference arrays as Copy; such non-Copy array values were not admitted by the
ordinary finite Copy array path either. Unknown T still receives no Copy grant.

An additional exploration is preserved in borrowed-projection-summary.json:
`(*p).n` for shared &P fails in both generic and nongeneric forms (nongeneric Types
requires a Copy dereference pointee), so this is not reported as a regression.
Its explicit place form `let n=&(*p).n; return *n` passes as a generic Store-element
helper, showing the old symbolic place / scalar read route remains available.

The bool comparison hint change correctly separates the result type from operand
inference; no new issue was found by code review. This report is not whole-workspace
or final 177 acceptance. No native programs, allocation consumers or full tests
were rerun for this supplemental check.

## Follow-up at 605bed2

Exact production: 605bed2ad33e9c99e867636bcbbbcff90da3e867. The independent
review177-composition174 build produced CLI SHA-256
7d30f74cf8cd1c453e2cfc91a15d609aa20eb9f1aaaf22ae69eb5fe6d947ecd9;
its receipt is run-logs/review177-composition174/replay02/cli-build.json.
The original three source files and their three nongeneric controls all pass
Verify unchanged on this binary. These three regressions are closed.

Static review confirms the grouped local identity and subsequent managed-transfer
Name extraction now agree. The new owned-State profile rejects non-Copy indexed
Package values before the ordinary legacy exception. That exception remains a
fixed-array shape admission in the old ordinary generic checker; it is not a new
complete symbolic index/move/refill protocol. Concrete Types still independently
requires a named owning array and fixed literal for the real take operation.
No broader indexed ownership/borrow guarantee is claimed here.

### Additional confirmed whole-local assignment regression

Source directory: moved_package_roundtrip_assignment. Using the same R/P/make:

```zil
fn relay(p:P)->P => p
fn read<T>()->i32 { var p=make(); p=relay(p); return p.n }
fn main()->i32 => read::<i32>()-41
```

Previous exact 174 CLI: Verify Success. 605: SafeReject / Expand /
E-GENERIC-ARGUMENT, "generic value used after move; Copy cannot be inferred from a
concrete instance". The nongeneric 605 control passes Verify. These actual results
are in assignment-summary.json and 605-summary.json. The f711 binary had already
changed when this later case was prepared, so that version was explicitly skipped
for this probe rather than mislabeling the new binary as f711.

Cause: symbolic Assign first checks the destination place, evaluates the RHS
(which consumes p), and never restores the complete assigned local to live. A
successful complete Set must restore the destination after RHS evaluation and
type agreement, while preserving true moved inputs during preparation. This does
not require indexed transfer support. Production is unchanged by this reviewer;
root and the principal reviewer have received the finding.

Another exploratory variant took old=p, then assigned p=make(). Both its old
compiler and nongeneric control already reject in Analyze for cleanup order;
that variant is preserved but is not reported as a compatibility regression.

## Final supplemental result: Pass at be6fd5c / production 8abc75b

Final source: be6fd5cbf089072a13853cecb614abde2d05d58a.
Production: 8abc75bbc1f98450a16efde00539d753fb836551. The final-source delta is
an author test/contract-note correction; production source is unchanged.
The independent reviewer174 CLI used for this final replay has SHA-256
50aa6fd19475e3a1bd23edf6d2bfb57ff5743381f65bb499da99a251a3a4a4ee.
Its digest was checked before and after all five invocations.

| Original source, unchanged | Final result |
|---|---|
| grouped_copy_field | Verify Success; one verification run |
| array_copy_field | Verify Success; one verification run |
| array_take | Verify Success; one verification run |
| moved_package_roundtrip_assignment | Verify Success; one verification run |
| New rhs_duplicate_consume negative | Expand SafeReject / E-GENERIC-ARGUMENT, used after move |

The four source byte hashes are recorded and matched to the preserved original
files. The negative has `p=choose(p,p)` inside an instantiated ordinary generic
function: successful assignment restoration does not revive p before evaluation
of the second RHS argument.

`final-summary.json` binds all five commands, results, source hashes, final source
and production identities. `final-be6fd5c/` retains the actual source and raw CLI
stdout/stderr for each case. The earlier failures and exploration records remain
unchanged. `artifact-manifest.json` hashes the complete small supplemental directory.
No production code or repository refs were changed. This supplemental final run
performed five CLI check requests, no build, no native execution and no workspace
suite. The principal reviewer and root own their independent native/full results.

The findings are closed within this finite compatibility scope. No general
indexed ownership/refill protocol is claimed. Owned-State non-Copy indexed
Package transfer remains closed. Restoring symbolic liveness after a complete
assignment does not establish partial whole-Package replacement: the existing
Analyze unpaid-resource check still rejects that unsupported path. Final full
177 acceptance and product/language readiness remain the integration owner's
separate decisions.

## Final read-only isolation check

`final-isolation.json` records a separate Pass for exact be6fd5c against baseline
81b480be45643be9fddd3955f118c1439e195507. `_zil_design.md`, `_zil_goal.md` and
`validation/product-gates.json` are byte-identical, with both committed SHA-256
values and byte lengths recorded. Verify's unchanged Cargo manifest permits only
zil-base and zil-ir; the corresponding dependency manifests and all 31 committed
Verify source files contain no production dependency or noncomment reference to
Types/Analyze proof functions. One documentation comment mentions their names.

The actual compiler and raw Wire versions are 21, fingerprint domain is
ZIL-TYPED-MODULE-V21, and opcode 86/87 fields/tags agree across spec/interfaces,
IR definitions, Wire encoders/decoders and fingerprint encoding. The interface
JSON document's own format version remains 1; this is separate from its
implemented_partial_schema compiler/raw-wire version 21.

All seven required normative inputs were rehashed from committed bytes using the
repository's sorted path + NUL + file SHA-256 + LF algorithm. The recomputed hash
is 8aecd0f052b3d8eb099d19d26cd0bad56b26d3a7be91c79aca5a962a010711be,
matching the frozen registry. Only spec/interfaces.json changed among these
normative inputs relative to the baseline. This isolation result is not a full
safety audit or an independent semantic approval of this reviewer's own 176 work.
No tests, production modifications or reference updates were performed for it.
