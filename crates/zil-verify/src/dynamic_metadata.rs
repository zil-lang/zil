//! Bounded raw dyn metadata traversal before hashing or recursive comparisons.
//! This grants no Context, port, cleanup or return-source authority.
use super::*;
fn pay(b: &mut Budget, nodes: &mut usize, cap: usize, n: usize) -> Check<()> {
    *nodes = nodes
        .checked_add(n)
        .ok_or_else(|| limit("dyn metadata node count overflow"))?;
    if *nodes > cap {
        return Err(limit("dyn metadata node count exceeded"));
    }
    b.spend(n)
}
fn path(b: &mut Budget, nodes: &mut usize, cap: usize, depth_cap: usize, n: usize) -> Check<()> {
    if n > depth_cap || n > cap {
        return Err(limit("dyn metadata path limit exceeded"));
    }
    pay(b, nodes, cap, n.saturating_add(1))
}
fn storage(
    b: &mut Budget,
    nodes: &mut usize,
    cap: usize,
    depth_cap: usize,
    c: &StorageLifeContract,
) -> Check<()> {
    pay(b, nodes, cap, 2)?;
    for row in &c.bindings {
        path(b, nodes, cap, depth_cap, row.position.path.len())?;
    }
    for row in &c.classes {
        path(b, nodes, cap, depth_cap, row.position.path.len())?;
    }
    Ok(())
}
fn retained(
    b: &mut Budget,
    nodes: &mut usize,
    cap: usize,
    depth_cap: usize,
    c: &RetainedLifeShape,
) -> Check<()> {
    pay(b, nodes, cap, c.ports.len().saturating_add(2))?;
    for row in &c.retained {
        path(b, nodes, cap, depth_cap, row.path.len())?;
    }
    for row in &c.nominal_arguments {
        path(b, nodes, cap, depth_cap, row.path.len())?;
    }
    Ok(())
}
pub(super) fn ty(root: &Type, b: &mut Budget) -> Check<()> {
    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    pay(b, &mut 0usize, cap, 1)?;
    let mut nodes = 0usize;
    let mut pending = std::collections::VecDeque::new();
    pending.push_back((root, 0usize));
    while let Some((ty, depth)) = pending.pop_front() {
        pay(b, &mut nodes, cap, 1)?;
        if depth > depth_cap {
            return Err(limit("dyn metadata type depth exceeded"));
        }
        let mut children = Vec::new();
        match ty {
            Type::BorrowedStorage { storage, life } => {
                pay(b, &mut nodes, cap, life.slots.len().saturating_add(1))?;
                for row in &life.slots {
                    path(b, &mut nodes, cap, depth_cap, row.path.len())?;
                }
                for row in &life.retained {
                    path(b, &mut nodes, cap, depth_cap, row.path.len())?;
                }
                for row in &life.nominal_arguments {
                    path(b, &mut nodes, cap, depth_cap, row.path.len())?;
                }
                children.push(storage.as_ref());
            }
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Slice { element: inner }
            | Type::Array { element: inner, .. }
            | Type::OwnedStore { element: inner }
            | Type::Mutex { payload: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner }
            | Type::Opaque { hidden: inner, .. } => children.push(inner.as_ref()),
            Type::Task {
                result,
                retained_life,
            }
            | Type::MutexGuard {
                payload: result,
                retained_life,
            } => {
                retained(b, &mut nodes, cap, depth_cap, retained_life)?;
                if retained_life.ports.len() > cap {
                    return Err(limit("dyn metadata port count exceeded"));
                }
                b.spend(retained_life.ports.len())?;
                children.extend(retained_life.ports.iter().map(|p| &p.leaf));
                children.push(result.as_ref());
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                b.spend(name.len())?;
                pay(b, &mut nodes, cap, fields.len())?;
                for (name, ty) in fields {
                    b.spend(name.len())?;
                    children.push(ty);
                }
            }
            Type::Tuple(fields) => {
                pay(b, &mut nodes, cap, fields.len())?;
                children.extend(fields);
            }
            Type::Enum { name, variants } => {
                b.spend(name.len())?;
                pay(b, &mut nodes, cap, variants.len())?;
                for (name, fields) in variants {
                    b.spend(name.len())?;
                    pay(b, &mut nodes, cap, fields.len())?;
                    children.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                children.push(error.as_ref());
                children.push(result.as_ref());
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                pay(b, &mut nodes, cap, parameters.len())?;
                children.extend(parameters);
                children.push(result.as_ref());
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                storage(b, &mut nodes, cap, depth_cap, life)?;
                pay(b, &mut nodes, cap, parameters.len())?;
                children.extend(parameters);
                children.push(result.as_ref());
            }
            Type::Named(name) | Type::Unsupported(name) => {
                b.spend(name.len())?;
            }
            Type::ByteBuffer
            | Type::ForeignHandle
            | Type::SyncCell(_)
            | Type::TaskI32
            | Type::CWideString
            | Type::Closure(_)
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. } => {}
            ty if ty.is_b0_scalar() => {}
            _ => return Err(limit("unrecognized dyn metadata type")),
        }
        if children.len() > cap.saturating_sub(nodes).saturating_sub(pending.len()) {
            return Err(limit("dyn metadata pending type count exceeded"));
        }
        b.spend(children.len())?;
        for child in children {
            pending.push_back((child, depth + 1));
        }
    }
    Ok(())
}

/// Pay the entire public retained directory independently of its Context type.
/// A forged directory need not have the Context's size or paths.
pub(super) fn retained_shape(shape: &RetainedLifeShape, b: &mut Budget) -> Check<()> {
    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    let mut nodes = 0usize;
    retained(b, &mut nodes, cap, depth_cap, shape)?;
    b.spend(shape.ports.len())?;
    for port in &shape.ports {
        ty(&port.leaf, b)?;
    }
    Ok(())
}

fn position(position: &StoragePosition, b: &mut Budget) -> Check<()> {
    path(
        b,
        &mut 0usize,
        b.limits.max_nodes,
        b.limits.max_call_depth.min(128),
        position.path.len(),
    )
}
fn actuals(values: &[DynGenericActual], b: &mut Budget) -> Check<()> {
    pay(b, &mut 0usize, b.limits.max_nodes, values.len())?;
    for actual in values {
        match actual {
            DynGenericActual::Type(node) => ty(node, b)?,
            DynGenericActual::Const(Constant::WideString(units)) => b.spend(units.len())?,
            _ => b.spend(1)?,
        }
    }
    Ok(())
}
fn kinds(kinds: &[LifeParameterKind], b: &mut Budget) -> Check<()> {
    pay(b, &mut 0usize, b.limits.max_nodes, kinds.len())?;
    for kind in kinds {
        if let LifeParameterKind::Const(node) = kind {
            ty(node, b)?;
        }
    }
    Ok(())
}
pub(super) fn declaration(declaration: &DynDeclaration, b: &mut Budget) -> Check<()> {
    pattern(declaration, b)?;
    kinds(&declaration.parameters, b)?;
    let identity = &declaration.identity;
    b.spend(
        identity
            .package
            .len()
            .saturating_add(identity.module_path.len())
            .saturating_add(identity.declaration_path.len())
            .saturating_add(identity.generation_path.len())
            .saturating_add(1),
    )?;
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        b.spend(part.len())?;
    }
    pay(
        b,
        &mut 0usize,
        b.limits.max_nodes,
        declaration.members.len(),
    )?;
    for member in &declaration.members {
        b.spend(member.name.len().saturating_add(1))?;
        match &member.kind {
            DynMemberKind::Constant(node) => ty(node, b)?,
            DynMemberKind::Method(formals) => kinds(formals, b)?,
            DynMemberKind::Type => {}
        }
    }
    Ok(())
}
pub(super) fn interface(module: &TypedModule, item: &DynInterface, b: &mut Budget) -> Check<()> {
    actuals(&item.application.actuals, b)?;
    pay(
        b,
        &mut 0usize,
        b.limits.max_nodes,
        item.inherited
            .len()
            .saturating_add(item.associated_types.len())
            .saturating_add(item.associated_constants.len())
            .saturating_add(item.slots.len()),
    )?;
    for application in &item.inherited {
        actuals(&application.actuals, b)?;
    }
    for associated in &item.associated_types {
        actuals(&associated.member.application.actuals, b)?;
        ty(&associated.ty, b)?;
    }
    for associated in &item.associated_constants {
        actuals(&associated.member.application.actuals, b)?;
        ty(&associated.ty, b)?;
        if let Constant::WideString(units) = &associated.value {
            b.spend(units.len())?;
        }
    }
    for slot in &item.slots {
        actuals(&slot.member.application.actuals, b)?;
        actuals(&slot.actuals, b)?;
        pay(
            b,
            &mut 0usize,
            b.limits.max_nodes,
            slot.parameters
                .len()
                .saturating_add(slot.parameter_modes.len()),
        )?;
        for node in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
            ty(node, b)?;
        }
        if let Some(life) = &slot.storage_life {
            storage(
                b,
                &mut 0usize,
                b.limits.max_nodes,
                b.limits.max_call_depth.min(128),
                life,
            )?;
        }
        let c = &slot.contract;
        b.spend(9)?;
        for requirement in c.requires.iter().flatten() {
            match requirement {
                DynRequirement::Read(p) | DynRequirement::Write(p) | DynRequirement::Manage(p) => {
                    position(p, b)?
                }
                DynRequirement::Apart(a, z) => {
                    position(a, b)?;
                    position(z, b)?;
                }
                DynRequirement::State {
                    position: p,
                    ty: node,
                    ..
                } => {
                    position(p, b)?;
                    ty(node, b)?;
                }
            }
        }
        for guarantee in c.guarantees.iter().flatten() {
            match guarantee {
                DynGuarantee::StorageSources(binding) => {
                    position(&binding.position, b)?;
                    pay(b, &mut 0usize, b.limits.max_nodes, binding.sources.len())?;
                    for source in &binding.sources {
                        position(source, b)?;
                    }
                }
                DynGuarantee::State {
                    position: p,
                    ty: node,
                    ..
                } => {
                    position(p, b)?;
                    ty(node, b)?;
                }
                DynGuarantee::ManageResult => b.spend(1)?,
            }
        }
        for p in c.hold.iter().flatten() {
            position(p, b)?;
        }
        if let Some(call) = &c.call {
            for p in call.reads.iter().chain(&call.writes) {
                position(p, b)?;
            }
        }
        if let Some(DynResultSource::Borrow { sources, .. }) = &c.result {
            pay(b, &mut 0usize, b.limits.max_nodes, sources.len())?;
            for p in sources {
                position(p, b)?;
            }
        }
        for effect in c.effects.iter().flatten() {
            match effect {
                DynEffect::Cleanup(node) => ty(node, b)?,
                DynEffect::Call(p) => position(p, b)?,
                _ => b.spend(1)?,
            }
        }
        crate::dynamic_cleanup::metadata(module, c.cleanup.as_ref(), b)?;
    }
    Ok(())
}

/// Bound declaration recipes before their fingerprints recurse. A pattern is
/// metadata only; this pass does not instantiate it or establish a class.
pub(super) fn pattern(declaration: &DynDeclaration, b: &mut Budget) -> Check<()> {
    enum Node<'a> {
        Nominal(&'a DynSelfPattern, usize),
        Actual(&'a DynPatternActual, usize),
        Ty(&'a DynTypePattern, usize),
        Constant(&'a DynConstPattern, usize),
    }

    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    let mut count = 0usize;
    pay(b, &mut count, cap, 2)?;
    let mut pending = Vec::new();
    for root in [&declaration.self_pattern, &declaration.implemented_trait]
        .into_iter()
        .flatten()
    {
        pending.push(Node::Nominal(root, 0));
    }
    while let Some(node) = pending.pop() {
        pay(b, &mut count, cap, 1)?;
        let mut children = Vec::new();
        let depth = match node {
            Node::Nominal(node, depth) => {
                pay(b, &mut count, cap, node.arguments.len())?;
                children.extend(node.arguments.iter().map(|a| Node::Actual(a, depth + 1)));
                depth
            }
            Node::Actual(actual, depth) => {
                match actual {
                    DynPatternActual::Type(ty) => children.push(Node::Ty(ty, depth + 1)),
                    DynPatternActual::Const(value) => {
                        children.push(Node::Constant(value, depth + 1))
                    }
                    DynPatternActual::LifeParameter(index) => {
                        if !matches!(
                            declaration.parameters.get(*index as usize),
                            Some(LifeParameterKind::Lifetime)
                        ) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "life recipe selects a non-life formal ordinal",
                            ));
                        }
                    }
                }
                depth
            }
            Node::Constant(pattern, depth) => {
                match pattern {
                    DynConstPattern::Parameter(index) => {
                        if !matches!(
                            declaration.parameters.get(*index as usize),
                            Some(LifeParameterKind::Const(_))
                        ) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "constant recipe selects a non-constant formal ordinal",
                            ));
                        }
                    }
                    DynConstPattern::Value(Constant::WideString(units)) => {
                        pay(b, &mut count, cap, units.len())?;
                    }
                    DynConstPattern::Value(_) => {}
                }
                depth
            }
            Node::Ty(pattern, depth) => {
                match pattern {
                    DynTypePattern::Concrete(node) => ty(node, b)?,
                    DynTypePattern::Parameter(index) => {
                        if !matches!(
                            declaration.parameters.get(*index as usize),
                            Some(LifeParameterKind::Type)
                        ) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "type recipe selects a non-type formal ordinal",
                            ));
                        }
                    }
                    DynTypePattern::Nominal(node) => children.push(Node::Nominal(node, depth + 1)),
                    DynTypePattern::Address {
                        lifetime, inner, ..
                    } => {
                        if lifetime.is_some_and(|index| {
                            !matches!(
                                declaration.parameters.get(index as usize),
                                Some(LifeParameterKind::Lifetime)
                            )
                        }) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "address lifetime recipe selects a non-life formal ordinal",
                            ));
                        }
                        children.push(Node::Ty(inner, depth + 1));
                    }
                    DynTypePattern::Tuple(fields) => {
                        pay(b, &mut count, cap, fields.len())?;
                        children.extend(fields.iter().map(|ty| Node::Ty(ty, depth + 1)));
                    }
                    DynTypePattern::Array { element, length } => {
                        children.push(Node::Ty(element, depth + 1));
                        children.push(Node::Constant(length, depth + 1));
                    }
                    DynTypePattern::Slice(node)
                    | DynTypePattern::Managed(node)
                    | DynTypePattern::Desc(node)
                    | DynTypePattern::OwnedStore(node) => children.push(Node::Ty(node, depth + 1)),
                    DynTypePattern::ErrorUnion { error, result } => {
                        children.push(Node::Ty(error, depth + 1));
                        children.push(Node::Ty(result, depth + 1));
                    }
                }
                depth
            }
        };
        if depth > depth_cap {
            return Err(limit("dyn recipe depth exceeds selected bound"));
        }
        if children
            .len()
            .saturating_add(pending.len())
            .saturating_add(count)
            > cap
        {
            return Err(limit("dyn recipe pending node bound exceeded"));
        }
        b.spend(children.len())?;
        pending.extend(children);
    }
    Ok(())
}
