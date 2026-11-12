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
) -> CompileResult<()> {
    let n = 1usize;
    b.spend(n, "actual dyn Context class inventory")?;
    if cp.len() > b.partial_depth {
        return Err(bad("dyn Context class path exceeds type depth"));
    }
    match ty {
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            let n = module.dyn_interfaces.len();
            b.spend(n, "actual dyn Context class inventory")?;
            let item = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .ok_or_else(|| bad("actual dyn Context class interface is absent"))?;
            if item.context.is_none() {
                return Ok(());
            }
            let n = item.retained_life.binder_count as usize;
            b.spend(n, "actual dyn Context class inventory")?;
            for class in 0..item.retained_life.binder_count {
                let n = cp.len().saturating_add(1);
                b.spend(n, "actual dyn Context class inventory")?;
                keys.push((cp.clone(), class));
            }
            for (index, row) in item.retained_life.ports.iter().enumerate() {
                let n = keys.len().saturating_add(bp.len()).saturating_add(1);
                b.spend(n, "actual dyn Context class inventory")?;
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
        let n = module.dyn_interfaces.len();
        b.spend(n, "actual dyn Context class inventory")?;
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
    fixed: Option<(&[usize], Option<usize>)>,
    e: &mut Equations,
    b: &mut Budget,
) -> CompileResult<()> {
    let formal = if let Some((fixed,_)) = fixed {
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
    if let (Some((_,Some(head))),Some(binder))=(fixed,super::signature_life::outer(c,StorageBase::Receiver)) {
        e.equal(head,select(&formal,binder)?,b)?;
    }
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
    fixed: Option<(&[usize], Option<usize>)>,
    context: (&mut Equations, Option<&array_outer::Flow>),
    b: &mut Budget,
) -> CompileResult<()> {
    let (e, flow) = context;
    if flow.is_none() {
        return call(c, args, output, values, fixed, e, b);
    }

    let formal = if let Some((fixed,_)) = fixed {
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
    if let (Some((_,Some(head))),Some(binder))=(fixed,super::signature_life::outer(c,StorageBase::Receiver)) {
        array_outer::binding(e,head,select(&formal,binder)?,flow,b)?;
    }
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
        super::signature_life::closure_relation(
            &module.closures,
            d.kind,
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
    let mut exclusive_head = false;
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && d.storage_life.is_some()
    {
        b.spend(d.captures.len(), "actual entry capture class ports")?;
        for ty in &d.captures {
            exclusive_head |= super::stored_callables::exclusive_capture(ty);
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
    // One receiver call region; separate SSA reads get separate loans, rather
    // than distinct rigid declared lifetimes. Pointee classes stay Fixed.
    let receiver_region = if let (Some(contract),Some(variables))=(assumptions,header.as_ref())
        && let Some(binder)=super::signature_life::outer(contract,StorageBase::Receiver) {
        Some(select(variables,binder)?)
    } else if exclusive_head { Some(e.fresh(true, b)?) } else { None };
