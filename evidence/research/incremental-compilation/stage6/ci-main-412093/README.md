# Completed official CI run 41209327141

Raw GitHub run/jobs/artifact-list JSON and the complete 503718-byte downloaded logs.zip
are retained. Run status is completed/success for exact head
`2e8f8c1f6e4bcb92d829cfa91cc4582c230f1e3e`, completed 2026-10-31T14:39:16Z:
https://github.com/zil-lang/zil/actions/runs/41209327141

This supplements, without modifying, the earlier ci-disk package that captured the
then-in-progress run's successful disk-recovery upload. Earlier 412064 DB-test and
412072 ENOSPC failures remain frozen. The artifacts JSON is a listing, not a claim
that every referenced artifact payload has been downloaded into this package.

This official CI result does not accept later 74431 combined LLVM candidates,
6ed research-workflow changes, runtime tools or current HEAD. Those require their
own exact-source CI and benchmark identities. Use stage6_archive_large.py --verify
or --extract with a new --dest. All five selected files are retained byte-exact.
