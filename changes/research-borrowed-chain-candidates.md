# Borrow checked-add candidate metadata until a chain is selected

Stage13 baseline 4cb3687127a1980b2ace965af9fdc8c8e5de93c5, production 743. The
existing necessary-condition gate correctly skips functions with fewer than two
candidate checked blocks, but independent branches can pass that gate and fail
adjacency. Before discovering this, checked_block currently owns eight names,
constructs temporary aggregate types and local operands for equality checks, and
then drops them. The retained opaque branch machine output has this shape across
its helper functions. This is repeated unsuccessful planning work, not a new
quadratic algorithm finding.

Let Check borrow names from the unchanged input Block. Keep scalar Kind and the
constant value copied as before. Match the exact two-field overflow-pair type and
local value directly instead of allocating equivalent temporary values. Only an
accepted Chain replacement takes ownership of names. The planner's input remains
immutable until all plans are owned and full original-module preflight succeeds.

Preserve every shape, attribute, sign, width, literal interpretation, adjacency,
predecessor, use-count, cycle, Phi, work/depth bound and failure decision, including
condition order. Preserve all earlier gates and complete preflight before mutation.
No language acceptance, default limit, ABI, public schema, optimization eligibility
or cache identity changes. Existing signed/unsigned/literal, malformed-prefix,
separate-branch, invalid-original and source/native controls are sufficient for
this ownership-only change; no new broad test matrix is requested. Root alone
builds and measures. Reduced temporary allocation is a structural fact; its
complete-request timing benefit remains unmeasured until the selected window.
