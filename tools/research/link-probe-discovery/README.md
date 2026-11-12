# Link-discovery investigation

Four discovery probes ran once each with the exact profile07 candidate private
request options and the same 16 ELF payloads extracted from its validated zobj
containers. All exited 0. No compilation, emission, object link, generated program
or performance comparison ran. This commit publishes only tools, tests and this
classification, **not raw traces, objects, stdout, or their Git ancestry**.

Raw is separately preserved in `out/evidence/link-probe-syscalls-01`, and local
commit `9a175edc274c2a4d892453bbcd75797cabee9fb2`; it is awaiting the established
archive workflow. Raw receipt SHA256:
`c429fae7004d3d4749fbfe0c2a23db29bc70d34db01ca92ceeb0f1b367f31e2b`.
Raw inventory SHA256:
`3f40f2aa6c4b3165e385a7a069255559be18bc1c7e95d241925347dd5f7b8e21`.
That inventory binds 49 files / 1,191,904 bytes; later report/tests are separately
listed in `followup-files.json`. The measured input source was
`5bf55f35ccdad6ca9eee3200a063115602d575ea`, investigation source
`fc4bba198410d24fe487a9b24dea019736fc1772`. Their `link_cache.rs` bytes match.
The complete original profile07 is in fixed archive
`520d2c7396ab62e1a87daa2551cd9b2c25347351` / v10.

| Probe | Syscalls | Path events / distinct spellings | ENOENT | execve |
| --- | ---: | ---: | ---: | ---: |
| clang `-###`, target x86_64 Linux, O2, 16 `.o`, output | 2,892 | 232 / 190 | 160 | 1 |
| ldd resolved clang-23 | 554 | 95 / 78 | 54 | 4 |
| ldd ld | 547 | 79 / 62 | 37 | 4 |
| ld `--verbose` | 128 | 32 / 32 | 18 | 1 |

Counts include status/readlink/exec, not just reads. The analysis stitches
interleaved unfinished/resumed syscalls. Full syscall names/results are captured;
buffer-bearing syscall arguments remain numeric. There is no `-v` or environment
dump. The environment digest binds all current inherited values with the archived
candidate SDK15 overrides; it does not reconstruct every historical value.

clang stats all 16 ephemeral objects, without opening them in this execution.
It never accesses the output path through a pathname syscall or executes ld.
The payloads remain unchanged and no output file exists. It also probes three
absent clang configs, alternate GCC/triple/multilib paths and startup objects;
enumerates the GCC triple directory twice; reads distro files; and uses
`/proc/self/exe` and `/proc/self/fd/3`. ldd launches the dynamic loader three times
per probe, reads locale/gconv data, checks cwd and attempts a tty open. All four
observe loader cache/preload and positive/negative DSO search paths. These are
real dependencies absent from a simple list of previously loaded libraries.

**Closure is not proved.** The abbreviated getdents trace contains counts, not
its complete names. The saved directory snapshot is a later observation.
Proc/FD views, credentials, uname, randomness, terminal state and loader auxv
conditions are not fully modeled. ldd also prints ASLR addresses, so raw stdout
is not an invariant value. These gaps prohibit enabling a file-list-only cache.
One observation is not proof that other options/input kinds behave the same.

The concrete next boundary is a bounded `BackendSession` discovery receipt in
`link_cache.rs::{key,Inputs::dynamic_dependencies}`, consumed by both `build.rs`
pre-link and post-link key checks. First prove the two ldd **parsed dependency
vectors** and ld default script, which have no ephemeral slots; keep clang fresh
until its larger closure and typed slot handling are proved. Existing object,
library and script content keys and strict native admission remain necessary.

A receipt must bind exact command/options, tool invocation spelling and resolved
bytes, shebang/interpreter/loader/DSOs, full environment (unset differs from empty),
cwd, credentials, auxv/hardware state and process I/O contract. Track descendants
and FD tables. Bind successful file content/status/symlink chains, failed lookups
and missing ancestors, and complete directory membership/type/order as required.
Validate the receipt **on every hit**, including post-link; returning a cached
key twice is not revalidation. Pre/post snapshots still need an explicit
concurrency model; they are not an atomic filesystem transaction. A read-only
pinned namespace is the stronger boundary. Unmodeled proc/device/syscall/FD/
network behavior, truncation or errors must fall back to normal discovery.

Only after pinned driver-source proof and adversarial tests, use typed
`ObjectSlot(i)` / `OutputSlot` recipe arguments, never text replacement. Revalidate
current slots and preserve fresh diagnostics for missing/unreadable/wrong-type
objects. If the driver reads slot contents or a slot-derived path, bind those
inputs or fall back. Test newly created config/library/hwcaps/GCC alternatives,
symlink/ancestor changes, changed directory entries/types, tool/env/cwd/locale/
credentials, every missing slot, unsupported flags, and pre/post-link races.
Actual work counters should count subprocesses removed, not cache lookups.

`readset-classification.json` contains the compact factual classification and
fixed source references: fsatrace
`4d4a967293eed5bd2a0298c5be6858e3f7fccb28` (injected access log is not a closure
certificate), Bazel `9ba2ab4062387b9b6b8240520c188c99cc102cb0` (undeclared input
and host absolute-path limits), and Ninja
`2daa09ba270b0a43e1929d29b073348aa985dfaa` (compiler-cooperating depfiles/dyndep).
Their exact reference bytes are preserved with raw. None was installed or run.

Light tests: `python3 -m unittest discover -s tools/research/link-probe-discovery -p test_trace.py`.
`analyze_trace.py EVIDENCE` only analyzes saved files. Acquisition explicitly
requires `--execute-four-probes` and a fresh output directory; do not run it in a
timing window. Recorder and raw reviewer are the same author, not an independent
review of the implementation. No performance gain is claimed.
