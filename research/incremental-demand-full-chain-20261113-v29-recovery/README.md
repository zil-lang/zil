# V29 complete TypedIR producer recovery

Use fixed INDEX from RECOVERY.json. Run `python restore_all_https.py 375ed1353872e4f110e0845de17d3623c102e1a6 --work-dir /new/download --restore-root /new/restored --receipt /new/receipt.json`. Directories must be new. Restore final metadata separately with that fixed index path/restore_https.py. Full hash/size/mode checks and exact union were actually closed per piece.

Reuse exact V23 LLVM SDK18/DSO785a and V27 native-link SDK192bd. Main9170 compiler8d8, source and binding remain in fixed V27 recovery3e068; current baseline target-cache removal does not remove that immutable binary. Prior V28 and cache/reference convergence commits are pinned in RECOVERY.json.

Formal compiled124/07a06aac and tool124 have102valid rows per mode,1080native and30complete hardELF pairs, both controllers0. Initial Clippy101 at3a1 and all3Astra source reviews are preserved. All formal raw, full ELF/LLVM/objects/runtime/parents and exact sources remain. Same-window checkpoint preserves O2 cold/bounded positives and check cold/positions limits; no universal/statistical superiority or old-candidate cross-window gain. Mutation controls were not requested, not a new pass.

Next f973 Cpp cold capture has full source plus implementation JSON/patch/root-review preserved as unaccepted successor evidence. Final docs5f6a contains its source but never rebinds formal124 runtime. Later ProducerPlan Rust WIP is not frozen or accepted.
