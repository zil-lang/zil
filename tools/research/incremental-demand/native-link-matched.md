# Native-link three-arm experiment

`native_link_matched_controller.py` prepares or runs separate check and O2
windows against exact main `9170bb4210554392e36eb7528596612596ebef80`, one frozen
candidate, and Rust 1.99.1 commit
`58cf8f9c35b355199727627d07ae34a4e2846d67` (Cargo commit
`5f94df4789f005f9a352888e8355ffc645b7ed0e`). No compiler is built by these tools.
The default only writes command records; `--run` starts the experiment and must
be launched in the approved outer execution window where user namespaces work.
Root must hold the shared `/workspace/.zil-env/language-build.lock` with an outer
`flock` across both mode windows; this controller does not acquire another lock.

Copy `native-link-matched-pins.example.json` to a new local file and fill every
null after compiler/source freeze. Source and binary SHA values must be exact.
`tool_source` identifies the committed tools checkout, separately from each
compiled production source. A compiler worktree can have subsequent committed
changes only under `tools/`; its compiled production commit must remain an
ancestor, and no production bytes may differ. Build receipts, actual compiler
bytes, mapped LLVM DSO bytes, tool source/files and final identities are bound.
An immutable `.binding.json` receipt can add the Git tree to an original build
receipt: its original path/SHA is checked, every original field must be unchanged,
and both original and enriched receipts are archived. No rebuild is implied.

```sh
python3 tools/research/incremental-demand/native_link_matched_controller.py \
  --pins /tmp/frozen-link-pins.json \
  --out-prefix /workspace/zil-push-main/out/runs/native-link-matched-01 \
  --blocks 2
# In the root-assigned approved outer window, use a fresh output prefix:
python3 tools/research/incremental-demand/native_link_matched_controller.py \
  --pins /tmp/frozen-link-pins.json \
  --out-prefix /workspace/zil-push-main/out/runs/native-link-matched-02 \
  --blocks 2 --run
```

Two blocks provide a complete initial control; `--blocks 6` covers all six arm
permutations. Each block contains the original 20 modules ×20 functions ×16
arithmetic steps, all 425 named declarations, and all nine requests in order:
cold, unchanged, private, multi, broad, positions, error, recovery, restore.
Every non-cold request has an immediately preceding named parent in all three
arms. Both source languages receive the same two-line position-only edit.
Each block has 51 rows: 27 measured and 24 parent rows. Thus two blocks have
102 rows per mode; six have 306. O2 executes all six fixed input/exit/output
oracles twice per successful row: 1080 outcomes for two blocks or 3240 for six.
Check creates no native artifacts. No fixture, budget, Verify or optimization
scope is reduced to obtain the smaller block count.

The candidate uses exactly `--push-reuse`. Only its O2 build command additionally
uses `--native-link-profile /workspace/.zil-environment/native-link-v1/manifest`;
check explicitly omits that build-only option. Zil uses the resident protocol,
cache=all/capacity256, CGU16/O2/jobs1. Rust uses offline/locked Cargo, a fresh
target per block, incremental=true, CGU16, opt-level2, jobs1 and lto=false. Cargo
starts a new process each request; Zil startup is outside request wall. Their
LLVM versions, safety work, CPU/RSS scopes and actual partition contents remain
different and are recorded, rather than treated as equivalent work.

All three arms retain successful output between measured and parent requests.
Nothing is deleted or overwritten before a normal request. An error must leave
the prior publication unchanged; the old executable is neither run nor counted
as a current success. Optional `--publication-recovery` adds delete/overwrite
requests only after the last native block. Their evidence is written to
`publication-correctness.jsonl`, excluded from the nine cells and performance
row counts. A failed recovery preserves the completed matrix but returns exit2
with a separate failed correctness result; it is never reported as a pass.

The explicit `native-link-exact-elf-reviewed-ir-v1` quality policy permits LLVM
IR representation differences for the reviewed early SSA and proven arithmetic
rewrite, supported by the candidate's independent source review and real O0/O2
native oracle checks. Complete raw LLVM and the strict second-line fingerprint
comparison remain observations. LLVM text equality is not the pass condition
for this experiment. The complete main/candidate ELF bytes must still agree in
every successful native pair, all three arms must pass every native oracle,
current independent Verify must execute, and both Zil arms must provide sixteen
actual unit receipts bound to physical object bytes, symbols and layout
ownership. Aggregate function exports must agree. Missing units, different ELF,
oracle failure or source/provenance drift stops the run and leaves raw evidence
for independent review. No machine-code normalization exception is enabled.

Before-codegen terminal reuse is a separate receipt shape: sixteen actual units,
zero current object reports/lookups, complete non-null checked+reused current
preflight counts summing to sixteen, the pinned native profile ID, retained
producer LLVM verification scope and no claimed current LLVM verifier execution.
It must match a prior physical producer in that same worker by full ELF hash and
complete LLVM hash with only the strict fingerprint value normalized. Prior
objects/exports/layout are rehashed and carried as producer provenance; old cache
states and stamps are labelled historical and current cache observations stay
null. No sixteen hits or new native work is invented. Late final reuse keeps its
real current unit reports and explicit null aggregate preflight counts. Missing
counts/profile/provenance, contradictory work or malformed scope is a failure.

Object bytes, full ELF, LLVM, layout files, raw responses/commands, Rust current
and historical sessions, and typed `stage_body` V1 observations are retained.
Unknown/missing observations remain null, never invented zeros. Full native SDK
inputs and scripts are archived after the entire native matrix, so the harness
does not pre-read those files before the candidate's first cold profile hash.
The compiler's first profile verification/hash belongs inside its cold request.
DSO capture and artifact collection remain outside compilation timers and may
warm OS caches; no OS cold-cache claim is made.

Legacy `matched_validate.py` expects a different eight-scenario/six-block schema
and intentionally cannot qualify these new records. The runner retains that
validator and its original runner source for provenance. This experiment only
compares these three same-window arms; it does not compare earlier candidates,
establish language parity, or infer statistical significance from two blocks.
