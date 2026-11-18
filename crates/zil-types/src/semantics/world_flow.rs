//! Exact reaching worlds. The join is a union of whole states, never an
//! equality between two lifetime images. A repeated loop state is the fixed
//! point; every new state and comparison is paid from the request budget.
use super::*;
use std::collections::{BTreeSet, VecDeque};

pub(super) struct State {
    // An implicit TypeId is (schema, slot kind, slot ordinal). World verifies
    // every initial input, SSA producer, Store, Unpack and capture binding
    // against that immutable Typed directory before it can populate a cell.
    pub schema: FunctionId,
    pub instance: SemanticInstanceId,
    pub locals: Vec<Option<SharedType>>,
    pub values: ssa::Values,
    pub declared: BTreeMap<LocalId, SharedType>,
    pub implicit: BTreeMap<u32, SemanticClassExpr>,
    pub vacant_captures: BTreeSet<(SemanticInstanceId, u32)>,
    pub stale_values: BTreeSet<ValueId>,
    pub stale_locals: BTreeSet<LocalId>,
    pub scopes: Vec<ScopeId>,
    pub unsafe_regions: Vec<u32>,
    pub owners: BTreeMap<LocalId, ScopeId>,
    pub vacant: BTreeSet<(LocalId, Vec<PlaceProjection>)>,
    pub role_sources: BTreeMap<LocalId, roles::Incoming>,
    pub value_role_sources: BTreeMap<ValueId, roles::Incoming>,
    pub reborrows: BTreeMap<ValueId, arguments::Edge>,
    pub reservations: BTreeMap<ReservationId, reservations::Reservation>,
    pub value_roots: BTreeMap<ValueId, BTreeSet<arguments::Root>>,
    pub local_roots: BTreeMap<LocalId, BTreeSet<arguments::Root>>,
}
impl State {
    pub fn snapshot(&self, world: &mut World<'_, '_>) -> R<Self> {
        let before = *world.left;
        world.pay(
            self.locals
                .len()
                .saturating_add(self.values.pages.len())
                .saturating_add(self.declared.len())
                .saturating_add(self.implicit.len())
                .saturating_add(self.stale_values.len())
                .saturating_add(self.vacant_captures.len())
                .saturating_add(self.stale_locals.len())
                .saturating_add(self.scopes.len())
                .saturating_add(self.unsafe_regions.len())
                .saturating_add(self.owners.len())
                .saturating_add(self.vacant.len())
                .saturating_add(self.role_sources.len())
                .saturating_add(self.value_role_sources.len())
                .saturating_add(self.reborrows.len())
                .saturating_add(self.reservations.len())
                .saturating_add(self.value_roots.len())
                .saturating_add(self.local_roots.len())
                .saturating_add(1),
        )?;
        for (_, path) in &self.vacant {
            world.pay(path.len())?;
        }
        let mut locals = Vec::with_capacity(self.locals.len());
        for value in &self.locals {
            locals.push(value.clone());
        }
        let values = self.values.clone();
        let mut declared = BTreeMap::new();
        for (local, value) in &self.declared {
            declared.insert(*local, value.clone());
        }
        for value in self.implicit.values() {
            crate::dyn_metadata::semantic_operand_snapshot(value, world.limits, world.left, 1)?;
        }
        let mut role_sources = BTreeMap::new();
        for (local, source) in &self.role_sources {
            role_sources.insert(*local, roles::snapshot_incoming(source, world.limits, world.left)?);
        }
        let mut value_role_sources = BTreeMap::new();
        for (value, source) in &self.value_role_sources {
            value_role_sources.insert(*value, roles::snapshot_incoming(source, world.limits, world.left)?);
        }
        let mut reborrows = BTreeMap::new();
        for (value, edge) in &self.reborrows {
            reborrows.insert(
                *value,
                arguments::Edge {
                    parent: edge.parent,
                    source: roles::copy_source(world, &edge.source)?,
                },
            );
        }
        let mut reservations = BTreeMap::new();
        for (id, reservation) in &self.reservations {
            reservations.insert(*id, super::reservations::snapshot(world, reservation)?);
        }
        let mut value_roots = BTreeMap::new();
        for (value, roots) in &self.value_roots {
            value_roots.insert(*value, arguments::copy_roots(world, roots)?);
        }
        let mut local_roots = BTreeMap::new();
        for (local, roots) in &self.local_roots {
            local_roots.insert(*local, arguments::copy_roots(world, roots)?);
        }
        let state = Self {
            schema: self.schema,
            instance: self.instance,
            locals,
            values,
            declared,
            implicit: self.implicit.clone(),
            vacant_captures: self.vacant_captures.clone(),
            stale_values: self.stale_values.clone(),
            stale_locals: self.stale_locals.clone(),
            scopes: self.scopes.clone(),
            unsafe_regions: self.unsafe_regions.clone(),
            owners: self.owners.clone(),
            vacant: self.vacant.clone(),
            role_sources,
            value_role_sources,
            reborrows,
            reservations,
            value_roots,
            local_roots,
        };
        world.snapshots = world.snapshots.saturating_add(1);
        world.snapshot_work = world
            .snapshot_work
            .saturating_add(before.saturating_sub(*world.left));
        Ok(state)
    }
    fn matches_entry(&self, other: &Self, world: &mut World<'_, '_>) -> R<bool> {
        let before = *world.left;
        let result = self.same(other, world);
        world.comparisons = world.comparisons.saturating_add(1);
        world.compare_work = world.compare_work.saturating_add(before.saturating_sub(*world.left));
        result
    }
    fn same(&self, other: &Self, world: &mut World<'_, '_>) -> R<bool> {
        world.pay(2)?;
        if self.schema != other.schema || self.instance != other.instance {
            return Ok(false);
        }
        // Reject different live worlds before visiting their immutable value
        // cells. Every element actually compared still pays its complete cost;
        // a different epoch/root/role never needs an unrelated SSA tree walk.
        if !same_sequence(
            world,
            self.scopes.iter(),
            other.scopes.iter(),
            |_, _| Ok(()),
        )? || !same_sequence(
            world,
            self.unsafe_regions.iter(),
            other.unsafe_regions.iter(),
            |_, _| Ok(()),
        )? || !same_map(world, &self.owners, &other.owners, |_, _| Ok(()))?
            || !same_sequence(
                world,
                self.vacant_captures.iter(),
                other.vacant_captures.iter(),
                |_, _| Ok(()),
            )?
            || !same_sequence(
                world,
                self.stale_values.iter(),
                other.stale_values.iter(),
                |_, _| Ok(()),
            )?
            || !same_sequence(
                world,
                self.stale_locals.iter(),
                other.stale_locals.iter(),
                |_, _| Ok(()),
            )?
            || !same_sequence(
                world,
                self.vacant.iter(),
                other.vacant.iter(),
                |world, (_, path)| world.pay(path.len()),
            )?
            || !same_map(world, &self.implicit, &other.implicit, |world, image| {
                crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)
            })?
            || !same_map(
                world,
                &self.role_sources,
                &other.role_sources,
                charge_incoming,
            )?
            || !same_map(
                world,
                &self.value_role_sources,
                &other.value_role_sources,
                charge_incoming,
            )?
            || !same_map(world, &self.reborrows, &other.reborrows, |world, edge| {
                world.pay(1)?;
                charge_source(world, &edge.source)
            })?
            || !same_map(
                world,
                &self.reservations,
                &other.reservations,
                reservations::charge,
            )?
            || !same_map(world, &self.value_roots, &other.value_roots, charge_roots)?
            || !same_map(world, &self.local_roots, &other.local_roots, charge_roots)?
        {
            return Ok(false);
        }
        world.pay(3)?;
        if self.locals.len() != other.locals.len()
            || self.values.len() != other.values.len()
            || self.declared.len() != other.declared.len()
        {
            return Ok(false);
        }
        for (left, right) in self
            .locals
            .iter()
            .zip(&other.locals)
        {
            world.pay(1)?;
            match (left, right) {
                (None, None) => {}
                (Some(left), Some(right)) => {
                    if !matches!(left.as_ref(), SemanticTypeValue::Resolved { .. })
                        || !matches!(right.as_ref(), SemanticTypeValue::Resolved { .. })
                    {
                        return Err(bad("class-flow comparison has an unresolved whole type"));
                    }
                    if !Rc::ptr_eq(left, right) {
                        charge_value_domain(world, left)?;
                        charge_value_domain(world, right)?;
                        if !same_value_domain(left, right)? {
                            return Ok(false);
                        }
                    }
                }
                _ => return Ok(false),
            }
        }
        for (left, right) in self.values.pages.iter().zip(&other.values.pages) {
            world.pay(1)?;
            match (left, right) {
                (None, None) => {}
                (Some(left), Some(right)) if Rc::ptr_eq(left, right) => {}
                (Some(left), Some(right)) if left.present == right.present => {
                    for ((_, left), (_, right)) in left.entries().zip(right.entries()) {
                        world.pay(1)?;
                        if !Rc::ptr_eq(left, right) {
                            charge_value_domain(world, left)?;
                            charge_value_domain(world, right)?;
                            if !same_value_domain(left, right)? {
                                return Ok(false);
                            }
                        }
                    }
                }
                _ => return Ok(false),
            }
        }
        for ((left_key, left), (right_key, right)) in self.declared.iter().zip(&other.declared) {
            world.pay(2)?;
            if left_key != right_key {
                return Ok(false);
            }
            if !matches!(left.as_ref(), SemanticTypeValue::Resolved { .. })
                || !matches!(right.as_ref(), SemanticTypeValue::Resolved { .. })
            {
                return Err(bad("class-flow comparison has an unresolved whole type"));
            }
            if !Rc::ptr_eq(left, right) {
                charge_value_domain(world, left)?;
                charge_value_domain(world, right)?;
                if !same_value_domain(left, right)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
// Local equality operations keep their own paid traversal. The caller may
// short-circuit only after a real unequal component has been established.
fn same_sequence<'a, T: Eq + 'a>(
    world: &mut World<'_, '_>,
    left: impl ExactSizeIterator<Item = &'a T>,
    right: impl ExactSizeIterator<Item = &'a T>,
    mut charge: impl FnMut(&mut World<'_, '_>, &T) -> R<()>,
) -> R<bool> {
    world.pay(1)?;
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.zip(right) {
        world.pay(2)?;
        charge(world, left)?;
        charge(world, right)?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}
fn same_map<K: Ord, V: Eq>(
    world: &mut World<'_, '_>,
    left: &BTreeMap<K, V>,
    right: &BTreeMap<K, V>,
    mut charge: impl FnMut(&mut World<'_, '_>, &V) -> R<()>,
) -> R<bool> {
    world.pay(1)?;
    if left.len() != right.len() {
        return Ok(false);
    }
    for ((left_key, left), (right_key, right)) in left.iter().zip(right) {
        world.pay(4)?;
        if left_key != right_key {
            return Ok(false);
        }
        charge(world, left)?;
        charge(world, right)?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}
fn charge_incoming(world: &mut World<'_, '_>, source: &roles::Incoming) -> R<()> {
    world.pay(source.origin.as_ref().map_or(0, |origin| origin.anchors.len()).saturating_add(1))?;
    charge_source(world, &source.source)
}

fn charge_source(world: &mut World<'_, '_>, source: &roles::Source) -> R<()> {
    world.pay(1)?;
    if let roles::Source::Class(image) = source {
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    }
    Ok(())
}
fn charge_roots(world: &mut World<'_, '_>, roots: &BTreeSet<arguments::Root>) -> R<()> {
    world.pay(roots.len())?;
    for root in roots {
        world.pay(root.path.len())?;
    }
    Ok(())
}
// The directory is immutable, but a reaching world must not remember a dead
// branch's correspondence as live state. Keep every metadata edge reachable
// from a real occupant/class, including ancestors of a surviving reborrow.
#[derive(Default)]
struct LiveMetadata {
    values: LiveValues,
    implicit: BTreeSet<u32>,
    pending: VecDeque<LiveNode>,
    track_values: bool,
}
enum LiveNode {
    Value(ValueId),
    Implicit(u32),
}
// Real SSA ordinals have a dense immutable directory. Out-of-directory
// historical/foreign heads remain represented exactly in a sparse tail.
#[derive(Default)]
struct LiveValues {
    limit: usize,
    words: Vec<u64>,
    overflow: BTreeSet<ValueId>,
    count: usize,
}
impl LiveValues {
    fn reaching(world: &mut World<'_, '_>, values: &ssa::Values) -> R<Self> {
        world.pay(values.pages.len().saturating_mul(2))?;
        let mut count = 0usize;
        let words = values.pages.iter().map(|page| {
            let present = page.as_ref().map_or(0, |page| page.present);
            count = count.saturating_add(present.count_ones() as usize);
            present
        }).collect();
        Ok(Self { limit: values.len(), words, count, ..Self::default() })
    }
    fn len(&self) -> usize {
        self.count
    }
    fn lookup_work(&self, value: &ValueId) -> usize {
        if (value.0 as usize) < self.limit {
            1
        } else {
            index::search(self.overflow.len())
        }
    }
    fn contains(&self, value: &ValueId) -> bool {
        if (value.0 as usize) < self.limit {
            self.words[value.0 as usize / 64] & (1u64 << (value.0 % 64)) != 0
        } else {
            self.overflow.contains(value)
        }
    }
    fn insert(&mut self, world: &mut World<'_, '_>, value: ValueId) -> R<bool> {
        world.pay(self.lookup_work(&value))?;
        let new = if (value.0 as usize) < self.limit {
            let word = &mut self.words[value.0 as usize / 64];
            let mask = 1u64 << (value.0 % 64);
            let new = *word & mask == 0;
            *word |= mask;
            new
        } else {
            self.overflow.insert(value)
        };
        if new {
            self.count += 1;
        }
        Ok(new)
    }
}
fn image_value(image: &SemanticClassExpr) -> Option<ValueId> {
    match image {
        SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(value),
            ..
        }
        | SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Value(value),
            ..
        } => Some(*value),
        _ => None,
    }
}
impl LiveMetadata {
    fn value_id(&mut self, world: &mut World<'_, '_>, value: ValueId) -> R<()> {
        if self.values.insert(world, value)? && self.track_values {
            world.pay(1)?;
            self.pending.push_back(LiveNode::Value(value));
        }
        self.bound(world)
    }
    fn implicit_id(&mut self, world: &mut World<'_, '_>, ordinal: u32) -> R<()> {
        world.pay(index::search(self.implicit.len()))?;
        if self.implicit.insert(ordinal) {
            world.pay(1)?;
            self.pending.push_back(LiveNode::Implicit(ordinal));
        }
        self.bound(world)
    }
    fn bound(&self, world: &World<'_, '_>) -> R<()> {
        if self.values.len().saturating_add(self.implicit.len()) > world.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "live CFG metadata closure exceeds node limit",
                None,
            ));
        }
        Ok(())
    }
    fn image(&mut self, world: &mut World<'_, '_>, image: &SemanticClassExpr) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
        if let Some(value) = image_value(image) {
            self.value_id(world, value)?;
        }
        if let SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = image
        {
            self.implicit_id(world, *ordinal)?;
        }
        Ok(())
    }
    fn value(&mut self, world: &mut World<'_, '_>, value: &SemanticTypeValue) -> R<()> {
        let SemanticTypeValue::Resolved { classes, .. } = value else {
            return Err(bad("live CFG metadata has an unresolved whole type"));
        };
        world.pay(classes.len().saturating_add(1))?;
        for image in classes {
            self.image(world, image)?;
        }
        Ok(())
    }
    fn source(&mut self, world: &mut World<'_, '_>, source: &roles::Source) -> R<()> {
        world.pay(1)?;
        if let roles::Source::Class(image) = source {
            self.image(world, image)?;
        }
        Ok(())
    }
}
impl State {
    fn canonicalize(&mut self, world: &mut World<'_, '_>) -> R<()> {
        // Only these three directories are retired by this pass. Scalar-only
        // worlds need no live metadata graph.
        world.pay(1)?;
        if self.implicit.is_empty() && self.reborrows.is_empty() && self.value_roots.is_empty() {
            return Ok(());
        }
        let mut live = LiveMetadata {
            values: LiveValues::reaching(world, &self.values)?,
            ..LiveMetadata::default()
        };
        live.bound(world)?;
        world.pay(
            self.values.pages
                .len()
                .saturating_add(self.locals.len())
                .saturating_add(self.declared.len()),
        )?;
        // The immutable pages seed every reaching SSA spelling, including
        // scalars whose own ID can be a metadata head. Only class-bearing
        // occupants contribute additional images to the closure.
        for value in self.values.classed() {
            world.pay(1)?;
            live.value(world, value)?;
        }
        for value in self.locals.iter().flatten().chain(self.declared.values()) {
            live.value(world, value)?;
        }
        for source in self
            .role_sources
            .values()
            .chain(self.value_role_sources.values())
        {
            live.source(world, &source.source)?;
        }
        world.pay(self.reservations.len())?;
        for reservation in self.reservations.values() {
            if let reservations::Target::Address(address) = reservation.target {
                live.value_id(world, address)?;
            }
            live.value(world, &reservation.parent)?;
            live.value(world, &reservation.expected)?;
        }
        // Form the exact sparse dependency graph once. Non-SSA images remain
        // conservative roots. An SSA head activates all correspondences that
        // name it; an active reborrow activates its real parent/source.
        let mut heads: BTreeMap<ValueId, Vec<u32>> = BTreeMap::new();
        world.pay(self.implicit.len().saturating_add(self.reborrows.len()))?;
        for (ordinal, image) in &self.implicit {
            if let Some(head) = image_value(image) {
                world.pay(index::search(heads.len()).saturating_add(1))?;
                heads.entry(head).or_default().push(*ordinal);
            } else {
                live.implicit_id(world, *ordinal)?;
            }
        }
        for value in self.reborrows.keys() {
            world.pay(index::search(heads.len()))?;
            heads.entry(*value).or_default();
        }
        // Initial scalar occupants with no outgoing metadata edges need no
        // queue entry. Newly reached values are queued exactly once.
        live.track_values = true;
        world.pay(heads.len())?;
        for head in heads.keys() {
            world.pay(live.values.lookup_work(head))?;
            if live.values.contains(head) {
                world.pay(1)?;
                live.pending.push_back(LiveNode::Value(*head));
            }
        }
        while let Some(node) = live.pending.pop_front() {
            world.pay(1)?;
            match node {
                LiveNode::Value(value) => {
                    if !heads.is_empty() {
                        world.pay(index::search(heads.len()))?;
                        if let Some(ordinals) = heads.get(&value) {
                            world.pay(ordinals.len())?;
                            for ordinal in ordinals {
                                live.implicit_id(world, *ordinal)?;
                            }
                        }
                    }
                    if !self.reborrows.is_empty() {
                        world.pay(index::search(self.reborrows.len()))?;
                        if let Some(edge) = self.reborrows.get(&value) {
                            live.value_id(world, edge.parent)?;
                            live.source(world, &edge.source)?;
                        }
                    }
                }
                LiveNode::Implicit(ordinal) => {
                    world.pay(index::search(self.implicit.len()))?;
                    if let Some(image) = self.implicit.get(&ordinal) {
                        live.image(world, image)?;
                    }
                }
            }
        }
        world.pay(
            self.implicit
                .len()
                .saturating_mul(index::search(live.implicit.len()))
                .saturating_add(
                    self.reborrows
                        .keys()
                        .chain(self.value_roots.keys())
                        .fold(0usize, |work, value| {
                            work.saturating_add(live.values.lookup_work(value))
                        }),
                ),
        )?;
        let before = self
            .implicit
            .len()
            .saturating_add(self.reborrows.len())
            .saturating_add(self.value_roots.len());
        self.implicit
            .retain(|ordinal, _| live.implicit.contains(ordinal));
        self.reborrows
            .retain(|value, _| live.values.contains(value));
        self.value_roots
            .retain(|value, _| live.values.contains(value));
        let after = self
            .implicit
            .len()
            .saturating_add(self.reborrows.len())
            .saturating_add(self.value_roots.len());
        world.retired_metadata = world
            .retired_metadata
            .saturating_add(before.saturating_sub(after));
        Ok(())
    }
    fn measured_key(&self, world: &mut World<'_, '_>, domains: &mut DomainDigests) -> R<zil_base::Fingerprint> {
        let before = *world.left;
        let result = self.key(world, domains);
        world.key_work = world.key_work.saturating_add(before.saturating_sub(*world.left));
        result
    }
    fn key(
        &self,
        world: &mut World<'_, '_>,
        domains: &mut DomainDigests,
    ) -> R<zil_base::Fingerprint> {
        let mut digest = zil_base::FingerprintBuilder::new();
        hash_binary(
            &mut digest,
            world.left,
            &(
                self.schema,
                self.instance,
                &self.vacant_captures,
                &self.stale_values,
                &self.stale_locals,
                &self.scopes,
                &self.unsafe_regions,
                &self.owners,
                &self.vacant,
            ),
        )?;
        hash_binary(&mut digest, world.left, &self.implicit.len())?;
        for (ordinal, image) in &self.implicit {
            hash_binary(&mut digest, world.left, ordinal)?;
            hash_image(&mut digest, world.left, image)?;
        }
        hash_binary(&mut digest, world.left, &self.role_sources.len())?;
        for (local, source) in &self.role_sources {
            hash_binary(&mut digest, world.left, local)?;
            hash_source(&mut digest, world.left, &source.source)?;
            hash_binary(&mut digest, world.left, &source.origin)?;
        }
        hash_binary(&mut digest, world.left, &self.value_role_sources.len())?;
        for (value, source) in &self.value_role_sources {
            hash_binary(&mut digest, world.left, value)?;
            hash_source(&mut digest, world.left, &source.source)?;
            hash_binary(&mut digest, world.left, &source.origin)?;
        }
        hash_binary(&mut digest, world.left, &self.reborrows.len())?;
        for (value, edge) in &self.reborrows {
            hash_binary(&mut digest, world.left, &(value, edge.parent))?;
            hash_source(&mut digest, world.left, &edge.source)?;
        }
        hash_binary(&mut digest, world.left, &self.value_roots.len())?;
        for (value, roots) in &self.value_roots {
            hash_binary(&mut digest, world.left, value)?;
            hash_roots(&mut digest, world.left, roots)?;
        }
        hash_binary(&mut digest, world.left, &self.local_roots.len())?;
        for (local, roots) in &self.local_roots {
            hash_binary(&mut digest, world.left, local)?;
            hash_roots(&mut digest, world.left, roots)?;
        }
        hash_binary(&mut digest, world.left, &self.reservations.len())?;
        for (id, reservation) in &self.reservations {
            hash_binary(&mut digest, world.left, id)?;
            hash_binary(
                &mut digest,
                world.left,
                &std::mem::discriminant(&reservation.target),
            )?;
            match reservation.target {
                reservations::Target::Local(local) => hash_binary(&mut digest, world.left, &local)?,
                reservations::Target::Address(value) => {
                    hash_binary(&mut digest, world.left, &value)?
                }
            }
            hash_binary(&mut digest, world.left, &reservation.path)?;
            hash_roots(&mut digest, world.left, &reservation.roots)?;
            hash_domain(&mut digest, world, domains, &reservation.parent)?;
            hash_domain(&mut digest, world, domains, &reservation.expected)?;
        }
        world.pay(self.locals.len())?;
        hash_binary(&mut digest, world.left, &self.locals.len())?;
        for (ordinal, value) in self.locals.iter().enumerate() {
            if let Some(value) = value {
                hash_binary(&mut digest, world.left, &ordinal)?;
                hash_domain(&mut digest, world, domains, value)?;
            }
        }
        world.pay(self.values.pages.len())?;
        hash_binary(&mut digest, world.left, &self.values.len())?;
        for (ordinal, value) in self.values.iter() {
            world.pay(1)?;
            hash_binary(&mut digest, world.left, &ordinal)?;
            hash_domain(&mut digest, world, domains, value)?;
        }
        world.pay(self.declared.len())?;
        hash_binary(&mut digest, world.left, &self.declared.len())?;
        for (local, value) in &self.declared {
            hash_binary(&mut digest, world.left, local)?;
            hash_domain(&mut digest, world, domains, value)?;
        }
        Ok(digest.digest())
    }
}
// Shared private query encoding retains every complete field, collection
// boundary and scalar value, charging actual bytes without Debug text.
fn hash_binary(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    value: &impl std::hash::Hash,
) -> R<()> {
    super::super::original::protocol::hash_into(digest, left, value)
}
fn hash_operand(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    operand: &SemanticOperand,
) -> R<()> {
    hash_binary(digest, left, &std::mem::discriminant(operand))?;
    match operand {
        SemanticOperand::Parameter(n)
        | SemanticOperand::Capture(n)
        | SemanticOperand::Environment(n) => hash_binary(digest, left, n),
        SemanticOperand::Receiver => Ok(()),
        SemanticOperand::Value(value) => hash_binary(digest, left, value),
        SemanticOperand::Local(local) => hash_binary(digest, left, local),
        SemanticOperand::TypeArgument {
            scope: SemanticScopeRef::Ancestor(depth),
            ordinal,
        } => hash_binary(digest, left, &(depth, ordinal)),
        SemanticOperand::CoreExpression {
            expression,
            occurrence,
        } => hash_binary(digest, left, &(expression, occurrence)),
    }
}
fn hash_image(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    image: &SemanticClassExpr,
) -> R<()> {
    hash_binary(digest, left, &std::mem::discriminant(image))?;
    match image {
        SemanticClassExpr::Formal {
            scope: SemanticScopeRef::Ancestor(depth),
            ordinal,
        }
        | SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(depth),
            ordinal,
        }
        | SemanticClassExpr::Environment {
            scope: SemanticScopeRef::Ancestor(depth),
            ordinal,
        } => hash_binary(digest, left, &(depth, ordinal)),
        SemanticClassExpr::Bound { depth, index } => hash_binary(digest, left, &(depth, index)),
        SemanticClassExpr::OuterOccurrence {
            operand,
            path,
            mode,
        } => {
            hash_operand(digest, left, operand)?;
            hash_binary(digest, left, &(path, mode))
        }
        SemanticClassExpr::IntrinsicClass {
            operand,
            path,
            class,
        } => {
            hash_operand(digest, left, operand)?;
            hash_binary(digest, left, &(path, class))
        }
    }
}
fn hash_source(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    source: &roles::Source,
) -> R<()> {
    hash_binary(digest, left, &std::mem::discriminant(source))?;
    match source {
        roles::Source::Static => Ok(()),
        roles::Source::Class(image) => hash_image(digest, left, image),
    }
}
fn hash_roots(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    roots: &BTreeSet<arguments::Root>,
) -> R<()> {
    hash_binary(digest, left, &roots.len())?;
    for root in roots {
        hash_binary(digest, left, &(root.local, &root.path))?;
    }
    Ok(())
}
// Retained allocations cannot change or reuse their address. An ordinal only
// abbreviates a complete, exactly equal life/classes pair in this CFG's key;
// it creates no lifetime equality and contains no reaching-state conclusion.
#[derive(Default)]
struct DomainDigests {
    allocations: BTreeMap<usize, (SharedType, u32)>,
    buckets: BTreeMap<zil_base::Fingerprint, Vec<(SharedType, u32)>>,
    entries: usize,
}
fn hash_domain(
    digest: &mut zil_base::FingerprintBuilder,
    world: &mut World<'_, '_>,
    domains: &mut DomainDigests,
    value: &SharedType,
) -> R<()> {
    let SemanticTypeValue::Resolved { life, classes, .. } = value.as_ref() else {
        return Err(bad("canonical CFG key has an unresolved whole type"));
    };
    world.pay(1)?;
    if life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
        && classes.is_empty()
    {
        digest.update(&[0]);
    } else {
        digest.update(&[1]);
        let key = Rc::as_ptr(value) as usize;
        world.pay(index::search(domains.allocations.len()))?;
        let ordinal = if let Some((retained, ordinal)) = domains.allocations.get(&key) {
            world.pay(1)?;
            if !Rc::ptr_eq(retained, value) {
                return Err(bad(
                    "immutable CFG domain cache changes its retained allocation",
                ));
            }
            *ordinal
        } else {
            if domains.allocations.len() >= world.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "immutable CFG domain cache exceeds node limit",
                    None,
                ));
            }
            charge_value_domain(world, value)?;
            let mut encoded = zil_base::FingerprintBuilder::new();
            hash_binary(&mut encoded, world.left, life)?;
            hash_binary(&mut encoded, world.left, &classes.len())?;
            for image in classes {
                hash_image(&mut encoded, world.left, image)?;
            }
            let fingerprint = encoded.digest();
            world.pay(index::search(domains.buckets.len()))?;
            let mut found = None;
            if let Some(bucket) = domains.buckets.get(&fingerprint) {
                world.pay(bucket.len())?;
                for (candidate, ordinal) in bucket {
                    charge_value_domain(world, candidate)?;
                    charge_value_domain(world, value)?;
                    if same_value_domain(candidate, value)? {
                        found = Some(*ordinal);
                        break;
                    }
                }
            }
            let ordinal = if let Some(ordinal) = found {
                ordinal
            } else {
                if domains.entries >= world.limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "canonical CFG domain entry limit exceeded",
                        None,
                    ));
                }
                let ordinal = u32::try_from(domains.entries).map_err(|_| {
                    Diagnostic::limit(Phase::Types, "canonical CFG domain ordinal exceeded", None)
                })?;
                world.pay(
                    index::search(domains.buckets.len())
                        .saturating_add(
                            std::mem::size_of::<(zil_base::Fingerprint, SharedType, u32)>(),
                        )
                        .saturating_add(1),
                )?;
                domains
                    .buckets
                    .entry(fingerprint)
                    .or_default()
                    .push((value.clone(), ordinal));
                domains.entries += 1;
                ordinal
            };
            world.pay(
                index::search(domains.allocations.len())
                    .saturating_add(std::mem::size_of::<(usize, SharedType, u32)>())
                    .saturating_add(1),
            )?;
            domains.allocations.insert(key, (value.clone(), ordinal));
            ordinal
        };
        hash_binary(digest, world.left, &ordinal)?;
    }
    Ok(())
}
fn charge_value_domain(world: &mut World<'_, '_>, value: &SemanticTypeValue) -> R<()> {
    let SemanticTypeValue::Resolved { life, classes, .. } = value else {
        return Err(bad("class-flow state retains an unresolved whole type"));
    };
    world.pay(
        life.slots
            .len()
            .saturating_add(life.retained.len())
            .saturating_add(life.nominal_arguments.len())
            .saturating_add(classes.len())
            .saturating_add(1),
    )?;
    for path in life
        .slots
        .iter()
        .map(|row| row.path.len())
        .chain(life.retained.iter().map(|row| row.path.len()))
        .chain(life.nominal_arguments.iter().map(|row| row.path.len()))
    {
        if path > world.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "class-flow value domain depth exceeded",
                None,
            ));
        }
        world.pay(path)?;
    }
    for image in classes {
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?
    }
    Ok(())
}
fn same_value_domain(left: &SemanticTypeValue, right: &SemanticTypeValue) -> R<bool> {
    let (
        SemanticTypeValue::Resolved {
            life: left_life,
            classes: left_classes,
            ..
        },
        SemanticTypeValue::Resolved {
            life: right_life,
            classes: right_classes,
            ..
        },
    ) = (left, right)
    else {
        return Err(bad("class-flow comparison has an unresolved whole type"));
    };
    Ok(left_life == right_life && left_classes == right_classes)
}
// Block ordinals index only this immutable function. Dense rows make both
// dominance intersections and reachability lookups independent of block IDs.
#[derive(Clone, PartialEq, Eq)]
struct BlockSet(Vec<u64>);

impl BlockSet {
    fn empty(count: usize) -> Self {
        Self(vec![0; count.div_ceil(64)])
    }

    fn insert(&mut self, ordinal: usize) -> bool {
        let mask = 1u64 << (ordinal % 64);
        let word = &mut self.0[ordinal / 64];
        let fresh = *word & mask == 0;
        *word |= mask;
        fresh
    }

    fn contains(&self, ordinal: usize) -> bool {
        self.0[ordinal / 64] & (1u64 << (ordinal % 64)) != 0
    }
}

// A loop epoch depends only on mutual reachability. Record one exact component
// ordinal per block and the reachable set of the real entry; never build the
// unused transitive closure of all block pairs.
fn components(
    left: &mut usize,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    entry: usize,
) -> R<(Vec<usize>, BlockSet)> {
    let count = successors.len();
    index::pay(left, count.div_ceil(64))?;
    let mut visited = BlockSet::empty(count);
    let mut reachable = None;
    let mut finished = Vec::new();
    let mut pending = Vec::new();
    for start in std::iter::once(entry).chain(0..count) {
        index::pay(left, 1)?;
        if !visited.insert(start) {
            continue;
        }
        index::pay(left, 1)?;
        pending.push((start, 0usize));
        while let Some((block, offset)) = pending.last_mut() {
            index::pay(left, 1)?;
            if let Some(&next) = successors[*block].get(*offset) {
                *offset += 1;
                index::pay(left, 1)?;
                if visited.insert(next) {
                    index::pay(left, 1)?;
                    pending.push((next, 0));
                }
            } else {
                index::pay(left, 1)?;
                finished.push(*block);
                pending.pop();
            }
        }
        if start == entry {
            index::pay(left, count.div_ceil(64))?;
            reachable = Some(visited.clone());
        }
    }

    index::pay(left, count)?;
    let mut labels = vec![usize::MAX; count];
    let mut pending = Vec::new();
    for start in finished.into_iter().rev() {
        index::pay(left, 1)?;
        if labels[start] != usize::MAX {
            continue;
        }
        index::pay(left, 2)?;
        labels[start] = start;
        pending.push(start);
        while let Some(block) = pending.pop() {
            index::pay(left, predecessors[block].len().saturating_add(1))?;
            for &previous in &predecessors[block] {
                if labels[previous] == usize::MAX {
                    index::pay(left, 2)?;
                    labels[previous] = start;
                    pending.push(previous);
                }
            }
        }
    }
    Ok((labels, reachable.ok_or_else(|| bad("class CFG has no reachable entry"))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_loop_components_preserve_forward_and_reverse_reachability() {
        const COUNT: usize = 3;
        for bitmap in 0..(1usize << (COUNT * COUNT)) {
            let mut successors = vec![Vec::new(); COUNT];
            let mut predecessors = vec![Vec::new(); COUNT];
            for (from, next) in successors.iter_mut().enumerate() {
                for (to, previous) in predecessors.iter_mut().enumerate() {
                    if bitmap & (1usize << (from * COUNT + to)) != 0 {
                        next.push(to);
                        previous.push(from);
                    }
                }
            }
            let reach: Vec<_> = (0..COUNT).map(|start| {
                let mut found = BTreeSet::new();
                let mut pending = vec![start];
                while let Some(node) = pending.pop() {
                    if found.insert(node) {
                        pending.extend(&successors[node]);
                    }
                }
                found
            }).collect();
            for entry in 0..COUNT {
                let (actual, from_entry) = components(&mut 10_000, &successors, &predecessors, entry).unwrap();
                for left in 0..COUNT {
                    assert_eq!(from_entry.contains(left), reach[entry].contains(&left));
                    for right in 0..COUNT {
                        assert_eq!(actual[left] == actual[right], reach[left].contains(&right) && reach[right].contains(&left));
                    }
                }
            }
        }
    }
}

pub(super) struct Shape {
    source: usize,
    schema: FunctionId,
    blocks: BTreeMap<BlockId, usize>,
    definitions: Vec<(usize, usize)>,
    dominators: Vec<BlockSet>,
    components: Vec<usize>,
    predecessors: Vec<Vec<usize>>,
    pub(super) protocol_relevance: std::cell::Cell<Option<bool>>,
}
impl Shape {
    pub(super) fn producer<'a>(
        &self,
        world: &mut World<'_, '_>,
        function: &'a TypedFunction,
        value: ValueId,
    ) -> R<(BlockId, usize, &'a Instruction)> {
        world.pay(1)?;
        if self.schema != function.id || self.source != function as *const TypedFunction as usize {
            return Err(bad("SSA producer query changes its exact actual function"));
        }
        let &(block, ordinal) = self.definitions.get(value.0 as usize)
            .ok_or_else(|| bad("SSA producer query selects an absent value"))?;
        let block = &function.blocks[block];
        Ok((block.id, ordinal, &block.instructions[ordinal]))
    }

    /// Look up a real program point and its reachable incoming edges in the
    /// same immutable body used to build the SSA directory.
    pub(super) fn block_and_predecessors<'f>(
        &self, world: &mut World<'_, '_>, function: &'f TypedFunction, block: BlockId,
    ) -> R<(&'f BasicBlock, &[usize])> {
        world.pay(index::search(self.blocks.len()).saturating_add(1))?;
        if self.source != function as *const TypedFunction as usize || self.schema != function.id {
            return Err(bad("capture source query changes its complete immutable CFG"));
        }
        let index = *self.blocks.get(&block)
            .ok_or_else(|| bad("capture read reaches an absent actual block"))?;
        Ok((&function.blocks[index], &self.predecessors[index]))
    }

    pub(super) fn for_function(world: &mut World<'_, '_>, function: &TypedFunction) -> R<Rc<Self>> {
        world.pay(index::search(world.cfg_shapes.len()).saturating_add(1))?;
        let cached = world.cfg_shapes.get(&function.id).cloned();
        let shape = if let Some(shape) = cached {
            if shape.source != function as *const TypedFunction as usize {
                return Err(bad(
                    "immutable CFG structure changed its actual function owner",
                ));
            }
            shape
        } else {
            if world.cfg_shapes.len() >= world.limits.max_functions {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "immutable CFG structure directory exceeded",
                    None,
                ));
            }
            let shape = Rc::new(Cfg::build(world, function)?);
            world.pay(
                index::search(world.cfg_shapes.len())
                    .saturating_add(std::mem::size_of::<(FunctionId, Rc<Shape>)>())
                    .saturating_add(1),
            )?;
            world.cfg_shapes.insert(function.id, shape.clone());
            shape
        };
        Ok(shape)
    }
}
pub(super) struct Cfg {
    shape: Rc<Shape>,
    instance: SemanticInstanceId,
    seen: Vec<BTreeMap<zil_base::Fingerprint, Vec<usize>>>,
    domains: DomainDigests,
}
impl std::ops::Deref for Cfg {
    type Target = Shape;
    fn deref(&self) -> &Shape {
        &self.shape
    }
}
impl Cfg {
    pub fn new(
        world: &mut World<'_, '_>,
        instance: &SemanticInstance,
        function: &TypedFunction,
    ) -> R<Self> {
        world.pay(1)?;
        if !matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function.id)
        {
            return Err(bad("class CFG changes its exact original instance target"));
        }
        let shape = Shape::for_function(world, function)?;
        // These are fresh for each actual instance: no seen state, live root,
        // ticket, domain equation or protocol outcome crosses this boundary.
        world.pay(function.blocks.len().saturating_add(2))?;
        Ok(Self {
            shape,
            instance: instance.id,
            seen: (0..function.blocks.len())
                .map(|_| BTreeMap::new())
                .collect(),
            domains: DomainDigests::default(),
        })
    }
    fn build(world: &mut World<'_, '_>, function: &TypedFunction) -> R<Shape> {
        let schema = function.id;
        let count = function.blocks.len();
        let words = count.div_ceil(64);
        world.pay(count.saturating_mul(5).saturating_add(function.values.len()))?;
        let mut blocks = BTreeMap::new();
        let missing = (usize::MAX, usize::MAX);
        let mut definitions = vec![missing; function.values.len()];
        for (index, block) in function.blocks.iter().enumerate() {
            if blocks.insert(block.id, index).is_some() {
                return Err(bad("class CFG repeats a block identity"));
            }
            world.pay(block.instructions.len())?;
            for (ordinal, instruction) in block.instructions.iter().enumerate() {
                if let Some(result) = instruction.result {
                    let definition = definitions
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("class CFG defines an absent SSA value"))?;
                    if *definition != missing {
                        return Err(bad("class CFG repeats an SSA definition"));
                    }
                    *definition = (index, ordinal);
                }
            }
        }
        if definitions.contains(&missing) {
            return Err(bad("class CFG has an SSA value without a definition"));
        }
        let entry = *blocks
            .get(&function.entry)
            .ok_or_else(|| bad("class CFG entry is absent"))?;
        let mut predecessors = vec![Vec::new(); count];
        let mut successors = vec![Vec::new(); count];
        for (index, block) in function.blocks.iter().enumerate() {
            let targets = match block.terminator {
                Terminator::Jump(target) => vec![target],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block, else_block],
                _ => Vec::new(),
            };
            for target in targets {
                world.pay(1)?;
                let target = *blocks
                    .get(&target)
                    .ok_or_else(|| bad("class CFG successor is absent"))?;
                predecessors[target].push(index);
                successors[index].push(target);
            }
        }
        let (components, reachable) = components(world.left, &successors, &predecessors, entry)?;
        world.pay(count.saturating_mul(words.saturating_add(1)))?;
        let mut dominators: Vec<_> = (0..count).map(|block| {
            if block != entry && reachable.contains(block) {
                reachable.clone()
            } else {
                BlockSet::empty(count)
            }
        }).collect();
        dominators[entry].insert(entry);
        loop {
            let mut changed = false;
            world.pay(count)?;
            for block in 0..count {
                if block == entry || !reachable.contains(block) {
                    continue;
                }
                world.pay(words.saturating_add(predecessors[block].len()))?;
                let mut common = reachable.clone();
                for &predecessor in &predecessors[block] {
                    if reachable.contains(predecessor) {
                        world.pay(words)?;
                        for (bits, incoming) in common.0.iter_mut().zip(&dominators[predecessor].0) {
                            *bits &= *incoming;
                        }
                    }
                }
                common.insert(block);
                world.pay(words.saturating_add(1))?;
                if common != dominators[block] {
                    dominators[block] = common;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for incoming in &mut predecessors {
            world.pay(incoming.len().saturating_add(1))?;
            incoming.retain(|ordinal| reachable.contains(*ordinal));
        }
        Ok(Shape {
            predecessors,
            protocol_relevance: Default::default(),
            source: function as *const TypedFunction as usize,
            schema,
            blocks,
            definitions,
            dominators,
            components,
        })
    }
    pub fn index(&self, block: BlockId) -> R<usize> {
        self.blocks
            .get(&block)
            .copied()
            .ok_or_else(|| bad("capture CFG block is absent"))
    }
    pub fn retain_dominating<T>(&self, index: usize, values: &mut [Option<T>]) {
        for (value, (definition, _)) in values.iter_mut().zip(&self.definitions) {
            if *definition == index || !self.dominators[index].contains(*definition) {
                *value = None;
            }
        }
    }
    pub fn check_sites(
        &self,
        world: &mut World<'_, '_>,
        function: &TypedFunction,
        sites: &[&SemanticSite],
    ) -> R<()> {
        world.pay(sites.len())?;
        for site in sites {
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge,
            } = site.at
            else {
                return Err(bad("Core site survives the actual class world"));
            };
            let index = *self
                .blocks
                .get(&block)
                .ok_or_else(|| bad("actual site block is absent"))?;
            let block = &function.blocks[index];
            let instruction = instruction as usize;
            if instruction > block.instructions.len() {
                return Err(bad("actual site lies outside its block"));
            }
            if matches!(site.action, SemanticSiteAction::TypeUse { .. }) {
                if edge != SemanticSiteEdge::Before {
                    return Err(bad("type use changes its actual Before boundary"));
                }
                continue;
            }
            if edge != SemanticSiteEdge::After || instruction == block.instructions.len() {
                return Err(bad(
                    "operation site has no actual After instruction boundary",
                ));
            }
            let actual = &block.instructions[instruction];
            let valid = match (&site.action, &actual.kind) {
                (
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(a),
                        ..
                    },
                    InstructionKind::StorageLive(b),
                ) => a == b,
                (
                    SemanticSiteAction::Construct {
                        result: SemanticOperand::Value(id),
                        ..
                    },
                    InstructionKind::MakeAggregate { .. }
                    | InstructionKind::Constant(Constant::Unit)
                    | InstructionKind::MakeOwnedTuple { .. }
                    | InstructionKind::MakeOwnedArray { .. }
                    | InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::MakeManaged { .. }
                    | InstructionKind::MakeVariant { .. },
                ) => actual.result == Some(*id),
                (
                    SemanticSiteAction::Closure {
                        result: SemanticOperand::Value(id),
                        closure: SemanticClosureTarget::Typed(a),
                        ..
                    },
                    InstructionKind::MakeClosure { closure: b, .. },
                ) => a == b && actual.result == Some(*id),
                (
                    SemanticSiteAction::Call {
                        target: SemanticFunctionTarget::Typed(a),
                        ..
                    },
                    InstructionKind::Call { function: b, .. }
                    | InstructionKind::ForeignCall { function: b, .. }
                    | InstructionKind::SuspendCall { function: b, .. },
                ) => a == b,
                (
                    SemanticSiteAction::CallableCall {
                        callee: SemanticOperand::Local(a),
                    },
                    InstructionKind::CallClosureShared { callee: b, .. }
                    | InstructionKind::CallClosureMut { callee: b, .. },
                ) => a == b,
                (
                    SemanticSiteAction::CallableCall {
                        callee: SemanticOperand::Value(a),
                    },
                    InstructionKind::CallClosure { callee: b, .. }
                    | InstructionKind::CallIndirect { callee: b, .. }
                    | InstructionKind::CallClosureOnce { callee: b, .. }
                    | InstructionKind::CallClosureBorrowed { callee: b, .. },
                ) => a == b,
                (
                    SemanticSiteAction::BodyCall { .. },
                    InstructionKind::Call { .. }
                    | InstructionKind::ForeignCall { .. }
                    | InstructionKind::SuspendCall { .. }
                    | InstructionKind::CallIndirect { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. },
                ) => true,
                (SemanticSiteAction::BodyCall { .. }, actual) => {
                    let owner = world.instance(site.instance)?.definition;
                    let ledger = world.ledger;
                    let proof = ledger.original.borrow();
                    if let Some(kind) = proof
                        .as_ref()
                        .and_then(|p| p.intrinsics.get(&(owner, site.recipe)))
                        .map(|c| &c.kind)
                    {
                        super::intrinsic_operation_matches(kind, actual, world.left)?
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !valid {
                return Err(bad(
                    "original site does not match its actual operation and operand",
                ));
            }
        }
        Ok(())
    }
    fn loop_occurrence(
        &self,
        world: &mut World<'_, '_>,
        entry: usize,
        value: Option<&SharedType>,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
        stable_outer: bool,
    ) -> R<bool> {
        let Some(SemanticTypeValue::Resolved { ty, life, classes }) = value.map(Rc::as_ref) else {
            return Ok(false);
        };
        world.pay(classes.len().saturating_add(life.slots.len()))?;
        let outer = if stable_outer && matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            life.slots.iter().find(|row| row.path.is_empty()).map(|row| row.binder as usize)
        } else { None };
        for (class, image) in classes.iter().enumerate() {
            // Only a separately traced outer referent can cross this access
            // mint's epoch. Every intrinsic/pointee class keeps its ordinary
            // loop-epoch obligation; no whole-Type or permission is relaxed.
            if outer == Some(class) { continue; }
            if self.loop_image(world, entry, image, implicit)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn loop_image(
        &self,
        world: &mut World<'_, '_>,
        entry: usize,
        image: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<bool> {
        world.pay(1)?;
        if let SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = image
        {
            if let Some((unknown, sources)) =
                roles::loop_sources(world, self.instance, *ordinal, implicit)?
            {
                if unknown {
                    return Ok(true);
                }
                for source in sources {
                    if self.loop_image(world, entry, &source, implicit)? {
                        return Ok(true);
                    }
                }
            }
        }
        if let SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(id),
            ..
        }
        | SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Value(id),
            ..
        } = image
        {
            let (definition, _) = *self
                .definitions
                .get(id.0 as usize)
                .ok_or_else(|| bad("loop role source selects an absent SSA occurrence"))?;
            return Ok(self.components[entry] == self.components[definition]);
        }
        Ok(false)
    }
    pub fn offer(
        &mut self,
        world: &mut World<'_, '_>,
        source: Option<usize>,
        target: BlockId,
        mut state: State,
        reaching: &mut [Vec<State>],
        work: &mut VecDeque<(usize, usize)>,
    ) -> R<()> {
        if state.schema != self.schema || state.instance != self.instance {
            return Err(bad(
                "class CFG state belongs to another immutable type or instance directory",
            ));
        }
        world.pay(index::search(self.blocks.len()).saturating_add(1))?;
        let index = *self
            .blocks
            .get(&target)
            .ok_or_else(|| bad("class CFG successor is absent"))?;
        // A loop may carry an old physical occurrence while reexecuting its
        // producer. Its public SSA spelling cannot make the two epochs equal.
        // Mark the occupant, not the spelling: replacement clears this mark.
        let closes_epoch = if let Some(source) = source {
            let predecessors = &self.predecessors[index];
            world.pay(predecessors.len().saturating_add(1))?;
            if !predecessors.contains(&source) {
                return Err(bad("class CFG state reaches an edge absent from its immutable function"));
            }
            // The real source->target edge supplies forward reachability.
            self.components[index] == self.components[source]
                && (self.dominators[source].contains(index) || index <= source)
        } else {
            false
        };
        if closes_epoch {
            for (ordinal, value) in state.locals.iter().enumerate() {
                world.pay(index::search(state.role_sources.len()).saturating_add(1))?;
                let stable = if let Some(source) = state.role_sources.get(&LocalId(ordinal as u32)) {
                    roles::stable_origin(world, source, state.instance, &state.vacant_captures)?
                } else { false };
                if self.loop_occurrence(world, index, value.as_ref(), &state.implicit, stable)? {
                    state.stale_locals.insert(LocalId(ordinal as u32));
                }
            }
            world.pay(state.values.pages.len())?;
            for (ordinal, value) in state.values.iter() {
                world.pay(1)?;
                if self.loop_occurrence(world, index, Some(value), &state.implicit, false)? {
                    state.stale_values.insert(ValueId(ordinal as u32));
                }
            }
            for (local, source) in &state.role_sources {
                // A checked directed source trace can transport a pointer to
                // a stable caller/capture referent across a loop. The local's
                // complete declared domain was checked above independently;
                // do not confuse the historic access-mint spelling with that
                // referent. SSA values retain their normal epoch retirement.
                if roles::stable_origin(world, source, state.instance, &state.vacant_captures)?
                { continue; }
                if let roles::Source::Class(image) = &source.source {
                    if self.loop_image(world, index, image, &state.implicit)? {
                        state.stale_locals.insert(*local);
                    }
                }
            }
            for (value, source) in &state.value_role_sources {
                if let roles::Source::Class(image) = &source.source {
                    if self.loop_image(world, index, image, &state.implicit)? {
                        state.stale_values.insert(*value);
                    }
                }
            }
        }
        // Only definitions strictly dominating entry to this block survive.
        // In particular a previous loop iteration cannot initialize this
        // iteration's SSA, and a sibling branch cannot donate a same-numbered value.
        state.values.retain(world, |ordinal| {
            let (definition, _) = self.definitions[ordinal];
            definition != index && self.dominators[index].contains(definition)
        })?;
        world.pay(state.stale_values.len()
            .saturating_add(state.stale_locals.len())
            .saturating_add(state.role_sources.len())
            .saturating_add(state.value_role_sources.len()))?;
        state
            .stale_values
            .retain(|id| state.values.get(id.0 as usize).is_some());
        state
            .stale_locals
            .retain(|id| state.locals[id.0 as usize].is_some());
        state
            .role_sources
            .retain(|id, _| state.locals[id.0 as usize].is_some());
        state
            .value_role_sources
            .retain(|id, _| state.values.get(id.0 as usize).is_some());
        let before = *world.left;
        let canonical = state.canonicalize(world);
        world.canonical_work = world
            .canonical_work
            .saturating_add(before.saturating_sub(*world.left));
        canonical?;
        // Most blocks receive a single world. Keep small frontiers as exact
        // states and build a digest directory only when comparisons fan out.
        // Even a digest hit always compares the complete live state.
        const INLINE_WORLDS: usize = 4;
        let previous = &mut reaching[index];
        let ordinal = previous.len();
        if ordinal < INLINE_WORLDS {
            for other in previous.iter() {
                if state.matches_entry(other, world)? {
                    return Ok(());
                }
            }
        } else {
            let directory = &mut self.seen[index];
            if directory.is_empty() {
                for (offset, other) in previous.iter().enumerate() {
                    let key = other.measured_key(world, &mut self.domains)?;
                    world.pay(index::search(directory.len()).saturating_add(2))?;
                    directory.entry(key).or_default().push(offset);
                }
            }
            let key = state.measured_key(world, &mut self.domains)?;
            world.pay(index::search(directory.len()))?;
            if let Some(candidates) = directory.get(&key) {
                world.pay(candidates.len())?;
                for &offset in candidates {
                    if state.matches_entry(&previous[offset], world)? {
                        return Ok(());
                    }
                }
            }
            world.pay(index::search(directory.len()).saturating_add(2))?;
            directory.entry(key).or_default().push(ordinal);
        }
        world.pay(2)?;
        previous.push(state);
        work.push_back((index, ordinal));
        Ok(())
    }
}
