# Dynamic dependency envelopes and source relations

`OwnedDyn<dyn Trait, Context>` retains one explicit, finite dependency envelope.
`Context` is a complete ordinary qualified type, including its nominal identity,
ordered type/const/life actuals, phantom classes and joint lifetime partition.
It contributes metadata rather than another runtime field or allocation. The
one-argument spelling has an independently proved empty envelope. A table does
not erase a provider's hidden borrowed dependencies into that spelling.

An interface's complete Context and trait application participate in semantic
identity. Two interfaces for the same trait with different Contexts remain
different types. A compiler token or relocation index supplies no type, lifetime
or implementation authority.

Each public port is a typed upper bound on a finite set of external targets.
Ports are canonicalized by complete leaf type, stored-borrow mode and fixed
class, preserving first-occurrence order. One provider may use none, one or
several actual Self leaves for a port. `DynRetainedPortBinding` therefore has one
explicit row for every public port and a possibly empty `sources` list. Missing
rows are Unknown. Each actual borrowed Self leaf must be covered exactly once,
with its complete type, mode and actual class relation. Owned fields cannot
stand in for borrowed leaves. Shared ports grant no external write or management
permission; replacing a shared-pointer slot requires exclusive Self access but
does not turn its pointee into writable storage.

`self_classes` maps the complete persistent Self class domain into the Context's
Fixed domain. It includes phantom classes. A nominal life occurrence is selected
by the original declaration and full argument ordinal. Equal actual lifetimes
may merge classes while retaining every original occurrence. Formal ordinals,
Self class numbers and Context class numbers are different namespaces. No
same-number or same-type lookup substitutes for the complete occurrence map.

At construction, each kernel follows the actual source SSA value and every real
borrowed leaf through the checked table rows. It retains all targets and parent
loans. The owner's own allocation/address and obligation remain separate from
those external targets. The source is cleaned exactly once on allocation failure;
a failed constructor publishes no live owner. The canonical error alternative
has explicit empty dependency rows because its success payload is inactive.
An empty row never supplies an owner, address, allocation or management right.

`retained_return_sources` is a source upper bound, not an exact assignment.
`None` is Unknown. `Some([])` is a finite empty upper and requires every actual
returned dependency to be empty. A nonempty table has one row for each result
port, including explicit empty rows; missing, repeated and unrelated rows reject.
Result positions include their actual aggregate/error/capture carrier followed
by `RetainedPort(k)`. Sources select actual parameter leaves or opaque ports.
Every selected source must have the complete compatible leaf type, mode and
actual signature lifetime class. Equal lifetime classes do not equate targets:
returning the first of two same-typed inputs does not satisfy an upper containing
only the second.

Each kernel proves every reachable Return's actual targets are included in that
function's declared upper. A caller substitutes the checked upper using actual
argument permissions and all their parents. It prepares all result and after
permissions from one call-before snapshot before publishing any update. A helper
cannot manufacture a root from an interface ID, return type, local pointer slot
or candidate plan. A fresh erased owner does not imply fresh external targets.
Ordinary Task and callable-result solvers retain their separate obligations;
this field does not authorize an otherwise unknown holder or environment.

`DynGuarantee::StorageSources` denotes a receiver port's post-call source upper.
Sources referring to receiver ports select the call-before state. A complete
port table is required. A conservative default may retain the old port together
with compatible same-Fixed argument leaves; the old borrow can consequently
last until owner cleanup. Narrower guarantees require proof from every actual
provider CFG. Establish new retained permissions and their parents before
replacing a row, and release retired permissions last. Only one actual receiver
head permits a strong update. Several candidate heads require weak updates so
that an unselected owner's old dependencies remain live.

A borrowed result pointing at an inline Self field retains the actual owner
head. A result obtained by loading a saved pointer retains that port's actual
external pointee targets. These sources are distinct. A short receiver-bounded
result also preserves the real owner/address parent loans, without adding the
owner's address to the pointee target set. A long Fixed result needs the actual
external source relation and cannot obtain its lifetime from the temporary
receiver borrow. Moves, replacement, Store mutation and cleanup remain subject
to all live head, payload, element-window and external loans.

Whole-owner cleanup uses `DynInterface.owning_cleanup`, independently of every
method's cleanup contract. Its public external positions are rooted at Result
and select retained ports. `environment = true` consumes the carrier obligation;
it never grants ownership of a borrowed external leaf. Concrete cleanup follows
the actual Self resource identities, user teardown, nested owners, live Store
prefix and closure remainder before the allocation is freed and before external
parent loans are released. Unknown or unproved asynchronous cleanup remains
Unknown even if an effects list names ordinary synchronous effects.
