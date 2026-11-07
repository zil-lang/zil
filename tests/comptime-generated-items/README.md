The original 32 suggested ZIL sources are preserved byte-for-byte under
`project/`; `preparation/SOURCE-MANIFEST.json` binds their original hashes.
The original Type-value and scalar-emit probes retain their bytes. No original
expectation is replaced because of an implementation failure.

`ORACLE.json` is independently derived by `tools/language/262/oracle.py` from
source operations, not compiler reports or native stdout. It covers 9 positive
entries, 9 handwritten equivalents, and 19 source negatives. The source target
for numeric layout is x86_64 with i32 size/alignment4. All source/native
observations remain zero until actual compiler runs are collected separately.

| Entry | Expected stdout | Exit | Purpose |
| --- | --- | ---: | --- |
| 00 | empty | 0 | original Type-position probe |
| 01 | empty | 0 | Type parameters/results, array constructor, field/type/layout queries |
| 02 | KKZ42 | 0 | all four categories, CT loop, repeated Syntax fresh bindings |
| 03 | AAZ42 | 0 | five providers, two Expr providers, one evaluation, caller/definition binding |
| 04 | BAAP | 0 | generated Package, normal/early cleanup, uncalled function |
| 05 | F | 0 | generated owning Ticket/ByteBuffer package and successful return |
| 05b | F | 1 | early source error before any allocation request |
| 06 | T | 0 | generated ordinary Drop impl in the owner module |
| 07 | KKZ | 0 | fresh/name_expr, Type value lexical shadowing, named provider entry |

The nine ordinary handwritten programs preserve the materialized behavior and
effects. Their separate execution is a runtime equivalence control, not an
assertion that byte-for-byte phase dumps or source identities must match.

The two strict lock files use the 261 public text contract. Source hashes bind
raw SourceUnit bytes. Grammar hashes are SHA256 of
`zil/provider-grammar/v1\0` + `ZIL-LANG-1.4` + `\0` + the raw 32 source-hash
bytes. No lock is repaired automatically. Standard CT modules are imported by
their reserved compiler identities; input graphs provide only real user
modules. `LOCK-NEGATIVES.json` lists six independent identity/rule failures.

The driver prepares one exact source tree for each graph and supplies those
same path/content/module identities to file and in-memory requests. Provider
discovery follows source-order breadth-first module numbering. CLI equivalence
uses the same live files and explicit `--syntax-lock` bytes; it requires an
actual CLI build identity with compiler_commit and binary_sha256. Run that
check before source inputs are retired, so a missing file is not misreported
as a language failure.

`BUDGET-SOURCES.json` is a separate, non-native source-operation pair. Two
actual source Type-function calls must exhaust the minimum CT-step allowance
for one call; this catches per-expression budget resets without inventing an
expected implementation-specific N. Cold/warm N, N-1, and fresh N are observed
only after the positive source control succeeds. Query events remain raw;
another phase's cache hit is not labeled a CT-cache hit.

The generated and handwritten O0/O2 matrix plans 36 ordinary native runs and
20 allocator runs, including 4 actual allocation failures if implementation
matches source semantics. Each source-level make path requests one reserve;
push into that capacity makes no second request. The early path requests zero.
All allocation counters are future expectations until measured with the real
realloc/free wrapper. Output paths encode actual stdin bytes as hex (empty
input produces the empty hex component), never as raw input characters.

Prepared driver tests require the 261 SyntaxLock request API and the complete
258/259/260/261 implementation. At initial preparation, they have only been
formatted, not Cargo-built or run. Missing capabilities are retained as Fail or
NotRun, never converted to successful Unsupported controls. All 19 negative
sources stop at Verify and can never execute native programs; their expected
phase/code comes from their frozen semantic risks and the agreed public
diagnostic contract.
