# Threaded foreign registration

Authority: `_zil_design.md` §12.3. The threaded route uses the existing `AsyncRegistration<F>` owner and pollable closing protocol. It does not grant that owner Transfer or Share.

## Provider and identity

The actual register, request-close and poll-drain functions must belong to the same role family. `ThreadAsyncRegister`, `ThreadAsyncRequestClose`, `ThreadAsyncPollDrain` use role tags 6, 7, 8 in the joint schema/Wire46. Each retains the ordinary exact async callback ABI, static-callback declaration, effects, and symbol identity checks. A same-named function, a mixture of role families or transported metadata cannot provide admission.

The owner retains an immutable prefix of actual site thunks. A borrowed generic close helper dispatches through that prefix; it cannot reinterpret a threaded record as a same-thread record. Existing same-thread thunks retain their non-atomic path.

The provider must publish fully initialized callback arguments to external threads. Before registration exposes userdata, the runtime publishes the initialized environment with a release store to OpenIdle; a successful callback acquires that publication. A failed registration may return null only after withdrawing every published entry and synchronously draining every raw holder and access. Failure then destroys the transferred environment and frame once.

## Separate callable obligations

`FnMut(i32)->i32`, `ClosedEnvironment`, `Transfer`, and `ThreadCallable` express distinct requirements. Transfer concerns the environment value and its cleanup, and does not imply that executing its entry is permitted on another thread.

`ThreadCallable` is compiler sealed. With the declared signature and receiver preconditions satisfied, the complete actual execution graph of the call must be valid on the external threads permitted by this protocol. The graph includes called helpers/callbacks and temporary-value cleanup during invocation. This capability grants neither concurrent FnMut access nor reentrancy, environment Transfer/Share, owner transfer, or synchronous final environment cleanup.

An unused generic installation definition must establish the required premises explicitly. `CoreCallableParameter.transfer` and `thread_callable` transport those premises with false defaults; neither field proves a concrete closure. Each actual instantiation is checked, and Analyze and Verify independently reconstruct the real threaded Start environment, entry graph, and cleanup qualification. A missing premise is not repaired by a successful concrete instance.

The implemented sealed execution profile consists of finite scalar/complete values, closed Copy Record/Array companions, and uniquely defined builtin Managed ByteBuffer leaves. The actual call graph admits checked synchronous scalar/product and capture operations, ByteBuffer length/capacity access, and proved static Direct calls. Borrowed environments, user Drop, other resource families, unknown foreign calls, TLS, indirect calls, and asynchronous cleanup require separate capability support and cannot inherit this profile. Final environment cleanup must independently be synchronous and valid on the closing owner's thread.

## Admission and close

One atomic gate combines admission and invocation state:

| Gate | Meaning |
| --- | --- |
| OpenIdle | Admission open; no invocation holds F. |
| OpenBusy | Admission open; one invocation holds F. |
| ClosingIdle | Admission closed; no invocation holds F. |
| ClosingBusy | Admission closed; the admitted invocation still holds F. |
| Closed | Environment cleanup completed. |

Callback admission uses a single acquire/release CAS from OpenIdle to OpenBusy. An unsuccessful admission never calls F. Closing uses CAS from either open state to its matching closing state, so there is no separate open-check/active-registration gap. Callback completion releases the busy bit without erasing a concurrent closing bit. A subsequent successful invocation acquires the preceding invocation's writes.

A separate atomic driver serializes request-close, poll-drain, and environment cleanup. It remains held throughout cleanup. Valid reentrant request/poll operations cannot drive the provider or cleanup again; reentrant poll returns Pending. Cleanup completes before publishing Closed with release ordering. Finish-destroy requires Closed and deallocates only after its acquire observation.

ClosingIdle or active=0 is never sufficient for reclamation. Provider drain result 1 must cover every original raw user, including threads that acquired userdata before entering the trampoline, rejected entries, retained queues/wakers, and provider accesses still in progress, and must promise no future access. Result 0 preserves the complete owner and returns Pending. Other result tags never authorize cleanup. Environment destruction occurs once, only with the driver held, ClosingIdle, and complete no-future/drain. Closed precedes frame deallocation.

The gate does not confer permission for a callback to acquire and close its own registration owner. Callback self-close needs a separate safe source API and ownership contract. Runtime white-box reentrancy controls do not establish such a source capability.

## Evidence obligations

Execution must cover initialization publication, serialized real external-thread calls, a raw holder paused before trampoline admission, close/admission races, late rejected entries, allocation/install failure, invalid drain, early destroy, and cleanup reentrancy. Repeating a stress loop does not establish a specific ClosingBusy interleaving. Source, independent raw/Wire, actual provider behavior, and precise source review remain required; transport and this contract alone do not establish release acceptance.
