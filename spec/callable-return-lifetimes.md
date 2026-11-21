# Callable result lifetime contracts

An opaque callable result preserves the source bound's lifetime relation before
lowering its physical parameter and result types. An explicit source lifetime
must never disappear merely because two qualified addresses have the same ABI.

`CoreCallableReturn.life` is an untrusted Parameter/Result signature proposal.
Its Fixed indices are local to that bound. `source_life` records the original
source declaration and full original mixed parameter order. Every proposed
Fixed class selects an actual creator-header position through one
`CoreCallableReturnFixed.input`; runtime parameter position, original generic
position, header binder and full borrow/class path remain separate identities.
Anonymous input classes have no invented original generic lifetime parameter.
Missing, duplicate, wrong-kind or unresolved selectors cannot establish a class.
This context has no synthetic callable generic parameter or bound ordinal.

Types resolves these selectors against the actual creator header and infers the
actual environment independently. It maps the bound into the returned closure's
complete canonical Fixed domain, including actual free classes that the bound
does not mention. The Typed `CallableReturnContract.life` carries the expected
Parameter/Result relation in that domain. Capture grounding belongs to the
complete actual closure relation and the actual factory result-to-input mapping;
it cannot be inferred from the expected signature alone. A bound mentioning only
Fixed(1) is therefore not evidence that actual Fixed(0) is absent or ungrounded.

Analyze and Verify separately check the expected signature, actual environment,
all reachable returns, source substitution and cleanup. A public proposal or a
matching physical type supplies no target, loan, owner or transfer permission.
The Fixed domain is rigid across the actual capture and result relations.
Invocation classes must be grounded in actual invocation inputs. Borrowed
receiver dependencies cannot be erased by adapting a callable to a consuming
bound. Returning an original exclusive capture requires an actual unique take;
a temporary read or a new reborrow does not establish that transfer.

The return relation participates in complete runtime identity and lossless raw
Wire transport. Source bridge metadata is preserved across Core CT copies.
Before cloning, hashing or comparing untrusted metadata, consumers bound and
reserve all rows, full paths, declaration identity components and original
parameter kinds, including each Const carrier's complete type. Structural size
walkers do not resolve declarations or validate lifetime authority. Absence is
legacy metadata; presence must never be silently ignored.
