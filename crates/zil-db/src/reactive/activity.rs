use super::{GraphError, Node, ReactiveGraph};

impl<K: Ord + Clone, V: Clone + Eq> ReactiveGraph<K, V> {
    /// Install an optional consumer without scheduling it. Its dependencies are
    /// learned when first observed. Unlike `ensure_query`, this does not promote
    /// an existing deferred owner to an observed root.
    pub fn ensure_deferred_query(&mut self, owner: K) -> Result<(), GraphError> {
        self.ready()?;
        self.stats.inventory_visits += 1;
        if let Some(node) = self.nodes.get(&owner) {
            return if node.input {
                Err(GraphError::InputQueryConflict)
            } else {
                Ok(())
            };
        }
        let mut node = Node::new(false, None);
        node.observed = false;
        self.insert_node(owner, node);
        Ok(())
    }

    /// Control a consumer root. Only roots and their actual dependency closure
    /// execute during `flush`. A shared dependency remains live until its last
    /// live receiver and its own root observation are released. Changes to a
    /// dormant node are retained as dirty, then settled on reactivation.
    ///
    /// `ensure_query` and public `read` observe their owner for compatibility.
    /// An input has no scheduled work, so observing it is a no-op.
    pub fn set_observed(&mut self, owner: &K, observed: bool) -> Result<(), GraphError> {
        self.ready()?;
        let node = self.nodes.get(owner).ok_or(GraphError::MissingOwner)?;
        if node.input || node.observed == observed {
            return Ok(());
        }
        let was_live = node.live();
        self.node_mut(owner)
            .expect("observed owner exists")
            .observed = observed;
        self.propagate_activity(owner, was_live);
        Ok(())
    }

    pub(super) fn change_live_receiver(&mut self, owner: &K, add: bool) {
        let was_live = self.nodes[owner].live();
        let node = self.node_mut(owner).expect("dependency exists");
        if add {
            node.live_receivers += 1;
        } else {
            node.live_receivers = node
                .live_receivers
                .checked_sub(1)
                .expect("live dependency bond exists");
        }
        self.propagate_activity(owner, was_live);
    }

    fn propagate_activity(&mut self, owner: &K, was_live: bool) {
        let mut pending = vec![(owner.clone(), was_live)];
        while let Some((key, was_live)) = pending.pop() {
            let node = &self.nodes[&key];
            let live = node.live();
            if live == was_live {
                continue;
            }
            self.stats.activity_visits += 1;
            let dependencies = node.dependencies.iter().cloned().collect::<Vec<_>>();
            let height = node.height;
            if node.dirty {
                if live {
                    self.schedule((height, key.clone()));
                    self.stats.enqueues += 1;
                } else {
                    self.unschedule(&(height, key.clone()));
                }
            }
            for dependency in dependencies {
                let node = self.node_mut(&dependency).expect("dependency exists");
                let was_live = node.live();
                if live {
                    node.live_receivers += 1;
                } else {
                    node.live_receivers = node
                        .live_receivers
                        .checked_sub(1)
                        .expect("live dependency bond exists");
                }
                pending.push((dependency, was_live));
            }
        }
    }
}
