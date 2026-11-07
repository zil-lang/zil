# Store element and field borrowing profile

This attachment records the enduring 178–180 language obligations. `_zil_design.md` remains the normative source; implementation and acceptance are separate.

- Compiler-reserved `StoreElement` constrains the admitted element shape. It does not grant Copy, Managed, address, or cleanup authority; users cannot implement or shadow it. An unconstrained type parameter or user empty trait does not establish eligibility.
- Local and field projections retain both the parent shell lifetime and the precise field access range. A disjoint sibling field is usable, while moving or cleaning up the complete parent and replacing an overlapping field remain prohibited while dependencies live.
- `store_at_mut(store, index)` forms a synchronous, nonescaping exclusive window into one complete element. Check the actual length before forming its address. The window may read or update Copy fields, but cannot take Managed responsibility, copy an entire Package, or create a hole. Unknown indices do not establish disjointness; structural mutation waits until the last use.
- Analyze and Verify must independently reconstruct source, layout, control flow, and real obligations. Neither a shared schema field nor producer metadata grants safety authority.

Historical adoption and run scope: `28b3e37bb4d8a1fe8b146c72cf891fc99de32440:changes/178-181-ante-r1-adoption.md`.
