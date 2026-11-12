use super::{Node, ReactiveGraph};
use std::collections::BTreeMap;

/// Before-images are collected on first write, not at transaction entry. The
/// owner and value should be small handles when products themselves are large.
#[derive(Clone)]
pub(super) struct Journal<K, V> {
    nodes: BTreeMap<K, Option<Node<K, V>>>,
    queue: BTreeMap<(u64, K), bool>,
    poisoned: bool,
    edges: u64,
    topology_revision: u64,
    active_count: u64,
}

struct Transaction<'a, K: Ord + Clone, V: Clone + Eq> {
    graph: &'a mut ReactiveGraph<K, V>,
    commit: bool,
}

impl<K: Ord + Clone, V: Clone + Eq> Drop for Transaction<'_, K, V> {
    fn drop(&mut self) {
        let journal = self
            .graph
            .journals
            .pop()
            .expect("transaction journal exists");
        if self.commit {
            return;
        }
        // Outer savepoints already recorded every write. Restoring inner state
        // must not replace their older before-images.
        for (key, node) in journal.nodes {
            if let Some(node) = node {
                self.graph.nodes.insert(key, node);
            } else {
                self.graph.nodes.remove(&key);
            }
        }
        for (entry, present) in journal.queue {
            if present {
                self.graph.queue.insert(entry);
            } else {
                self.graph.queue.remove(&entry);
            }
        }
        self.graph.poisoned = journal.poisoned;
        self.graph.edges = journal.edges;
        self.graph.topology_revision = journal.topology_revision;
        self.graph.active_count = journal.active_count;
        self.graph.stats.rolled_back_transactions += 1;
    }
}

impl<K: Ord + Clone, V: Clone + Eq> ReactiveGraph<K, V> {
    /// Apply a change atomically to this graph. Error and panic unwinding restore
    /// values, dependency bonds, consumer activity and scheduled work exactly.
    /// Nested calls are savepoints. Successful changes retain existing nodes;
    /// entry/commit never scan or clone the graph, including its clean nodes.
    ///
    /// The callback must propagate errors from graph operations. Evaluator
    /// external state and published files are outside this transaction: produce
    /// immutable handles here, then publish external artifacts after success.
    /// Work counters intentionally include failed attempts and rollback work.
    pub fn transaction<T, E>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        self.journals.push(Journal {
            nodes: BTreeMap::new(),
            queue: BTreeMap::new(),
            poisoned: self.poisoned,
            edges: self.edges,
            topology_revision: self.topology_revision,
            active_count: self.active_count,
        });
        let mut transaction = Transaction {
            graph: self,
            commit: false,
        };
        let result = change(&mut *transaction.graph);
        transaction.commit = result.is_ok();
        result
    }

    pub(super) fn record_node(&mut self, key: &K) {
        for journal in &mut self.journals {
            if !journal.nodes.contains_key(key) {
                journal
                    .nodes
                    .insert(key.clone(), self.nodes.get(key).cloned());
                self.stats.journal_nodes += 1;
            }
        }
    }

    pub(super) fn node_mut(&mut self, key: &K) -> Option<&mut Node<K, V>> {
        self.record_node(key);
        self.nodes.get_mut(key)
    }

    pub(super) fn insert_node(&mut self, key: K, node: Node<K, V>) {
        self.record_node(&key);
        self.nodes.insert(key, node);
    }

    pub(super) fn remove_node(&mut self, key: &K) -> Option<Node<K, V>> {
        self.record_node(key);
        self.nodes.remove(key)
    }

    fn record_queue(&mut self, entry: &(u64, K)) {
        for journal in &mut self.journals {
            if !journal.queue.contains_key(entry) {
                journal
                    .queue
                    .insert(entry.clone(), self.queue.contains(entry));
                self.stats.journal_queue_entries += 1;
            }
        }
    }

    pub(super) fn schedule(&mut self, entry: (u64, K)) {
        self.record_queue(&entry);
        self.queue.insert(entry);
    }

    pub(super) fn unschedule(&mut self, entry: &(u64, K)) {
        self.record_queue(entry);
        self.queue.remove(entry);
    }

    pub(super) fn clear_queue(&mut self) {
        if !self.journals.is_empty() {
            for entry in self.queue.iter().cloned().collect::<Vec<_>>() {
                self.record_queue(&entry);
            }
        }
        self.queue.clear();
    }
}
