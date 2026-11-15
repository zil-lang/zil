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
    pub values: Vec<Option<SharedType>>,
    pub declared: BTreeMap<LocalId, SharedType>,
    pub implicit: BTreeMap<u32, SemanticClassExpr>,
    pub vacant_captures: BTreeSet<(SemanticInstanceId, u32)>,
    pub stale_values: BTreeSet<ValueId>,
    pub stale_locals: BTreeSet<LocalId>,
    pub scopes: Vec<ScopeId>,
    pub unsafe_regions: Vec<u32>,
    pub owners: BTreeMap<LocalId, ScopeId>,
    pub vacant: BTreeSet<(LocalId, Vec<PlaceProjection>)>,
    pub role_sources: BTreeMap<LocalId, roles::Source>,
    pub value_role_sources: BTreeMap<ValueId, roles::Source>,
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
                .saturating_add(self.values.len())
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
        let mut values = Vec::with_capacity(self.values.len());
        for value in &self.values {
            values.push(value.clone());
        }
        let mut declared = BTreeMap::new();
        for (local, value) in &self.declared {
            declared.insert(*local, value.clone());
        }
        for value in self.implicit.values() {
            crate::dyn_metadata::semantic_operand_snapshot(value, world.limits, world.left, 1)?;
        }
        let mut role_sources = BTreeMap::new();
        for (local, source) in &self.role_sources {
            role_sources.insert(*local, roles::copy_source(world, source)?);
        }
        let mut value_role_sources = BTreeMap::new();
        for (value, source) in &self.value_role_sources {
            value_role_sources.insert(*value, roles::copy_source(world, source)?);
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
                charge_source,
            )?
            || !same_map(
                world,
                &self.value_role_sources,
                &other.value_role_sources,
                charge_source,
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
            .chain(self.values.iter().zip(&other.values))
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
    values: BTreeSet<ValueId>,
    implicit: BTreeSet<u32>,
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
        world.pay(index::search(self.values.len()))?;
        self.values.insert(value);
        Ok(())
    }
    fn implicit_id(&mut self, world: &mut World<'_, '_>, ordinal: u32) -> R<()> {
        world.pay(index::search(self.implicit.len()))?;
        self.implicit.insert(ordinal);
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
        let mut live = LiveMetadata::default();
        world.pay(
            self.values
                .len()
                .saturating_add(self.locals.len())
                .saturating_add(self.declared.len()),
        )?;
        for (ordinal, value) in self.values.iter().enumerate() {
            if let Some(value) = value {
                live.value_id(world, ValueId(ordinal as u32))?;
                live.value(world, value)?;
            }
        }
        for value in self.locals.iter().flatten().chain(self.declared.values()) {
            live.value(world, value)?;
        }
        for source in self
            .role_sources
            .values()
            .chain(self.value_role_sources.values())
        {
            live.source(world, source)?;
        }
        world.pay(self.reservations.len())?;
        for reservation in self.reservations.values() {
            if let reservations::Target::Address(address) = reservation.target {
                live.value_id(world, address)?;
            }
            live.value(world, &reservation.parent)?;
            live.value(world, &reservation.expected)?;
        }
        loop {
            let before = live.values.len().saturating_add(live.implicit.len());
            world.pay(self.implicit.len().saturating_add(self.reborrows.len()))?;
            for (ordinal, image) in &self.implicit {
                // Non-SSA formal/environment images are kept conservatively.
                // Matching an SSA head keeps a correspondence; it never merges
                // two original ordinals or establishes a class equality.
                world.pay(
                    index::search(live.implicit.len())
                        .saturating_add(index::search(live.values.len())),
                )?;
                if live.implicit.contains(ordinal)
                    || image_value(image).is_none_or(|head| live.values.contains(&head))
                {
                    live.implicit_id(world, *ordinal)?;
                    live.image(world, image)?;
                }
            }
            for (value, edge) in &self.reborrows {
                world.pay(index::search(live.values.len()))?;
                if live.values.contains(value) {
                    live.value_id(world, edge.parent)?;
                    live.source(world, &edge.source)?;
                }
            }
            let after = live.values.len().saturating_add(live.implicit.len());
            if after > world.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "live CFG metadata closure exceeds node limit",
                    None,
                ));
            }
            if before == after {
                break;
            }
        }
        world.pay(
            self.implicit
                .len()
                .saturating_mul(index::search(live.implicit.len()))
                .saturating_add(
                    self.reborrows
                        .len()
                        .saturating_add(self.value_roots.len())
                        .saturating_mul(index::search(live.values.len())),
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
            hash_source(&mut digest, world.left, source)?;
        }
        hash_binary(&mut digest, world.left, &self.value_role_sources.len())?;
        for (value, source) in &self.value_role_sources {
            hash_binary(&mut digest, world.left, value)?;
            hash_source(&mut digest, world.left, source)?;
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
        for cells in [&self.locals, &self.values] {
            world.pay(cells.len().saturating_add(8))?;
            digest.update(&cells.len().to_le_bytes());
            for (ordinal, value) in cells.iter().enumerate() {
                if let Some(value) = value {
                    world.pay(8)?;
                    digest.update(&ordinal.to_le_bytes());
                    hash_domain(&mut digest, world, domains, value)?;
                }
            }
        }
        world.pay(self.declared.len().saturating_add(8))?;
        digest.update(&self.declared.len().to_le_bytes());
        for (local, value) in &self.declared {
            world.pay(4)?;
            digest.update(&local.0.to_le_bytes());
            hash_domain(&mut digest, world, domains, value)?;
        }
        Ok(digest.digest())
    }
}
// The key is private to this request. Encode the actual structure directly:
// Debug field names and punctuation have no role in exact state identity.
// Every byte supplied by Hash is paid, including collection lengths and tags.
struct StateDigest<'a> {
    digest: &'a mut zil_base::FingerprintBuilder,
    left: &'a mut usize,
    exhausted: bool,
}
impl std::hash::Hasher for StateDigest<'_> {
    fn finish(&self) -> u64 {
        // This adapter feeds the complete FingerprintBuilder; no caller uses
        // the lossy standard Hasher finish value.
        0
    }
    fn write(&mut self, value: &[u8]) {
        if self.exhausted {
            return;
        }
        if let Some(left) = self.left.checked_sub(value.len()) {
            *self.left = left;
            self.digest.update(value);
        } else {
            self.exhausted = true;
        }
    }
}
fn hash_binary(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    value: &impl std::hash::Hash,
) -> R<()> {
    let mut sink = StateDigest {
        digest,
        left,
        exhausted: false,
    };
    std::hash::Hash::hash(value, &mut sink);
    if sink.exhausted {
        return Err(Diagnostic::limit(
            Phase::Types,
            "canonical CFG state digest work exceeded",
            None,
        ));
    }
    Ok(())
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
// A retained Rc makes this exact allocation immutable and prevents pointer
// reuse. This table only abbreviates the same complete life/classes bytes;
// it contains no reaching-state or class-equation conclusion.
#[derive(Default)]
struct DomainDigests(BTreeMap<usize, (SharedType, zil_base::Fingerprint)>);
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
        world.pay(index::search(domains.0.len()))?;
        let fingerprint = if let Some((retained, fingerprint)) = domains.0.get(&key) {
            world.pay(1)?;
            if !Rc::ptr_eq(retained, value) {
                return Err(bad(
                    "immutable CFG domain cache changes its retained allocation",
                ));
            }
            *fingerprint
        } else {
            if domains.0.len() >= world.limits.max_nodes {
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
            world.pay(
                index::search(domains.0.len())
                    .saturating_add(std::mem::size_of::<(
                        usize,
                        SharedType,
                        zil_base::Fingerprint,
                    )>())
                    .saturating_add(1),
            )?;
            domains.0.insert(key, (value.clone(), fingerprint));
            fingerprint
        };
        world.pay(fingerprint.0.len())?;
        digest.update(&fingerprint.0);
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
pub(super) struct Cfg {
    schema: FunctionId,
    blocks: BTreeMap<BlockId, usize>,
    definitions: Vec<usize>,
    dominators: Vec<BTreeSet<usize>>,
    reachable_from: Vec<BTreeSet<usize>>,
    instance: SemanticInstanceId,
    seen: Vec<BTreeMap<zil_base::Fingerprint, Vec<usize>>>,
    domains: DomainDigests,
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
        let instance = instance.id;
        let schema = function.id;
        let count = function.blocks.len();
        world.pay(
            count
                .saturating_mul(count)
                .saturating_mul(3)
                .saturating_add(function.values.len())
                .saturating_add(count),
        )?;
        let mut blocks = BTreeMap::new();
        let mut definitions = vec![usize::MAX; function.values.len()];
        for (index, block) in function.blocks.iter().enumerate() {
            if blocks.insert(block.id, index).is_some() {
                return Err(bad("class CFG repeats a block identity"));
            }
            world.pay(block.instructions.len())?;
            for instruction in &block.instructions {
                if let Some(result) = instruction.result {
                    let definition = definitions
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("class CFG defines an absent SSA value"))?;
                    if *definition != usize::MAX {
                        return Err(bad("class CFG repeats an SSA definition"));
                    }
                    *definition = index;
                }
            }
        }
        if definitions.contains(&usize::MAX) {
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
        let mut reachable = BTreeSet::new();
        let mut work = vec![entry];
        while let Some(index) = work.pop() {
            world.pay(successors[index].len().saturating_add(1))?;
            if reachable.insert(index) {
                work.extend(successors[index].iter().copied());
            }
        }
        let mut dominators = vec![BTreeSet::new(); count];
        for index in &reachable {
            world.pay(reachable.len())?;
            dominators[*index] = if *index == entry {
                BTreeSet::from([entry])
            } else {
                reachable.clone()
            };
        }
        loop {
            let mut changed = false;
            for index in &reachable {
                if *index == entry {
                    continue;
                }
                world.pay(reachable.len())?;
                let mut common = reachable.clone();
                for predecessor in &predecessors[*index] {
                    if !reachable.contains(predecessor) {
                        continue;
                    }
                    world.pay(common.len())?;
                    common.retain(|value| dominators[*predecessor].contains(value));
                }
                common.insert(*index);
                world.pay(common.len().saturating_add(dominators[*index].len()))?;
                if common != dominators[*index] {
                    dominators[*index] = common;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut reachable_from = Vec::with_capacity(count);
        for start in 0..count {
            let mut reached = BTreeSet::new();
            let mut pending = vec![start];
            while let Some(block) = pending.pop() {
                world.pay(successors[block].len().saturating_add(1))?;
                if reached.insert(block) {
                    pending.extend(successors[block].iter().copied());
                }
            }
            reachable_from.push(reached);
        }
        Ok(Self {
            schema,
            blocks,
            definitions,
            dominators,
            reachable_from,
            instance,
            seen: (0..count).map(|_| BTreeMap::new()).collect(),
            domains: DomainDigests::default(),
        })
    }
    pub fn index(&self, block: BlockId) -> R<usize> {
        self.blocks
            .get(&block)
            .copied()
            .ok_or_else(|| bad("capture CFG block is absent"))
    }
    pub fn retain_dominating<T>(&self, index: usize, values: &mut [Option<T>]) {
        for (value, definition) in values.iter_mut().zip(&self.definitions) {
            if *definition == index || !self.dominators[index].contains(definition) {
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
    ) -> R<bool> {
        let Some(SemanticTypeValue::Resolved { classes, .. }) = value.map(Rc::as_ref) else {
            return Ok(false);
        };
        world.pay(classes.len())?;
        for image in classes {
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
            let definition = *self
                .definitions
                .get(id.0 as usize)
                .ok_or_else(|| bad("loop role source selects an absent SSA occurrence"))?;
            return Ok(self.reachable_from[entry].contains(&definition)
                && self.reachable_from[definition].contains(&entry));
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
        world.pay(state.values.len().saturating_add(1))?;
        let index = *self
            .blocks
            .get(&target)
            .ok_or_else(|| bad("class CFG successor is absent"))?;
        // A loop may carry an old physical occurrence while reexecuting its
        // producer. Its public SSA spelling cannot make the two epochs equal.
        // Mark the occupant, not the spelling: replacement clears this mark.
        if source.is_some_and(|source| {
            self.reachable_from[index].contains(&source)
                && (self.dominators[source].contains(&index) || index <= source)
        }) {
            for (ordinal, value) in state.locals.iter().enumerate() {
                if self.loop_occurrence(world, index, value.as_ref(), &state.implicit)? {
                    state.stale_locals.insert(LocalId(ordinal as u32));
                }
            }
            for (ordinal, value) in state.values.iter().enumerate() {
                if self.loop_occurrence(world, index, value.as_ref(), &state.implicit)? {
                    state.stale_values.insert(ValueId(ordinal as u32));
                }
            }
            for (local, source) in &state.role_sources {
                if let roles::Source::Class(image) = source {
                    if self.loop_image(world, index, image, &state.implicit)? {
                        state.stale_locals.insert(*local);
                    }
                }
            }
            for (value, source) in &state.value_role_sources {
                if let roles::Source::Class(image) = source {
                    if self.loop_image(world, index, image, &state.implicit)? {
                        state.stale_values.insert(*value);
                    }
                }
            }
        }
        // Only definitions strictly dominating entry to this block survive.
        // In particular a previous loop iteration cannot initialize this
        // iteration's SSA, and a sibling branch cannot donate a same-numbered value.
        for (value, definition) in state.values.iter_mut().zip(&self.definitions) {
            if *definition == index || !self.dominators[index].contains(definition) {
                *value = None;
            }
        }
        state
            .stale_values
            .retain(|id| state.values[id.0 as usize].is_some());
        state
            .stale_locals
            .retain(|id| state.locals[id.0 as usize].is_some());
        state
            .role_sources
            .retain(|id, _| state.locals[id.0 as usize].is_some());
        state
            .value_role_sources
            .retain(|id, _| state.values[id.0 as usize].is_some());
        let before = *world.left;
        let canonical = state.canonicalize(world);
        world.canonical_work = world
            .canonical_work
            .saturating_add(before.saturating_sub(*world.left));
        canonical?;
        let before = *world.left;
        let key = state.key(world, &mut self.domains);
        world.key_work = world
            .key_work
            .saturating_add(before.saturating_sub(*world.left));
        let key = key?;
        world.pay(index::search(self.seen[index].len()))?;
        if let Some(candidates) = self.seen[index].get(&key) {
            world.pay(candidates.len())?;
            for ordinal in candidates {
                world.comparisons = world.comparisons.saturating_add(1);
                let before = *world.left;
                let checked = state.same(&reaching[index][*ordinal], world);
                world.compare_work = world
                    .compare_work
                    .saturating_add(before.saturating_sub(*world.left));
                if checked? {
                    return Ok(());
                }
            }
        }
        world.pay(index::search(self.seen[index].len()).saturating_add(2))?;
        let ordinal = reaching[index].len();
        reaching[index].push(state);
        self.seen[index].entry(key).or_default().push(ordinal);
        work.push_back((index, ordinal));
        Ok(())
    }
}
