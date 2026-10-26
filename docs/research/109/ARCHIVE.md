# 109 raw archive representation

`evidence/impl/109/archive_raw.py` stores closed raw sources using standard
gzip-compressed tar. This changes storage only: every logical original file
still has its own path, source path, complete permission mode, byte length and
SHA256 in `raw-files.json`. Observation identities, samples and numerical
decisions remain the responsibility of the separately audited controller.

Files are visited in sorted logical-path order. The first occurrence of each
`(mode, size, SHA256)` tuple is a regular tar member. Later equal occurrences
are standard hardlink members pointing directly to that preceding regular
member. Different permissions prevent sharing. Empty files and long Unicode
paths use the same rule. There are no symlinks, link chains, forward links,
absolute member paths, `..` components or private archive extensions.

The writer requires `--sealed`, fresh output, explicit source roots and any
explicit exclusions. It hashes every source file, checks file identity/size/
mtime/mode before and after reading, rechecks the full source inventory, and
refuses a source selection that changes during writing or verification.
Special permission bits are rejected rather than silently dropped. Owners
and archive timestamps are normalized; the recovery contract is every file's
original bytes, ordinary permission bits and logical path.

Verification restores a fresh complete tree. Each link is resolved to its
already restored regular member, and every logical path is opened and hashed
again. Mode, length, exact path set and the canonical hardlink representation
are checked. Gzip is read through its CRC/footer; nonzero bytes hidden after
the last tar member and incomplete tar end records are refused. Successful
creation removes only this verified temporary restoration, leaves every
original source intact, and writes an independently hashed archive/manifest
receipt. Failures retain partial output and a failure receipt. Existing output
or restoration paths are never overwritten.

Example source selection (paths must already be closed):

```json
[
  {"prefix": "author", "path": "/tmp/zil-109-author001"},
  {"prefix": "compilers", "path": "/tmp/zil-109-compilers001", "exclude": ["target"], "exclude_names": ["__pycache__"]}
]
```

```sh
python3 evidence/impl/109/archive_raw.py create --sources closed-sources.json --out out/new-archive --sealed
python3 evidence/impl/109/archive_raw.py verify --archive out/new-archive/raw.tar.gz --manifest out/new-archive/raw-files.json --restore-dir out/new-restoration
```

The bounded self-test uses eight logical files, five regular payloads and
three hardlinks, including equal bytes with different modes and long Unicode
PAX paths. Actual system `tar` restores their full bytes/modes and expected
inode sharing. Twenty-four nearest refusal controls cover path traversal,
absolute paths, link targets/chains, duplication/missing members, mode/content
corruption, truncated gzip, hidden trailing payload, JSON number types and
duplicate keys, source symlinks, output reuse, missing closed-source
acknowledgement and optimized Python entry points. No product compiler,
consumer or performance observation is executed by this validation.

Formal 109 directories must finish and be declared closed before inclusion.
The final source list will be fixed with root after all owners have sealed
their raw directories. The already frozen task 108 archive is not rewritten.
