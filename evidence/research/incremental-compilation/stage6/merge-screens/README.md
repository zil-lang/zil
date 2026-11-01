# MergeFunctions candidate screening

The raw records cover three completed 160-request windows and one deliberately
stopped window. The stopped closed/before-and-after window contains 55 completed
valid requests, one manually aborted request and 104 requests never started.
Overall there are 535 completed valid observations out of 640 planned requests;
the aborted request is not a timing sample. `STOP.json`, the parent execution
receipt, source identities, request commands, raw rows/stdout/stderr and LLVM text
are preserved exactly.

The stop followed roughly 100-second large8192 cold/edit observations versus about
2 seconds with merging disabled. The partial matrix is not 640 completed requests
and cannot support a general speedup claim. This remains a two-block candidate
screen with the same compiler on both sides, not final performance acceptance.

`FILES.json` maps 2,300 retained paths to exact SHA-addressed raw bytes. Binary
products/caches are separately enumerated in `OMITTED.json`. `VERIFY.json` records
complete blob/sidecar verification. Use `stage6_archive.py --extract
<this-directory> --dest <new-directory>` to restore the retained evidence.
