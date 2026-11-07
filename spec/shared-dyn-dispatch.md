# Finite shared dynamic dispatch

A borrowed shared `dyn` value contains the actual receiver address and an immutable static table address. A real trait interface with fixed scalar method slots may select among distinct concrete finite Copy providers at runtime. Preserve original trait, implementation, member, module and provider identities. The pair must never point at a temporary carrier; each derived borrow remains tied to its actual owner.

Each table entry and call has a complete executable provider contract: prerequisites, access, guarantees, hold, result source, cleanup, effects, resume and thread requirements are proven where needed. Empty effects must be established, not inferred from missing fields. A provider's reads fit the erased shared receiver contract; shared access gains no mutable or owning authority. A scalar result adds no borrow or management right.

Types checks used and unused tables and Source coercions. Analyze and Verify independently reconstruct roots, entry/slot conformance, body facts and lifetime, including raw/Wire inputs. Lower emits the real pair, table and indirect call signature through both supported native backends. Governing rule: `_zil_design.md` §9.4. The admitted profile is finite: fixed scalar slots, concrete Copy providers and shared borrowed receivers.
