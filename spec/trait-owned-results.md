# Owned results from concrete Package trait methods

An ordinary nongeneric nominal trait may expose a fixed shared or exclusive
Package receiver method whose result is `Self` or `Error!Self`, where `Error`
is a plain nominal type or an existing scalar. The retained fixed parameter
profile permits scalar values and shared `Self` inputs. The actual error and
Package types must independently satisfy the existing ordinary value and
ErrorUnion rules; a spelling alone grants no ownership or storage capability.

`Self` substitution descends through both actual ErrorUnion arms. Full method
signatures, nominal declaration/provider identity, visibility and coherence
remain checked before an implementation becomes a callable target. Generic
consumers use their declared nominal bound, including when their definitions
are unused. Receiving a non-Copy result consumes it normally; a trait method
declaration does not establish Copy or authorize taking a field through a
shared receiver.

The implementation body constructs or transfers actual owners through the
ordinary call/result path. Types checks the complete concrete result, Analyze
and Verify independently check actual receiver permissions, result transfers
and cleanup, and an error edge cleans only its real constructed temporaries.
No new Core/Typed/Wire field, runtime dictionary or permission bit is introduced.

An unrelated life-generic trait does not change the checking category of a
concrete provider body. After the actual owner, premises, method set and complete
signature are checked, an ordinary nonconditional implementation of a
nongeneric trait keeps its closed method bodies on the ordinary Core/Types and
independent Analyze/Verify path. These bodies remain present and checked even
when unused. The original and aligned method headers must have no generics,
where obligations or additional contracts; symbolic definitions retain their
definition-time checker. Generic helper definitions and their selected error
arms are checked and preserved independently of the concrete caller.

The two immediate ErrorUnion children inspected by the new signature selector
are prepaid on the existing request ledger. Default limits are unchanged.

The cross-module program in
`tests/language-readiness/owning-trait-hosts/owned_results/` retains the original
ByteBuffer Package while producing and consuming separate owned results,
including a real error after allocating a temporary and allocator failures.
Concrete direct calls may save the result into an explicitly typed local before
matching it; inference for a direct temporary method-call ErrorUnion remains a
separate boundary. Generic consumers with a declared return type provide the
complete result context.

This does not add owning parameters, arbitrary nominal result shapes, trait or
method type parameters, new default-body rights, dyn slots, borrowed result
storage or new cleanup/transfer rules. Existing lending and scalar-result
methods retain their own profiles.
