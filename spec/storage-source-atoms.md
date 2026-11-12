# Storage source expressions

`StorageSourceBinding.position` selects an actual output leaf. Its `sources`
is a list of untrusted `StorageSource` expressions. The list is an upper bound,
not evidence of an address, initialization, ownership, access mode or lifetime.
An absent relation is unknown; an empty relation is not static provenance.

The expressions have distinct operations:

| Expression | Required independent proof |
| --- | --- |
| `Exact(position)` | Select the existing actual input leaf with identical complete Type, access mode and binder relation. No implicit region view or address operation is added. |
| `PlaceAddress { place, mode }` | Address the selected initialized physical place under its actual permission. Loading the contents and addressing the place are different operations; the output Type cannot select one implicitly. |
| `OwnedView { head, leaf, mode }` | Resolve the real owner head and its current permission, then select a reachable region through actual owning edges. The complete borrowed leaf Type and mode must match. An Address or Desc payload edge ends this owning traversal. |
| `ReborrowedLeaf { source, head }` | Keep the selected source's original pointee Fixed classes; derive the child's invocation upper bound from the real selected head signature and preserve all original parents. |
| `Static { leaf, mode }` | Prove the actual output has static provenance and the complete borrowed leaf Type and permitted mode. Empty source lists, local addresses and matching Types do not prove this. |

All selectors retain the existing `StoragePosition` interpretation. Class and
retained metadata selectors do not become physical addresses. A head proposal,
descriptor or matching nominal name cannot establish an owning route. Every
view retains the real head, per-level source and parent loans. Replacement,
retirement, cleanup and allocation reuse invalidate the corresponding actual
dependencies; a repeated Type, address bit pattern or descriptor cannot revive
them. Taking a view does not move or copy the owner.

`leaf` is the complete borrowed leaf Type, including qualification and nominal
arguments, not merely the pointee or its machine layout. `mode` is independent
metadata and must agree with the actual leaf and access proof. In particular,
an expression cannot change the Fixed domain by choosing another head or
freshening a child lifetime.

`DynResultSource::Borrow.sources` uses these same expressions.
`DynRetainedPortBinding.sources` continues to contain actual Self borrowed-leaf
positions. It does not accept region expressions or an owning head in place of
a retained leaf.

Each producer and independent kernel checks the supported operations against
the actual declarations, complete Types, CFG, initialization, permissions,
parent loans and cleanup obligations. A consumer without that independent
proof rejects an unsupported expression explicitly; it cannot ignore it,
replace it with an empty set, or promote an `Exact` position to `OwnedView`.
Raw/Wire values and structural CT transport grant no such proof.

Discriminants, complete positions, leaf Types and modes participate in identity
and transport. Their complete metadata is bounded and charged before copying,
hashing or comparing it. Type and CT graph visitors preserve full identity and
scope, including nonphysical binder references, and apply node, depth and work
bounds to the embedded leaf Types.
