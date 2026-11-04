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

The isolated public41 transport now passes all87 zil-ir tests (64 library,
13 independent Vec identity,3 slice Wire,5 byte/scoped Wire,2 value-transfer;
zero doctests) and lib/tests Clippy with warnings denied. The new raw control
round-trips all25 enumerated order pairs, preserves three distinct operands and
both order tags in identity, rejects unknown order bytes and every truncated
prefix. Raw transport deliberately does not endorse the16 incompatible order
pairs: actual independent semantic consumers must reject those before execution.
No runtime producer/kernel/native admission is established by these tests.
The initial test-only Diagnostic indexing and stale current-envelope assertions
were corrected; their failed build/test logs remain outside the source tree.
