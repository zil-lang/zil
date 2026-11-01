//! Bounded function emission with source-ordered results and diagnostics.
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_base::{CompileResult, Diagnostic, Phase, ResultCategory};

pub(super) const MIN_PARALLEL_FUNCTIONS: usize = 128;
pub(super) const BATCH_SIZE: usize = 64;

// Construction estimates choose execution strategy only; they are not budgets.
const MIN_WORK_PER_WORKER: usize = 4096;

#[derive(Default)]
pub(super) struct RemainingWork {
    total: usize,
    largest: usize,
    batches: usize,
}

impl RemainingWork {
    pub(super) fn add(&mut self, work: usize) {
        if work == 0 {
            return;
        }
        self.total = self.total.saturating_add(work);
        self.largest = self.largest.max(work);
        self.batches = self.batches.saturating_add(1);
    }

    pub(super) fn jobs(&self, requested: usize) -> usize {
        if self.total.saturating_sub(self.largest) < MIN_WORK_PER_WORKER {
            return 1;
        }
        requested
            .clamp(1, 64)
            .min(self.batches)
            .min(self.total / MIN_WORK_PER_WORKER)
            .max(1)
    }
}

pub(super) fn ordered_batches<T, U, F>(input: &[T], jobs: usize, emit: F) -> CompileResult<Vec<U>>
where
    T: Sync,
    U: Send,
    F: Fn(usize, &[T]) -> CompileResult<U> + Sync,
{
    let jobs = jobs.clamp(1, 64);
    if jobs == 1 || input.len() < MIN_PARALLEL_FUNCTIONS {
        return emit(0, input).map(|batch| vec![batch]);
    }
    let count = input.len().div_ceil(BATCH_SIZE);
    let next = AtomicUsize::new(0);
    let mut batches = std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(jobs.min(count));
        let mut failed = None;
        for _ in 0..jobs.min(count) {
            let next = &next;
            let emit = &emit;
            match std::thread::Builder::new().spawn_scoped(scope, move || {
                let mut batches = Vec::new();
                loop {
                    let id = next.fetch_add(1, Ordering::Relaxed);
                    if id >= count {
                        break;
                    }
                    let start = id * BATCH_SIZE;
                    let end = (start + BATCH_SIZE).min(input.len());
                    batches.push((id, emit(start, &input[start..end])));
                }
                batches
            }) {
                Ok(worker) => workers.push(worker),
                Err(_) => {
                    failed = Some("cannot start machine lowering worker");
                    break;
                }
            }
        }
        let mut batches = Vec::with_capacity(count);
        // Do not return on a source error or panic until every started worker
        // has joined. No partial items escape the failed request.
        for worker in workers {
            match worker.join() {
                Ok(output) => batches.extend(output),
                Err(_) => failed = Some("machine lowering worker panicked"),
            }
        }
        if let Some(message) = failed {
            Err(vec![Diagnostic::new(
                Phase::Codegen,
                ResultCategory::InfraError,
                "E-CODEGEN-WORKER",
                message,
                None,
            )])
        } else {
            Ok(batches)
        }
    })?;
    batches.sort_unstable_by_key(|(id, _)| *id);
    batches.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, mpsc};

    #[test]
    fn workers_require_substantial_independent_ordinary_batch_work() {
        // Exact constant and compact-leaf plans never add ordinary body work.
        let cheap_only = RemainingWork::default();
        assert_eq!(cheap_only.jobs(64), 1);

        let mut tiny_batches = RemainingWork::default();
        for _ in 0..128 {
            tiny_batches.add(8);
        }
        assert_eq!(tiny_batches.jobs(4), 1);

        let mut distributed = RemainingWork::default();
        for _ in 0..4 {
            distributed.add(4096);
        }
        assert_eq!(distributed.jobs(1), 1);
        assert_eq!(distributed.jobs(2), 2);
        assert_eq!(distributed.jobs(64), 4);

        let mut one_batch = RemainingWork::default();
        one_batch.add(8192);
        assert_eq!(one_batch.jobs(64), 1);

        let mut dominant = RemainingWork::default();
        dominant.add(100_000);
        dominant.add(64);
        assert_eq!(dominant.jobs(64), 1);
    }

    #[test]
    fn source_order_survives_reversed_batch_completion() {
        let input = (0..128).collect::<Vec<usize>>();
        let (sent, received) = mpsc::channel();
        let received = Mutex::new(received);
        let output = ordered_batches(&input, 2, |start, batch| {
            assert_eq!(start, batch[0]);
            if batch[0] == 0 {
                received
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            } else {
                sent.send(()).unwrap();
            }
            Ok(batch.to_vec())
        })
        .unwrap();
        assert_eq!(output.into_iter().flatten().collect::<Vec<_>>(), input);

        let (sent, received) = mpsc::channel();
        let received = Mutex::new(received);
        let error = ordered_batches(&input, 2, |start, batch| {
            assert_eq!(start, batch[0]);
            if batch[0] == 0 {
                received
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            } else {
                sent.send(()).unwrap();
            }
            Err::<(), _>(vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-ORDER-CONTROL",
                None,
                batch[0].to_string(),
            )])
        })
        .unwrap_err();
        assert_eq!(error[0].message, "0");
    }

    #[test]
    fn panic_joins_other_workers_and_never_returns_partial_success() {
        let input = (0..128).collect::<Vec<usize>>();
        let finished = AtomicUsize::new(0);
        let error = ordered_batches(&input, 2, |start, batch| {
            assert_eq!(start, batch[0]);
            assert_ne!(batch[0], 0, "intentional worker panic");
            finished.fetch_add(batch.len(), Ordering::SeqCst);
            Ok(batch.len())
        })
        .unwrap_err();
        assert_eq!(finished.load(Ordering::SeqCst), 64);
        assert_eq!(error[0].code, "E-CODEGEN-WORKER");
        assert_eq!(error[0].category, ResultCategory::InfraError);
    }

    #[test]
    fn small_input_or_one_job_remains_on_the_calling_thread() {
        let current = std::thread::current().id();
        for (count, jobs) in [(0, 64), (127, 2), (128, 1), (256, 0)] {
            let input = vec![(); count];
            assert_eq!(
                ordered_batches(&input, jobs, |start, batch| {
                    assert_eq!(start, 0);
                    assert_eq!(std::thread::current().id(), current);
                    Ok(batch.len())
                })
                .unwrap(),
                vec![count]
            );
        }
    }
}
