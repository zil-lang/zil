# Fixed shared address arrays

A nonempty fixed array `[&'a T; N]` can be an ordinary value, a field of a qualified Record, or the pointee of an ordinary shared view. Its direct elements are thin shared addresses to finite closed Copy storage. Construction, constant indexing, shared copying, and direct helper arguments and returns preserve every element's actual external source and lifetime upper bound. Different elements may refer to different roots even when their types and lifetime classes are equal. A Record containing these arrays adds no owning resource debt.

For example, `struct Batch<life a>{items:[&'a i32;2]}` stores two independently proved address leaves. A helper returning `items[1]` substitutes the caller's actual second leaf, including its parent loans. A helper returning the whole array or its shared view preserves all of its leaves. Ordered lifetime arguments remain ordered: a helper using `Batch<'b,'a>` must map both actual classes and their physical leaves through that precise instance.

The physical path for an array element is `PlaceProjection::Field(i)`. At each such step, Types, Analyze, Verify and code generation independently inspect the actual full `Type::Array`, check `i < length`, and select its actual element type. This is an inline aggregate ordinal, never an unchecked struct-field offset. Machine GEP emission uses the actual array type and the checked ordinal. `LifeTypeProjection::Element` is type metadata only; it cannot supply a physical slot, address, actual source or permission. No IR opcode or Wire version is added.

Holding a shared array view keeps both its actual parent window and its element dependencies alive. Copying or explicitly reborrowing that view cannot discard those dependencies. Projecting a qualified Record's array field reconstructs the child leaves from the actual projected address. After an element pointer is copied out and the array view's last use has passed, that pointer retains its own external source; it does not acquire unrelated sibling sources.

Both kernels independently check constructor arity, every required source leaf, returned origins, bounds, lifetime mappings and live-loan conflicts. A missing element binding, a false root, a metadata-only Element class, a changed ordinal with stale candidate facts, or an escaping local target is rejected. Candidate Hold rows and common class labels do not establish these facts.

This slice excludes empty address arrays, dynamic indexing, exclusive address-array copying, and arbitrary nested target-bearing containers. Closure inference over array-bearing locals or array-bearing helper signatures retains its existing rejection boundary; array capture is outside this slice. It does not add array slot extraction/refill or dynamic index disjointness. Expansion, per-element metadata, complete type snapshots and physical snapshot planning use the request's existing limits. A large source can exhaust those limits before code generation; the default limits are unchanged.

Types collects a private accounting bit during its already paid actual inventory traversal and when an actual shared array type forms. Once set, it reserves each complete type snapshot before copying. This bit supplies no shape, Copy or source authority. Scalar function cost reuse stays on its existing path before this mode applies; the checker rebuilds functions after it applies, so cached costs cannot cross accounting modes. Permanent controls compare both function orders and their cold, warm and fresh budget boundaries.

| Operation | Contract |
| --- | --- |
| Construct or copy a fixed shared array | Preserve every actual element source and its original upper bound |
| Constant `items[i]` | Check the actual array length and return exactly leaf `i` |
| Pass or return an array through a direct helper | Substitute sources by the exact physical ordinal |
| Copy, reborrow or return a shared array view | Preserve its parent window and all retained element loans |
| Project a qualified Record's array field | Rebuild child permissions through the actual field address |
| Replace a live view's parent or write a retained source | Reject the conflicting access |
| Emit large physical snapshots | Keep value semantics and separate snapshots; grant no language Copy authority |

Permanent controls are in `tests/fixtures/resource-packages/shared_address_array.rs`. They cover Source, raw IR and Wire, two distinct roots with a common upper bound, two nonidentity lifetime mappings, actual live-window write conflicts, all four native backend/optimization combinations, and exact request-local budget boundaries with fresh and warm retries.
