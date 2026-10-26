# From fixed interfaces to responsibility — premises and counterexamples

## Paper facts and their limits

Deng/Hani/Ma's §2.3 and Proposition 8.14 identify the two new ends with the original
bond variable and compose ordered integral operators. Definition 11.10 selects
globally low atoms and cuts locally high descendants; Proposition 11.12 preserves
opposite absences of top/bottom fixed ends on the two regions. Definition 12.1 and
Proposition 12.2 extend this to regular molecules and C-fixed ends. Proposition
9.7 counts good/bad elementary molecules for an analytic gain estimate. None is a
general compiler ownership, bounded-width or polynomial-time theorem. In the
paper's normal {3} molecule, “unique fixed end” is a local degree property.

The mapping below is our proposed finite relational interpretation. It is not a
claim made by the paper. Background variable elimination and decision diagrams
are established algorithms; citations are in SOURCES.md.

## A. Exact boundary theorem (finite Boolean semiring)

Let internal variables be X and observable interface variables B. Suppose the
event network denotes `R(B) = exists X. (F1 and ... and Fm)`. Every occurrence of a
shared guard, identity or state port refers to one coordinate. For an internal
variable x, partition the factors into A containing all occurrences of x and C
containing none. Then

`exists x. (A and C) = (exists x. A) and C`.

Repeated application proves that eliminating x after its last incident factor is
exact for any schedule. Conjunction and existential quantification are the finite
counterparts of combining factors and integrating internal variables; no equality
of analytic estimates or geometric hypotheses is asserted. Unchanged state fields
are frame equalities, not forgotten variables. This identity also works with an
arbitrarily correlated input relation included as a factor.

For a transfer with overwritten input bits W and fresh output bits W', its image
is `exists W. input(pre) and guard(pre) and equations(pre,W')`, followed by renaming
W' to the output coordinates. Cut and monolithic methods use this exact same
formula. The transfer therefore agrees on every set of finite states. Induction
over a worklist yields the same least reachable fixed point, including cycles.
Guard-violation relations are recorded before applying the transfer; eliminating
an impossible guard cannot erase the evidence of an earlier violation or abort.

### A.1 Identity-end substitution and the ablation

For a final incident equation `y = x` (or `y = not x`), with fresh output y,
`exists x. R(x,B) and (y=x) = R(y,B)` (respectively `R(not y,B)`). The candidate
may use this substitution only when: (i) y is the unique output coordinate of one
equation; (ii) x is an overwritten pre-state coordinate; (iii) every other factor
mentioning x has already been combined into R; and (iv) R retains its entire joint
relation with B. Output coordinates never occur on another equation's right side.
The BDD implementation uses ITE to restore ordering when x and y have different
positions; changing a node label in place would be wrong. An input bit that also
remains an output is not eligible. A shared input read by two equations is eligible
only at its last incidence, after the earlier equation's correlation is in R.

`mono` joins first; `cut` performs general last-incidence quantification; `wire`
adds only the identity-end shortcut to cut. All use identical event relations and
the same BDD variable order. The three-way ablation separates scheduling from the
shortcut. No uniqueness-of-owner conclusion follows merely from finding such an
equation. This is elementary finite relational substitution, not a new Boolean
algebra algorithm or an application of UP without its geometric premises.

## B. Unique responsibility is a separate invariant

For each active incarnation r, exactly one actual carrier owns r. A manufacture
creates r only when it is absent; a move consumes one carrier and produces one;
a release/return/caller handoff retires the caller's responsibility. Descriptions
carry no ownership. Assuming the corresponding preconditions and independently
correct call-boundary contracts, induction proves uniqueness until retirement.
The event-cut theorem preserves this invariant because it preserves the **whole**
relation. It does not establish the premise by graph topology alone.

Closure transport must include every captured identity. Replacing a closure by a
single Boolean “live” loses responsibility width. Repeated allocation requires a
new actual incarnation: reuse of a finite site coordinate is permitted only after
a checked absence of its previous active token. This rejects the false bridge
from a unique syntactic point to a unique dynamic object.

Three obligations are separate: **at most one carrier**, **no unpaid debt at a
normal/error return**, and **the required release order**. A leaked sole owner
satisfies the first and violates the second; two reversed drops can satisfy the
first two and violate the third. RustBelt's affine logic permits abandoning a
resource, so its safety theorem cannot supply the second obligation here.

Finite site reuse additionally relies on the description invalidation check:
an old Desc or borrowed closure that can still be used after retirement must
prevent that path from entering a later activation. Token absence alone is not
sufficient. The current guard-insensitive future-use query can over-reject this
case; it cannot authorize a stale description merely because the same position
became live again. Permissions and arbitrary allocation multiplicity remain
outside the admitted domain.

## C. Required nearest counterexamples

1. A branch transfers exactly one of A/B. Its joint relation contains two rows;
   Cartesian products of marginals add both-live and neither-live rows.
2. Two pieces use the same condition. Cutting it into independent conditions
   admits a mixed path that the original CFG cannot execute.
3. A loop redefines a Bool or reconstructs a resource. A global fixed old guard or
   an unchecked allocation-site identity conflates different activations.
4. Two consumers use one fixed-position carrier. Location identity does not
   authorize two moves or two releases; uniqueness must reject the second.
5. Two live resources cross an interface. Dropping the initialization-order edge
   can authorize an incorrect cleanup sequence even if both are released once.
6. An error path retains debt, or an abort path is labelled paid. Keeping only the
   normal returned state is an unsound cut.
7. A description/borrowed closure crosses the cut. Retaining owner bits while
   forgetting its actual root permits invalidation through a live future use.

Executable controls are in `tools/research/059/tests.rs`; complete performance
receipts are separate from the finite algebra and model-boundary arguments here.

## D. What would qualify as a structural opportunity

The useful property is small **semantic interface**, including guards, identities,
description roots, lifecycle and ordering, together with inexpensive factor
construction and a compact joint input relation. A small CFG separator, one
release-site syntax node or an UP-looking order is insufficient. A positive result
requires exactness, honest fragment admission and full-cost gains over the same
symbolic representation, followed by independent review before any core proposal.

The next production-facing target is the joint interface among Window, Model,
Subscription and callbacks that survive a call/frame boundary. The current
package UI example only borrows Model fields inside a local view and moves the
whole value after its last use; saved cross-frame callbacks are not enabled.
That future interface requires activity/permission and responsibility histories,
not just the number of variables in one transition's elimination plan.
If a future Subscription retains a callback borrowing Model, closing Window must
prove that cancellation/draining or a Hold contract removes future access before
Model is destroyed. Today's declaration/cleanup order is not automatically valid
for that different temporal interface.
