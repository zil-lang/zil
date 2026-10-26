# 096 implementation record

Author: /root. Baseline: 1ec3bf43 (schema17 public byte-storage contract).
Authority: main .task/impl/096.md and changes/PROPOSAL-0094-0096-BYTE-STORAGE.md.

Implement the sealed byte head as an inline LLVM `{ptr,i64,i64}` with existing
Managed carrier flags. Linux calls realloc/free explicitly. Win64 uses the
process heap through GetProcessHeap/HeapAlloc/HeapReAlloc/HeapFree, with flags0;
the empty-pointer path uses HeapAlloc, not HeapReAlloc(null). Both paths publish
the new pointer/capacity only on success, and never call an allocator with zero
requested size. Push does not allocate, initializes the byte before len, and
returns false without mutation when full. Drop settles the carrier flag before
free, and never accesses freed payload. No user callbacks occur in these
primitive sequences. Failing HeapFree is an unrecoverable trap, never a retry.

The original local scalar/slice program paths emit no allocator dependency.
Allocator symbols supplied by source foreign declarations must have exactly
the required native ABI; otherwise emission rejects before writing runnable
artifacts. All code generation remains bound to the exact VerifiedModule.

Validation to run after frontend/kernel commits are available: real multi-module
ZIL with dynamic input and ordinary/give factories; O0/O2 and Rust oracle;
finite deterministic allocation failures and free counters through explicit
test-only linker interception; raw/wire/fingerprint correspondence; Linux and
Win64 portable source separately. Uncompleted checks are not Pass.

Primary allocator contracts consulted 2026-10-25:
- https://pubs.opengroup.org/onlinepubs/9799919799/functions/realloc.html
- https://www.open-std.org/jtc1/sc22/wg14/issues/c11c17/issue0400.html
- https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heaprealloc
- https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapfree

Public schema and proposal were authored on the integration branch before this
worktree. Independent review must include those exact ancestor commits, not
only this worktree's later backend delta.
