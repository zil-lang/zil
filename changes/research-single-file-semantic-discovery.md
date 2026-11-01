# Discover exact import-free semantic hits before parsing the root

Stage14 continuation at 70ba1b271b19bd01f0c0c4803b490f2d218428d7. Filesystem
compilation currently canonicalizes and reads the root, parses it to discover
imports, then loads the exact trusted semantic cache in project_pipeline. For a
previously successful import-free root, that parse cannot discover another file
and repeats work despite an exact source/compiler/target/full-Limits cache key.

After the unchanged canonical read, root byte charge and root-stem path seeding,
try the existing Store only for Native, non-closed requests with no active query
and no literal `import` or `syntax` substring anywhere in the root text. Parser
keywords compare original source bytes, and these two words cover ordinary and
`syntax use` imports. The deliberately conservative text guard rejects comments,
strings and identifiers too. It also excludes self-imports: a single-module graph
alone would not prove filesystem independence, because a later `main/mod.zil`
could make `import main` ambiguous. Do not introduce a second cache protocol or
infer discovery authority from the number of cached Typed modules.

Reuse Store's existing stage/artifact-option/LD_PRELOAD/LD_AUDIT/compiler gates and
exact whole-project key. Current validate_sources and physical ArtifactPlan checks
must succeed before the read; failed probes silently resume the original parsing
and filesystem discovery, preserving diagnostic priority. Pass an already decoded
Typed/Candidate pair to the normal pipeline as raw input, without decoding twice.
The pipeline repeats current source/artifact guards, binds the exact identity and
runs independent current Verify. No public compile_project behavior or report
schema changes.

A rejected raw pair clears imported queries and follows the existing current-source
Parse/Expand/Types/Analyze retry, retaining the original immutable root bytes and
all Verify/events counts. This retry cannot need filesystem imports because the
same immutable root passed the conservative no-import guard. Cancellation never
retries. Misses, corruption, edited imports, multi-file graphs, closed source-name
modes and low-limit misses retain ordinary discovery; failed/cancelled requests
publish nothing. Existing semantic corruption/rejection/cancellation tests plus
compact import-free hit, import-edit/ambiguity and low-limit controls cover the
new boundary. Root alone builds/tests/measures; no speed claim before measurement.
