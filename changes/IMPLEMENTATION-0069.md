# IMPLEMENTATION-0069: owned callable factories and retained callbacks

Status: implementation, draft14 slice 069.

## Intended behavior and public boundary

A finite `-> impl Fn(A) -> R`, `impl FnMut(A) -> R`, or `impl FnOnce(A) -> R` declaration defines one opaque callable identity. The factory returns the actual concrete closure environment by value; callers may store, move, call, and clean up that environment using ordinary language operations. The callback parameters and result are finite Copy values. Captures are self-contained Copy snapshots or owned Managed values. No heap box, reference count, implicit lifetime extension, or private-callee body substitution is introduced.

Core records the declared callable bound in `CoreFunction.callable_return`; its return marker is `Type::Opaque` with the function identity and function signature. Typed IR resolves that marker to `Type::Closure` and publishes `callable_return_contract`. The contract has the actual closure id and exact callable kind, no external hold/call/write obligations, and sorted owned Managed capture indices in cleanup. Analyze and Verify validate the contract independently; the latter remains the only public interface publisher.

The first slice requires the declared kind to equal the capture-derived kind. Kind weakening/adaptation, factory recursion or forwarding, escaping Address/Desc captures, multiple hidden closure sites, and unconstrained opaque results produce explicit diagnostics. Ordinary Copy/Managed factory inputs and calls to ordinary constructors remain available. Source bodies are processed with factory results resolved before ordinary callers, retaining deterministic function identities and exposing only the public result contract across modules.

## Validation plan

Exercise Copy snapshot factories, owned Fn/FnMut/FnOnce environments, zero/multiple calls and ordinary cleanup, declaration order, cross-module visibility, return paths, and nearest rejected cases. Keep bounded type/inference metadata handling and source admission tests. Integration backend checks and independent raw-IR attacks are owned by 070. Preserve the first relevant failure and corrected rerun with exact candidate/spec/norm identities before review.

## Scope

This is synchronous retained ownership, not dynamic trait objects, asynchronous callbacks, heterogeneous callback collections, or a platform UI registry. It creates the callable factory boundary needed by future ZUI work without claiming those later facilities.

## Implementation progress

The source factory pass is complete for single modules and projects. Ordinary function ids remain source-ordered; synthesized closure entries follow that reserved range. Factories are resolved before ordinary bodies, and helper signature snapshots are installed afterwards so helpers receive the concrete public result. Cross-module bodies are checked for visibility in their defining module. A factory's body currently rejects direct calls to another factory or a generic callable helper; ordinary constructors/helper-free ordinary calls remain admitted.

Analyze validates each factory while visiting its existing per-function CFG, independently reconstructing self-contained capture cleanup and checking one creation site for the returned identity. This avoids a new whole-module pass for modules without factories. Factory call resource flow consumes the public envelope and fresh returned ownership, without consulting private effect rows.

Initial targeted checks found a misspelled private inference method; the corrected owned crate library tests passed. The first complete owned suite exposed an avoidable extra per-function analysis charge caused by a separate contract sweep; the check was moved into the existing function traversal instead of altering established exact-budget tests. Raw logs are retained under `evidence/impl/069`. Verify/backend integration is owned by slice 070; final verification remains pending until the schema11 consumers are available.

## Integrated targeted results

After consuming 070's CT and independent Verify/lowering/interface commits, all 18 retained-factory tests pass. They cover Copy and Managed Fn, Copy/Managed FnMut, consuming FnOnce, repeated calls across a loop, zero calls and normal cleanup, same-identity returns across branches, mixed capture cleanup indices, source declaration order, two modules with the same factory name under reversed module order, public helper composition, error callbacks, and rejection of borrow escape, kind weakening, recursion/forwarding, different hidden identities, moved/consumed reuse, and private capture projection. Three tests also seed raw Core/Typed inputs to check bounded metadata, absence of body substitution, and independent Analyze cleanup/missing-contract rejection.

The selected existing source regressions comprise 22 closure frontend cases, 17 helper cases and 9 generic cases. The owned Types/Expand/Analyze suites contain 50 tests. All targeted commands passed; these are author checks, not the independent review or full formal gate. Native execution of the new retained factory examples is owned by 070.

During integrated static review, 069 identified that skipping a factory's private resource effect row requires independently rejecting Address/Desc factory inputs in Verify, even though Types and Analyze already reject them. This was reported to 070, which is adding raw-IR and wire counterexamples and the independent public-boundary check. Source result mode is now exactly Infer, matching the frozen ordinary Value-return boundary.

## Author acceptance identity

The exact clean code candidate `bc5725727652ac18f8d7f0d4a646daa523231322` passed 116 test functions across the bounded owned and driver regression commands, both formatting checks, and both `-D warnings` Clippy commands. The tree was clean before and after those commands. The record is `evidence/impl/069/acceptance.json`, with raw stdout/stderr and commands under `author-final/`. Root's independent factory-input hardening (`20bb4ca`) is included. This author record does not replace independent review, the full integration gates, or 070's new native examples.
