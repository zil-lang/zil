# 103 — bounded transparent generic aliases

Baseline: `24ca9583d8789cfdfe3ccdbfe3eeff614bf56429`; task: main-tree
`.task/impl/103.md`. This implements existing transparent type composition. It
does not add a runtime type, ownership rule, inference rule, ABI or schema tag.

## Closed source contract

* A module alias may declare distinct ordinary, unbounded type parameters:
  `type PairOf<T> = Pair<T>`, `type Swap<A, B> = Pair<B, A>` and `type Id<T> = T`.
  Application supplies exactly one type argument per parameter. Substitution is
  simultaneous and lexical: a caller's `T` is not substituted a second time.
* The supported target language is the existing finite alias/type syntax:
  nominal types, parameters, grouping, tuples, fixed arrays, borrowed addresses
  and slices, function signatures and the already implemented error union.
  Nominal identity, field privacy, Copy/Move, Managed/Desc and address authority
  are established by the existing ordinary stages after normalization.
* Definitions are checked even when unused. Unknown types, wrong arity, duplicate
  parameters, invalid callback/slice formation and alias dependency cycles are
  rejected. An alias cycle is illegal even if its arguments grow or the cyclic
  argument would later be discarded by a phantom parameter.
* Type syntax uses `Alias<T>`. A nominal constructor/method namespace uses
  `Alias::<T>`; it does not infer alias arguments from a later value context.
  Existing concrete aliases keep their bare namespace and enum-pattern support.
  A concrete alias such as `type IntOption = OptionOf<i64>` can be used in a
  pattern; bare generic aliases in patterns need explicit concretization first.
  This avoids erasing constraints on fixed/repeated alias arguments or adding a
  second inference algorithm.
* Project normalization retains definition-module identities and private-field
  boundaries. The single-module API must protect nominal declaration identities
  too: names which collide with a lexical generic binder are made hygienic before
  alias substitution. The existing lexical project visitor rewrites declarations
  and their references while respecting type and value shadowing. This prevents
  a definition's nominal `T` from becoming the caller's type parameter `T`, both
  within another alias and during later function specialization. Multiple imports
  of the same module do not create new types.
  Every lexical type binder and its scoped references also receive a distinct
  internal identity before substitution, including binders legally named `i64`.
  That protects builtin definitions without adding reserved binder names. A
  separate value binding is never rewritten merely for sharing that spelling.
  Alias parameters shadow types only in their lexical type scope; value names
  are not substituted.
* Const/lifetime parameters, bounds/where solving, attributed aliases, explicit
  lifetimes, Optional/union/dyn/opaque and CT-produced types remain outside this
  existing finite profile. Literal const arguments are diagnosed explicitly;
  they are never silently dropped or reinterpreted as types. Bounds on another
  generic declaration still reach that declaration's existing checker.

## Algorithm and budgets

The alias pass first bounds borrowed source declarations, then builds a finite
definition table. Each declaration is normalized once in its own parameter
scope. An active declaration set rejects dependency cycles, independently of
whether concrete arguments repeat. Every argument is normalized and checked
before substitution, including unused/phantom arguments.

Before cloning a resolved body, the existing generic budget traverses its entire
simultaneously substituted shape at the current depth. The resulting tree is
also checked before it is retained, passed to the generic engine or cloned into
a namespace/pattern. Node, depth and metadata/work quotas are unchanged. Alias
normalization grants no proof and cannot construct a Verified wrapper.

## Required evidence

Source-to-Verify positives and nearest precise rejections cover nested and
reordered aliases, lexical shadowing, phantom arguments, unused definitions,
cross-module re-export/private boundaries, CT consumers, resources/addresses,
callback formation, cycles, deep/wide expansion and exact N/N-1 budgets. A real
multi-module consumer combines aliases with existing generic records/enums and
functions; its stages and O0/O2 native results are retained. Existing alias,
module and slice-formation regressions are exercised without a full gate run.

The single historical test that classified `type Alias<T> = T` as Unsupported
must move to the supported cases; other old invalid-alias cases remain intact.
No product performance conclusion follows from these functional checks.
