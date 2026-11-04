# Public atomic compare-exchange successor

Root publishes the dedicated opcode133 and schema/Wire41 successor in a separate
integration branch. The ongoing40 callable/CT5 baseline remains frozen; no
consumer is asked to infer the new operation from an ordinary load/store pair.
The343 request and explicit order matrix in changes/343-general-sync.md are now
normative §11.4. Root owns public fields/version and343 owns the CAS-specific
consumer and operand visitor hunks, coordinating with342 runtime life changes.

The LLVM model/API/text milestone16ed316d0 is actual machine-level support only.
Source3-argument SeqCst/SeqCst, independent kernels, decoded Wire, native
contention and nearest invalid cells/orders remain required before admission.
No old acceptance receipt, main39 or pending40 result is upgraded by publication.
