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

/// Full bounded declaration recipe traversal before clone or fingerprint.
pub(super) fn pattern(root: Option<&DynSelfPattern>, b: &mut Budget) -> Check<()> {
    enum Node<'a> {
        Nominal(&'a DynSelfPattern),
        Actual(&'a DynPatternActual),
        Ty(&'a DynTypePattern),
        Const(&'a DynConstPattern),
    }
    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    let mut nodes = 0usize;
    pay(b, &mut nodes, cap, 1)?;
    let Some(root) = root else { return Ok(()) };
    let mut pending = vec![(Node::Nominal(root), 0usize)];
    while let Some((node, depth)) = pending.pop() {
        if depth >= depth_cap {
            return Err(limit("dyn Self recipe metadata limit exceeded"));
        }
        pay(b, &mut nodes, cap, 1)?;
        match node {
            Node::Nominal(value) => {
                pay(b, &mut nodes, cap, value.arguments.len())?;
                for actual in &value.arguments {
                    pending.push((Node::Actual(actual), depth + 1));
                }
            }
            Node::Actual(value) => match value {
                DynPatternActual::Type(value) => pending.push((Node::Ty(value), depth + 1)),
                DynPatternActual::Const(value) => pending.push((Node::Const(value), depth + 1)),
                DynPatternActual::LifeParameter(_) => {}
            },
            Node::Const(DynConstPattern::Value(Constant::WideString(units))) => {
                let n = units.len();
                if n > cap {
                    return Err(limit("dyn Self recipe metadata limit exceeded"));
                }
                b.spend(n)?;
            }
            Node::Const(_) => {}
            Node::Ty(value) => match value {
                DynTypePattern::Concrete(value) => ty(value, b)?,
                DynTypePattern::Nominal(value) => pending.push((Node::Nominal(value), depth + 1)),
                DynTypePattern::Parameter(_) => {}
                DynTypePattern::Address { inner, .. }
                | DynTypePattern::Slice(inner)
                | DynTypePattern::Managed(inner)
                | DynTypePattern::Desc(inner)
                | DynTypePattern::OwnedStore(inner) => pending.push((Node::Ty(inner), depth + 1)),
                DynTypePattern::Array { element, length } => {
                    pending.push((Node::Const(length), depth + 1));
                    pending.push((Node::Ty(element), depth + 1));
                }
                DynTypePattern::Tuple(values) => {
                    pay(b, &mut nodes, cap, values.len())?;
                    for value in values {
                        pending.push((Node::Ty(value), depth + 1));
                    }
                }
                DynTypePattern::ErrorUnion { error, result } => {
                    pending.push((Node::Ty(error), depth + 1));
                    pending.push((Node::Ty(result), depth + 1));
                }
            },
        }
    }
    Ok(())
}
