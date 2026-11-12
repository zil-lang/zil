# Link-discovery observations

This records four **discovery-only** executions. It does not implement or enable
a cache, measure their duration, or establish a complete dependency closure.
No compilation, object emission, actual object link, or generated executable ran.

`evidence-01/receipt.json` binds the exact original and substituted argv, current
environment digest (values are not dumped), cwd, tool bytes, script bytes, and all
16 ELF payloads. The objects are the candidate **private** request from
`full-push-native-profile-07`, source
`5bf55f35ccdad6ca9eee3200a063115602d575ea`; this is not a new profile of current
source. The production `link_cache.rs` bytes are identical at that source and
investigation base `fc4bba198410d24fe487a9b24dea019736fc1772`:
`dd55b4e4a4a5ae069a3da1808c2b6bf5605d94c46474790c52c4e86eacfddb79`.
The complete profile is preserved in fixed archive
`520d2c7396ab62e1a87daa2551cd9b2c25347351`, directory
`research/incremental-demand-full-push-20261112-v10`.

The acquisition directory was
`/workspace/zil-push-main/out/evidence/link-probe-syscalls-01`.
`evidence-01/inventory.json` hashes its saved files, excluding the inventory itself.
The acquisition-time recorder and receipt remain unchanged; later analysis is
separately hashed. `probe-*.analysis.json` stitches per-PID unfinished/resumed calls;
the earlier `*.summary.json` is only a quick unstitched index and must not supply
complete syscall counts. `supplemental-identities.json` records the implicit
`#!/bin/bash` interpreter, whose execution is implicit in the kernel's ldd exec.

| Probe (all exit 0) | Syscalls | Path events / distinct spellings | ENOENT | execve |
| --- | ---: | ---: | ---: | ---: |
| clang `-### --target=x86_64-unknown-linux-gnu -O2` + 16 `.o` + `-o` | 2,892 | 232 / 190 | 160 | 1 |
| `/usr/bin/ldd` resolved clang-23 | 554 | 95 / 78 | 54 | 4 |
| `/usr/bin/ldd /usr/bin/ld` | 547 | 79 / 62 | 37 | 4 |
| `/usr/bin/ld --verbose` | 128 | 32 / 32 | 18 | 1 |

Path events include exec and access/stat/readlink operations, not just successful
reads. Distinct spellings are not canonical distinct files. Descriptor reads and
mapped bytes are numeric in the trace; no read/write buffers or environment
values are decoded. All syscall names/results are present. There is no `-v`.
Ordinary probe stdout/stderr are preserved separately, with hashes.

## Actual inputs and boundaries

* clang performs one successful `newfstatat` on each of the **16 temporary object
  paths**, but does not open/read their contents in this execution. The output
  path appears in argv/printed recipe only; no pathname syscall accesses it.
  No ld child is executed by clang. The 16 payloads remain byte-identical and
  `slots/program` is absent after all probes. This one observation does not prove
  slot substitution safe for other driver versions/options/input kinds.
* clang checks three absent SDK/bin configs:
  `x86_64-unknown-linux-gnu-clang.cfg`, `clang.cfg`, and
  `x86_64-unknown-linux-gnu.cfg`. `/opt/rh`, `/etc/env.d/gcc`, GCC-cross roots and
  alternate vendor triples are also absent. These negative dependencies can
  change discovery even if every previously loaded file is unchanged.
* GCC detection opens `/usr/lib/gcc/x86_64-linux-gnu` twice and makes four
  `getdents64` calls (72 bytes then EOF each time). It probes version 14 startup
  objects, multilib directories and C++ include roots. Ordinary abbreviated
  strace records entry counts, **not entry names**. A later directory snapshot
  contains `14`; it is explicitly not an acquisition-time atomic read receipt.
* clang reads `/etc/os-release` (FD target `/usr/lib/os-release`) and
  `/etc/debian_version`, and fails to open `/etc/lsb-release` and
  `/etc/redhat-release`. Canonicalizing only successful paths would discard
  meaningful absent alternatives and symlink spelling/retarget information.
* All probes load DSOs through `/etc/ld.so.cache`; all test absent
  `/etc/ld.so.preload`. SDK15 `LD_LIBRARY_PATH` and clang's SDK-relative RUNPATH
  create failed library and `glibc-hwcaps/x86-64-v[234]` candidates before the
  system libraries. The two ldd processes additionally read locale/gconv files
  and attempt `/dev/tty` (ENXIO). Bash checks `.` and calls `getcwd`.
* Each ldd exec launches the dynamic loader three times (`--version`, `--verify`,
  and dependency tracing of its argument), with clones/pipe FD transfers.
  Its stdout contains load addresses; raw stdout is not an invariant value even
  under unchanged filesystem inputs. Cache a proved parsed dependency result,
  not an alleged byte-identical stdout. None of these launches executes the
  generated program or an object-file entry point.
* `/proc/self/exe` and `/proc/self/fd/3` are really accessed. The latter resolves
  the already-open release files. The process also observes uid/gid, uname,
  randomness and terminal state. Their return values/arguments are deliberately
  not fully decoded here. `/proc`, inherited FD state, loader auxv/hardware state,
  and non-file observations are therefore **unclosed conditions**, not evidence
  that a file-only snapshot is sufficient. There were no observed network
  syscalls, but one trace is not a future syscall admission policy.

The cwd is the real compiler cwd, `/workspace/zil-push-main`. Its full inherited
environment is hashed, with the exact archived candidate runtime overrides
(SDK15 libraries, original SDK clang, no LD_PRELOAD/AUDIT). This binds this
execution; it does not reconstruct every environment value of historical
profile07. Absence and emptiness must remain distinct in any future key.

## Smallest defensible implementation boundary

The source consumers are `build.rs` around 892 and 953 (`link_cache::key` before
link and after a real link), `link_cache.rs::{key,Inputs::dynamic_dependencies}`,
and `incremental.rs::BackendSession`. A new bounded session-owned discovery
receipt could replace subprocess discovery **only after** the following proof
and validation exist. Existing current library/script/object content hashing,
strict native-input admission, argv parser and the post-link recheck stay.

1. Split a proved semantic discovery result from the final link-content key.
   Initially scope the receipt to the two ldd dependency vectors and ld's default
   script, whose argv contains no temporary object/output slots. Keep the clang
   probe fresh until its larger closure and slot handling are proved. This is
   a smaller review surface, not a claim that its closure is already complete.
2. Bind exact executable invocation spelling and resolved identity, shebang and
   interpreter, dynamic loader and loaded DSOs, command/options/target/thread
   mode, full environment, cwd resolution/metadata, credentials, relevant auxv
   hardware/platform/secure-exec state and process I/O contract. A changed or
   unavailable binding misses. Unknown behavior never yields a reusable receipt.
3. Track every descendant and the FD table across fork/exec/dup/close/chdir.
   Successful open/mmap/read dependencies require content plus relevant stat
   and symlink-chain identity, not merely one canonical pathname. Stat/access
   results, executable permissions, failed lookups with their exact errno, and
   each missing component/nearest existing ancestor are dependencies too.
   Directory enumerations require the complete returned membership/type data
   (and order unless the pinned consumer is proved order independent). The
   later `post-trace-directory-snapshots.json` cannot satisfy this requirement.
4. `/proc/self/exe` can be a narrowly specified view of the bound executable;
   `/proc/self/fd/N` can only be a view of a fully tracked live FD. All other
   proc/device/unsupported lookups, unknown syscalls/flags, truncated traces,
   untracked children/FDs, network, signals/errors, and unproved non-file
   observations disable caching. Source review must justify ignoring ASLR,
   random values, IDs, uname, terminal queries and timing for the *parsed result*;
   recording syscall names alone does not justify ignoring them.
5. Validate the whole dependency receipt on every hit, including the after-link
   check; do not return the same cached digest twice without validation. A
   changing namespace must miss. Guarded pre/post snapshots need the same
   explicit concurrency model as the existing link guard; they are not an
   atomic filesystem transaction and cannot prove safety against arbitrary ABA
   mutations. A read-only pinned toolchain/config namespace is the stronger
   practical boundary. Neither hashes nor a trace alone makes host state frozen.
6. Only after pinned driver-source and adversarial tests establish it, parse the
   linker argv into typed `ObjectSlot(i)` / `OutputSlot` tokens and fixed tokens.
   Substitute only exact argument tokens, never global text replacement.
   Validate every current `.o` slot's required status, type, suffix and admitted
   native-ELF contract; current object contents remain in the final-link key.
   If the driver reads a slot or consults a slot-derived path/config, either bind
   it completely or fall back. Retain fresh current diagnostics on missing,
   unreadable or wrong-type input; do not replay old successful stdout over it.

Required counterexamples include a new earlier GCC version; new earlier config
or library/hwcaps file; missing-path creation/deletion; symlink retarget and
ancestor replacement; same names with changed file type; changed cwd/env/locale/
credentials/tool/loader; every object slot missing/unreadable/symlinked; output
directory changes; flags outside the proved schema; FD/proc/unknown syscall
events; and dependency changes between the pre-link and post-link validations.
Actual hit counters must count removed subprocess executions, not just a cache
lookup. This investigation supplies no performance prediction.

## Fixed industry references

The exact reference bytes and hashes are saved in `evidence-01/industry/`:

* [fsatrace README, 4d4a967293eed5bd2a0298c5be6858e3f7fccb28](https://github.com/jacereda/fsatrace/blob/4d4a967293eed5bd2a0298c5be6858e3f7fccb28/README.md)
  describes injection and read/write/stat event formats, including platform
  injection restrictions. Such a path event log by itself is not a certificate
  for loader startup, missing searches, readdir or arbitrary direct syscalls.
  No fsatrace library was installed or injected in these probes.
* [Bazel sandboxing, 9ba2ab4062387b9b6b8240520c188c99cc102cb0](https://github.com/bazelbuild/bazel/blob/9ba2ab4062387b9b6b8240520c188c99cc102cb0/site/en/docs/sandboxing.md)
  explicitly describes stale results from undeclared inputs and the limits of
  a symlink execroot/host absolute paths; read-only isolation is different from
  hiding all undeclared inputs. This trace supplies neither isolation property.
* [Ninja manual, 2daa09ba270b0a43e1929d29b073348aa985dfaa](https://github.com/ninja-build/ninja/blob/2daa09ba270b0a43e1929d29b073348aa985dfaa/doc/manual.asciidoc)
  documents compiler-cooperating depfiles/dyndep and command-line dependency.
  A depfile is not automatic observation of failed tool/config/library searches;
  the listed positive input graph cannot be assumed to cover these probes.

Run only the light parser tests with
`python3 -m unittest discover -s tools/research/link-probe-discovery -p test_trace.py`.
For analysis of saved evidence, `python3 analyze_trace.py evidence-01` runs no
external probes. Acquisition is an explicit operation requiring
`--execute-four-probes` and a new output directory; do not rerun it in a timing
window. The recorder author and raw reviewer here are the same person; this is
not an independent review of the recorder implementation.
