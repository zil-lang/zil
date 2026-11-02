# Legacy test migration under approved §8.1.5

Role: test migration author, separate from 243 production author evidence.
Baseline P=d393f47db; no production, normative, fixture-source or raw-construction
changes are authorized here. Root's formal P result remains 3548 pass/12 fail,
zero ignored/filtered; this migration does not rewrite that failure receipt.

| Test/case | New expectation | Reason / retained negative |
| --- | --- | --- |
| impl166 whole_local_reassignment | Original source O0/O2, AZB, exit0 | Ordinary owning ErrorUnion retains its stable outer slot; old value A drops on replacement, later Z then current B on exit. |
| impl183 union-reassignment | Original source Verify success, verification_runs1 | Existing direct Managed-success ErrorUnion may be replaced; result(create)/give and other rejected boundaries stay unchanged. |
| impl206_analyze raw sum parent | Original coordinated bool!Outer mutation Analyze success | Complete nested Product payload admitted; address already legal; array, stale canonical Store definition and incoherent capture field mutation still reject. |
| impl206_independent source25/source28 | Original fixtures Verify success once | E!Root and Product containing E!Leaf are finite ordinary shapes. |
| impl206_independent raw metadata variant5 | Same unused Package/E!Leaf metadata raw and Wire acceptance | No runtime initialized debt; valid finite Product containing Sum. Variants0/1/2/3/4/6/7 retain name/field/owning-leaf/borrowed/array/callback/unregistered-resource rejections. |
| impl223 arbitrary-sum, product-result-i32 | Original sources Verify success once | E!Root and Root!i32 are ordinary admitted Product payloads. Owned arrays and mutable Product Store window remain closed. |
| impl235 sum | Original void!Session source Verify success once | Session's actual canonical Store<Product> fields remain legal ordinary payloads; element, array, shared and capture gates unchanged. |
| review223 raw variants0/1/3 | Exact original raw+Wire accept | Root!i32, Root!Root and bool!Root satisfy current owned_shapes::shape. Variant2 puts non-Copy ErrorUnion directly in ErrorUnion error slot, outside Package/Enum alternatives, and must still reject. |
| review235 sum | Original void!Pair source Verify success once | Same ordinary Product Store payload scope. Capture, nested StoreElement, array and shared negatives unchanged. |

No raw profile is accepted merely because current Verify accepts it. In
particular impl183_raw_owned_success is untouched; its direct nested owning
ErrorUnion profile is being repaired independently by 242. No oracle/source is
changed to avoid a production failure. Original Rust tests and fixture hashes
are retained in originals/ and ORIGINAL-MANIFEST.json.
