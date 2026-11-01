# Move current emitted call operands into their machine events

Stage12 baseline f15cf54a43. The current FunctionEmitter constructs owned machine
parameters and call arguments, but the final event constructor clones the
parameter vector once and clones the call argument vector twice before moving
those clones into another vector. The original owned temporaries have no later
consumer. This work affects ordinary, indirect and concrete closure calls,
including dynamic scalar call graphs; closed zero-argument calls already folded
by the existing scalar summary path do not take the ordinary call branch.

Move the completed parameter/argument vectors directly into the same events.
In these four event constructors only, move the single-use result name and
machine return type too, and consume the String returned by symbol without an
extra to_string allocation. Preserve every fallible llvm_type/result expression
in its original order and position, every argument's order and Unit omission,
closure environment position, event order, ABI attribute, preflight and current
Verify. No public interface, source acceptance, budget or native behavior changes.

This is a small ownership adjustment, not a new optimization pass. Existing real
source direct, indirect and closure controls and the frozen machine/native
comparison are the appropriate validation. Reduced allocations are evident from
the removed ownership copies; any whole-request speed claim still needs the
root-controlled combined measurement. Do not attribute the entire measured
machine-lowering phase to these copies or extrapolate a separate latency gain.
