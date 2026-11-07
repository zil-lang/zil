# 108: binding policy, preserved obligations and actual counterexamples

## What changes

Only declarations of existing foreign functions on the exact checked target
`x86_64-unknown-linux-gnu` gain LLVM's `nonlazybind` attribute. The function
symbol, return and parameter types, C calling convention, `zeroext`, call-site
arguments and all ordinary checked operations keep their existing emission.
Windows declarations do not gain the attribute.

The [LLVM Language Reference](https://llvm.org/docs/LangRef.html#function-attributes)
describes this as a binding-time choice with a possible call-speed/startup-cost
tradeoff. It provides no claim that the external function is pure, cannot
allocate/free, cannot synchronize, cannot throw, or cannot call back. None of
those attributes is added. The emitted LLVM bodies remain otherwise identical
in the focused attribute-ablation controls.

The current source contract gives finite C ABI and symbol information, unsafe
effects and explicit noescape/static-callback obligations. It does not promise
ELF lazy binding. This justifies proposing a Linux backend policy, not silently
claiming every loader observation is identical. The proposal explicitly calls
out startup resolution and missing-symbol/IFUNC timing.

## The implemented checks and what they establish

`impl108_foreign_binding` constructs real ZIL source, retains the Driver's
source-through-Lower reports, then uses public ordinary Types/Analyze/Verify/
lower/emission APIs to obtain LLVM for explicit links against C ABI fixtures.
It does not fabricate a Verified wrapper. The C files implement external test
oracles; the production compiler and benchmark applications remain ZIL/Rust.

| Control | Observation and scope |
| --- | --- |
| Scalar and unsigned narrow arguments/results | Real C reads/writes high `u8` and `u16` values at O0/O2; Linux `zeroext` remains on declaration and call |
| Stateful calls | Two calls to an external counter return 1 then 2; no purity or call-removal assumption |
| Mutable byte address | C updates the borrowed four-byte array; ZIL reads the updated fields after the call |
| Object/shared/full static links | The same source computes the same full result; static executables are checked for absence of an ELF interpreter |
| `LD_PRELOAD` | A replacement external symbol changes the result in both candidate and attribute-ablation programs |
| Range failure | EOF passed to checked `u8` conversion still reaches SIGILL before output at O0/O2 |
| Missing symbol | A used unresolved symbol fails link; an unused declaration does not create an eager binding use |
| Win64 emission | Foreign declarations retain the original ABI spelling and do not acquire `nonlazybind`; existing 087 C layout/static-callback regression also runs |

Readelf shows the focused candidate's selected external symbol as `GLOB_DAT`;
the attribute-ablation program has `JUMP_SLOT`. Disassembly is retained. This
establishes an actual machine-call change, not a whole-program speed result.
The ablation is accurately labeled as a one-attribute LLVM control; the runtime
experiment separately builds the exact BASE compiler from Git.

## Binding timing is observably different

The IFUNC fixture sets a private C flag when its loader resolver selects the
implementation. ZIL reads the flag before calling the selected function and
again afterward. At both O0/O2, the candidate prints 152 and the deferred-binding
ablation prints 52: the resolver has already run in the candidate.

This is deliberately retained as a difference, not reported as an equal-output
test. Resolver timing is a Linux loader observation outside the current ZIL
promise of exact lazy binding. Any later contract which grants deferred binding
as a source-level requirement would need to retain that promise. Ordinary
external function bodies are not moved out of their call sites by this change.

The complete-process experiment includes every startup control precisely
because eager work can cost more before useful execution. Observing a shorter
hot call path alone cannot grant production admission.

## Initial failures kept in the record

The first author run failed because the new test helper requested the Driver's
Lower stop and then expected an LLVM artifact. Lower intentionally precedes
LLVM emission. The helper now retains that valid phase report and performs the
explicit public verified emission/link pipeline required by the C fixture.

The second run's missing-symbol negative control initially rejected a target
triple override under `-Werror`: its standalone link command lacked the checked
target argument. The command was fixed to name that target; only then was the
expected unresolved-symbol diagnostic counted. Neither first failure was
deleted or relabeled as a passed negative test.

Four new groups, the original 087 real C ABI/static-retention group and targeted
Clippy passed in author run 003. The actual command/phase/native records and
test binaries are retained. The fresh 270 comparison/observer checks, memory
controls, formal runtime campaign and independent review are separate pending
obligations until their exact results are recorded.
