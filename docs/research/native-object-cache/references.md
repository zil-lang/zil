# Primary sources inspected for 212 (2026-10-28)

- Rust compiler development guide, incremental backend work products:
  https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html
  CGU objects/bitcode are distinct from ordinary query values. Adopt the separation
  between current semantic validation and reuse of a completed machine work product;
  do not import rustc_private or bypass ZIL's per-request Verify.
- Go `src/cmd/go/internal/cache/cache.go`, inspected blob
  b2fcafdbdf6c99df8516850529662ff3f8fdd0c4:
  https://github.com/golang/go/blob/master/src/cmd/go/internal/cache/cache.go
  ActionID hashes inputs, OutputID identifies output. Local concurrent caches and
  remote/network caches have different synchronization assumptions. ZIL uses one
  bounded checksum envelope and atomic same-directory replacement; no remote trust.
- Go `src/cmd/go/internal/work/buildid.go`:
  https://go.dev/src/cmd/go/internal/work/buildid.go
  Build identity is not source authorization. ZIL keys trusted installed compiler
  and actual loaded LLVM library by build-id + install metadata, falling back to
  full SHA256 if no build-id. Hashing every large LLVM library on every short CLI
  request is avoided; modified files/install identities conservatively miss.
- Rust codegen options:
  https://doc.rust-lang.org/rustc/codegen-options/
  Incremental and CGU settings are separately controlled; keep them explicit in
  fresh cross-compiler reference, rather than mixing historical measurements.

Original implementation. No foreign source bodies or third-party license text were
copied. The referenced projects retain their own licenses. Build IDs and object
checksums do NOT authenticate hostile tools or caches: this feature assumes user-
controlled, immutable installed tools and a private local cache. Loader interposition
via LD_PRELOAD/LD_AUDIT disables reuse rather than guessing complete tool identity.
