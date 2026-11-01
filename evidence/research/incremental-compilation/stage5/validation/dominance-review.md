# Independent dominance-query review

Verdict: **Pass**, limited to exact `801933a9e8f060d44b8305786d01399527aa6137` relative to `c6a60d3ec15db9df6953d4e38f640ba23b1d8a94`. Reviewer: `/root/review_incremental_slice1`. Production files were not modified; no benchmark or product gate was run.

The old immediate-dominator computation is unchanged. Only current reachable dominator-tree ancestry is indexed, at 32 or more blocks. A nonempty entry and legal CFG edges are checked first. The iterative traversal produces preorder/exclusive-end subtree intervals; unreachable entries stay sentinel values and are never consulted before the original unreachable-use/definition rules. Root/self shortcuts match the old parent walk. No caller/cached index supplies authority. SSA, phi, first-error checks and subsequent scheduling remain in their original order.

42 affected API tests passed from a separate exact Git snapshot with `text-reference`, including native loop/phi execution and malformed machine-input rejection. Complete command and log identity are in `dominance-review.json`.

An independently authored external harness compiles byte-exact old and new preflight sources with the same model. It compared 4,525 CFGs / 1,497,729 ordered block pairs against both the old walk and an independent set-intersection fixed-point dominator oracle. It exhausts entry-predecessor-free graphs up to four blocks, then random reachable/unreachable/cyclic graphs including 31/32/33-block boundaries and repeated edges. Another 1,540 full machine modules produced exactly the same success schedule or first-error string (865 accepted, 675 rejected), including padded/reordered phi diamonds and invalid SSA/phi/entry cases. A 131,072-node chain checks iterative indexing without recursive stack growth. All controls passed. Sources, exact hashes, harness, command and output are in `review-dominance-oracle.tar.gz`.

These are bounded correctness controls, not a language-safety or performance gate. The existing idom algorithm is unchanged; constant-time refers only to indexed ancestry queries. Extra O(V) index memory/setup belongs in later full compile measurements. CPU-intensive review work is complete.
