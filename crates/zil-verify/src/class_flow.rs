//! Independently reconstruct invariant signature equations. This grants no roots,
//! storage initialization, parent loans, effects or physical access rights.
use super::*;
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
            if b.stored_enum_addresses && super::stored_flow::enum_address::signature(ty) =>
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
struct Equations {
    representative: Vec<usize>,
    rigid: Vec<Option<usize>>,
    grounded: Option<Vec<bool>>,
    next_rigid: usize,
}
impl Equations {
    fn fresh(&mut self, rigid: bool, b: &mut Budget) -> Check<usize> {
        b.spend(3)?;
        let id = self.representative.len();
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
        b.spend(
            self.representative
                .len()
                .saturating_mul(2)
                .saturating_add(1),
        )?;
        let a = self.representative[a];
        let z = self.representative[z];
        if a == z {
            return Ok(());
        }
        if self.rigid[a].is_some() && self.rigid[z].is_some() && self.rigid[a] != self.rigid[z] {
            return Err(bad(
                "actual retained class equality is not universally valid",
            ));
        }
        for row in &mut self.representative {
            if *row == z {
                *row = a;
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
        if b.task_borrowed_results {
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
            && !(b.task_borrowed_results && !child.task_ports.is_empty() && child.outer.is_none())
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
            if !row.position.path.is_empty() && b.task_borrowed_results {
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
            &d.parameters,
            &d.captures,
            super::stored_callables::life_result(d),
            life,
            Some(f.span),
        )?;
    } else {
        super::borrowed_fp::relation(&d.parameters, &d.result, life, Some(f.span))?;
    }
    check_with(module, f, reachable, b, Some(life))
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
    {
        return Err(unsupported(
            "captured Fixed environment result needs retained target transfer",
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
    // Source structure first; capture-fixed substitution second; calls last.
    // A first call cannot pick an unconstrained capture's semantic class.
    for phase in 0..if environments { 3 } else { 1 } {
        for block in &f.blocks {
            for i in &block.instructions {
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
                    InstructionKind::TaskCreate {
                        arguments,
                        port_bindings,
                        ..
                    } if b.task_borrowed_results => {
                        if let Some(out) = out {
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
                        if b.task_borrowed_results
                            && matches!(super::tasks::task_payload(&f.values[task.0 as usize]),
                            Some(Type::Task { result, .. })
                                if matches!(result.as_ref(), Type::Address { mutable: false, .. })) =>
                    {
                        if let Some(out) = out
                            && !values[task.0 as usize].task_ports.is_empty()
                        {
                            let input = &values[task.0 as usize];
                            let port = input
                                .task_ports
                                .iter()
                                .find(|(p, _)| {
                                    p == &vec![zil_ir::ContractProjection::RetainedPort(0)]
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
                    InstructionKind::ReadCapture { index } if !captures.is_empty() => {
                        let source = captures
                            .get(*index as usize)
                            .ok_or_else(|| bad("capture class index absent"))?;
                        if let Some(out) = out {
                            e.copy(out, source, b)?;
                        }
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
                                bind(
                                    c,
                                    StorageBase::Capture(index as u32),
                                    &values[actual.0 as usize],
                                    &vars,
                                    &mut e,
                                    b,
                                )?;
                            }
                            e.freeze(fixed, b)?;
                        }
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        if let Some(out) = out {
                            e.copy(out, &locals[local.0 as usize], b)?;
                        }
                    }
                    InstructionKind::Store { local, value } => {
                        e.copy(&locals[local.0 as usize], &values[value.0 as usize], b)?
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        if let Some(out) = out {
                            e.ground_address(out.outer, b)?;
                            let stored = &locals[local.0 as usize];
                            e.copy_elements(out, stored, b)?;
                            b.spend(out.classes.len().saturating_add(stored.classes.len()))?;
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
                            e.copy(out, &values[address.0 as usize], b)?;
                        }
                    }
                    InstructionKind::AddressIndex { address, .. } => {
                        if let Some(out) = out {
                            let parent = &values[address.0 as usize];
                            if let (Some(a), Some(z)) = (out.outer, parent.outer) {
                                e.equal(a, z, b)?;
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
                                let shape =
                                    shape(&f.values[address.0 as usize]).ok_or_else(|| {
                                        bad("array field projection has no actual qualified parent")
                                    })?;
                                b.spend(out.array_elements.len().saturating_mul(
                                    shape.slots.len().saturating_mul(3).saturating_add(1),
                                ))?;
                                for (ordinal, upper) in out.array_elements.iter().enumerate() {
                                    let path = [
                                        PlaceProjection::Field(*index),
                                        PlaceProjection::Field(ordinal as u32),
                                    ];
                                    let row = shape
                                        .slots
                                        .iter()
                                        .find(|row| row.path == path)
                                        .ok_or_else(|| {
                                            bad("array field lacks its actual physical class row")
                                        })?;
                                    e.equal(*upper, parent.classes[row.binder as usize], b)?;
                                }
                            }

                            if let (Some(a), Some(z)) = (out.outer, parent.outer) {
                                e.equal(a, z, b)?;
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
                            e.equal(out, parent, b)?;
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
                            e.equal(
                                out.and_then(|o| o.outer)
                                    .ok_or_else(|| bad("array load lacks Address result"))?,
                                class,
                                b,
                            )?;
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
                        e.equal(
                            out.and_then(|o| o.outer)
                                .ok_or_else(|| bad("stored class load lacks Address result"))?,
                            class,
                            b,
                        )?;
                    }
                    InstructionKind::MakeAggregate { fields } => {
                        if let Some(out) = out
                            && !out.array_elements.is_empty()
                        {
                            b.spend(fields.len())?;
                            if fields.len() != out.array_elements.len() {
                                return Err(bad("array constructor address upper arity differs"));
                            }
                            for (upper, argument) in out.array_elements.iter().zip(fields) {
                                e.equal(
                                    *upper,
                                    values[argument.0 as usize].outer.ok_or_else(|| {
                                        bad("array constructor operand is not an Address")
                                    })?,
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
                                let upper = match tail {
                                [] => operand.outer.ok_or_else(|| bad("stored class operand is not an Address"))?,
                                [PlaceProjection::Field(ordinal)] => *operand.array_elements.get(*ordinal as usize)
                                    .ok_or_else(|| bad("stored Array class operand lacks its actual ordinal"))?,
                                _ => return Err(bad("nested constructor class requires its physical operand consumer")),
                            };
                                e.equal(out.classes[slot.binder as usize], upper, b)?;
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
                            e.copy_elements(out, input, b)?;
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
                    InstructionKind::MakeErrorUnion { value, is_error }
                        if b.stored_enum_addresses =>
                    {
                        if let (Some(out), Some(id)) = (out, i.result)
                            && (super::stored_flow::enum_address::signature(
                                &f.values[id.0 as usize],
                            ) || b.task_borrowed_results && !out.task_ports.is_empty())
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
                        if b.stored_enum_addresses =>
                    {
                        if let Some(out) = out
                            && super::stored_flow::enum_address::signature(
                                &f.values[value.0 as usize],
                            )
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
                            e.copy(out, &values[aggregate.0 as usize], b)?;
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
                            e.copy(out, &values[aggregate.0 as usize], b)?;
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
                            call(c, arguments, i.result, &values, &mut e, b)?;
                        }
                    }
                    InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::CallClosureOnce { .. } => {
                        let (d, arguments) = super::stored_callables::called(module, f, &i.kind)
                            .ok_or_else(|| bad("call has no actual qualified closure"))?;
                        let role = super::stored_callables::role(module, f, &i.kind, d, b)?;
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
                            for (index, argument) in arguments.iter().enumerate() {
                                bind(
                                    definition,
                                    StorageBase::Parameter(index as u32),
                                    &values[argument.0 as usize],
                                    &vars,
                                    &mut e,
                                    b,
                                )?;
                            }
                            if let Some(output) = i.result {
                                bind(
                                    definition,
                                    StorageBase::Result,
                                    &values[output.0 as usize],
                                    &vars,
                                    &mut e,
                                    b,
                                )?;
                            }
                        } else if let Some(life) = role {
                            call(life, arguments, i.result, &values, &mut e, b)?;
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
                        call(life, arguments, i.result, &values, &mut e, b)?;
                    }
                    _ => {}
                }
            }
        }
    }
    if let (Some(c), Some(vars)) = (assumptions, header) {
        for (block, live) in f.blocks.iter().zip(reachable) {
            if *live && let Terminator::Return(Some(v)) = block.terminator {
                bind(
                    c,
                    StorageBase::Result,
                    &values[v.0 as usize],
                    &vars,
                    &mut e,
                    b,
                )?;
            }
        }
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
