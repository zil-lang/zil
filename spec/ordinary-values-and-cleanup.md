# Ordinary values and synchronous iteration

`Value` is a reserved structural bound for a complete finite ordinary value that can be moved and placed in an inline field. It grants no `Copy`, `StoreElement`, `SyncDrop`, `Transfer`, `Share`, or empty retained context. A sealed `StoreElement` premise entails both `Value` and `SyncDrop`: its operational contract includes synchronously cleaning any live prefix and any failed insertion input. Every future element admission must prove that cleanup contract. `Copy` also establishes both structural obligations. Neither implication grants `Copy`, `Transfer`, `Share`, or an empty retained context. The reverse implications do not hold. The spellings are reserved contextually in bound and type-binder positions: users cannot implement or declare traits for these bounds, or shadow them with a type binder. Ordinary nominal types named `Value` or `SyncDrop` remain valid; type positions resolve those actual nominal declarations, while a bare bound keeps its builtin meaning.

`SyncDrop` is a separate cleanup premise. An unused generic definition must prove it when it leaves a live ordinary `Value` at a cleanup edge, discards a value, or overwrites an initialized value. Returns, propagation, scope exits, match arms and loop exits use the actual move state. An expression that transfers the old value out before replacement does not drop that old value. Moving a complete value through `identity<T:Value>(value:T)->T` does not require `SyncDrop`. Already transferred fields do not contribute another cleanup obligation.

These are definition obligations, not core permissions. Actual canonical Product, Sum, closure, resource and borrowed types still enter Types and both independent kernels. A borrowed value retains its actual source, class, access mode and parent loan through helpers. Copying a type with a retained context does not close that context. Task and asynchronous registration values do not gain synchronous destruction from `Value`; their actual executable state and cleanup protocol remain required, including in unused definitions.

A generic Product recipe records whether each ordinary formal requires `Value` or `StoreElement`. Types checks the actual argument against that declaration and independently checks every written `Store<T>` recipe. Changing a Value-only field recipe to a Store field cannot manufacture an element qualification. These source recipes do not become TypedIR ownership certificates.

An ordinary generic Enum may declare a direct formal payload without spelling
an additional bound, for example `enum Option<T> { None, Some(T) }`. This is a
parameterized type-constructor scheme with conditional payload-formation
obligations; it does not establish a global `Value` fact for an arbitrary T.
Check the original declaration, all variants and field recipes, nominal
identity, finite recursion and every nested formation requirement even when
the declaration is unused. At each complete application, independently prove
the required inline payload qualification in that application's original
caller context, preserving the actual carrier mode and complete borrowed and
phantom domains. Checking one concrete specialization cannot supply a missing
premise for a generic caller. In particular, constructing `Option<T>` in a
generic function still requires its original formation premises; a later i32
call cannot provide them. Source may transport normalized formation conditions
derived from the original field recipes, but Types, Analyze and Verify must
independently reconstruct and check them. These conditions grant no Copy,
SyncDrop, StoreElement, ownership, access or thread capability, and do not
exempt ordinary user declarations by name or by a compiler flag.

`std.iter.Iterator` transfers each actual owning `Item` out of its receiver. Its item and mapper output explicitly require `Value+SyncDrop`. Only `collect` separately requires `where I.Item:StoreElement`; ordinary inline ownership remains a weaker promise than Store admission. `Map` and `Filter` own their complete source and callback as ordinary `Value+SyncDrop` fields. They are lazy iterators and can be nested without a collection between adapters. `Drain` owns a Store header and can itself be placed in a Store when its complete finite element shape proves the sealed element and cleanup contract. Each layer retains a distinct actual manager, heap interval and parent loan. This also permits a Package containing Store headers and explicit direct owners such as `Store<Managed<Store<Item>>>`, `Store<Managed<ByteBuffer>>` and `Store<Managed<Ticket>>`. A bare resource name remains a description, so `Store<Store<Item>>` does not silently become owning storage. Inline unguarded recursion and unresolved element shapes do not qualify.

`drain` visits the last stored element first. `into_iter` prepares a reverse stack once so later traversal follows insertion order. `collect` grows its private Store through `std.store.push_growing`; allocation failure cleans the rejected item, partial result, remaining iterator and callback owners exactly once. `Collected` transfers that storage through `into_store`. `for_each_owned`, `for_each_lent` and `fold_owned` use the same associated Item identity with ordinary mutable callback helpers. Lending callbacks receive only the current invocation's shared Item address; capturing and mutating that iterator's live receiver conflicts with its actual loan. A fold callback consumes its previous owning accumulator and yields its next complete accumulator.

Qualified ordinary Type actuals retain separate physical syntax and semantic source application, ordered free classes and lexical domain. An instantiated helper alpha-renames the actual group's joint class partition into its own hidden life binders; source spelling never becomes a global identity. Parameter and result source applications are captured before header erasure, while final Core contracts and actual call origins remain independent obligations.

Associated arguments keep the complete trait application and each independent binder throughout definition checking and specialization, including protected binders and cross-module declarations. A candidate trait must actually declare the requested method. Equal parameter spellings, equal machine layout and unrelated bare method names do not merge providers. Nominal premises cannot supply a missing structural, callable, or cleanup obligation.

Executable controls are in `crates/zil-driver/tests/iterator_adapters.rs`, `generic_product_methods.rs` and `borrowed_product_store_captures.rs`, with ordinary library consumers and an allocation observer in `tests/language-readiness/iterator-adapters`. Native acceptance uses Clang and LLVM API at O0/O2. Raw Core recipe mutations and raw/Wire TypedIR mutations must be rejected by their actual independent consumers after refreshing the untrusted fingerprint.

An omitted lifetime on an ordinary Source function's data Address input denotes
an implicit invocation domain at that exact declaration, parameter ordinal and
nested type path. Independent omitted input occurrences have independent source
domains; actual call substitution may relate them only through the real inputs.
These hygienic domains are not additional user generic parameters. A bare
borrowed result uses the receiver domain, or the unique input domain when there
is no receiver; an ambiguous result needs an explicit relation. A generic result
that transfers the whole actual `T` preserves the complete joint input shape
rather than selecting one of its borrowed leaves. Ordinary helper result hints
are substituted through the complete semantic input applications before another
generic call uses them. Code signatures introduce their own invocation domain:
`Fn`, `FnMut` and `FnOnce` parameter/result lifetimes are code contracts, not
inferred retained slots of the callable environment. Neither elision nor a
Source hint supplies actual roots, class equality, ownership or access rights.
