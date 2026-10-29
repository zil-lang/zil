#![forbid(unsafe_code)]
//! Query metadata and untrusted disk artifacts. Neither content hashes nor
//! cached bytes can manufacture a verifier capability.

mod cache;
pub use cache::{ArtifactCache, CacheCorruption, CacheRead, CacheWriteError};

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use zil_base::{Diagnostic, Fingerprint, ModuleId, Phase, ResultCategory};

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}
#[derive(Debug, Default)]
struct CancellationState {
    requested: AtomicBool,
    deadline: Option<Instant>,
}
impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            state: Arc::new(CancellationState {
                requested: AtomicBool::new(false),
                deadline: Instant::now().checked_add(timeout),
            }),
        }
    }
    pub fn cancel(&self) {
        self.state.requested.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.state.requested.load(Ordering::Acquire)
            || self
                .state
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }
    pub fn check(&self, phase: Phase) -> Result<(), Diagnostic> {
        if self.is_cancelled() {
            Err(Diagnostic::new(
                phase,
                ResultCategory::Cancelled,
                "E-CANCELLED",
                "compilation was cancelled before publishing this phase",
                None,
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryKey {
    pub module: ModuleId,
    pub phase: Phase,
    pub input: Fingerprint,
    pub rules: Fingerprint,
    pub target: Fingerprint,
}
impl QueryKey {
    pub fn fingerprint(&self) -> Fingerprint {
        let mut bytes = b"ZIL-QUERY-KEY-2\0".to_vec();
        bytes.extend_from_slice(&self.module.0.to_le_bytes());
        bytes.extend_from_slice(self.phase.as_str().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&self.input.0);
        bytes.extend_from_slice(&self.rules.0);
        bytes.extend_from_slice(&self.target.0);
        Fingerprint::of(&bytes)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FingerprintKind {
    Semantic,
    Abi,
    PrivateImplementation,
    SourcePositions,
    BuildInputs,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultFingerprints {
    pub semantic: Fingerprint,
    pub abi: Fingerprint,
    pub private_implementation: Fingerprint,
    pub source_positions: Fingerprint,
    pub build_inputs: Fingerprint,
}
impl ResultFingerprints {
    pub fn get(&self, kind: FingerprintKind) -> Fingerprint {
        match kind {
            FingerprintKind::Semantic => self.semantic,
            FingerprintKind::Abi => self.abi,
            FingerprintKind::PrivateImplementation => self.private_implementation,
            FingerprintKind::SourcePositions => self.source_positions,
            FingerprintKind::BuildInputs => self.build_inputs,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub key: QueryKey,
    pub kind: FingerprintKind,
    pub observed: Fingerprint,
}
#[derive(Clone, Debug)]
pub struct QuerySnapshot {
    pub key: QueryKey,
    pub status: ResultCategory,
    pub fingerprints: ResultFingerprints,
    pub dependencies: Vec<Dependency>,
    pub bytes: Arc<[u8]>,
    pub revision: u64,
}
#[derive(Default)]
pub struct Database {
    snapshots: BTreeMap<QueryKey, Arc<QuerySnapshot>>,
    /// Exact producer -> consumer edges for the currently published snapshots.
    /// Edge kinds and observations remain in the snapshot; duplicate inputs
    /// need only one reverse entry. This index is not verifier authority.
    reverse: BTreeMap<QueryKey, BTreeSet<QueryKey>>,
    active: BTreeSet<QueryKey>,
    revision: u64,
}
fn query_error(code: &str, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Driver, code, None, message)
}
impl Database {
    pub fn begin(&mut self, key: QueryKey) -> Result<(), Diagnostic> {
        if !self.active.insert(key) {
            return Err(query_error(
                "E-QUERY-CYCLE",
                "query depends on an unfinished query",
            ));
        }
        Ok(())
    }
    pub fn abort(&mut self, key: &QueryKey) {
        self.active.remove(key);
    }

    /// Check the whole dependency closure, including in-progress ancestors.
    /// Looking only at direct fingerprints could expose a transitively stale
    /// snapshot after one of its inputs was replaced.
    pub fn read(&self, key: &QueryKey) -> Result<Option<Arc<QuerySnapshot>>, Diagnostic> {
        let mut pending = vec![(key.clone(), false)];
        let mut visiting = BTreeSet::new();
        let mut complete = BTreeSet::new();
        while let Some((current, finished)) = pending.pop() {
            if finished {
                visiting.remove(&current);
                complete.insert(current);
                continue;
            }
            if self.active.contains(&current) {
                return Err(query_error(
                    "E-QUERY-CYCLE",
                    "cannot observe an active query dependency",
                ));
            }
            if complete.contains(&current) {
                continue;
            }
            let Some(snapshot) = self.snapshots.get(&current) else {
                return Ok(None);
            };
            if !visiting.insert(current.clone()) {
                return Err(query_error(
                    "E-QUERY-CYCLE",
                    "cached query dependency cycle",
                ));
            }
            pending.push((current, true));
            for dependency in snapshot.dependencies.iter().rev() {
                if self.snapshots.get(&dependency.key).is_none_or(|input| {
                    input.fingerprints.get(dependency.kind) != dependency.observed
                }) {
                    return Ok(None);
                }
                pending.push((dependency.key.clone(), false));
            }
        }
        Ok(self.snapshots.get(key).cloned())
    }

    /// Publish only a completed query. Failed publication aborts the active
    /// record; cancellation and infrastructure failure are never cache entries.
    pub fn publish(
        &mut self,
        key: QueryKey,
        status: ResultCategory,
        fingerprints: ResultFingerprints,
        dependencies: Vec<Dependency>,
        bytes: Vec<u8>,
    ) -> Result<Arc<QuerySnapshot>, Diagnostic> {
        let checked = (|| {
            if !self.active.contains(&key) {
                return Err(query_error(
                    "E-QUERY-NOT-ACTIVE",
                    "publish requires a begun query",
                ));
            }
            if matches!(
                status,
                ResultCategory::Cancelled | ResultCategory::InfraError
            ) {
                return Err(query_error(
                    "E-QUERY-INCOMPLETE",
                    "cancelled or infrastructure-failed work cannot be published",
                ));
            }
            for dependency in &dependencies {
                let input = self.read(&dependency.key)?;
                if input.is_none_or(|input| {
                    input.fingerprints.get(dependency.kind) != dependency.observed
                }) {
                    return Err(query_error(
                        "E-QUERY-STALE-INPUT",
                        "input changed or became incomplete during query",
                    ));
                }
            }
            Ok(())
        })();
        self.active.remove(&key);
        checked?;
        self.invalidate(&key, &fingerprints);
        self.revision = self.revision.saturating_add(1);
        let result = Arc::new(QuerySnapshot {
            key: key.clone(),
            status,
            fingerprints,
            dependencies,
            bytes: bytes.into(),
            revision: self.revision,
        });
        if let Some(previous) = self.snapshots.insert(key.clone(), result.clone()) {
            self.remove_reverse_edges(&previous);
        }
        for dependency in &result.dependencies {
            self.reverse
                .entry(dependency.key.clone())
                .or_default()
                .insert(key.clone());
        }
        Ok(result)
    }

    /// Invalidate dependents only for the result kind each edge observes, then
    /// follow the actual reverse dependency closure. Replacing an input through
    /// `publish` invokes this operation automatically.
    pub fn invalidate(&mut self, key: &QueryKey, new: &ResultFingerprints) -> Vec<QueryKey> {
        let mut gone = BTreeSet::new();
        let mut pending = Vec::new();
        if let Some(consumers) = self.reverse.get(key) {
            for consumer in consumers {
                let snapshot = &self.snapshots[consumer];
                if snapshot.dependencies.iter().any(|dependency| {
                    dependency.key == *key && new.get(dependency.kind) != dependency.observed
                }) {
                    gone.insert(consumer.clone());
                    pending.push(consumer);
                }
            }
        }
        // A missing input invalidates every consumer, regardless of which
        // fingerprint kind its edge observes. Traverse only that lost closure.
        while let Some(producer) = pending.pop() {
            if let Some(consumers) = self.reverse.get(producer) {
                for consumer in consumers {
                    if gone.insert(consumer.clone()) {
                        pending.push(consumer);
                    }
                }
            }
        }
        // Keep the whole index until traversal finishes, then unlink each
        // deleted snapshot from its producers. The public result stays sorted.
        for key in &gone {
            if let Some(snapshot) = self.snapshots.remove(key) {
                self.remove_reverse_edges(&snapshot);
            }
        }
        gone.into_iter().collect()
    }

    fn remove_reverse_edges(&mut self, snapshot: &QuerySnapshot) {
        for dependency in &snapshot.dependencies {
            if let Some(consumers) = self.reverse.get_mut(&dependency.key) {
                consumers.remove(&snapshot.key);
                if consumers.is_empty() {
                    self.reverse.remove(&dependency.key);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.reverse.clear();
        self.active.clear();
    }
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

#[cfg(test)]
mod tests;
