# Independent guarded fan-in review

Verdict: **Pass**, limited to `59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9` relative to `801933a9e8f060d44b8305786d01399527aa6137`. Reviewer: `/root/review_incremental_slice1`. Only production `crates/zil-llvm-api/src/preflight.rs` changes. No performance/product verdict; original801/283 windows retain their identity.

The final dense path requires at least32 incoming edges and at least ceil(V/4). Each merge initializes/project-walks at most O(V) data, now bounded by a constant multiple of that block's total incoming-edge count. This resolves the unmeasured05e intermediate candidate's risk of repeatedly preparing whole-graph vectors for many sparse joins. It does not make all CHK iterations or full compilation linear.

The projection cache is local to one merge with an immutable current idom tree. Each node maps to its first intersection on the first predecessor's ancestor chain; lowest RPO rank on that chain is exactly the common ancestor. Known-predecessor filtering and fixed-point update order remain unchanged. Local standard HashMap tables are only lookup indexes. The only unordered iteration is a collision existence check returning one fixed error; source traversal and schedules remain ordered.

Independent exact-snapshot API/text-reference verification passed43 tests. An external harness built byte-exact801/59d preflight sources and the identical machine model: all5,087 CFGs have identical reachable/idom vectors, and5,975,139 ordered block pairs match an independent set fixed-point oracle and old walk. This includes472 targeted high-fanin graphs and90 controls on both density boundaries, with reversed/random/duplicate/unreachable/backedge inputs. A131,072-node chain preserves iterative behavior.

The complete validator compared1,645 machine modules (904 accepted/741 rejected), including105 checked-overflow intrinsic/extract/shared-trap models with LCA1.16 fresh candidate HashMap validations per module—26,320 total—returned identical success schedules or first-error strings. These are explicitly machine models; affected native integration tests are separately counted.

Exact commands, input/log hashes, scope and limitations are in `review.json`; `review-fanin-oracle.tar.gz` contains the reproducible old/new source snapshots and independent harness.05e intermediate controls are retained separately and are not final performance evidence. All reviewer CPU tests have ended; subsequent exclusive measurement may begin.
