# Analyze persistent callback slot audit

Read-only root review, 2026-11-17 Asia/Shanghai. Reviewed Analyze implementation and public Typed only; no Verify or Types implementation used. Evidence source is candidate `9d27da59d8d93c9d6ebaf7d469058995d7de112a`, run `alpha-resume-complete-persistent-slot-logical-physical-boundaries/pending-scalar-types-env-loaded`.

## Actual evidence

Function 14, original definition 19 / instance 18 / site 4: DeclareLocal expected Closure0 (N0), After B2 I23 StorageLive Local5. Local5 physical type is Array<Closure0,1>. B2 I22 Value13 MakeOwnedArray[Value12], following real Call Function12; I24 Store Local5,Value13. Later B4 I3 TakeOwnedArrayElement Local5[0] Value15; B7 I16 DropOwnedArrayElement Local5[0], I18 StoreOwnedArrayElement Local5[0],Value26; B10 I30 DropOwnedArray Local5. These are existing IR operations, not a general Closure/Array type equivalence.

## Complete minimal adapter consumers

- `closed/operations.rs:660`: DeclareLocal currently requires local physical type equal original Closure. Preserve original claim, establish exact shell witness from original site and real initialization. TypeUse/captured operand validation must select logical element only through that witness.
- `closed/operations.rs:1560,1777`: declaration image/value directories are logical; Store checks consume physical array SSA. Both directory paths need explicit shell conversion, including match_heads, rather than comparing array directly to logical closure. Keep ordinary physical Store type equality.
- `closed/classes.rs:1306,1877,2472`: SSA values, physical local values, and original full_value_at have different receiver meanings. Support actual shell MakeOwnedArray and element take/borrow/refill class transport without making every physical local query return a Closure. Preserve full child forest under Element and remove only the witnessed shell layer at a logical use. N0 reconstruction does not authorize access or cleanup.
- `closed/borrows.rs:122,156`: anchored_value/receiver_local recognize Load/TakeLocal, but original binding calls/moves now read element zero. Recognize exact TakeOwnedArrayElement/BorrowOwnedArrayElement through the witness. Original binding remains Local5; never choose by equal closure type.
- `closed/borrow_places.rs:60,525`: owning transfer and checked projected borrow must account for the witnessed Element0 shell path, including actual root, fresh borrow, mode, current activation, and full child value. Do not strip Element on arbitrary arrays.
- `closed/capture_heads.rs:493`: local_heads currently follows Store and rejects local vacancy, but ignores element take/refill/drop. Add the same actual element reaching-definition semantics and all initialized predecessor checks. Source capture/recapture callers must use this route; equal image does not merge owners or epochs.
- `closed/cleanup.rs:44`: cleanup occurrence is tied to original anchor, but compares physical local type directly to logical closure and scans only DropClosure. Map that exact occurrence to shell element conditional drop and final array drop. Keep vacant conditional-drop semantics distinct from strict take/borrow; preserve original Gamma versus actual Suspend frame proof distinction.

## Witness and rejection boundary

Original recipe/instance/site + exact local and After StorageLive + real single-element MakeOwnedArray/Store in the same function establish representation only. Actual CFG initialization, all reaching writes, element index zero, take/refill/drop, and activation must remain checked. Retain complete class/N, retained and phantom rows, Static origins, source identity, loan and cleanup obligations. No arbitrary Array1 compatibility, global type equality relaxation, Source substitution, or permission from an empty class count.

Author owns all implementation changes. This inventory is a pre-implementation check, not acceptance. Exit requires unchanged original Sources, independent raw/Wire controls, both native backends O0/O2, failure cleanup and relevant regression checks on one candidate.
