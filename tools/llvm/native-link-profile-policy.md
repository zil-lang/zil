# Closed native link installation policy

`build_native_link_profile.py` installs one Linux x86-64 GNU ld profile. Its
canonical policy is SDK input, reviewed and compiled into the Rust consumer.
Reading a manifest from the installation itself never grants it authority.
The profile ID is SHA256 of the complete canonical policy bytes. The external
snapshot root is deliberately absent from those bytes.

The generator discovers four driver jobs: O0/O2, each without/with pthread.
Each has one absolute GNU ld executable, one object-list marker and one output
marker. The accepted options are the finite ordinary native options in the
generator; plugins, LTO, response files, extra jobs, unknown options and unknown
script grammar are unsupported. The existing placeholder must be an ordinary
native relocatable object. It is used only for driver discovery and is excluded
from the snapshot and policy.

All explicit CRT/object/archive/script inputs, all existing possible `-l`
candidates in driver and default GNU ld search roots, recursive script members,
and the complete observed ELF loader closures of GNU ld, bwrap and selected
DSOs are copied. Each original alias and canonical absolute filename becomes a
separate regular file with identical bytes; no snapshot symlinks exist. The
namespace exposes only this selected inventory. Driver paths containing `..`
are resolved before use; scripts retain original absolute member spellings.
Directory aliases are flattened, not exposed as links. Runtime RPATH/RUNPATH,
thin archives, unknown member formats and conflicting SONAME payloads decline
installation. The policy records full file sizes, permission modes and SHA256,
plus every directory, including empty runtime mount points and search roots.

The line protocol uses ASCII keywords, lowercase hex for path/argument bytes,
decimal sizes and four-digit octal permission modes. It ends with one newline.
Order is fixed: TARGET, LOADER, BWRAP, sorted LIBDIRs, variants O0/0, O0/1, O2/0,
O2/1, sorted FILEs, sorted DIRs, END. Every variant begins with the GNU ld
executable as an ARG. `ARG` takes exactly one hex value; `OBJECTS` and `OUTPUT`
are individual lines. Each variant contains one `OBJECTS`, and one `ARG` for
`-o` immediately followed by `OUTPUT`. Every variant ends with `ENDVARIANT`.

```text
ZILLINK1
TARGET <hex>
LOADER <absolute-path-hex>
BWRAP <absolute-path-hex>
LIBDIR <absolute-directory-hex>
VARIANT O0 0
ARG <absolute-GNU-ld-path-hex>
...
OBJECTS
ARG 2d6f
OUTPUT
ENDVARIANT
...
FILE <absolute-path-hex> <decimal-size> <mode-octal> <sha256>
DIR <absolute-directory-hex>
END
```

Installation requires a successful snapshot-only isolation probe. The outer
snapshot loader executes snapshot bwrap with `--inhibit-cache`, an empty
`--glibc-hwcaps-mask`, and snapshot-prefixed library directories. Bwrap creates
user, PID and network namespaces and mounts the snapshot read-only at `/`.
Inside, an empty `/work` is cwd, `/out` is a fresh writable tmpfs, `/proc` belongs
to the new PID namespace and `/dev` is a new minimal device mount. Environment
is cleared and `LC_ALL=C` is set. The snapshot loader runs GNU ld `--version`
using original absolute library directories. Restricted `uid_map`, unavailable
bwrap or any failed isolation is Unsupported; installation has no fallback to
the ambient host. Actual object inputs are separately mounted read-only at
`/inputs/unit-N.o` by the consumer, and the output marker targets `/out`.

Before every runtime use, the consumer must check exact policy bytes against
its compiled policy, exact full file/directory inventory without symlinks, all
file hashes on first load, and current complete stamps thereafter. First-load
hashing belongs to the cold request's accounting. Installation/discovery is SDK
setup and is not a measured compiler request. The consumer must also require
that host `/etc/ld.so.preload` is absent immediately before execution; the
generator checks this during installation. No externally supplied policy,
build ID or metadata-only receipt can replace the compiled policy anchor.
