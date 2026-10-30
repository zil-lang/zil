//! Opt-in compiler wall-clock diagnostics, never part of a semantic/cache key.
//!
//! A parent includes its children. Worker intervals can overlap, so neither
//! summing nested spans nor summing workers gives process wall time or CPU time.
//! The disabled path does not allocate, lock, or read a clock. The enabled path
//! deliberately includes collection overhead in the enclosing measured span.
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimingSpan {
    pub id: usize,
    pub parent: Option<usize>,
    pub name: &'static str,
    pub kind: &'static str,
    pub unit_index: Option<usize>,
    pub start_ns: u64,
    pub wall_ns: u64,
    pub outcome: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimingReport {
    pub spans: Vec<TimingSpan>,
}
impl TimingReport {
    pub fn to_json(&self) -> String {
        let mut out = String::from(
            "{\"clock\":\"monotonic_wall\",\"unit\":\"ns\",\"scope\":\"compiler request; excludes process startup and report serialization\",\"accounting\":\"parents include children; parallel workers overlap; not CPU time\",\"spans\":[",
        );
        for (index, span) in self.spans.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            write!(
                out,
                "{{\"id\":{},\"parent\":{},\"name\":{},\"kind\":{},\"unit_index\":{},\"start_ns\":{},\"wall_ns\":{},\"outcome\":{}}}",
                span.id,
                span.parent.map_or_else(|| "null".into(), |id| id.to_string()),
                quoted(span.name),
                quoted(span.kind),
                span.unit_index.map_or_else(|| "null".into(), |id| id.to_string()),
                span.start_ns,
                span.wall_ns,
                quoted(span.outcome),
            )
            .expect("String writes cannot fail");
        }
        out.push_str("]}");
        out
    }
}

fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Whether this compiler was built with the opt-in collection capability.
pub const fn available() -> bool {
    cfg!(feature = "timings")
}

#[cfg(feature = "timings")]
pub use collector::{TimingScope, TimingTrace};

#[cfg(feature = "timings")]
mod collector {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    #[derive(Debug)]
    struct State {
        origin: Instant,
        spans: Vec<TimingSpan>,
    }

    /// An optional observation sink. Cloning a disabled sink is allocation-free.
    #[derive(Clone, Debug, Default)]
    pub struct TimingTrace {
        state: Option<Arc<Mutex<State>>>,
        parent: Option<usize>,
        unit_index: Option<usize>,
    }
    impl TimingTrace {
        pub fn new(enabled: bool) -> Self {
            Self {
                state: enabled.then(|| {
                    Arc::new(Mutex::new(State {
                        origin: Instant::now(),
                        spans: Vec::new(),
                    }))
                }),
                ..Self::default()
            }
        }
        pub fn scope(&self, name: &'static str, kind: &'static str) -> TimingScope {
            self.unit_scope(name, kind, self.unit_index)
        }
        pub fn unit_scope(
            &self,
            name: &'static str,
            kind: &'static str,
            unit_index: Option<usize>,
        ) -> TimingScope {
            let Some(state) = &self.state else {
                return TimingScope {
                    trace: Self::default(),
                    start: None,
                };
            };
            let start = Instant::now();
            let mut state_guard = state.lock().unwrap_or_else(|error| error.into_inner());
            let id = state_guard.spans.len();
            let start_ns = nanos(start.saturating_duration_since(state_guard.origin));
            state_guard.spans.push(TimingSpan {
                id,
                parent: self.parent,
                name,
                kind,
                unit_index,
                start_ns,
                wall_ns: 0,
                outcome: "running",
            });
            TimingScope {
                trace: Self {
                    state: Some(state.clone()),
                    parent: Some(id),
                    unit_index,
                },
                start: Some(start),
            }
        }
        pub fn measure<T, E>(
            &self,
            name: &'static str,
            kind: &'static str,
            action: impl FnOnce(&Self) -> Result<T, E>,
        ) -> Result<T, E> {
            let mut scope = self.scope(name, kind);
            let result = action(scope.trace());
            scope.finish(if result.is_ok() { "ok" } else { "error" });
            result
        }
        pub fn value<T>(&self, name: &'static str, action: impl FnOnce() -> T) -> T {
            let mut scope = self.scope(name, "stage");
            let value = action();
            scope.finish("ok");
            value
        }
        pub fn report(&self) -> Option<TimingReport> {
            self.state.as_ref().map(|state| TimingReport {
                spans: state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .spans
                    .clone(),
            })
        }
    }

    /// Unfinished spans remain visible on early return or unwinding. A successful
    /// scope must explicitly finish; telemetry never converts an error to success.
    pub struct TimingScope {
        trace: TimingTrace,
        start: Option<Instant>,
    }
    impl TimingScope {
        pub fn trace(&self) -> &TimingTrace {
            &self.trace
        }
        pub fn finish(&mut self, outcome: &'static str) {
            let Some(start) = self.start.take() else {
                return;
            };
            let elapsed = nanos(start.elapsed());
            let state = self.trace.state.as_ref().expect("enabled scope has state");
            let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
            let span = &mut state.spans[self.trace.parent.expect("scope has span")];
            span.wall_ns = elapsed;
            span.outcome = outcome;
        }
    }
    impl Drop for TimingScope {
        fn drop(&mut self) {
            if self.start.is_none() {
                return;
            }
            self.finish(if std::thread::panicking() {
                "panic"
            } else {
                "early-exit"
            });
        }
    }
    fn nanos(duration: std::time::Duration) -> u64 {
        duration.as_nanos().min(u64::MAX as u128) as u64
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn disabled_trace_has_no_state_or_records() {
            let trace = TimingTrace::new(false);
            assert!(trace.state.is_none());
            assert_eq!(trace.measure("unused", "stage", |_| Ok::<_, ()>(7)), Ok(7));
            assert!(trace.report().is_none());
        }
        #[test]
        fn nested_errors_and_worker_intervals_keep_their_parentage() {
            let trace = TimingTrace::new(true);
            let result = trace.measure("request", "request", |request| {
                request.measure("workers", "parallel", |workers| {
                    std::thread::scope(|scope| {
                        let handles = (0..2)
                            .map(|index| {
                                scope.spawn(move || {
                                    let mut unit =
                                        workers.unit_scope("unit", "worker", Some(index));
                                    unit.trace().value("body", || index);
                                    unit.finish("ok");
                                })
                            })
                            .collect::<Vec<_>>();
                        for handle in handles {
                            handle.join().unwrap();
                        }
                    });
                    Err::<(), _>("preserved failure")
                })
            });
            assert_eq!(result, Err("preserved failure"));
            let spans = trace.report().unwrap().spans;
            assert_eq!(spans.len(), 6);
            assert_eq!(spans[0].outcome, "error");
            assert_eq!(spans[1].outcome, "error");
            for span in &spans[1..] {
                let parent = &spans[span.parent.unwrap()];
                assert!(parent.start_ns <= span.start_ns);
                assert!(span.start_ns + span.wall_ns <= parent.start_ns + parent.wall_ns);
                assert_ne!(span.outcome, "running");
            }
        }
    }
}

/// In ordinary builds timing propagation is zero-sized and needs no destructor.
/// Requests that require a report must check `available()`; the driver rejects
/// such requests before compilation when collection was not compiled in.
#[cfg(not(feature = "timings"))]
#[derive(Clone, Debug, Default)]
pub struct TimingTrace {
    _private: (),
}

#[cfg(not(feature = "timings"))]
#[derive(Default)]
pub struct TimingScope(TimingTrace);

#[cfg(not(feature = "timings"))]
impl TimingTrace {
    #[inline(always)]
    pub const fn new(_enabled: bool) -> Self {
        Self { _private: () }
    }
    #[inline(always)]
    pub fn scope(&self, _name: &'static str, _kind: &'static str) -> TimingScope {
        TimingScope(Self { _private: () })
    }
    #[inline(always)]
    pub fn unit_scope(
        &self,
        _name: &'static str,
        _kind: &'static str,
        _unit_index: Option<usize>,
    ) -> TimingScope {
        TimingScope(Self { _private: () })
    }
    #[inline(always)]
    pub fn measure<T, E>(
        &self,
        _name: &'static str,
        _kind: &'static str,
        action: impl FnOnce(&Self) -> Result<T, E>,
    ) -> Result<T, E> {
        action(self)
    }
    #[inline(always)]
    pub fn value<T>(&self, _name: &'static str, action: impl FnOnce() -> T) -> T {
        action()
    }
    #[inline(always)]
    pub const fn report(&self) -> Option<TimingReport> {
        None
    }
}

#[cfg(not(feature = "timings"))]
impl TimingScope {
    #[inline(always)]
    pub const fn trace(&self) -> &TimingTrace {
        &self.0
    }
    #[inline(always)]
    pub fn finish(&mut self, _outcome: &'static str) {}
}

#[cfg(all(test, not(feature = "timings")))]
mod stripped_tests {
    use super::*;
    #[test]
    fn propagation_is_zero_sized_without_drop_or_collection() {
        assert!(!available());
        assert_eq!(std::mem::size_of::<TimingTrace>(), 0);
        assert_eq!(std::mem::size_of::<TimingScope>(), 0);
        assert!(!std::mem::needs_drop::<TimingTrace>());
        assert!(!std::mem::needs_drop::<TimingScope>());
        let trace = TimingTrace::new(false);
        let mut calls = 0;
        let error = trace.measure("request", "stage", |child| {
            child.value("nested", || calls += 1);
            Err::<(), _>("original error")
        });
        assert_eq!(calls, 1);
        assert_eq!(error, Err("original error"));
        assert!(trace.report().is_none());
    }
}
