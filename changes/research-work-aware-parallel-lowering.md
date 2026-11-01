# Select machine lowering workers from remaining body work

Stage15 baseline 014c5d301. Stage14 subtracts constant summaries from its
parallel-lowering function count, but still counts parameter-plus-literal leaves
whose compact emission consists mostly of allocation. Large families of these
leaves can therefore start workers without enough independent ordinary lowering
work to repay the overhead.

Split the existing bounded scalar-leaf recognizer from its emitter. A Copy
Prepared record holds exactly the accepted operation, integer width, result,
literal order and source identity. The original emit wrapper remains. Preparation
uses the identical existing checks and budgets; emission constructs the identical
machine function and intrinsics. Preparing does not grant Verify authority.

Only when the previous jobs/function-count/closed-collection gate would have
started workers, prepare remaining scalar leaves once. Store aligned optional
Prepared records, borrow the appropriate slice in each source-indexed batch and
consume those records instead of repeating recognition. Constant summaries,
foreign declarations and discarded closed functions need no leaf preparation.
Jobs=1 and below-threshold cases retain the ordinary single-pass emitter.

Choose worker count from ordinary bodies left after these exact fast paths.
Estimate their construction work from the current parameter/local/value counts
and per-block instruction counts, summed into the actual 64-function dispatch
batches. Require at least 4096 estimated work units outside the largest batch and
at least 4096 per selected worker; otherwise emit serially. Bound workers by the
number of nonempty work batches and the existing requested jobs/64 cap.
These constants are scheduling heuristics, not language or compilation budgets:
no input is rejected, and a serial choice uses the same complete emitter. A
single dominant batch cannot justify parallelism by itself, even if it contains
several expensive bodies.

The existing source order, first ordinary error, intrinsic ordering, worker join
rules, cancellation boundaries, VerifiedModule binding, full current Verify and
machine preflight, LLVM text and cache bytes remain unchanged. Prepared records
are request-local raw plans; no persistent cache or new dependency/unsafe code.

Root runs focused serial/parallel real-source equivalence and complete-request
measurements. Compact controls cover cheap-only, distributed heavy and dominant
batch scheduling plus retained scalar-leaf emission equality. Gains and CPU/RSS
costs must use the new candidate identity, not inherit Stage14 timings.
