# Replay an exact current filesystem graph before semantic restoration

Stage15 baseline 014c5d301c7a7283f5a39665d81d145ccaa6bc1f. Extend the existing
trusted semantic-cache discovery shortcut to multi-module filesystem projects.
The public ProjectSources API remains unchanged. A private, bounded discovery
receipt records only successful parser inputs/results: BFS module paths, source
content fingerprints, token charges and ordered logical imports with source spans.
Its namespace binds the canonical root/source, complete Limits, target, explicit
build inputs, schema/rules and installed compiler. It is raw local cache data,
never a Verified capability.

On a candidate receipt, reread current source bytes within the original aggregate
byte budget and compare every recorded content identity. Replay imports in original
BFS/item order through the unchanged resolve_file_module, including already-known
aliases. Thus file-vs-directory ambiguity, missing paths, symlink canonicalization,
root confinement and unique-module identity are checked now. Replay exact token
and node charges for identical source/compiler/limits; validate all path/depth/span
and decoding bounds. Only a complete replay may load the existing exact whole-
project semantic pair, which still passes current independent Verify. No cached
import success or absence can substitute for current filesystem resolution.

Use the existing Stage14 Native/non-closed semantic-mode gates. Private directories
must not be symlinks or group/world writable; each decoder length is checked before
allocation. Receipt bytes are capped by the existing source byte limit and a fixed
8 MiB optional-cache limit, with at most 65,536 module/import records. Rejection or
cache-size limits only suppress reuse, never source acceptance. Non-UTF8 canonical
paths are excluded from this private format.

Cache corruption, changed source/import resolution or a semantic miss resumes
ordinary discovery. Reuse immutable bytes already read by a failed/partial replay
when the same canonical file is discovered again, retaining current source snapshot
semantics; do not reread them from a concurrently changed file. Cancellation stops
the attempt and does not restart discovery. The ordinary path retains original
first-diagnostic order for all non-cancellation misses.

Only a fully successful request with an actual semantic work product may publish
its receipt. Add that receipt destination to a fresh full ArtifactPlan for every
source and output before atomic, cancellable cache publication. An optional write
failure cannot fail a completed compilation; failed/cancelled requests publish no
receipt. Focused controls cover actual multi-file reuse, edits, missing/ambiguous
and symlink paths, corrupt receipts, low budgets, destination aliasing and failure
publication. Root owns all builds/tests/measurements and integration; no timing
claim is implied by eliminating discovery parsing.
