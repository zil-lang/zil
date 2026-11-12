//! Signature equations are invariant metadata. This pass grants no roots,
//! storage initialization, parent loans, effects or physical access rights.
use super::*;
mod array_outer;

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
) -> CompileResult<()> {
    if path.len() > b.partial_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "actual pure class path exceeds type depth",
            None,
        )]);
    }
    b.spend(1, "actual pure composite class inventory")?;
    match ty {
        Type::BorrowedStorage { life, .. } => {
            for class in 0..life.binder_count {
                b.spend(path.len().saturating_add(1), "actual pure class path")?;
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
struct Equations {
    parent: Vec<usize>,
    // Shared address arrays introduce one variable per ordinal. Balance their
    // equations and charge actual ancestry instead of the whole arena bound.
    sizes: Option<Vec<usize>>,
    rigid: Vec<Option<usize>>,
    next_rigid: usize,
    grounded: Option<Vec<bool>>,
}

impl Equations {
    fn fresh(&mut self, rigid: bool, b: &mut Budget) -> CompileResult<usize> {
        b.spend(
            if self.grounded.is_some() { 4 } else { 3 },
            "class variable allocation",
        )?;
        let i = self.parent.len();
        if let Some(sizes) = &mut self.sizes {
            b.spend(1, "array class component allocation")?;
            sizes.push(1);
        }
        self.parent.push(i);
        if let Some(grounded) = &mut self.grounded {
            grounded.push(rigid);
        }
        self.rigid.push(if rigid {
            let id = self.next_rigid;
            self.next_rigid += 1;
            Some(id)
        } else {
            None
        });
        Ok(i)
    }
    fn root(&self, mut i: usize, b: &mut Budget) -> CompileResult<usize> {
        if self.sizes.is_some() {
            loop {
                b.spend(1, "actual array class ancestry")?;
                let parent = self.parent[i];
                if parent == i {
                    return Ok(i);
                }
                i = parent;
            }
        }
        b.spend(
            self.parent.len().saturating_add(1),
            "class variable ancestry",
        )?;
        while self.parent[i] != i {
            i = self.parent[i];
        }
        Ok(i)
    }
    fn equal(&mut self, a: usize, z: usize, b: &mut Budget) -> CompileResult<()> {
        let mut a = self.root(a, b)?;
        let mut z = self.root(z, b)?;
        if a == z {
            return Ok(());
        }
        if let (Some(a), Some(z)) = (self.rigid[a], self.rigid[z])
            && a != z
        {
            return Err(bad(
                "actual retained classes do not satisfy this invocation relation",
            ));
        }
        if let Some(sizes) = &mut self.sizes {
            b.spend(3, "array class component merge")?;
            if sizes[a] < sizes[z] {
                std::mem::swap(&mut a, &mut z);
            }
            sizes[a] += sizes[z];
        }
        if let Some(grounded) = &mut self.grounded {
            b.spend(1, "actual class grounding propagation")?;
            grounded[a] |= grounded[z];
        }
        self.parent[z] = a;
        self.rigid[a] = self.rigid[a].or(self.rigid[z]);
        Ok(())
    }
    fn address_origin(&mut self, variable: usize, b: &mut Budget) -> CompileResult<()> {
        if self.grounded.is_some() {
            let root = self.root(variable, b)?;
            b.spend(1, "actual local Address class origin")?;
            if let Some(grounded) = &mut self.grounded {
                grounded[root] = true;
            }
        }
        Ok(())
    }
    fn retain(&mut self, variable: usize, b: &mut Budget) -> CompileResult<()> {
        let root = self.root(variable, b)?;
        b.spend(1, "freeze actual environment class")?;
        if !self.grounded.as_ref().is_some_and(|g| g[root]) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "captured Fixed class has no actual header or Address origin",
                None,
            )]);
        }
        if self.rigid[root].is_none() {
            self.rigid[root] = Some(self.next_rigid);
            self.next_rigid += 1;
        }
        Ok(())
    }
    fn ports(&mut self, module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<Ports> {
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
        let mut array_elements = Vec::new();
        if super::stored_flow::shared_array_value(ty) {
            if self.sizes.is_none() {
                b.spend(self.parent.len(), "array class component table")?;
                let mut sizes = vec![0; self.parent.len()];
                for i in 0..self.parent.len() {
                    let mut root = i;
                    loop {
                        b.spend(1, "array class component inventory")?;
                        let parent = self.parent[root];
                        if root == parent {
                            break;
                        }
                        root = parent;
                    }
                    b.spend(1, "array class component size")?;
                    sizes[root] += 1;
                }
                self.sizes = Some(sizes);
            }
            let Type::Array { length, .. } = ty else {
                unreachable!()
            };
            let count = usize::try_from(*length).map_err(|_| bad("array class count overflow"))?;
            b.spend(count, "array address upper variable rows")?;
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
                .map_or(0, |life| life.fixed_binder_count);
            b.spend(count as usize, "class port allocation")?;
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
            b.spend(life.binder_count as usize, "class port allocation")?;
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
            b.spend(usize::from(outer.is_some()), "pure class pointee path")?;
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
            ) -> CompileResult<()> {
                b.spend(1, "actual Task semantic class")?;
                match ty {
                    Type::Task { retained_life, .. } => {
                        super::tasks::charge_result_context(ty, b)?;
                        super::tasks::require_closed_context(ty)?;
                        for class in 0..retained_life.binder_count {
                            keys.push((cp.clone(), class));
                        }
                        for (port, row) in retained_life.ports.iter().enumerate() {
                            b.spend(1, "actual Task semantic class")?;
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
        let count = keys.len();
        if !direct {
            b.spend(count, "class port allocation")?;
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
            environment: matches!(ty, Type::Closure(_)) && count != 0,
            task_ports,
            array_elements,
        })
    }
    fn copy_elements(&mut self, a: &Ports, z: &Ports, b: &mut Budget) -> CompileResult<()> {
        if a.array_elements.is_empty() && z.array_elements.is_empty() {
            return Ok(());
        }
        b.spend(
            a.array_elements
                .len()
                .saturating_add(z.array_elements.len()),
            "array address upper propagation",
        )?;
        if a.array_elements.len() != z.array_elements.len() {
            return Err(bad("array address upper arity differs"));
        }
        for (a, z) in a.array_elements.iter().zip(&z.array_elements) {
            self.equal(*a, *z, b)?;
        }
        Ok(())
    }
    fn copy(&mut self, a: &Ports, z: &Ports, b: &mut Budget) -> CompileResult<()> {
        self.copy_elements(a, z, b)?;
        b.spend(
            a.classes
                .len()
                .saturating_add(z.classes.len())
                .saturating_add(1),
            "class port propagation",
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
) -> CompileResult<()> {
    if child.keys.is_empty() {
        return Ok(());
    }
    b.spend(
        usize::from(matches!(parent_ty, Type::Address { .. })),
        "projected class pointee prefix",
    )?;
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
        b.spend(scan, "actual projected lookup fee computation")?;
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
        b.spend(
            lookup_work.saturating_sub(scan),
            "actual projected class lookup",
        )?;
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
) -> CompileResult<(Vec<usize>, Vec<usize>)> {
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
fn select(v: &(Vec<usize>, Vec<usize>), binder: StorageBinder) -> CompileResult<usize> {
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
) -> CompileResult<()> {
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
                b.spend(p.task_ports.len(), "actual Task semantic class")?;
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
            b.spend(p.keys.len(), "actual signature lookup fee computation")?;
            let lookup_work = p.keys.iter().fold(0usize, |n, (path, _)| {
                n.saturating_add(
                    path.len()
                        .saturating_add(row.position.path.len())
                        .saturating_add(1),
                )
            });
            b.spend(
                lookup_work.saturating_sub(p.keys.len()),
                "actual signature class path selection",
            )?;
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
) -> CompileResult<()> {
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
                b.spend(p.task_ports.len(), "actual Task semantic class")?;
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
            b.spend(p.keys.len(), "actual signature lookup fee computation")?;
            let lookup_work = p.keys.iter().fold(0usize, |n, (path, _)| {
                n.saturating_add(
                    path.len()
                        .saturating_add(row.position.path.len())
                        .saturating_add(1),
                )
            });
            b.spend(
                lookup_work.saturating_sub(p.keys.len()),
                "actual signature class path selection",
            )?;
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
    fixed: Option<&[usize]>,
    e: &mut Equations,
    b: &mut Budget,
) -> CompileResult<()> {
    let formal = if let Some(fixed) = fixed {
        crate::signature_life::charge(c, b)?;
        b.spend(fixed.len(), "actual closure persistent Fixed snapshot")?;
        if fixed.len() != c.fixed_binder_count as usize {
            return Err(bad(
                "actual closure free Fixed domain differs from call relation",
            ));
        }
        let mut invocation = Vec::new();
        for _ in 0..c.invocation_binder_count {
            invocation.push(e.fresh(false, b)?);
        }
        (fixed.to_vec(), invocation)
    } else {
        variables(c, e, false, b)?
    };
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
    fixed: Option<&[usize]>,
    context: (&mut Equations, Option<&array_outer::Flow>),
    b: &mut Budget,
) -> CompileResult<()> {
    let (e, flow) = context;
    if flow.is_none() {
        return call(c, args, output, values, fixed, e, b);
    }

    let formal = if let Some(fixed) = fixed {
        crate::signature_life::charge(c, b)?;
        b.spend(fixed.len(), "actual closure persistent Fixed snapshot")?;
        if fixed.len() != c.fixed_binder_count as usize {
            return Err(bad(
                "actual closure free Fixed domain differs from call relation",
            ));
        }
        let mut invocation = Vec::new();
        for _ in 0..c.invocation_binder_count {
            invocation.push(e.fresh(false, b)?);
        }
        (fixed.to_vec(), invocation)
    } else {
        variables(c, e, false, b)?
    };
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
) -> CompileResult<()> {
    check_with(module, f, reachable, b, f.storage_life.as_ref())
}
pub(super) fn check_role(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    life: &StorageLifeContract,
    b: &mut Budget,
) -> CompileResult<()> {
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
    if let Some(actual) = &d.storage_life {
        super::signature_life::charge(actual, b)?;
        if actual != life {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "Fixed helper role needs the complete actual capture relation",
                None,
            )]);
        }
        super::borrowed_fp::qualified_array_relation_work(&d.captures, &Type::Unit, b)?;
        super::signature_life::capture_relation(
            &d.parameters,
            &d.captures,
            super::stored_callables::life_result(d),
            life,
        )?;
    } else {
        super::borrowed_fp::relation(&d.parameters, &d.result, life)?;
    }
    check_with(module, f, reachable, b, Some(life))
}
fn check_with(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    b: &mut Budget,
    assumptions: Option<&StorageLifeContract>,
) -> CompileResult<()> {
    let mut e = Equations {
        parent: Vec::new(),
        sizes: None,
        rigid: Vec::new(),
        next_rigid: 0,
        grounded: b.capture_relations.then(Vec::new),
    };
    b.spend(
        f.values.len().saturating_add(f.locals.len()),
        "function class fact allocation",
    )?;
    let mut values = Vec::new();
    let mut retained_environment = false;
    for ty in &f.values {
        let p = e.ports(module, ty, b)?;
        retained_environment |= p.environment;
        values.push(p);
    }
    let mut locals = Vec::new();
    for local in &f.locals {
        let p = e.ports(module, &local.ty, b)?;
        retained_environment |= p.environment;
        locals.push(p);
    }
    let mut captures = Vec::new();
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && d.storage_life.is_some()
    {
        b.spend(d.captures.len(), "actual entry capture class ports")?;
        for ty in &d.captures {
            captures.push(e.ports(module, ty, b)?);
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
        for (i, capture) in captures.iter().enumerate() {
            bind(c, StorageBase::Capture(i as u32), capture, &vars, &mut e, b)?;
        }
        Some(vars)
    } else {
        if f.environment.is_some() || retained_environment {
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
    let outer = if b.array_outer_functions.is_empty() {
        None
    } else {
        b.spend(
            b.array_outer_functions.len(),
            "dynamic upper function scheduling",
        )?;
        if b.array_outer_functions.contains(&f.id) {
            Some(array_outer::Flow::build(
                module,
                f,
                reachable,
                (&values, &locals, &captures),
                e.parent.len(),
                b,
            )?)
        } else {
            None
        }
    };
    let mut deferred_helpers = Vec::new();
    let mut deferred_closures = Vec::new();
    for (block_index, block) in f.blocks.iter().enumerate() {
        for (instruction_index, i) in block.instructions.iter().enumerate() {
            if let Some(flow) = &outer {
                flow.enter(block_index, instruction_index);
            }
            b.spend(1, "class equation instruction")?;
            let out = i.result.map(|v| &values[v.0 as usize]);
            match &i.kind {
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
                        {
                            if let Some(Type::ErrorUnion { error, .. }) =
                                i.result.and_then(|id| f.values.get(id.0 as usize))
                            {
                                if let Some(error_shape) = shape(error) {
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
                            }
                        }
                        for row in port_bindings {
                            b.spend(out.task_ports.len(), "actual Task semantic class")?;
                            let path = vec![
                                zil_ir::ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                                zil_ir::ContractProjection::RetainedPort(row.port),
                            ];
                            let index = out
                                .task_ports
                                .iter()
                                .find(|(p, _)| p == &path)
                                .map(|(_, i)| *i)
                                .ok_or_else(|| bad("Task constructor class omits actual port"))?;
                            let StorageBase::Parameter(input) = row.source.base else {
                                return Err(bad("Task port class has no actual input"));
                            };
                            let arg = arguments
                                .get(input as usize)
                                .ok_or_else(|| bad("Task class input is absent"))?;
                            e.equal(
                                out.classes[index],
                                values[arg.0 as usize]
                                    .outer
                                    .ok_or_else(|| bad("Task input has no outer Address class"))?,
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
                            .find(|(p, _)| matches!(p.as_slice(), [zil_ir::ContractProjection::RetainedPort(port)]
                                        if super::tasks::result_port(&f.values[task.0 as usize], *port)))
                            .ok_or_else(|| bad("TaskPoll has no actual retained class"))?
                            .1;
                        let result = out
                            .keys
                            .iter()
                            .position(|(p, c)| {
                                p == &vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(
                                    false,
                                ))] && *c == 0
                            })
                            .ok_or_else(|| bad("TaskPoll has no actual Ready class"))?;
                        e.equal(out.classes[result], input.classes[port], b)?;
                    }
                }
                InstructionKind::UnpackTuple { local, fields }
                    if super::stored_flow::failure_unpack(f, i, b)? =>
                {
                    let parent_shape = shape(&f.locals[local.0 as usize].ty)
                        .ok_or_else(|| bad("failure tuple class source is absent"))?;
                    for (position, target) in fields.iter().enumerate() {
                        if let Some(to) = locals[target.0 as usize].outer {
                            let source = b
                                .physical_slot(
                                    parent_shape,
                                    &[PlaceProjection::Field(position as u32)],
                                )?
                                .ok_or_else(|| {
                                    bad("failure tuple field has no actual parent class")
                                })?
                                .0;
                            let from = *locals[local.0 as usize]
                                .classes
                                .get(source as usize)
                                .ok_or_else(|| bad("failure tuple parent class is absent"))?;
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
                InstructionKind::Load(local) => {
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
                InstructionKind::TakeLocal { local }
                    if (b.task_sources() && !locals[local.0 as usize].task_ports.is_empty())
                        || super::stored_callables::fixed_value(
                            module,
                            &f.locals[local.0 as usize].ty,
                        ) =>
                {
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
                InstructionKind::ReadCapture { index } if !captures.is_empty() => {
                    if let Some(out) = out {
                        let capture = captures
                            .get(*index as usize)
                            .ok_or_else(|| bad("actual capture class source is absent"))?;
                        array_outer::copy(&mut e, out, capture, outer.as_ref(), b)?;
                    }
                }
                InstructionKind::MakeClosure {
                    closure,
                    captures: actuals,
                } => {
                    let d = &module.closures[closure.0 as usize];
                    if let Some(c) = &d.storage_life {
                        let out = out
                            .ok_or_else(|| bad("captured environment lacks actual class result"))?;
                        b.spend(out.classes.len(), "environment Fixed class snapshot")?;
                        let mut invocation = Vec::new();
                        for _ in 0..c.invocation_binder_count {
                            invocation.push(e.fresh(false, b)?);
                        }
                        let vars = (out.classes.clone(), invocation);
                        for (index, value) in actuals.iter().enumerate() {
                            bind_with_outer(
                                c,
                                StorageBase::Capture(index as u32),
                                &values[value.0 as usize],
                                &vars,
                                &mut e,
                                outer.as_ref(),
                                b,
                            )?;
                        }
                    }
                }
                InstructionKind::Store { local, value } => array_outer::copy(
                    &mut e,
                    &locals[local.0 as usize],
                    &values[value.0 as usize],
                    outer.as_ref(),
                    b,
                )?,
                InstructionKind::BorrowPlaceLocal { .. }
                    if super::stored_flow::ordinary_error_borrow(f, i, b)? =>
                {
                    if let Some(out) = out
                        && let Some(outer) = out.outer
                    {
                        e.address_origin(outer, b)?;
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    if let Some(out) = out {
                        if let Some(outer) = out.outer {
                            e.address_origin(outer, b)?;
                        }
                        let stored = &locals[local.0 as usize];
                        array_outer::copy_elements(&mut e, out, stored, outer.as_ref(), b)?;
                        b.spend(
                            out.classes.len().saturating_add(stored.classes.len()),
                            "borrowed carrier classes",
                        )?;
                        if out.classes.len() != stored.classes.len() {
                            return Err(bad("borrowed actual class arity differs"));
                        }
                        for (a, z) in out.classes.iter().zip(&stored.classes) {
                            e.equal(*a, *z, b)?;
                        }
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
                        if !out.array_elements.is_empty() {
                            let shape = shape(&f.values[address.0 as usize]).ok_or_else(|| {
                                bad("array field projection has no actual qualified parent")
                            })?;
                            b.spend(
                                out.array_elements.len(),
                                "array field physical upper inventory",
                            )?;
                            for (ordinal, upper) in out.array_elements.iter().enumerate() {
                                let path = [
                                    PlaceProjection::Field(*index),
                                    PlaceProjection::Field(ordinal as u32),
                                ];
                                // Preserve the original first-row policy; independent
                                // physical inventory still authenticates the shape.
                                let (binder, _) =
                                    b.physical_slot(shape, &path)?.ok_or_else(|| {
                                        bad("array field lacks its actual physical class row")
                                    })?;
                                array_outer::transport(
                                    &mut e,
                                    *upper,
                                    parent.classes[binder as usize],
                                    outer.as_ref(),
                                    b,
                                )?;
                            }
                        }

                        if let (Some(a), Some(z)) = (out.outer, parent.outer) {
                            array_outer::transport(&mut e, a, z, outer.as_ref(), b)?;
                        }
                        link_projection(
                            &f.values[address.0 as usize],
                            parent,
                            out,
                            LifeTypeProjection::Place(PlaceProjection::Field(*index)),
                            &mut e,
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
                InstructionKind::StoreStoredBorrow { .. } => {
                    // The independently built physical source/expected row is
                    // checked after all original declarations have been bound.
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
                        let child =
                            out.ok_or_else(|| bad("failure class projection result is absent"))?;
                        for selected in &child_shape.slots {
                            let mut actual_path = path.clone();
                            actual_path.extend(&selected.path);
                            let source = b
                                .physical_slot(parent_shape, &actual_path)?
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
                                .ok_or_else(|| bad("failure parent class variable is absent"))?;
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
                        "stored class slot selection",
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
                InstructionKind::MakeAggregate { fields } => {
                    if let Some(out) = out
                        && !out.array_elements.is_empty()
                    {
                        b.spend(fields.len(), "array constructor address uppers")?;
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
                            b.spend(
                                slot.path.len().saturating_add(1),
                                "aggregate physical class operand",
                            )?;
                            let (PlaceProjection::Field(index), tail) =
                                slot.path.split_first().ok_or_else(|| {
                                    bad("stored constructor slot lacks an actual field")
                                })?
                            else {
                                return Err(bad("stored constructor slot is not an inline field"));
                            };
                            let arg = fields
                                .get(*index as usize)
                                .ok_or_else(|| bad("aggregate class operand is absent"))?;
                            let operand = &values[arg.0 as usize];
                            let upper = match tail {
                                [] => operand
                                    .outer
                                    .ok_or_else(|| bad("stored class operand is not an Address"))?,
                                [PlaceProjection::Field(ordinal)] => *operand
                                    .array_elements
                                    .get(*ordinal as usize)
                                    .ok_or_else(|| {
                                        bad("stored Array class operand lacks its actual ordinal")
                                    })?,
                                _ => {
                                    return Err(bad(
                                        "nested constructor class requires its physical operand consumer",
                                    ));
                                }
                            };
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
                        if super::stored_flow::shared_array_value(&f.values[aggregate.0 as usize]) {
                            let upper = out.outer.ok_or_else(|| {
                                bad("dynamic shared Array result lacks an Address upper")
                            })?;
                            let elements = &values[aggregate.0 as usize].array_elements;
                            let Type::Array { length, .. } = &f.values[aggregate.0 as usize] else {
                                unreachable!()
                            };
                            b.spend(
                                elements.len().saturating_add(1),
                                "dynamic Array upper alternatives",
                            )?;
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
                        link_projection(
                            &f.values[aggregate.0 as usize],
                            &values[aggregate.0 as usize],
                            out,
                            LifeTypeProjection::Place(PlaceProjection::Field(*index)),
                            &mut e,
                            b,
                        )?;
                    }
                }
                InstructionKind::MakeErrorUnion { value, is_error } if b.stored_enum_addresses => {
                    if let (Some(out), Some(id)) = (out, i.result)
                        && (super::stored_flow::enum_address::signature(&f.values[id.0 as usize])
                            || b.task_sources() && !out.task_ports.is_empty())
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
                InstructionKind::ReadErrorUnion { value, is_error } if b.stored_enum_addresses => {
                    if let Some(out) = out
                        && (super::stored_flow::enum_address::signature(
                            &f.values[value.0 as usize],
                        ) || b.task_sources()
                            && !values[value.0 as usize].task_ports.is_empty())
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
                        let defer = if retained_environment {
                            b.spend(arguments.len(), "actual environment argument detection")?;
                            arguments.iter().any(|v| values[v.0 as usize].environment)
                        } else {
                            false
                        };
                        if defer {
                            b.spend(1, "deferred actual environment helper call")?;
                            deferred_helpers.push((
                                c,
                                arguments.as_slice(),
                                i.result,
                                (block_index, instruction_index),
                            ));
                        } else {
                            call_with_outer(
                                c,
                                arguments,
                                i.result,
                                &values,
                                None,
                                (&mut e, outer.as_ref()),
                                b,
                            )?;
                        }
                    }
                }
                InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. } => {
                    let role = super::stored_callables::role(module, f, &i.kind, b)?;
                    let (definition, arguments) =
                        super::stored_callables::called(module, f, &i.kind)
                            .ok_or_else(|| bad("class call has no actual qualified closure"))?;
                    if let Some(actual) = &definition.storage_life {
                        let life = role.unwrap_or(actual);
                        if let Some(role) = role {
                            crate::signature_life::charge(actual, b)?;
                            crate::signature_life::charge(role, b)?;
                            if role != actual {
                                return Err(vec![Diagnostic::unsupported(
                                    Phase::Analyze,
                                    "Fixed role differs from complete actual capture relation",
                                    None,
                                )]);
                            }
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
                        b.spend(1, "deferred actual environment invocation")?;
                        deferred_closures.push((
                            life,
                            arguments,
                            i.result,
                            receiver.classes.as_slice(),
                            (block_index, instruction_index),
                        ));
                    } else if let Some(life) = role {
                        call_with_outer(
                            life,
                            arguments,
                            i.result,
                            &values,
                            None,
                            (&mut e, outer.as_ref()),
                            b,
                        )?;
                    }
                }
                InstructionKind::CallIndirect { callee, arguments } => {
                    let Type::BorrowedFunctionPointer { life, .. } = &f.values[callee.0 as usize]
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
                        None,
                        (&mut e, outer.as_ref()),
                        b,
                    )?;
                }
                _ => {}
            }
        }
    }
    // Ordinary sources and structural propagation precede any use of a free
    // environment class. Only grounded capture facts get persistent identities;
    // neither a local phantom constructor nor the first call can choose one.
    if let Some(flow) = &outer {
        flow.checkpoint(&mut e, b)?;
    }
    if retained_environment {
        b.spend(values.len(), "retained environment inventory")?;
        for p in &values {
            if p.environment {
                b.spend(p.classes.len(), "actual environment class retention")?;
                for variable in &p.classes {
                    e.retain(*variable, b)?;
                }
            }
        }
    }
    for (contract, arguments, output, site) in deferred_helpers {
        if let Some(flow) = &outer {
            flow.enter(site.0, site.1);
        }
        call_with_outer(
            contract,
            arguments,
            output,
            &values,
            None,
            (&mut e, outer.as_ref()),
            b,
        )?;
    }
    for (contract, arguments, output, fixed, site) in deferred_closures {
        if let Some(flow) = &outer {
            flow.enter(site.0, site.1);
        }
        call_with_outer(
            contract,
            arguments,
            output,
            &values,
            Some(fixed),
            (&mut e, outer.as_ref()),
            b,
        )?;
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
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
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
) -> CompileResult<()> {
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
            b.spend(
                target.parameters.len().saturating_add(1),
                "rich pointer target signature",
            )?;
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
                b.type_work(p, false, "rich indirect argument equality")?;
                if f.values.get(v.0 as usize) != Some(p) {
                    return Err(bad("indirect call actual argument differs"));
                }
            }
            b.type_work(result, false, "rich indirect result equality")?;
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
    fn balanced_components_preserve_rigid_conflicts_and_actual_grounding() {
        let mut b = Budget::new(&Limits::default());
        let mut e = Equations {
            parent: Vec::new(),
            sizes: Some(Vec::new()),
            rigid: Vec::new(),
            next_rigid: 0,
            grounded: Some(Vec::new()),
        };
        let a = e.fresh(false, &mut b).unwrap();
        let z = e.fresh(false, &mut b).unwrap();
        e.equal(a, z, &mut b).unwrap();
        let rigid = e.fresh(true, &mut b).unwrap();
        // The grounded rigid singleton loses the representative to a larger
        // flexible component; both properties must follow the real equality.
        e.equal(rigid, a, &mut b).unwrap();
        e.retain(z, &mut b).unwrap();
        let other = e.fresh(true, &mut b).unwrap();
        assert!(e.equal(z, other, &mut b).is_err());

        let x = e.fresh(false, &mut b).unwrap();
        let y = e.fresh(false, &mut b).unwrap();
        e.equal(x, y, &mut b).unwrap();
        assert!(e.retain(y, &mut b).is_err());
        e.address_origin(y, &mut b).unwrap();
        e.retain(y, &mut b).unwrap();
        assert!(e.equal(x, a, &mut b).is_err());
    }
}
