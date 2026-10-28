# 168 follow-up: persistent callback slots and complete layout work

Independent review of implementation `da49a4271fb2941eb7d2b6aa118ccd7ead5db7d5`
found two real gaps. Replacing an owning local callback with a later live resource
failed the existing reverse-initialization cleanup check. Inline closure DAG
budgeting counted a wide non-closure field as one node, although machine layout
walks its fields on every expanded occurrence. The original rejection and raw
counterexample remain in the independent review history.

## Implementation basis and scope

This follow-up is based on task 167 candidate
`02040876a7f56f14b2058b91f6277b9db86ec393`, including the real fixed-array shell,
element move/refill, and `BorrowOwnedArrayElement` consumers. No new public opcode,
schema, allocation, or ownership protocol is proposed.

A mutable concrete callback binding which is actually reassigned will use a
private one-element owning array shell. Its source-visible type and opaque
identity stay the concrete closure. Calls and borrows address the element; value
transfer and move capture take that element. Replacement prepares the new value,
drops the previous element, then fills the element using the existing checked
operations. The shell retains its original cleanup position, including when a
FnOnce element was consumed before refill. The private array never becomes a
captured source value. Other callback bindings keep their existing representation.
This does not add assignment of a captured environment field or escaping loans.

Layout work will include the bounded actual type subtree of Copy, Managed, and
Package fields at every inline closure occurrence. Multiplicity in admitted
fixed arrays is charged under the existing analysis budget. Arithmetic is checked;
layout cycles, excessive depth, and expanded work remain explicit rejections.
Types source admission and independent Verify compute their own checks and do
not trust a frontend certificate. Budget defaults and language rules are unchanged.

## Required verification

- O0/O2 replacement with a later live guard, checking old-value cleanup at
  replacement and the original shell order at scope exit.
- FnOnce consumption followed by refill, actual-element move recapture, and
  rejection of replacing an element while its previous borrow remains live.
- Existing concrete forwarding, source opaque identity, recursive cleanup,
  raw/wire ownership and callback-array tests remain valid.
- The reviewer's wide Copy leaf plus repeated-child DAG must produce a bounded
  `ResourceLimit` from raw and wire verification without raising any limit.

The root owns task metadata, shared specifications, final integration, and full
workspace validation. This branch contributes only the follow-up implementation
and compact evidence; old author and review records are not rewritten.
