//! Stable-owner reactive queries. Values describe validity; owner keys describe
//! identity. This graph is an in-memory scheduling aid, not verifier authority.
//!
//! Inputs are installed as a batch before any receiver is notified. A changed
//! value only notifies its direct receivers; equal query results stop the wave.
//! Evaluators record their actual reads, replacing both sides of obsolete bonds.
//! An evaluation failure poisons the candidate graph until `clear`. A
//! `transaction` instead restores the previous epoch on failure or unwinding;
//! callers never observe a successful value from a partially applied batch.
//!
//! Nodes are acyclic scheduling units. A language-level recursive component must
//! be represented by one owner whose evaluator solves the component according to
//! that stage's explicit fixed-point contract. An arbitrary value equality is
//! not a convergence proof; dynamic graph cycles remain errors.

mod activity;
mod transaction;
use transaction::Journal;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    MissingOwner,
    Cycle,
    InputQueryConflict,
    Poisoned,
    Evaluation(String),
}
impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOwner => f.write_str("reactive dependency owner is missing"),
            Self::Cycle => f.write_str("reactive query dependency cycle"),
            Self::InputQueryConflict => {
                f.write_str("reactive owner changed between input and query")
            }
            Self::Poisoned => f.write_str("failed reactive candidate must be cleared"),
            Self::Evaluation(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for GraphError {}

/// Work counters, excluding evaluator-specific AST/encoding work. Retained
/// counts are current gauges; other fields count work since the last reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReactiveStats {
    pub executed: u64,
    pub equal_results: u64,
    pub changed_results: u64,
    pub cycle_check_edges: u64,
    /// Edges inspected to pull a clean query or trim its obsolete read set.
    pub validation_edge_reads: u64,
    pub rank_edge_reads: u64,
    pub edge_reads: u64,
    pub notifications: u64,
    pub enqueues: u64,
    pub deduplicated: u64,
    pub reused_bonds: u64,
    pub new_bonds: u64,
    pub removed_bonds: u64,
    pub rank_visits: u64,
    pub input_updates: u64,
    pub inventory_visits: u64,
    /// Nodes snapshotted on their first mutation in a transaction.
    pub journal_nodes: u64,
    pub journal_queue_entries: u64,
    pub rolled_back_transactions: u64,
    /// Nodes whose consumer liveness changed (not an inventory scan).
    pub activity_visits: u64,
    pub retained_nodes: u64,
    pub retained_edges: u64,
}

/// The evaluator's immutable environment belongs to the current input batch.
/// Every value affecting a query must be obtained through `read` (or be covered
/// by an input read); external mutable state is not tracked by the graph.
pub trait ReactiveEvaluator<K, V> {
    fn evaluate(
        &self,
        owner: &K,
        read: &mut dyn FnMut(&K) -> Result<V, GraphError>,
    ) -> Result<V, GraphError>;
}
impl<K, V, F> ReactiveEvaluator<K, V> for F
where
    F: Fn(&K, &mut dyn FnMut(&K) -> Result<V, GraphError>) -> Result<V, GraphError>,
{
    fn evaluate(
        &self,
        owner: &K,
        read: &mut dyn FnMut(&K) -> Result<V, GraphError>,
    ) -> Result<V, GraphError> {
        self(owner, read)
    }
}

#[derive(Clone)]
struct Node<K, V> {
    input: bool,
    value: Option<V>,
    dependencies: BTreeSet<K>,
    receivers: BTreeSet<K>,
    active_reads: BTreeSet<K>,
    pending_read: Option<K>,
    height: u64,
    dirty: bool,
    active: bool,
    rank_pending: bool,
    observed: bool,
    live_receivers: usize,
}
impl<K, V> Node<K, V> {
    fn live(&self) -> bool {
        self.observed || self.live_receivers != 0
    }

    fn new(input: bool, value: Option<V>) -> Self {
        Self {
            input,
            value,
            dependencies: BTreeSet::new(),
            receivers: BTreeSet::new(),
            active_reads: BTreeSet::new(),
            pending_read: None,
            height: u64::from(!input),
            dirty: !input,
            active: false,
            rank_pending: false,
            observed: !input,
            live_receivers: 0,
        }
    }
}

pub struct ReactiveGraph<K, V> {
    nodes: BTreeMap<K, Node<K, V>>,
    // One queue membership per dirty owner. Rank changes move that membership.
    queue: BTreeSet<(u64, K)>,
    poisoned: bool,
    edges: u64,
    topology_revision: u64,
    active_count: u64,
    stats: ReactiveStats,
    journals: Vec<Journal<K, V>>,
}
// A snapshot owns its current state, never another graph's rollback history.
// In particular cloning inside a transaction must not create an orphaned
// savepoint which retains old products forever.
impl<K: Clone, V: Clone> Clone for ReactiveGraph<K, V> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            queue: self.queue.clone(),
            poisoned: self.poisoned,
            edges: self.edges,
            topology_revision: self.topology_revision,
            active_count: self.active_count,
            stats: self.stats,
            journals: Vec::new(),
        }
    }
}

impl<K, V> Default for ReactiveGraph<K, V> {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            queue: BTreeSet::new(),
            poisoned: false,
            edges: 0,
            topology_revision: 0,
            active_count: 0,
            stats: ReactiveStats::default(),
            journals: Vec::new(),
        }
    }
}

impl<K: Ord + Clone, V: Clone + Eq> ReactiveGraph<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    fn ready(&self) -> Result<(), GraphError> {
        if self.poisoned {
            Err(GraphError::Poisoned)
        } else {
            Ok(())
        }
    }

    /// Duplicate owners in a batch use the last supplied value. No evaluator
    /// runs, and no notification occurs, until the complete batch is installed.
    pub fn set_inputs(
        &mut self,
        inputs: impl IntoIterator<Item = (K, V)>,
    ) -> Result<(), GraphError> {
        self.ready()?;
        let inputs = inputs.into_iter().collect::<BTreeMap<_, _>>();
        self.stats.inventory_visits += inputs.len() as u64;
        if inputs
            .keys()
            .any(|key| self.nodes.get(key).is_some_and(|n| !n.input))
        {
            return Err(GraphError::InputQueryConflict);
        }
        let mut changed = Vec::new();
        for (key, value) in inputs {
            self.stats.inventory_visits += 1;
            if self
                .nodes
                .get(&key)
                .is_some_and(|node| node.value.as_ref() == Some(&value))
            {
                continue;
            }
            if let Some(node) = self.node_mut(&key) {
                node.value = Some(value);
            } else {
                self.insert_node(key.clone(), Node::new(true, Some(value)));
            }
            self.stats.input_updates += 1;
            changed.push(key);
        }
        for key in changed {
            self.notify(&key);
        }
        Ok(())
    }

    pub fn ensure_query(&mut self, owner: K) -> Result<(), GraphError> {
        self.ready()?;
        self.stats.inventory_visits += 1;
        if let Some(node) = self.nodes.get(&owner) {
            return if node.input {
                Err(GraphError::InputQueryConflict)
            } else {
                self.set_observed(&owner, true)
            };
        }
        self.insert_node(owner.clone(), Node::new(false, None));
        self.schedule((1, owner));
        self.stats.enqueues += 1;
        Ok(())
    }

    fn enqueue(&mut self, key: &K) {
        let node = self.node_mut(key).expect("bond has a live receiver");
        if node.dirty {
            self.stats.deduplicated += 1;
        } else {
            node.dirty = true;
            let height = node.height;
            if node.live() {
                self.schedule((height, key.clone()));
                self.stats.enqueues += 1;
            }
        }
    }

    fn notify(&mut self, key: &K) {
        let receivers = self.nodes[key]
            .receivers
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for receiver in receivers {
            self.stats.notifications += 1;
            // An old bond not yet read by an active evaluator is provisional.
            // Its later read pulls the current value; it needs no extra run.
            let node = &self.nodes[&receiver];
            if !node.active
                || (node.active_reads.contains(key) && node.pending_read.as_ref() != Some(key))
            {
                self.enqueue(&receiver);
            }
        }
    }

    /// Flush only scheduled owners, in actual dependency-height order.
    pub fn flush<E: ReactiveEvaluator<K, V> + ?Sized>(
        &mut self,
        evaluator: &E,
    ) -> Result<(), GraphError> {
        self.ready()?;
        let result = self.flush_inner(evaluator);
        if result.is_err() {
            self.poison();
        }
        result
    }

    fn flush_inner<E: ReactiveEvaluator<K, V> + ?Sized>(
        &mut self,
        evaluator: &E,
    ) -> Result<(), GraphError> {
        while let Some((_, owner)) = self.queue.first().cloned() {
            self.ensure_current(&owner, evaluator)?;
        }
        Ok(())
    }

    /// Public reads settle the whole installed input batch before returning.
    /// Evaluator reads pull their own dependencies instead of running unrelated
    /// receivers of an unfinished query, which would create false cycles.
    pub fn read<E: ReactiveEvaluator<K, V> + ?Sized>(
        &mut self,
        owner: &K,
        evaluator: &E,
    ) -> Result<V, GraphError> {
        self.set_observed(owner, true)?;
        self.flush(evaluator)?;
        let result = self.ensure_current(owner, evaluator);
        if result.is_err() {
            self.poison();
        }
        result
    }

    fn ensure_current<E: ReactiveEvaluator<K, V> + ?Sized>(
        &mut self,
        owner: &K,
        evaluator: &E,
    ) -> Result<V, GraphError> {
        let node = self.nodes.get(owner).ok_or(GraphError::MissingOwner)?;
        if node.active {
            return Err(GraphError::Cycle);
        }
        if node.input {
            return node.value.clone().ok_or(GraphError::MissingOwner);
        }
        // A clean node can still have a scheduled transitive ancestor. Usually
        // height scheduling has already settled it; dynamic reads may reach
        // outside that order, so only then inspect the actual dependency path.
        if !node.dirty
            && (!node.live()
                || self
                    .queue
                    .first()
                    .is_some_and(|(height, _)| *height < node.height))
        {
            let dependencies = node.dependencies.iter().cloned().collect::<Vec<_>>();
            for dependency in dependencies {
                self.stats.validation_edge_reads += 1;
                self.ensure_current(&dependency, evaluator)?;
            }
        }
        if !self.nodes[owner].dirty {
            return self.nodes[owner]
                .value
                .clone()
                .ok_or(GraphError::MissingOwner);
        }
        self.evaluate(owner, evaluator)
    }

    fn evaluate<E: ReactiveEvaluator<K, V> + ?Sized>(
        &mut self,
        owner: &K,
        evaluator: &E,
    ) -> Result<V, GraphError> {
        let height = self.nodes[owner].height;
        self.unschedule(&(height, owner.clone()));
        let node = self.node_mut(owner).expect("query exists");
        node.dirty = false;
        node.active = true;
        node.active_reads.clear();
        self.active_count += 1;
        self.stats.executed += 1;
        let mut read_error = None;
        let mut structural_change = false;
        let result = evaluator.evaluate(owner, &mut |dependency| {
            self.stats.edge_reads += 1;
            let result = (|| {
                // Record the read intent before pulling: nested queries must
                // see this in-progress edge when checking their actual graph.
                let first_read = self
                    .node_mut(owner)
                    .expect("query exists")
                    .active_reads
                    .insert(dependency.clone());
                if first_read {
                    self.node_mut(owner).expect("query exists").pending_read =
                        Some(dependency.clone());
                }
                let topology_before = self.topology_revision;
                let value = self.ensure_current(dependency, evaluator)?;
                self.node_mut(owner).expect("query exists").pending_read = None;
                if !first_read {
                    return Ok(value);
                }
                if self.nodes[owner].dependencies.contains(dependency) {
                    // An unchanged, strictly ordered bond needs no closure
                    // walk. Nested topology changes or postponed rank updates
                    // invalidate that proof, even though the bond is reused.
                    let source = &self.nodes[dependency];
                    let receiver = &self.nodes[owner];
                    if (topology_before != self.topology_revision
                        || source.rank_pending
                        || receiver.rank_pending
                        || source.height >= receiver.height)
                        && self.reaches(dependency, owner)
                    {
                        return Err(GraphError::Cycle);
                    }
                    // Reuse the actual existing bond and its reverse link.
                    self.stats.reused_bonds += 1;
                } else {
                    // New bonds check the actual read graph. Active queries
                    // retain old bonds physically, but those bonds do not
                    // participate before they are read in this evaluation.
                    if dependency == owner || self.reaches(dependency, owner) {
                        return Err(GraphError::Cycle);
                    }
                    self.node_mut(owner)
                        .expect("query exists")
                        .dependencies
                        .insert(dependency.clone());
                    self.node_mut(dependency)
                        .expect("read dependency exists")
                        .receivers
                        .insert(owner.clone());
                    if self.nodes[owner].live() {
                        self.change_live_receiver(dependency, true);
                    }
                    self.edges += 1;
                    self.topology_revision += 1;
                    self.stats.new_bonds += 1;
                    structural_change = true;
                }
                Ok(value)
            })();
            if let Err(error) = &result {
                read_error = Some(error.clone());
            }
            result
        });
        if let Some(error) = read_error {
            return Err(error);
        }
        let value = result?;
        self.stats.validation_edge_reads += self.nodes[owner].dependencies.len() as u64;
        let obsolete = self.nodes[owner]
            .dependencies
            .difference(&self.nodes[owner].active_reads)
            .cloned()
            .collect::<Vec<_>>();
        structural_change |= !obsolete.is_empty();
        for dependency in obsolete {
            self.node_mut(owner)
                .expect("query exists")
                .dependencies
                .remove(&dependency);
            self.node_mut(&dependency)
                .expect("dependency exists")
                .receivers
                .remove(owner);
            if self.nodes[owner].live() {
                self.change_live_receiver(&dependency, false);
            }
            self.edges -= 1;
            self.stats.removed_bonds += 1;
            self.topology_revision += 1;
        }
        self.active_count -= 1;
        let node = self.node_mut(owner).expect("query exists");
        node.active = false;
        node.active_reads.clear();
        if structural_change || node.rank_pending {
            self.update_heights(owner);
        }
        let node = self.node_mut(owner).expect("query exists");
        let changed = node.value.as_ref() != Some(&value);
        node.value = Some(value.clone());
        if changed {
            self.stats.changed_results += 1;
            self.notify(owner);
        } else {
            self.stats.equal_results += 1;
        }
        Ok(value)
    }

    // Cycle detection follows actual bonds, with no arbitrary depth threshold.
    fn reaches(&mut self, from: &K, target: &K) -> bool {
        if self.nodes[target].receivers.is_empty() && self.active_count <= 1 {
            return false;
        }
        let mut pending = vec![from.clone()];
        let mut seen = BTreeSet::new();
        while let Some(key) = pending.pop() {
            if &key == target {
                return true;
            }
            if seen.insert(key.clone()) {
                let node = &self.nodes[&key];
                let dependencies = if node.active {
                    &node.active_reads
                } else {
                    &node.dependencies
                };
                self.stats.cycle_check_edges += dependencies.len() as u64;
                pending.extend(dependencies.iter().cloned());
            }
        }
        false
    }

    // Structural changes raise or lower the affected receiver closure. The
    // queue is local, deduplicated, and independent of numeric graph depth.
    fn update_heights(&mut self, owner: &K) {
        let mut pending = VecDeque::from([owner.clone()]);
        let mut queued = BTreeSet::from([owner.clone()]);
        while let Some(key) = pending.pop_front() {
            queued.remove(&key);
            self.stats.rank_visits += 1;
            let node = &self.nodes[&key];
            self.stats.rank_edge_reads += node.dependencies.len() as u64;
            let height = if node.input {
                0
            } else {
                node.dependencies
                    .iter()
                    .map(|dep| self.nodes[dep].height)
                    .max()
                    .unwrap_or(0)
                    + 1
            };
            if node.height == height {
                self.node_mut(&key).expect("rank owner exists").rank_pending = false;
                continue;
            }
            let node = &self.nodes[&key];
            if node.dirty && node.live() {
                let old_height = node.height;
                self.unschedule(&(old_height, key.clone()));
                self.schedule((height, key.clone()));
            }
            let node = self.node_mut(&key).expect("rank owner exists");
            node.rank_pending = false;
            node.height = height;
            let receivers = node.receivers.iter().cloned().collect::<Vec<_>>();
            for receiver in receivers {
                self.stats.rank_edge_reads += 1;
                if self.nodes[&receiver].active {
                    self.node_mut(&receiver)
                        .expect("receiver exists")
                        .rank_pending = true;
                } else if queued.insert(receiver.clone()) {
                    pending.push_back(receiver);
                }
            }
        }
    }

    /// Returns a settled value only. Pending batches may have transitive dirty
    /// receivers, so the accessor is deliberately unavailable until flush ends.
    pub fn get_current(&self, owner: &K) -> Option<&V> {
        if self.poisoned || !self.queue.is_empty() {
            return None;
        }
        self.nodes
            .get(owner)
            .filter(|node| !node.active && !node.dirty && (node.input || node.live()))?
            .value
            .as_ref()
    }

    /// Remove obsolete inventory owners, sever both bond directions, and dirty
    /// retained receivers. Their evaluator must stop reading removed owners.
    /// Call this only when the owner inventory changes, not for ordinary edits.
    pub fn retain(&mut self, owners: &BTreeSet<K>) -> Result<(), GraphError> {
        self.ready()?;
        self.stats.inventory_visits += self.nodes.len() as u64;
        let removed = self
            .nodes
            .keys()
            .filter(|key| !owners.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        self.remove_owners_inner(removed, false)
    }

    /// Remove only supplied owners. Work is proportional to the deleted owners
    /// and their incident bonds, plus the affected rank/consumer closures.
    /// Retained evaluators must stop reading a removed owner, or report a
    /// missing dependency. Wrap deletion and replacement in `transaction` when
    /// failure must preserve the previous successful epoch.
    pub fn remove_owners(&mut self, owners: impl IntoIterator<Item = K>) -> Result<(), GraphError> {
        self.ready()?;
        self.remove_owners_inner(owners, true)
    }

    fn remove_owners_inner(
        &mut self,
        owners: impl IntoIterator<Item = K>,
        count_owners: bool,
    ) -> Result<(), GraphError> {
        let mut affected = BTreeSet::new();
        for key in owners {
            self.stats.inventory_visits += u64::from(count_owners);
            let Some(node) = self.remove_node(&key) else {
                continue;
            };
            self.unschedule(&(node.height, key.clone()));
            self.stats.inventory_visits += (node.dependencies.len() + node.receivers.len()) as u64;
            let was_live = node.live();
            for dependency in node.dependencies {
                if let Some(dependency_node) = self.node_mut(&dependency) {
                    dependency_node.receivers.remove(&key);
                    if was_live {
                        self.change_live_receiver(&dependency, false);
                    }
                }
                self.edges -= 1;
                self.stats.removed_bonds += 1;
                self.topology_revision += 1;
            }
            for receiver in node.receivers {
                if let Some(receiver_node) = self.node_mut(&receiver) {
                    receiver_node.dependencies.remove(&key);
                    self.edges -= 1;
                    self.stats.removed_bonds += 1;
                    self.topology_revision += 1;
                    affected.insert(receiver);
                }
            }
        }
        for receiver in affected {
            if self.nodes.contains_key(&receiver) {
                self.enqueue(&receiver);
                self.update_heights(&receiver);
            }
        }
        Ok(())
    }

    fn poison(&mut self) {
        self.poisoned = true;
        self.clear_queue();
    }

    pub fn clear(&mut self) {
        if !self.journals.is_empty() {
            for key in self.nodes.keys().cloned().collect::<Vec<_>>() {
                self.record_node(&key);
            }
        }
        self.nodes.clear();
        self.clear_queue();
        self.poisoned = false;
        self.edges = 0;
        self.active_count = 0;
        self.topology_revision = 0;
    }

    pub fn stats(&self) -> ReactiveStats {
        ReactiveStats {
            retained_nodes: self.nodes.len() as u64,
            retained_edges: self.edges,
            ..self.stats
        }
    }

    pub fn take_stats(&mut self) -> ReactiveStats {
        let stats = self.stats();
        self.stats = ReactiveStats::default();
        stats
    }
}

#[cfg(test)]
mod tests;
