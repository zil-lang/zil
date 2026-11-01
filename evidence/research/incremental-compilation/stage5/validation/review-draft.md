# Stage-5 O0 chunk review

**Pass**, exact commit `5189f09750c138eb61a4c86bea3654d7f0e57f6f` relative to `12fc97bde3a89de450c0eabebbcf11d512123018`. This file retains the requested draft filename; its production conclusion is now exact-SHA bound. No performance/product acceptance.

Only O0 with multiple requested CGUs uses 256-nonforeign-LIR-function chunks. Existing local-linkage/reference unions, private/global identity, wrapper placement, declaration preflight and exact machine-object cache inputs remain intact. O2 and count1 retain their prior grouping. Stable locality requires unchanged generated function set/order/IDs, not merely unchanged source declarations.

An isolated source export and separate target independently ran **8 tests: 6 machine partition controls + 2 real-source large-file controls**, all passing. These cover same-file body edits/cache reuse, insert/delete/restore, current rejection preserving output, count1/O2, shared private globals across chunks and resource/owning closure/drop/function-address native execution on API and Clang paths.

Review found and resolved the Clang test-helper assumption. The initial failure, a later attempt interrupted by second-candidate source compilation, and an incomplete snapshot setup are retained; only the final exact isolated run is the binding test result.

The separately hash-bound benchmark protocol now checks full four-byte integers, records complete source/edit identity, and retains eight classes of failed outcomes with null ratios. An external **synthetic** protocol probe retained all 20 rows and rejected an incorrect result differing by 127. It is not real compiler/performance evidence. Current optional closed-executable argument additions belong to the next candidate.

Commands, hashes and limitations are in `review-draft.json`. O0 CPU tests have ended; the next closed-executable commit is not covered by this receipt.
