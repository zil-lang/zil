//! Independently reconstruct invariant signature equations. This grants no roots,
//! storage initialization, parent loans, effects or physical access rights.
use super::*;
mod array_outer;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-CLASS-EQUATION", None, message)
}

#[derive(Clone, Default)]
struct Ports {
    outer: Option<usize>,
    classes: Vec<usize>,
    keys: Vec<(Vec<LifeTypeProjection>, u32)>,
    direct: bool,
    environment: bool,
    task_ports: Vec<(Vec<zil_ir::ContractProjection>, usize)>,
    // Per-ordinal address upper variables, distinct from nominal classes.
    array_elements: Vec<usize>,
}
fn pure_class_keys(
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
    b: &mut Budget,
) -> Check<()> {
    if path.len() > b.limits.max_call_depth {
        return Err(reject(
            "E-VERIFY-LIMIT",
            None,
            "actual pure class path exceeds type depth",
        ));
    }
    b.spend(1)?;
    match ty {
        Type::BorrowedStorage { life, .. } => {
            for class in 0..life.binder_count {
                b.spend(path.len().saturating_add(1))?;
                out.push((path.clone(), class));
            }
        }
        Type::ErrorUnion { error, result }
            if b.task_failure_carriers
                || b.stored_enum_addresses && super::stored_flow::enum_address::signature(ty) =>
        {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)));
                pure_class_keys(child, path, out, b)?;
                path.pop();
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            pure_class_keys(element, path, out, b)?;
            path.pop();
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                pure_class_keys(field, path, out, b)?;
                path.pop();
            }
        }
        Type::Record { fields, .. } => {
            for (index, (_, field)) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                pure_class_keys(field, path, out, b)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                for (index, field) in fields.iter().enumerate() {
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    pure_class_keys(field, path, out, b)?;
                    path.pop();
                }
            }
        }
        _ => {}
    }
    Ok(())
}
// Verify uses explicit representative substitution, not Analyze's parent arena.
// Array equations keep member lists so only the smaller component is rewritten.

// This only allocates invariant class variables. A class is never a target root,
// an initialized payload or an independent retained borrow permission.
#[allow(clippy::too_many_arguments)]
fn dyn_class_keys(
    module: &TypedModule,
    ty: &Type,
    cp: &mut Vec<LifeTypeProjection>,
    bp: &mut Vec<zil_ir::ContractProjection>,
    keys: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
    ports: &mut Vec<(Vec<zil_ir::ContractProjection>, usize)>,
    b: &mut Budget,
) -> Check<()> {
    let n = 1usize;
    b.spend(n)?;
    if cp.len() > b.limits.max_call_depth {
        return Err(bad("dyn Context class path exceeds type depth"));
    }
    match ty {
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            let n = module.dyn_interfaces.len();
            b.spend(n)?;
            let item = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .ok_or_else(|| bad("actual dyn Context class interface is absent"))?;
            if item.context.is_none() {
                return Ok(());
            }
            let n = item.retained_life.binder_count as usize;
            b.spend(n)?;
            for class in 0..item.retained_life.binder_count {
                let n = cp.len().saturating_add(1);
                b.spend(n)?;
                keys.push((cp.clone(), class));
            }
            for (index, row) in item.retained_life.ports.iter().enumerate() {
                let n = keys.len().saturating_add(bp.len()).saturating_add(1);
                b.spend(n)?;
                let key = keys
                    .iter()
                    .position(|(path, class)| path == cp && *class == row.binder)
                    .ok_or_else(|| bad("actual dyn port binder has no retained class"))?;
                let mut path = bp.clone();
                path.push(zil_ir::ContractProjection::RetainedPort(index as u32));
                ports.push((path, key));
            }
        }
        Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => {
            cp.push(LifeTypeProjection::Pointee);
            dyn_class_keys(module, inner, cp, bp, keys, ports, b)?;
            cp.pop();
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                let selector = PlaceProjection::ErrorArm(arm);
                cp.push(LifeTypeProjection::Place(selector));
                bp.push(zil_ir::ContractProjection::Place(selector));
                dyn_class_keys(module, child, cp, bp, keys, ports, b)?;
                bp.pop();
                cp.pop();
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (index, (_, child)) in fields.iter().enumerate() {
                let selector = PlaceProjection::Field(index as u32);
                cp.push(LifeTypeProjection::Place(selector));
                bp.push(zil_ir::ContractProjection::Place(selector));
                dyn_class_keys(module, child, cp, bp, keys, ports, b)?;
                bp.pop();
                cp.pop();
            }
        }
        Type::Tuple(fields) => {
            for (index, child) in fields.iter().enumerate() {
                let selector = PlaceProjection::Field(index as u32);
                cp.push(LifeTypeProjection::Place(selector));
                bp.push(zil_ir::ContractProjection::Place(selector));
                dyn_class_keys(module, child, cp, bp, keys, ports, b)?;
                bp.pop();
                cp.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                for (index, child) in fields.iter().enumerate() {
                    let selector = PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: variant as u32,
                        index: index as u32,
                    });
                    cp.push(LifeTypeProjection::Place(selector));
                    bp.push(zil_ir::ContractProjection::Place(selector));
                    dyn_class_keys(module, child, cp, bp, keys, ports, b)?;
                    bp.pop();
                    cp.pop();
                }
            }
        }
        // BorrowedStorage and Closure already have complete enclosing class
        // domains. Their nested retained rows are checked by those consumers.
        _ => {}
    }
    Ok(())
}

struct Equations {
    representative: Vec<usize>,
    members: Option<Vec<Vec<usize>>>,
    rigid: Vec<Option<usize>>,
    grounded: Option<Vec<bool>>,
    next_rigid: usize,
}

impl Equations {
    fn fresh(&mut self, rigid: bool, b: &mut Budget) -> Check<usize> {
        b.spend(3)?;
        let id = self.representative.len();
        if let Some(members) = &mut self.members {
            b.spend(2)?;
            members.push(vec![id]);
        }
        self.representative.push(id);
        if let Some(grounded) = &mut self.grounded {
            b.spend(1)?;
            grounded.push(rigid);
        }
        self.rigid.push(if rigid {
            let label = self.next_rigid;
            self.next_rigid += 1;
            Some(label)
        } else {
            None
        });
        Ok(id)
    }
    fn equal(&mut self, a: usize, z: usize, b: &mut Budget) -> Check<()> {
        b.spend(if self.members.is_some() {
            3
        } else {
            self.representative
                .len()
                .saturating_mul(2)
                .saturating_add(1)
        })?;
        let mut a = self.representative[a];
        let mut z = self.representative[z];
        if a == z {
            return Ok(());
        }
        if self.rigid[a].is_some() && self.rigid[z].is_some() && self.rigid[a] != self.rigid[z] {
            return Err(bad(
                "actual retained class equality is not universally valid",
            ));
        }
        if let Some(members) = &mut self.members {
            b.spend(2)?;
            let mut a_count = members[a].len();
            let mut z_count = members[z].len();
            if a_count < z_count {
                std::mem::swap(&mut a, &mut z);
                std::mem::swap(&mut a_count, &mut z_count);
            }
            b.spend(1)?;
            let grow = members[a].capacity().saturating_sub(a_count) < z_count;
            b.spend(
                z_count
                    .saturating_mul(3)
                    .saturating_add(1)
                    .saturating_add(if grow { a_count } else { 0 }),
            )?;
            if grow {
                // Pay copying the existing larger component before reserve;
                // none of the following member pushes can reallocate.
                members[a].reserve(z_count);
            }
            for member in std::mem::take(&mut members[z]) {
                self.representative[member] = a;
                members[a].push(member);
            }
        } else {
            for row in &mut self.representative {
                if *row == z {
                    *row = a;
                }
            }
        }
        self.rigid[a] = self.rigid[a].or(self.rigid[z]);
        if let Some(grounded) = &mut self.grounded {
            b.spend(1)?;
            grounded[a] |= grounded[z];
        }
        Ok(())
    }
    fn track_grounding(&mut self, b: &mut Budget) -> Check<()> {
        if self.grounded.is_none() {
            b.spend(self.rigid.len())?;
            self.grounded = Some(self.rigid.iter().map(Option::is_some).collect());
        }
        Ok(())
    }
    fn ground_address(&mut self, port: Option<usize>, b: &mut Budget) -> Check<()> {
        if let (Some(grounded), Some(port)) = (&mut self.grounded, port) {
            b.spend(1)?;
            grounded[self.representative[port]] = true;
        }
        Ok(())
    }
    fn ports(&mut self, module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Ports> {
        let outer = if matches!(ty, Type::Address { .. }) {
            Some(self.fresh(false, b)?)
        } else {
            None
        };
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        let environment = matches!(ty, Type::Closure(id)
            if module.closures.get(id.0 as usize).is_some_and(|d| d.storage_life.is_some()));
        if environment {
            self.track_grounding(b)?;
        }
        let mut array_elements = Vec::new();
        if super::stored_flow::shared_array_value(ty) {
            if self.members.is_none() {
                b.spend(self.representative.len().saturating_mul(3))?;
                let mut members = vec![Vec::new(); self.representative.len()];
                for (variable, representative) in self.representative.iter().enumerate() {
                    members[*representative].push(variable);
                }
                self.members = Some(members);
            }
            let Type::Array { length, .. } = ty else {
                unreachable!()
            };
            let count = usize::try_from(*length).map_err(|_| bad("array class count overflow"))?;
            b.spend(count)?;
            for _ in 0..count {
                array_elements.push(self.fresh(false, b)?);
            }
        }
        let mut keys = Vec::new();
        let mut direct = false;
        if let Type::Closure(id) = ty {
            let count = module
                .closures
                .get(id.0 as usize)
                .and_then(|d| d.storage_life.as_ref())
                .map_or(0, |c| c.fixed_binder_count);
            b.spend(count as usize)?;
            direct = true;
            for class in 0..count {
                keys.push((
                    if outer.is_some() {
                        vec![LifeTypeProjection::Pointee]
                    } else {
                        Vec::new()
                    },
                    class,
                ));
            }
        } else if let Type::BorrowedStorage { life, .. } = ty {
            b.spend(life.binder_count as usize)?;
            direct = true;
            for class in 0..life.binder_count {
                keys.push((
                    if outer.is_some() {
                        vec![LifeTypeProjection::Pointee]
                    } else {
                        Vec::new()
                    },
                    class,
                ));
            }
        } else if !module.life_instances.is_empty()
            && matches!(
                ty,
                Type::Array { .. } | Type::Tuple(_) | Type::Record { .. } | Type::Enum { .. }
            )
            || b.stored_enum_addresses
                && matches!(ty, Type::ErrorUnion { .. })
                && super::stored_flow::enum_address::signature(ty)
            || b.task_failure_carriers && matches!(ty, Type::ErrorUnion { .. })
        {
            b.spend(usize::from(outer.is_some()))?;
            let mut path = if outer.is_some() {
                vec![LifeTypeProjection::Pointee]
            } else {
                Vec::new()
            };
            pure_class_keys(ty, &mut path, &mut keys, b)?;
        }
        let mut task_ports = Vec::new();
        if b.task_sources() {
            fn walk(
                ty: &Type,
                cp: Vec<LifeTypeProjection>,
                bp: Vec<zil_ir::ContractProjection>,
                keys: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
                rows: &mut Vec<(Vec<zil_ir::ContractProjection>, usize)>,
                b: &mut Budget,
            ) -> Check<()> {
                b.spend(1)?;
                match ty {
                    Type::Task { retained_life, .. } => {
                        super::tasks::charge_result_context(ty, b)?;
                        super::tasks::require_closed_context(ty, None)?;
                        for class in 0..retained_life.binder_count {
                            keys.push((cp.clone(), class));
                        }
                        for (port, row) in retained_life.ports.iter().enumerate() {
                            b.spend(1)?;
                            let index = keys
                                .iter()
                                .position(|(p, c)| p == &cp && *c == row.binder)
                                .ok_or_else(|| bad("Task port has no actual semantic class"))?;
                            let mut path = bp.clone();
                            path.push(zil_ir::ContractProjection::RetainedPort(port as u32));
                            rows.push((path, index));
                        }
                    }
                    Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
                        let mut path = cp;
                        path.push(LifeTypeProjection::Pointee);
                        walk(inner, path, bp, keys, rows, b)?;
                    }
                    Type::ErrorUnion { result, .. } => {
                        let step = PlaceProjection::ErrorArm(false);
                        let mut a = cp;
                        a.push(LifeTypeProjection::Place(step));
                        let mut z = bp;
                        z.push(zil_ir::ContractProjection::Place(step));
                        walk(result, a, z, keys, rows, b)?;
                    }
                    _ => {}
                }
                Ok(())
            }
            let path = if outer.is_some() {
                vec![LifeTypeProjection::Pointee]
            } else {
                vec![]
            };
            walk(ty, path, vec![], &mut keys, &mut task_ports, b)?;
        }
        let n = module.dyn_interfaces.len();
        b.spend(n)?;
        if module
            .dyn_interfaces
            .iter()
            .any(|item| item.context.is_some())
        {
            let mut cp = if outer.is_some() {
                vec![LifeTypeProjection::Pointee]
            } else {
                vec![]
            };
            dyn_class_keys(
                module,
                ty,
                &mut cp,
                &mut vec![],
                &mut keys,
                &mut task_ports,
                b,
            )?;
        }
        let count = keys.len();
        if !direct {
            b.spend(count)?;
        }
        let mut classes = Vec::with_capacity(count);
        for _ in 0..count {
            classes.push(self.fresh(false, b)?);
        }
        Ok(Ports {
            outer,
            classes,
            keys,
            direct,
            environment,
            task_ports,
            array_elements,
        })
    }
    fn freeze(&mut self, fixed: &[usize], b: &mut Budget) -> Check<()> {
        b.spend(fixed.len())?;
        for slot in fixed {
            let root = self.representative[*slot];
            if !self
                .grounded
                .as_ref()
                .is_some_and(|grounded| grounded[root])
            {
                return Err(unsupported(
                    "captured Fixed has no independently established source class",
                    None,
                ));
            }
            if self.rigid[root].is_none() {
                self.rigid[root] = Some(self.next_rigid);
                self.next_rigid += 1;
            }
        }
        Ok(())
    }
    fn copy_elements(&mut self, a: &Ports, z: &Ports, b: &mut Budget) -> Check<()> {
        if a.array_elements.is_empty() && z.array_elements.is_empty() {
            return Ok(());
        }
        b.spend(
            a.array_elements
                .len()
                .saturating_add(z.array_elements.len()),
        )?;
        if a.array_elements.len() != z.array_elements.len() {
            return Err(bad("array address upper arity differs"));
        }
        for (a, z) in a.array_elements.iter().zip(&z.array_elements) {
            self.equal(*a, *z, b)?;
        }
        Ok(())
    }
    fn copy(&mut self, a: &Ports, z: &Ports, b: &mut Budget) -> Check<()> {
        self.copy_elements(a, z, b)?;
        b.spend(
            a.classes
                .len()
                .saturating_add(z.classes.len())
                .saturating_add(1),
        )?;
        if a.classes.len() != z.classes.len() {
            return Err(bad("class port arity differs from actual value"));
        }
        if let (Some(a), Some(z)) = (a.outer, z.outer) {
            self.equal(a, z, b)?;
        }
        for (a, z) in a.classes.iter().zip(&z.classes) {
            self.equal(*a, *z, b)?;
        }
        Ok(())
    }
}
fn failure_view_slot(
    shape: &StorageLifeShape,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<(u32, StoredBorrowMode)>> {
    let mut found = None;
    b.spend(shape.slots.len())?;
    for row in &shape.slots {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if row.path == path && found.replace((row.binder, row.mode)).is_some() {
            return Err(bad("failure view has duplicate actual physical class rows"));
        }
    }
    Ok(found)
}
fn shape(ty: &Type) -> Option<&StorageLifeShape> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::BorrowedStorage { life, .. } = ty {
        Some(life)
    } else {
        None
    }
}
fn link_projection(
    parent_ty: &Type,
    parent: &Ports,
    child: &Ports,
    step: LifeTypeProjection,
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    if child.keys.is_empty() {
        return Ok(());
    }
    b.spend(usize::from(matches!(parent_ty, Type::Address { .. })))?;
    let (physical, prefix) = if let Type::Address { inner, .. } = parent_ty {
        (inner.as_ref(), vec![LifeTypeProjection::Pointee])
    } else {
        (parent_ty, Vec::new())
    };
    for (index, (path, class)) in child.keys.iter().enumerate() {
        let retained = if let Type::BorrowedStorage { life, .. } = physical {
            life.retained.as_slice()
        } else {
            &[]
        };
        let scan = parent.keys.len().saturating_add(retained.len());
        b.spend(scan)?;
        let full_len = path.len().saturating_add(1);
        let lookup_work = parent
            .keys
            .iter()
            .fold(0usize, |n, (p, _)| {
                n.saturating_add(p.len().saturating_add(full_len).saturating_add(1))
            })
            .saturating_add(retained.iter().fold(0usize, |n, row| {
                n.saturating_add(row.path.len().saturating_add(full_len).saturating_add(1))
            }))
            .saturating_add(full_len.saturating_mul(3))
            .saturating_add(1);
        b.spend(lookup_work.saturating_sub(scan))?;
        let tail = if path.first() == Some(&LifeTypeProjection::Pointee)
            && !(b.task_sources() && !child.task_ports.is_empty() && child.outer.is_none())
        {
            &path[1..]
        } else {
            path.as_slice()
        };
        let mut full = vec![step];
        full.extend_from_slice(tail);
        let parent_class = if let Type::BorrowedStorage { life, .. } = physical {
            let row = life
                .retained
                .iter()
                .find(|row| row.path == full && row.class == *class)
                .ok_or_else(|| bad("projected qualified child lacks actual retained class path"))?;
            parent
                .keys
                .iter()
                .position(|(path, local)| path == &prefix && *local == row.binder)
        } else {
            let mut wanted = prefix.clone();
            wanted.extend(full);
            parent
                .keys
                .iter()
                .position(|(path, local)| path == &wanted && local == class)
        }
        .ok_or_else(|| bad("projected class has no exact parent type path"))?;
        e.equal(parent.classes[parent_class], child.classes[index], b)?;
    }
    Ok(())
}
fn variables(
    c: &StorageLifeContract,
    e: &mut Equations,
    rigid: bool,
    b: &mut Budget,
) -> Check<(Vec<usize>, Vec<usize>)> {
    crate::signature_life::charge(c, b)?;
    let mut fixed = Vec::new();
    let mut invocation = Vec::new();
    for _ in 0..c.fixed_binder_count {
        fixed.push(e.fresh(rigid, b)?);
    }
    for _ in 0..c.invocation_binder_count {
        invocation.push(e.fresh(rigid, b)?);
    }
    Ok((fixed, invocation))
}
fn select(v: &(Vec<usize>, Vec<usize>), binder: StorageBinder) -> Check<usize> {
    let value = match binder {
        StorageBinder::Fixed(i) => v.0.get(i as usize),
        StorageBinder::Invocation(i) => v.1.get(i as usize),
    };
    value
        .copied()
        .ok_or_else(|| bad("class binder outside invocation domain"))
}
fn bind(
    c: &StorageLifeContract,
    base: StorageBase,
    p: &Ports,
    v: &(Vec<usize>, Vec<usize>),
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    crate::signature_life::charge(c, b)?;
    for row in &c.bindings {
        if row.position.base == base {
            if !p.array_elements.is_empty()
                && let [zil_ir::ContractProjection::Place(PlaceProjection::Field(index))] =
                    row.position.path.as_slice()
            {
                let actual = *p
                    .array_elements
                    .get(*index as usize)
                    .ok_or_else(|| bad("array binding selects absent actual ordinal"))?;
                e.equal(actual, select(v, row.binder)?, b)?;
                continue;
            }
            if !row.position.path.is_empty() && b.task_sources() {
                b.spend(p.task_ports.len())?;
                if let Some((_, index)) = p
                    .task_ports
                    .iter()
                    .find(|(path, _)| path == &row.position.path)
                {
                    e.equal(p.classes[*index], select(v, row.binder)?, b)?;
                    continue;
                }
            }
            if !row.position.path.is_empty() {
                return Err(bad(
                    "class substitution cannot interpret physical/retained access paths",
                ));
            }
            e.equal(
                p.outer
                    .ok_or_else(|| bad("borrow binding has no actual Address"))?,
                select(v, row.binder)?,
                b,
            )?;
        }
    }
    for row in &c.classes {
        if row.position.base == base {
            if p.direct {
                let index = row.position.class as usize;
                let (path, class) = p
                    .keys
                    .get(index)
                    .ok_or_else(|| bad("qualified class has no actual carrier position"))?;
                if *path != row.position.path || *class != row.position.class {
                    return Err(bad("qualified class has no exact actual type path"));
                }
                e.equal(p.classes[index], select(v, row.binder)?, b)?;
                continue;
            }
            b.spend(p.keys.len())?;
            let lookup_work = p.keys.iter().fold(0usize, |n, (path, _)| {
                n.saturating_add(
                    path.len()
                        .saturating_add(row.position.path.len())
                        .saturating_add(1),
                )
            });
            b.spend(lookup_work.saturating_sub(p.keys.len()))?;
            let index = p
                .keys
                .iter()
                .position(|(path, class)| {
                    *path == row.position.path && *class == row.position.class
                })
                .ok_or_else(|| bad("qualified class has no exact actual type path"))?;
            let actual = &p.classes[index];
            e.equal(*actual, select(v, row.binder)?, b)?;
        }
    }
    Ok(())
}
fn bind_with_outer(
    c: &StorageLifeContract,
    base: StorageBase,
    p: &Ports,
    v: &(Vec<usize>, Vec<usize>),
    e: &mut Equations,
    flow: Option<&array_outer::Flow>,
    b: &mut Budget,
) -> Check<()> {
    crate::signature_life::charge(c, b)?;
    for row in &c.bindings {
        if row.position.base == base {
            if !p.array_elements.is_empty()
                && let [zil_ir::ContractProjection::Place(PlaceProjection::Field(index))] =
                    row.position.path.as_slice()
            {
                let actual = *p
                    .array_elements
                    .get(*index as usize)
                    .ok_or_else(|| bad("array binding selects absent actual ordinal"))?;
                array_outer::binding(e, actual, select(v, row.binder)?, flow, b)?;
                continue;
            }
            if !row.position.path.is_empty() && b.task_sources() {
                b.spend(p.task_ports.len())?;
                if let Some((_, index)) = p
                    .task_ports
                    .iter()
                    .find(|(path, _)| path == &row.position.path)
                {
                    array_outer::binding(e, p.classes[*index], select(v, row.binder)?, flow, b)?;
                    continue;
                }
            }
            if !row.position.path.is_empty() {
                return Err(bad(
                    "class substitution cannot interpret physical/retained access paths",
                ));
            }
            array_outer::binding(
                e,
                p.outer
                    .ok_or_else(|| bad("borrow binding has no actual Address"))?,
                select(v, row.binder)?,
                flow,
                b,
            )?;
        }
    }
    for row in &c.classes {
        if row.position.base == base {
            if p.direct {
                let index = row.position.class as usize;
                let (path, class) = p
                    .keys
                    .get(index)
                    .ok_or_else(|| bad("qualified class has no actual carrier position"))?;
                if *path != row.position.path || *class != row.position.class {
                    return Err(bad("qualified class has no exact actual type path"));
                }
                array_outer::binding(e, p.classes[index], select(v, row.binder)?, flow, b)?;
                continue;
            }
            b.spend(p.keys.len())?;
            let lookup_work = p.keys.iter().fold(0usize, |n, (path, _)| {
                n.saturating_add(
                    path.len()
                        .saturating_add(row.position.path.len())
                        .saturating_add(1),
                )
            });
            b.spend(lookup_work.saturating_sub(p.keys.len()))?;
            let index = p
                .keys
                .iter()
                .position(|(path, class)| {
                    *path == row.position.path && *class == row.position.class
                })
                .ok_or_else(|| bad("qualified class has no exact actual type path"))?;
            let actual = &p.classes[index];
            array_outer::binding(e, *actual, select(v, row.binder)?, flow, b)?;
        }
    }
    Ok(())
}

fn call(
    c: &StorageLifeContract,
    args: &[ValueId],
    output: Option<ValueId>,
    values: &[Ports],
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    let formal = variables(c, e, false, b)?;
    for (i, arg) in args.iter().enumerate() {
        bind(
            c,
            StorageBase::Parameter(i as u32),
            &values[arg.0 as usize],
            &formal,
            e,
            b,
        )?;
    }
    if let Some(out) = output {
        bind(
            c,
            StorageBase::Result,
            &values[out.0 as usize],
            &formal,
            e,
            b,
        )?;
    }
    Ok(())
}
fn call_with_outer(
    c: &StorageLifeContract,
    args: &[ValueId],
    output: Option<ValueId>,
    values: &[Ports],
    e: &mut Equations,
    flow: Option<&array_outer::Flow>,
    b: &mut Budget,
) -> Check<()> {
    if flow.is_none() {
        return call(c, args, output, values, e, b);
    }

    let formal = variables(c, e, false, b)?;
    for (i, arg) in args.iter().enumerate() {
        bind_with_outer(
            c,
            StorageBase::Parameter(i as u32),
            &values[arg.0 as usize],
            &formal,
            e,
            flow,
            b,
        )?;
    }
    if let Some(out) = output {
        bind_with_outer(
            c,
            StorageBase::Result,
            &values[out.0 as usize],
            &formal,
            e,
            flow,
            b,
        )?;
    }
    Ok(())
}

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    b: &mut Budget,
) -> Check<()> {
    check_with(module, f, reachable, b, f.storage_life.as_ref())
}
pub(super) fn check_role(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    life: &StorageLifeContract,
    b: &mut Budget,
) -> Check<()> {
    crate::signature_life::charge(life, b)?;
    let d = module
        .closures
        .get(
            f.environment
                .ok_or_else(|| bad("role actual entry has no environment"))?
                .0 as usize,
        )
        .ok_or_else(|| bad("role entry environment is absent"))?;
    super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
    if d.storage_life.is_some() {
        super::signature_life::closure_relation(
            module,
            &d.parameters,
            &d.captures,
            d.kind,
            super::stored_callables::life_result(d),
            life,
            Some(f.span),
        )?;
    } else {
        super::borrowed_fp::relation(&d.parameters, &d.result, life, Some(f.span))?;
    }
    check_with(module, f, reachable, b, Some(life))
}
#[allow(clippy::too_many_arguments)]
fn address_field(
    f: &TypedFunction,
    address: ValueId,
    index: u32,
    parent: &Ports,
    out: &Ports,
    e: &mut Equations,
    outer: Option<&array_outer::Flow>,
    b: &mut Budget,
) -> Check<()> {
    if !out.array_elements.is_empty() {
        let shape = shape(&f.values[address.0 as usize])
            .ok_or_else(|| bad("array field projection has no actual qualified parent"))?;
        b.spend(out.array_elements.len().saturating_add(1))?;
        let mut first_rows = vec![None; out.array_elements.len()];
        for (position, row) in shape.slots.iter().enumerate() {
            b.spend(1)?; // row and complete path arity
            if row.path.len() == 2 {
                b.spend(3)?; // both scalar Projections and current-field comparison
                if let [
                    PlaceProjection::Field(field),
                    PlaceProjection::Field(element),
                ] = row.path.as_slice()
                    && *field == index
                {
                    b.spend(1)?; // bounded ordinal lookup
                    if let Some(first) = first_rows.get_mut(*element as usize) {
                        b.spend(1)?;
                        if first.is_none() {
                            b.spend(1)?;
                            *first = Some(position);
                        }
                    }
                }
            }
        }
        for (ordinal, upper) in out.array_elements.iter().enumerate() {
            b.spend(2)?; // table cell and original physical row
            let position = first_rows[ordinal]
                .ok_or_else(|| bad("array field lacks its actual physical class row"))?;
            let row = &shape.slots[position];
            array_outer::transport(e, *upper, parent.classes[row.binder as usize], outer, b)?;
        }
    }

    if let (Some(a), Some(z)) = (out.outer, parent.outer) {
        array_outer::transport(e, a, z, outer, b)?;
    }
    link_projection(
        &f.values[address.0 as usize],
        parent,
        out,
        LifeTypeProjection::Place(PlaceProjection::Field(index)),
        e,
        b,
    )?;
    Ok(())
}

fn check_with(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    b: &mut Budget,
    assumptions: Option<&StorageLifeContract>,
) -> Check<()> {
    let mut e = Equations {
        representative: Vec::new(),
        members: None,
        rigid: Vec::new(),
        grounded: None,
        next_rigid: 0,
    };
    b.spend(f.values.len().saturating_add(f.locals.len()))?;
    let mut values = Vec::new();
    let mut environments = false;
    for ty in &f.values {
        let ports = e.ports(module, ty, b)?;
        environments |= ports.environment;
        values.push(ports);
    }
    let mut locals = Vec::new();
    for local in &f.locals {
        let ports = e.ports(module, &local.ty, b)?;
        environments |= ports.environment;
        locals.push(ports);
    }
    let returned = if let Type::Address { inner, .. } = &f.return_type {
        inner.as_ref()
    } else {
        &f.return_type
    };
    if matches!(returned, Type::Closure(id) if module.closures.get(id.0 as usize).is_some_and(|d| d.storage_life.is_some()))
        && !(matches!(f.return_type, Type::Closure(_))
            && f.callable_return_contract.is_some()
            && assumptions.is_some_and(|c| {
                c.classes
                    .iter()
                    .any(|row| row.position.base == StorageBase::Result)
            }))
    {
        return Err(unsupported(
            "captured Fixed environment result needs a complete factory output relation",
            Some(f.span),
        ));
    }
    if environments && assumptions.is_none() {
        b.spend(f.parameters.len())?;
        if f.parameters
            .iter()
            .any(|l| locals[l.0 as usize].environment)
        {
            return Err(bad(
                "captured Fixed parameter needs a complete enclosing relation",
            ));
        }
    }
    let header = if let Some(c) = assumptions {
        let vars = variables(c, &mut e, true, b)?;
        for (i, local) in f.parameters.iter().enumerate() {
            bind(
                c,
                StorageBase::Parameter(i as u32),
                &locals[local.0 as usize],
                &vars,
                &mut e,
                b,
            )?;
        }
        Some(vars)
    } else {
        if f.environment.is_some() || environments {
            for local in &f.parameters {
                let p = &locals[local.0 as usize];
                if let Some(outer) = p.outer {
                    let rigid = e.fresh(true, b)?;
                    e.equal(outer, rigid, b)?;
                }
                for class in &p.classes {
                    let rigid = e.fresh(true, b)?;
                    e.equal(*class, rigid, b)?;
                }
            }
        }
        None
    };
    let mut captures = Vec::new();
    if let Some(id) = f.environment
        && let Some(d) = module.closures.get(id.0 as usize)
        && d.storage_life.is_some()
    {
        environments = true;
        e.track_grounding(b)?;
        b.spend(d.captures.len())?;
        let vars = header
            .as_ref()
            .ok_or_else(|| bad("captured entry has no complete relation"))?;
        for (index, ty) in d.captures.iter().enumerate() {
            let ports = e.ports(module, ty, b)?;
            bind(
                assumptions.ok_or_else(|| bad("captured entry relation absent"))?,
                StorageBase::Capture(index as u32),
                &ports,
                vars,
                &mut e,
                b,
            )?;
            captures.push(ports);
        }
    }
    // One invocation domain belongs to this actual entry. Each captured
    // exclusive read creates its own permission, while its outer class refers
    // to this shared call region and its pointee Fixed classes stay invariant.
    let receiver_region = if let Some(id) = f.environment {
        let definition = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| bad("receiver entry environment is absent"))?;
        b.spend(definition.captures.len())?;
        let explicit =
            assumptions.and_then(|life| crate::signature_life::outer(life, StorageBase::Receiver));
        if let Some(binder) = explicit {
            Some(select(
                header
                    .as_ref()
                    .ok_or_else(|| bad("receiver relation absent"))?,
                binder,
            )?)
        } else if definition.kind != CallableKind::FnOnce
            && definition
                .captures
                .iter()
                .any(|ty| matches!(ty, Type::Address { mutable: true, .. }))
        {
            Some(e.fresh(true, b)?)
        } else {
            None
        }
    } else {
        None
    };
    let outer = if b.array_outer_functions.is_empty() {
        None
    } else {
        b.spend(b.array_outer_functions.len())?;
        if b.array_outer_functions.contains(&f.id) {
            Some(array_outer::Flow::build(
                module,
                f,
                reachable,
                (&values, &locals, &captures),
                e.representative.len(),
                b,
            )?)
        } else {
            None
        }
    };
    // Source structure first; capture-fixed substitution second; calls last.
    // A first call cannot pick an unconstrained capture's semantic class.
    for phase in 0..if environments { 3 } else { 1 } {
        for (block_index, block) in f.blocks.iter().enumerate() {
            for (instruction_index, i) in block.instructions.iter().enumerate() {
                if let Some(flow) = &outer {
                    flow.enter(block_index, instruction_index);
                }
                b.spend(1)?;
                if environments {
                    if let InstructionKind::Call { arguments, .. } = &i.kind {
                        b.spend(arguments.len())?;
                    }
                    let stage = match &i.kind {
                        InstructionKind::MakeClosure { closure, .. }
                            if module
                                .closures
                                .get(closure.0 as usize)
                                .is_some_and(|d| d.storage_life.is_some()) =>
                        {
                            1
                        }
                        InstructionKind::CallClosureBorrowed { .. }
                        | InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureOnce { .. } => 2,
                        InstructionKind::Call { arguments, .. }
                            if arguments.iter().any(|v| values[v.0 as usize].environment)
                                || i.result.is_some_and(|v| values[v.0 as usize].environment) =>
                        {
                            2
                        }
                        _ => 0,
                    };
                    if phase != stage {
                        continue;
                    }
                }
                let out = i.result.map(|v| &values[v.0 as usize]);
                match &i.kind {
                    InstructionKind::MakeDynOwned { vtable, source, .. }
                    | InstructionKind::MakeDynBorrow { vtable, source, .. } => {
                        if b.dyn_contexts {
                            b.spend(module.dyn_vtables.len())?;
                            let table = module
                                .dyn_vtables
                                .iter()
                                .find(|table| table.id == *vtable)
                                .ok_or_else(|| bad("Dyn constructor actual table is absent"))?;
                            let iface =
                                crate::dynamic_ports::interface(module, table.interface, b)?;
                            let actual = &values[source.0 as usize];
                            let result = i
                                .result
                                .ok_or_else(|| bad("Dyn constructor has no actual result"))?;
                            let output = &values[result.0 as usize];
                            if actual.classes.len() != table.self_classes.len() {
                                return Err(bad(
                                    "Dyn Self class witness differs from actual SSA class arity",
                                ));
                            }
                            let prefix = if matches!(i.kind, InstructionKind::MakeDynOwned { .. }) {
                                vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))]
                            } else {
                                vec![LifeTypeProjection::Pointee]
                            };
                            for (index, class) in table.self_classes.iter().enumerate() {
                                let StorageBinder::Fixed(class) = class else {
                                    return Err(bad("Dyn constructor cannot freshen a Self class"));
                                };
                                if *class >= iface.retained_life.binder_count {
                                    return Err(bad("Dyn constructor Context class is absent"));
                                }
                                b.spend(
                                    output
                                        .keys
                                        .len()
                                        .saturating_mul(prefix.len().saturating_add(1)),
                                )?;
                                let selected = output.keys.iter().position(|(path, ordinal)| *path == prefix && ordinal == class)
                                    .ok_or_else(|| bad("Dyn result has no exact complete Context class occurrence"))?;
                                e.equal(actual.classes[index], output.classes[selected], b)?;
                            }
                            if matches!(i.kind, InstructionKind::MakeDynBorrow { .. })
                                && let (Some(a), Some(z)) = (actual.outer, output.outer)
                            {
                                e.equal(a, z, b)?;
                            }
                        }
                    }
                    InstructionKind::BorrowDynOwned { local, .. } if b.dyn_contexts => {
                        if let Some(out) = out {
                            let actual = &locals[local.0 as usize];
                            for (index, (path, class)) in out.keys.iter().enumerate() {
                                let tail = path
                                    .strip_prefix(&[LifeTypeProjection::Pointee])
                                    .ok_or_else(|| {
                                        bad("borrowed Dyn result has no pointee Context class path")
                                    })?;
                                let mut expected = if actual.outer.is_some() {
                                    vec![LifeTypeProjection::Pointee]
                                } else {
                                    vec![]
                                };
                                expected.extend_from_slice(tail);
                                b.spend(
                                    actual
                                        .keys
                                        .len()
                                        .saturating_mul(expected.len().saturating_add(1)),
                                )?;
                                let source=actual.keys.iter().position(|(p,c)|p==&expected&&c==class).ok_or_else(||bad("borrowed Dyn owner omits an actual complete Context class"))?;
                                e.equal(actual.classes[source], out.classes[index], b)?;
                            }
                            if let (Some(a), Some(z)) = (actual.outer, out.outer) {
                                e.equal(a, z, b)?;
                            }
                        }
                    }
                    InstructionKind::CallDynSlot {
                        callee,
                        interface,
                        slot,
                        arguments,
                    } if b.dyn_contexts => {
                        let item = crate::dynamic_ports::interface(module, *interface, b)?;
                        if let Some(c) = item
                            .slots
                            .get(*slot as usize)
                            .and_then(|slot| slot.storage_life.as_ref())
                        {
                            b.spend(arguments.len().saturating_add(1))?;
                            let mut args = vec![*callee];
                            args.extend(arguments);
                            call_with_outer(
                                c,
                                &args,
                                i.result,
                                &values,
                                &mut e,
                                outer.as_ref(),
                                b,
                            )?;
                        } else {
                            return Err(bad("dynamic call has no complete public class relation"));
                        }
                    }
                    InstructionKind::TakeVariantField { local, path } if b.dyn_contexts => {
                        if let (Some(out), [step]) = (out, path.as_slice())
                            && matches!(step, PlaceProjection::ErrorArm(_))
                            && !out.classes.is_empty()
                        {
                            link_projection(
                                &f.locals[local.0 as usize].ty,
                                &locals[local.0 as usize],
                                out,
                                LifeTypeProjection::Place(*step),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::TaskCreate {
                        arguments,
                        port_bindings,
                        ..
                    }
                    | InstructionKind::TaskCreateOwned {
                        arguments,
                        port_bindings,
                        ..
                    } if b.task_sources() => {
                        if let Some(out) = out {
                            if matches!(i.kind, InstructionKind::TaskCreateOwned { .. })
                                && b.task_failure_carriers
                                && let Some(Type::ErrorUnion { error, .. }) =
                                    i.result.and_then(|id| f.values.get(id.0 as usize))
                                && let Some(error_shape) = shape(error)
                            {
                                for slot in &error_shape.slots {
                                    let [
                                        PlaceProjection::Field(1),
                                        PlaceProjection::Field(argument),
                                    ] = slot.path.as_slice()
                                    else {
                                        return Err(bad(
                                            "failure class source has no actual input position",
                                        ));
                                    };
                                    let path = [LifeTypeProjection::Place(
                                        PlaceProjection::ErrorArm(true),
                                    )];
                                    let class = out
                                        .keys
                                        .iter()
                                        .position(|(p, c)| {
                                            p.as_slice() == path && *c == slot.binder
                                        })
                                        .ok_or_else(|| bad("failure actual class is absent"))?;
                                    let input = *arguments
                                        .get(*argument as usize)
                                        .ok_or_else(|| bad("failure actual input is absent"))?;
                                    let source =
                                        values[input.0 as usize].outer.ok_or_else(|| {
                                            bad("failure actual input Address class is absent")
                                        })?;
                                    e.equal(out.classes[class], source, b)?;
                                }
                            }
                            for row in port_bindings {
                                b.spend(out.task_ports.len())?;
                                let path = vec![
                                    zil_ir::ContractProjection::Place(PlaceProjection::ErrorArm(
                                        false,
                                    )),
                                    zil_ir::ContractProjection::RetainedPort(row.port),
                                ];
                                let index = out
                                    .task_ports
                                    .iter()
                                    .find(|(p, _)| p == &path)
                                    .map(|(_, i)| *i)
                                    .ok_or_else(|| {
                                        bad("Task constructor class omits actual port")
                                    })?;
                                let StorageBase::Parameter(input) = row.source.base else {
                                    return Err(bad("Task port class has no actual input"));
                                };
                                let arg = arguments
                                    .get(input as usize)
                                    .ok_or_else(|| bad("Task class input is absent"))?;
                                e.equal(
                                    out.classes[index],
                                    values[arg.0 as usize].outer.ok_or_else(|| {
                                        bad("Task input has no outer Address class")
                                    })?,
                                    b,
                                )?;
                            }
                        }
                    }
                    InstructionKind::TaskPoll { task }
                        if b.task_sources()
                            && matches!(super::tasks::task_payload(&f.values[task.0 as usize]),
                            Some(Type::Task { result, .. })
                                if matches!(result.as_ref(), Type::Address { mutable: false, .. })) =>
                    {
                        if let Some(out) = out
                            && !values[task.0 as usize].task_ports.is_empty()
                        {
                            let input = &values[task.0 as usize];
                            super::tasks::charge_result_selection(&f.values[task.0 as usize], b)?;
                            let port = input
                                .task_ports
                                .iter()
                                .find(|(p, _)| {
                                    matches!(p.as_slice(), [zil_ir::ContractProjection::RetainedPort(port)]
                                        if super::tasks::result_port(&f.values[task.0 as usize], *port))
                                })
                                .ok_or_else(|| bad("TaskPoll has no actual retained class"))?
                                .1;
                            let result = out
                                .keys
                                .iter()
                                .position(|(p, c)| {
                                    p == &vec![LifeTypeProjection::Place(
                                        PlaceProjection::ErrorArm(false),
                                    )] && *c == 0
                                })
                                .ok_or_else(|| bad("TaskPoll has no actual Ready class"))?;
                            e.equal(out.classes[result], input.classes[port], b)?;
                        }
                    }
                    InstructionKind::BorrowCapture { index, .. } if !captures.is_empty() => {
                        let source = captures
                            .get(*index as usize)
                            .ok_or_else(|| bad("borrowed capture class index is absent"))?;
                        let result = out.ok_or_else(|| bad("borrowed capture lacks result"))?;
                        e.ground_address(result.outer, b)?;
                        b.spend(source.classes.len().saturating_add(result.classes.len()))?;
                        if source.classes.len() != result.classes.len() {
                            return Err(bad(
                                "borrowed capture changes its complete child class domain",
                            ));
                        }
                        for (from, to) in source.classes.iter().zip(&result.classes) {
                            e.equal(*from, *to, b)?;
                        }
                    }
                    InstructionKind::ReadCapture { index } if !captures.is_empty() => {
                        let source = captures
                            .get(*index as usize)
                            .ok_or_else(|| bad("capture class index absent"))?;
                        if let Some(out) = out {
                            let exclusive = f
                                .environment
                                .and_then(|id| module.closures.get(id.0 as usize))
                                .and_then(|d| d.captures.get(*index as usize))
                                .is_some_and(|ty| {
                                    matches!(ty, Type::Address { mutable: true, .. })
                                });
                            if exclusive && let Some(region) = receiver_region {
                                b.spend(
                                    out.classes
                                        .len()
                                        .saturating_add(source.classes.len())
                                        .saturating_add(1),
                                )?;
                                if out.classes.len() != source.classes.len() {
                                    return Err(bad(
                                        "capture reborrow changes its complete pointee class domain",
                                    ));
                                }
                                e.equal(
                                    out.outer.ok_or_else(|| {
                                        bad("capture reborrow lacks an outer address")
                                    })?,
                                    region,
                                    b,
                                )?;
                                for (to, from) in out.classes.iter().zip(&source.classes) {
                                    e.equal(*to, *from, b)?;
                                }
                            } else {
                                array_outer::copy(&mut e, out, source, outer.as_ref(), b)?;
                            }
                        }
                    }
                    InstructionKind::BindPackageCapture { local, index }
                        if !captures.is_empty() =>
                    {
                        let source = captures
                            .get(*index as usize)
                            .ok_or_else(|| bad("owning capture view class index absent"))?;
                        array_outer::copy(
                            &mut e,
                            &locals[local.0 as usize],
                            source,
                            outer.as_ref(),
                            b,
                        )?;
                    }
                    InstructionKind::MakeClosure {
                        closure,
                        captures: actuals,
                    } => {
                        if let Some(c) = module
                            .closures
                            .get(closure.0 as usize)
                            .and_then(|d| d.storage_life.as_ref())
                        {
                            let fixed = &out
                                .ok_or_else(|| bad("environment construction has no value"))?
                                .classes;
                            b.spend(fixed.len())?;
                            let vars = (fixed.clone(), Vec::new());
                            for (index, actual) in actuals.iter().enumerate() {
                                bind_with_outer(
                                    c,
                                    StorageBase::Capture(index as u32),
                                    &values[actual.0 as usize],
                                    &vars,
                                    &mut e,
                                    outer.as_ref(),
                                    b,
                                )?;
                            }
                            if let Some(flow) = &outer {
                                flow.checkpoint(&mut e, b)?;
                            }
                            e.freeze(fixed, b)?;
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields }
                        if super::stored_flow::anonymous_operation(f, i, b)? =>
                    {
                        let actual = shape(&f.locals[local.0 as usize].ty)
                            .ok_or_else(|| bad("anonymous Tuple class directory absent"))?;
                        for (index, target) in fields.iter().enumerate() {
                            if let Some(to) = locals[target.0 as usize].outer {
                                let (class, _) = failure_view_slot(actual, &[PlaceProjection::Field(index as u32)], b)?
                                    .ok_or_else(|| bad("anonymous child has no exact parent slot class"))?;
                                let from = *locals[local.0 as usize].classes.get(class as usize)
                                    .ok_or_else(|| bad("anonymous parent class variable absent"))?;
                                array_outer::transport(&mut e, to, from, outer.as_ref(), b)?;
                            } else {
                                link_projection(&f.locals[local.0 as usize].ty, &locals[local.0 as usize],
                                    &locals[target.0 as usize], LifeTypeProjection::Place(PlaceProjection::Field(index as u32)), &mut e, b)?;
                            }
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields }
                        if super::stored_flow::compound_operation(f, i, b)? =>
                    {
                        for (index, target) in fields.iter().enumerate() {
                            link_projection(
                                &f.locals[local.0 as usize].ty,
                                &locals[local.0 as usize],
                                &locals[target.0 as usize],
                                LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields }
                        if super::stored_flow::failure_unpack(f, i, b)? =>
                    {
                        let parent_shape = shape(&f.locals[local.0 as usize].ty)
                            .ok_or_else(|| bad("failure tuple class source is absent"))?;
                        for (position, target) in fields.iter().enumerate() {
                            if let Some(to) = locals[target.0 as usize].outer {
                                let source = failure_view_slot(
                                    parent_shape,
                                    &[PlaceProjection::Field(position as u32)],
                                    b,
                                )?
                                .ok_or_else(|| {
                                    bad("failure tuple field has no actual parent class")
                                })?
                                .0;
                                let from = *locals[local.0 as usize]
                                    .classes
                                    .get(source as usize)
                                    .ok_or_else(|| {
                                    bad("failure tuple parent class is absent")
                                })?;
                                array_outer::transport(&mut e, to, from, outer.as_ref(), b)?;
                            }
                        }
                    }
                    InstructionKind::TakeVariantField { local, path }
                        if b.task_failure_carriers
                            && path.as_slice() == [PlaceProjection::ErrorArm(true)] =>
                    {
                        if let Some(child) = out {
                            link_projection(
                                &f.locals[local.0 as usize].ty,
                                &locals[local.0 as usize],
                                child,
                                LifeTypeProjection::Place(PlaceProjection::ErrorArm(true)),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::TakeErrorUnion {
                        value,
                        is_error: true,
                    } if b.task_failure_carriers => {
                        if let Some(child) = out {
                            link_projection(
                                &f.values[value.0 as usize],
                                &values[value.0 as usize],
                                child,
                                LifeTypeProjection::Place(PlaceProjection::ErrorArm(true)),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        if let Some(out) = out {
                            array_outer::copy(
                                &mut e,
                                out,
                                &locals[local.0 as usize],
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                    InstructionKind::Store { local, value } => array_outer::copy(
                        &mut e,
                        &locals[local.0 as usize],
                        &values[value.0 as usize],
                        outer.as_ref(),
                        b,
                    )?,
                    InstructionKind::BorrowPackageField { .. }
                        if super::stored_flow::qualified_package_borrow(f, i, b)? =>
                    {
                        if let Some(out) = out {
                            e.ground_address(out.outer, b)?;
                        }
                    }
                    InstructionKind::BorrowPlaceLocal { .. }
                        if super::stored_flow::ordinary_error_borrow(f, i, b)? =>
                    {
                        if let Some(out) = out {
                            e.ground_address(out.outer, b)?;
                        }
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        if let Some(out) = out {
                            e.ground_address(out.outer, b)?;
                            let stored = &locals[local.0 as usize];
                            array_outer::copy_elements(&mut e, out, stored, outer.as_ref(), b)?;
                            b.spend(out.classes.len().saturating_add(stored.classes.len()))?;
                            if out.classes.len() != stored.classes.len() {
                                return Err(bad("borrowed actual class arity differs"));
                            }
                            for (a, z) in out.classes.iter().zip(&stored.classes) {
                                e.equal(*a, *z, b)?;
                            }
                        }
                    }
                    InstructionKind::BorrowDynOwned { local, .. } => {
                        if let Some(out) = out {
                            array_outer::copy(
                                &mut e,
                                out,
                                &locals[local.0 as usize],
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                    InstructionKind::ReborrowAddress { address, .. } => {
                        if let Some(out) = out {
                            array_outer::copy(
                                &mut e,
                                out,
                                &values[address.0 as usize],
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                    InstructionKind::AddressIndex { address, .. } => {
                        if let Some(out) = out {
                            let parent = &values[address.0 as usize];
                            if let (Some(a), Some(z)) = (out.outer, parent.outer) {
                                array_outer::transport(&mut e, a, z, outer.as_ref(), b)?;
                            }
                            link_projection(
                                &f.values[address.0 as usize],
                                parent,
                                out,
                                LifeTypeProjection::Element,
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::AddressField { address, index } => {
                        if let Some(out) = out {
                            let parent = &values[address.0 as usize];
                            address_field(
                                f,
                                *address,
                                *index,
                                parent,
                                out,
                                &mut e,
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                    InstructionKind::AddressSlice { address, .. } => {
                        if let (Some(out), Some(parent)) =
                            (out.and_then(|o| o.outer), values[address.0 as usize].outer)
                        {
                            array_outer::transport(&mut e, out, parent, outer.as_ref(), b)?;
                        }
                    }
                    InstructionKind::StoreStoredBorrow {
                        storage,
                        path,
                        value,
                    } => {
                        let parent = &values[storage.0 as usize];
                        let expected = if !parent.array_elements.is_empty() {
                            let [PlaceProjection::Field(index)] = path.as_slice() else {
                                return Err(bad("array write lacks an actual constant ordinal"));
                            };
                            *parent
                                .array_elements
                                .get(*index as usize)
                                .ok_or_else(|| bad("array write ordinal is absent"))?
                        } else {
                            let actual = shape(&f.values[storage.0 as usize]).ok_or_else(|| {
                                bad("stored write lacks its complete actual class shape")
                            })?;
                            b.spend(
                                actual
                                    .slots
                                    .len()
                                    .saturating_mul(path.len().saturating_add(1)),
                            )?;
                            let slot = actual
                                .slots
                                .iter()
                                .find(|slot| slot.path == *path)
                                .ok_or_else(|| bad("stored write selects no actual class slot"))?;
                            *parent
                                .classes
                                .get(slot.binder as usize)
                                .ok_or_else(|| bad("stored write class variable is absent"))?
                        };
                        let source = values[value.0 as usize].outer.ok_or_else(|| {
                            bad("stored write replacement lacks an actual address class")
                        })?;
                        array_outer::binding(&mut e, source, expected, outer.as_ref(), b)?;
                    }
                    InstructionKind::TakePackageField { local, index: 1 }
                        if b.task_failure_carriers =>
                    {
                        if let Some(child_ty) = super::stored_flow::failure_projection(f, i, b)? {
                            let parent_shape =
                                shape(&f.locals[local.0 as usize].ty).ok_or_else(|| {
                                    bad("failure Product projection lacks actual parent")
                                })?;
                            let child_shape = shape(&child_ty).ok_or_else(|| {
                                bad("failure Product projection lacks actual child")
                            })?;
                            let child =
                                out.ok_or_else(|| bad("failure Product projection lacks result"))?;
                            for row in &child_shape.slots {
                                b.spend(row.path.len().saturating_add(2))?;
                                let mut actual_path = vec![PlaceProjection::Field(1)];
                                actual_path.extend(&row.path);
                                let source = failure_view_slot(parent_shape, &actual_path, b)?
                                    .ok_or_else(|| {
                                        bad("failure Product projection omitted original slot")
                                    })?
                                    .0;
                                let from = *locals[local.0 as usize]
                                    .classes
                                    .get(source as usize)
                                    .ok_or_else(|| {
                                    bad("failure Product parent class absent")
                                })?;
                                let to = *child
                                    .classes
                                    .get(row.binder as usize)
                                    .ok_or_else(|| bad("failure Product child class absent"))?;
                                array_outer::transport(&mut e, to, from, outer.as_ref(), b)?;
                            }
                        }
                    }
                    InstructionKind::LoadStoredBorrow { storage, path, .. } => {
                        if !values[storage.0 as usize].array_elements.is_empty() {
                            let [PlaceProjection::Field(index)] = path.as_slice() else {
                                return Err(bad("array load has no actual constant ordinal"));
                            };
                            let class = *values[storage.0 as usize]
                                .array_elements
                                .get(*index as usize)
                                .ok_or_else(|| bad("array load selects absent actual ordinal"))?;
                            array_outer::transport(
                                &mut e,
                                out.and_then(|o| o.outer)
                                    .ok_or_else(|| bad("array load lacks Address result"))?,
                                class,
                                outer.as_ref(),
                                b,
                            )?;
                            continue;
                        }
                        if let Some(child_ty) = super::stored_flow::failure_projection(f, i, b)? {
                            let parent_shape = shape(&f.values[storage.0 as usize])
                                .ok_or_else(|| bad("failure class projection parent is absent"))?;
                            let child_shape = shape(&child_ty)
                                .ok_or_else(|| bad("failure class projection child is absent"))?;
                            let child = out
                                .ok_or_else(|| bad("failure class projection result is absent"))?;
                            for selected in &child_shape.slots {
                                let mut actual_path = path.clone();
                                actual_path.extend(&selected.path);
                                let source = failure_view_slot(parent_shape, &actual_path, b)?
                                    .ok_or_else(|| {
                                        bad("failure child class has no exact parent input slot")
                                    })?
                                    .0;
                                let to = *child
                                    .classes
                                    .get(selected.binder as usize)
                                    .ok_or_else(|| bad("failure child class variable is absent"))?;
                                let from = *values[storage.0 as usize]
                                    .classes
                                    .get(source as usize)
                                    .ok_or_else(|| {
                                        bad("failure parent class variable is absent")
                                    })?;
                                array_outer::transport(&mut e, to, from, outer.as_ref(), b)?;
                            }
                            continue;
                        }
                        let actual = shape(&f.values[storage.0 as usize])
                            .ok_or_else(|| bad("stored load lacks actual class shape"))?;
                        b.spend(
                            actual
                                .slots
                                .len()
                                .saturating_mul(path.len().saturating_add(1)),
                        )?;
                        let leaf = actual
                            .slots
                            .iter()
                            .find(|s| s.path == *path)
                            .ok_or_else(|| bad("stored class slot is absent"))?;
                        let class = values[storage.0 as usize].classes[leaf.binder as usize];
                        array_outer::transport(
                            &mut e,
                            out.and_then(|o| o.outer)
                                .ok_or_else(|| bad("stored class load lacks Address result"))?,
                            class,
                            outer.as_ref(),
                            b,
                        )?;
                    }
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeOwnedTuple { fields } => {
                        if let Some(out) = out
                            && !out.array_elements.is_empty()
                        {
                            b.spend(fields.len())?;
                            if fields.len() != out.array_elements.len() {
                                return Err(bad("array constructor address upper arity differs"));
                            }
                            for (upper, argument) in out.array_elements.iter().zip(fields) {
                                array_outer::transport(
                                    &mut e,
                                    *upper,
                                    values[argument.0 as usize].outer.ok_or_else(|| {
                                        bad("array constructor operand is not an Address")
                                    })?,
                                    outer.as_ref(),
                                    b,
                                )?;
                            }
                        }

                        if let (Some(out), Some(id)) = (out, i.result)
                            && let Some(actual) = shape(&f.values[id.0 as usize])
                        {
                            for slot in &actual.slots {
                                b.spend(slot.path.len().saturating_add(1))?;
                                let (PlaceProjection::Field(index), tail) =
                                    slot.path.split_first().ok_or_else(|| {
                                        bad("stored constructor slot lacks an actual field")
                                    })?
                                else {
                                    return Err(bad(
                                        "stored constructor slot is not an inline field",
                                    ));
                                };
                                let arg = fields
                                    .get(*index as usize)
                                    .ok_or_else(|| bad("aggregate class operand is absent"))?;
                                let operand = &values[arg.0 as usize];
                                let upper = if !tail.is_empty() && matches!(f.values[arg.0 as usize], Type::BorrowedStorage { .. }) {
                                    let actual_child = shape(&f.values[arg.0 as usize]).ok_or_else(||bad("nested constructor lacks actual child shape"))?;
                                    let (class, _) = failure_view_slot(actual_child, tail, b)?.ok_or_else(||bad("nested constructor has no exact physical child slot"))?;
                                    *operand.classes.get(class as usize).ok_or_else(||bad("nested constructor child class absent"))?
                                } else { match tail {
                                [] => operand.outer.ok_or_else(|| bad("stored class operand is not an Address"))?,
                                [PlaceProjection::Field(ordinal)] => *operand.array_elements.get(*ordinal as usize)
                                    .ok_or_else(|| bad("stored Array class operand lacks its actual ordinal"))?,
                                _ => return Err(bad("nested constructor class requires its physical operand consumer")),
                            }};
                                array_outer::binding(
                                    &mut e,
                                    upper,
                                    out.classes[slot.binder as usize],
                                    outer.as_ref(),
                                    b,
                                )?;
                            }
                        }
                        if let (Some(out), Some(id)) = (out, i.result)
                            && !out.classes.is_empty()
                            && !matches!(shape(&f.values[id.0 as usize]), Some(life) if life.retained.is_empty())
                        {
                            let parent_ty = &f.values[id.0 as usize];
                            for (index, field) in fields.iter().enumerate() {
                                let step = if matches!(parent_ty, Type::Array { .. }) {
                                    LifeTypeProjection::Element
                                } else {
                                    LifeTypeProjection::Place(PlaceProjection::Field(index as u32))
                                };
                                link_projection(
                                    parent_ty,
                                    out,
                                    &values[field.0 as usize],
                                    step,
                                    &mut e,
                                    b,
                                )?;
                            }
                        }
                    }
                    InstructionKind::ReadAddress { address } => {
                        if let Some(out) = out {
                            let input = &values[address.0 as usize];
                            array_outer::copy_elements(&mut e, out, input, outer.as_ref(), b)?;
                            if out.classes.len() != input.classes.len() {
                                return Err(bad("pure projection class arity differs"));
                            }
                            for (a, z) in out.classes.iter().zip(&input.classes) {
                                e.equal(*a, *z, b)?;
                            }
                        }
                    }
                    InstructionKind::ExtractIndex { aggregate, .. } => {
                        if let Some(out) = out {
                            if super::stored_flow::shared_array_value(
                                &f.values[aggregate.0 as usize],
                            ) {
                                let upper = out.outer.ok_or_else(|| {
                                    bad("dynamic shared Array result lacks an Address upper")
                                })?;
                                let elements = &values[aggregate.0 as usize].array_elements;
                                let Type::Array { length, .. } = &f.values[aggregate.0 as usize]
                                else {
                                    unreachable!()
                                };
                                b.spend(elements.len().saturating_add(1))?;
                                if elements.len() != *length as usize {
                                    return Err(bad(
                                        "dynamic Array upper facts lack a physical candidate",
                                    ));
                                }
                                if outer.is_none() {
                                    for element in elements {
                                        e.equal(upper, *element, b)?;
                                    }
                                }
                            } else {
                                link_projection(
                                    &f.values[aggregate.0 as usize],
                                    &values[aggregate.0 as usize],
                                    out,
                                    LifeTypeProjection::Element,
                                    &mut e,
                                    b,
                                )?;
                            }
                        }
                    }
                    InstructionKind::ExtractField { aggregate, index } => {
                        if let Some(out) = out {
                            if super::stored_flow::anonymous_operation(f, i, b)? {
                                if let Some(to) = out.outer {
                                    let actual = shape(&f.values[aggregate.0 as usize])
                                        .ok_or_else(|| bad("anonymous snapshot class directory absent"))?;
                                    let (class, _) = failure_view_slot(actual, &[PlaceProjection::Field(*index)], b)?
                                        .ok_or_else(|| bad("anonymous snapshot has no exact actual slot class"))?;
                                    let from = *values[aggregate.0 as usize].classes.get(class as usize)
                                        .ok_or_else(|| bad("anonymous snapshot parent class variable absent"))?;
                                    array_outer::transport(&mut e, to, from, outer.as_ref(), b)?;
                                } else {
                                    link_projection(&f.values[aggregate.0 as usize], &values[aggregate.0 as usize], out,
                                        LifeTypeProjection::Place(PlaceProjection::Field(*index)), &mut e, b)?;
                                }
                            } else { link_projection(
                                &f.values[aggregate.0 as usize],
                                &values[aggregate.0 as usize],
                                out,
                                LifeTypeProjection::Place(PlaceProjection::Field(*index)),
                                &mut e,
                                b,
                            )?; }
                        }
                    }
                    InstructionKind::MakeErrorUnion { value, is_error }
                        if b.stored_enum_addresses || b.dyn_contexts =>
                    {
                        if let (Some(out), Some(id)) = (out, i.result)
                            && (super::stored_flow::enum_address::signature(
                                &f.values[id.0 as usize],
                            ) || b.task_sources() && !out.task_ports.is_empty()
                                || b.dyn_contexts && !out.classes.is_empty())
                        {
                            link_projection(
                                &f.values[id.0 as usize],
                                out,
                                &values[value.0 as usize],
                                LifeTypeProjection::Place(PlaceProjection::ErrorArm(*is_error)),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::ReadErrorUnion { value, is_error }
                    | InstructionKind::TakeErrorUnion { value, is_error }
                        if b.stored_enum_addresses || b.dyn_contexts =>
                    {
                        if let Some(out) = out
                            && (super::stored_flow::enum_address::signature(
                                &f.values[value.0 as usize],
                            ) || b.dyn_contexts && !values[value.0 as usize].classes.is_empty())
                        {
                            link_projection(
                                &f.values[value.0 as usize],
                                &values[value.0 as usize],
                                out,
                                LifeTypeProjection::Place(PlaceProjection::ErrorArm(*is_error)),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::ReadVariant {
                        value,
                        variant,
                        index,
                    } => {
                        if let Some(out) = out {
                            link_projection(
                                &f.values[value.0 as usize],
                                &values[value.0 as usize],
                                out,
                                LifeTypeProjection::Place(PlaceProjection::Variant(
                                    zil_ir::VariantProjection {
                                        variant: *variant,
                                        index: *index,
                                    },
                                )),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::MakeVariant { variant, fields } => {
                        if let (Some(out), Some(id)) = (out, i.result)
                            && !out.classes.is_empty()
                            && !matches!(shape(&f.values[id.0 as usize]), Some(life) if life.retained.is_empty())
                        {
                            for (index, field) in fields.iter().enumerate() {
                                link_projection(
                                    &f.values[id.0 as usize],
                                    out,
                                    &values[field.0 as usize],
                                    LifeTypeProjection::Place(PlaceProjection::Variant(
                                        zil_ir::VariantProjection {
                                            variant: *variant,
                                            index: index as u32,
                                        },
                                    )),
                                    &mut e,
                                    b,
                                )?;
                            }
                        }
                    }
                    InstructionKind::UpdateField {
                        aggregate,
                        index,
                        value,
                    } => {
                        if let Some(out) = out {
                            array_outer::copy(
                                &mut e,
                                out,
                                &values[aggregate.0 as usize],
                                outer.as_ref(),
                                b,
                            )?;
                            link_projection(
                                &f.values[aggregate.0 as usize],
                                out,
                                &values[value.0 as usize],
                                LifeTypeProjection::Place(PlaceProjection::Field(*index)),
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::UpdateIndex {
                        aggregate, value, ..
                    } => {
                        if let Some(out) = out {
                            array_outer::copy(
                                &mut e,
                                out,
                                &values[aggregate.0 as usize],
                                outer.as_ref(),
                                b,
                            )?;
                            link_projection(
                                &f.values[aggregate.0 as usize],
                                out,
                                &values[value.0 as usize],
                                LifeTypeProjection::Element,
                                &mut e,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let callee = &module.functions[function.0 as usize];
                        if let Some(c) = &callee.storage_life {
                            call_with_outer(
                                c,
                                arguments,
                                i.result,
                                &values,
                                &mut e,
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                    InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::CallClosureOnce { .. } => {
                        let (d, arguments) = super::stored_callables::called(module, f, &i.kind)
                            .ok_or_else(|| bad("call has no actual qualified closure"))?;
                        // A complete environment relation is instantiated from
                        // this actual receiver's class variables. The helper
                        // kernel separately proves role provenance and traces;
                        // selecting a role by a moved local or ClosureId would
                        // confuse distinct instances of the same entry.
                        let role = if d.storage_life.is_some() {
                            None
                        } else {
                            super::stored_callables::role(module, f, &i.kind, d, b)?
                        };
                        if let Some(definition) = &d.storage_life {
                            if let Some(role) = role {
                                super::signature_life::charge(role, b)?;
                            }
                            super::signature_life::charge(definition, b)?;
                            if role.is_some_and(|r| r != definition) {
                                return Err(unsupported(
                                    "captured Fixed role requires its actual capture map",
                                    Some(f.span),
                                ));
                            }
                            let receiver = match &i.kind {
                                InstructionKind::CallClosureShared { callee, .. }
                                | InstructionKind::CallClosureMut { callee, .. } => {
                                    &locals[callee.0 as usize]
                                }
                                InstructionKind::CallClosureBorrowed { callee, .. }
                                | InstructionKind::CallClosure { callee, .. }
                                | InstructionKind::CallClosureOnce { callee, .. } => {
                                    &values[callee.0 as usize]
                                }
                                _ => unreachable!(),
                            };
                            b.spend(receiver.classes.len())?;
                            let mut vars = (receiver.classes.clone(), Vec::new());
                            for _ in 0..definition.invocation_binder_count {
                                vars.1.push(e.fresh(false, b)?);
                            }
                            if let Some(binder) =
                                crate::signature_life::outer(definition, StorageBase::Receiver)
                            {
                                let actual = select(&vars, binder)?;
                                if let Some(head) = receiver.outer {
                                    e.equal(actual, head, b)?;
                                } else {
                                    e.ground_address(Some(actual), b)?;
                                }
                            }
                            for (index, argument) in arguments.iter().enumerate() {
                                bind_with_outer(
                                    definition,
                                    StorageBase::Parameter(index as u32),
                                    &values[argument.0 as usize],
                                    &vars,
                                    &mut e,
                                    outer.as_ref(),
                                    b,
                                )?;
                            }
                            if let Some(output) = i.result {
                                bind_with_outer(
                                    definition,
                                    StorageBase::Result,
                                    &values[output.0 as usize],
                                    &vars,
                                    &mut e,
                                    outer.as_ref(),
                                    b,
                                )?;
                            }
                        } else if let Some(life) = role {
                            call_with_outer(
                                life,
                                arguments,
                                i.result,
                                &values,
                                &mut e,
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                    InstructionKind::CallIndirect { callee, arguments } => {
                        let Type::BorrowedFunctionPointer { life, .. } =
                            &f.values[callee.0 as usize]
                        else {
                            return Err(bad(
                                "stored indirect class call requires its complete borrowed signature",
                            ));
                        };
                        call_with_outer(
                            life,
                            arguments,
                            i.result,
                            &values,
                            &mut e,
                            outer.as_ref(),
                            b,
                        )?;
                    }
                    _ => {}
                }
            }
        }
    }
    if let (Some(c), Some(vars)) = (assumptions, header) {
        for (block_index, (block, live)) in f.blocks.iter().zip(reachable).enumerate() {
            if *live && let Terminator::Return(Some(v)) = block.terminator {
                if let Some(flow) = &outer {
                    flow.enter(usize::MAX, block_index);
                }
                bind_with_outer(
                    c,
                    StorageBase::Result,
                    &values[v.0 as usize],
                    &vars,
                    &mut e,
                    outer.as_ref(),
                    b,
                )?;
            }
        }
    }
    if let Some(flow) = &outer {
        flow.finish(&mut e, b)?;
    }
    Ok(())
}

/// The first rich executable pointer lane returns ordinary Copy values. Actual
/// returned-target substitution remains closed here until separately proved.
fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64
    )
}
pub(super) fn pointer_instruction(f: &TypedFunction, i: &Instruction) -> bool {
    match &i.kind {
        InstructionKind::FunctionRef { .. } => i
            .result
            .and_then(|v| f.values.get(v.0 as usize))
            .is_some_and(|t| matches!(t,Type::BorrowedFunctionPointer{result,..}if scalar(result))),
        InstructionKind::CallIndirect { callee, .. } => f
            .values
            .get(callee.0 as usize)
            .is_some_and(|t| matches!(t,Type::BorrowedFunctionPointer{result,..}if scalar(result))),
        _ => false,
    }
}

pub(super) fn pointer_types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    match &i.kind {
        InstructionKind::FunctionRef { function } => {
            let ty = i
                .result
                .and_then(|v| f.values.get(v.0 as usize))
                .ok_or_else(|| bad("function reference lacks actual result"))?;
            let Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } = ty
            else {
                return Err(bad("stored pointer producer needs full borrowed signature"));
            };
            let target = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| bad("pointer target is absent"))?;
            b.spend(target.parameters.len().saturating_add(1))?;
            super::borrowed_fp::signature_work(parameters, result, b)?;
            if target.foreign.is_some()
                || target.environment.is_some()
                || target.call_kind != CallKind::Direct
                || target.callable_contract.is_some()
                || target.parameters.len() != parameters.len()
                || target.return_type != **result
            {
                return Err(bad("rich pointer target complete signature/effect differs"));
            }
            for (p, ty) in target.parameters.iter().zip(parameters) {
                if target.locals.get(p.0 as usize).map(|p| &p.ty) != Some(ty) {
                    return Err(bad("rich pointer target parameter differs"));
                }
            }
            if let Some(required) = &target.storage_life {
                super::signature_life::charge(required, b)?;
                super::signature_life::charge(life, b)?;
                if !super::signature_life::entails(required, life) {
                    return Err(bad(
                        "pointer relation does not entail actual target class requirements",
                    ));
                }
            } else if !life.classes.is_empty() {
                return Err(bad(
                    "qualified pointer target omits its actual class contract",
                ));
            }
        }
        InstructionKind::CallIndirect { callee, arguments } => {
            let Type::BorrowedFunctionPointer {
                parameters, result, ..
            } = &f.values[callee.0 as usize]
            else {
                return Err(bad("indirect call lacks rich signature"));
            };
            if parameters.len() != arguments.len() {
                return Err(bad("indirect call arity differs"));
            }
            for (p, v) in parameters.iter().zip(arguments) {
                resources::charge_payload(p, b)?;
                resources::charge_payload(p, b)?;
                if f.values.get(v.0 as usize) != Some(p) {
                    return Err(bad("indirect call actual argument differs"));
                }
            }
            resources::charge_payload(result, b)?;
            resources::charge_payload(result, b)?;
            if **result == Type::Unit {
                if i.result.is_some() {
                    return Err(bad("Unit indirect call fabricates result"));
                }
            } else if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(result.as_ref()) {
                return Err(bad("rich indirect result differs"));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod equation_tests {
    use super::*;

    #[test]
    fn merge_prepays_copying_a_full_larger_component() {
        let mut b = Budget::new(&Limits::default());
        let mut e = Equations {
            representative: Vec::new(),
            members: Some(Vec::new()),
            rigid: Vec::new(),
            grounded: None,
            next_rigid: 0,
        };
        let a = e.fresh(false, &mut b).unwrap();
        let z = e.fresh(false, &mut b).unwrap();
        e.equal(a, z, &mut b).unwrap();
        let extra = e.fresh(false, &mut b).unwrap();
        e.members.as_mut().unwrap()[a].shrink_to_fit();
        // Representative reads/compare (3), sizes (2), capacity (1), one
        // transferred row (3), take (1), and copying two existing rows (2).
        let mut short = Budget::new(&Limits {
            max_analysis_steps: 11,
            ..Limits::default()
        });
        assert!(e.equal(a, extra, &mut short).is_err());
        assert_eq!(e.representative[extra], extra);
        let mut exact = Budget::new(&Limits {
            max_analysis_steps: 12,
            ..Limits::default()
        });
        e.equal(a, extra, &mut exact).unwrap();
        assert_eq!(e.representative[extra], e.representative[z]);
    }

    #[test]
    fn member_substitution_preserves_rigid_conflicts_and_actual_grounding() {
        let mut b = Budget::new(&Limits::default());
        let mut e = Equations {
            representative: Vec::new(),
            members: Some(Vec::new()),
            rigid: Vec::new(),
            grounded: None,
            next_rigid: 0,
        };
        let a = e.fresh(false, &mut b).unwrap();
        let z = e.fresh(false, &mut b).unwrap();
        e.equal(a, z, &mut b).unwrap();
        let rigid = e.fresh(true, &mut b).unwrap();
        e.track_grounding(&mut b).unwrap();
        e.equal(rigid, a, &mut b).unwrap();
        e.freeze(&[z], &mut b).unwrap();
        let other = e.fresh(true, &mut b).unwrap();
        assert!(e.equal(z, other, &mut b).is_err());

        let x = e.fresh(false, &mut b).unwrap();
        let y = e.fresh(false, &mut b).unwrap();
        e.equal(x, y, &mut b).unwrap();
        assert!(e.freeze(&[y], &mut b).is_err());
        e.ground_address(Some(y), &mut b).unwrap();
        e.freeze(&[y], &mut b).unwrap();
        assert!(e.equal(x, a, &mut b).is_err());
    }
}
