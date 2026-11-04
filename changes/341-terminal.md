# 341: complete ordinary const and nominal trait abstraction

Authority is current `_zil_design.md` §16.1–16.2 and §18.1, together with its
named grammar, diagnostics, interfaces and dependency attachments. This proposal
implements already authorized terminal semantics; it does not redefine the
permanent language boundary or raise any request limit.

The baseline accepts concrete generic trait applications and ordinary default
methods. It rejects const binders, associated types/constants, symbolic
projections, generic or conditional impls, supertraits and generic methods.
Those rejections remain implementation gaps, not terminal negative examples.

Implementation proceeds in reviewable milestones:

1. Preserve declaration identity and ordered argument identity while extending
   substitution and instance keys to typed const arguments. Check every unused
   definition under its declared binders before generating concrete instances.
2. Add first-order nominal impl-head matching, scoped substitutions, overlap and
   ownership checks. A selected candidate is provisional. Prove all its where,
   inline and supertrait premises using independent already available evidence
   before committing its witness or associated equations.
3. Normalize associated types/constants from committed evidence. Symbolic
   projections in generic definitions require the actual declared trait bound;
   concrete instances cannot rescue an invalid unused definition. Distinguish
   an active repeated projection cycle from nonrepeating growth, which exhausts
   deterministic request budgets.
4. Specialize generic methods with distinct trait/type/method binder scopes and
   publish existing ordinary calls and machine signatures. Preserve default
   declaration context and check unused or overridden bodies.

No runtime dictionary, new allocation, permission, or trusted solver boolean is
introduced. Expanded programs still pass ordinary Types, Analyze, independent
Verify and existing Wire/native backends. Shared schema or syntax changes are
owned by the integrator and will be requested as exact patches when needed.

Closest required counterexamples include an impl whose associated equation
would prove its own premise; overlapping generic/concrete heads; foreign trait
and foreign outer Self through a local wrapper argument; two distinct trait
instances exposing the same member; missing declared projection bounds in an
unused definition; overridden invalid defaults; binder shadowing; repeated
projection cycles; and strictly growing obligations at low budgets.

Validation uses existing `generics` and `impl162_trait_bounds` test targets,
retained source positives and nearest negatives, unused definitions, raw/Wire
round trips, actual Clang/LLVM API O0/O2 where enabled, and N/N−1/fresh-cache
budget controls. No formal whole-workspace acceptance or terminal-completion
claim is made by a milestone commit. Rust comparison evidence distinguishes
actual safety/runtime/expression/cost checks from unmeasured product gates.
