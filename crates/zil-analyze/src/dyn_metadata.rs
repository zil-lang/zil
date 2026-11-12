//! Bounded raw dyn metadata traversal before hashing or recursive comparisons.
//! This grants no Context, port, cleanup or return-source authority.
use super::*;
fn limit_error(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::limit(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
fn pay(b: &mut Budget, nodes: &mut usize, cap: usize, n: usize) -> CompileResult<()> {
    *nodes = nodes
        .checked_add(n)
        .ok_or_else(|| limit_error("dyn metadata node count overflow"))?;
    if *nodes > cap {
        return Err(limit_error("dyn metadata node count exceeded"));
    }
    b.spend(n, "dyn raw metadata")
}
fn path(
    b: &mut Budget,
    nodes: &mut usize,
    cap: usize,
    depth_cap: usize,
    n: usize,
) -> CompileResult<()> {
    if n > depth_cap || n > cap {
        return Err(limit_error("dyn metadata path limit exceeded"));
    }
    pay(b, nodes, cap, n.saturating_add(1))
}
fn storage(
    b: &mut Budget,
    nodes: &mut usize,
    cap: usize,
    depth_cap: usize,
    c: &StorageLifeContract,
) -> CompileResult<()> {
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
) -> CompileResult<()> {
    pay(b, nodes, cap, c.ports.len().saturating_add(2))?;
    for row in &c.retained {
        path(b, nodes, cap, depth_cap, row.path.len())?;
    }
    for row in &c.nominal_arguments {
        path(b, nodes, cap, depth_cap, row.path.len())?;
    }
    Ok(())
}
pub(super) fn ty(root: &Type, b: &mut Budget) -> CompileResult<()> {
    ty_at(root, 0, b)
}
fn ty_at(root: &Type, initial_depth: usize, b: &mut Budget) -> CompileResult<()> {
    let cap = b.helper_nodes;
    let depth_cap = b.partial_depth;
    pay(b, &mut 0usize, cap, 1)?;
    let mut nodes = 0usize;
    let mut pending = Vec::new();
    pending.push((root, initial_depth));
    while let Some((ty, depth)) = pending.pop() {
        pay(b, &mut nodes, cap, 1)?;
        if depth > depth_cap {
            return Err(limit_error("dyn metadata type depth exceeded"));
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
                    return Err(limit_error("dyn metadata port count exceeded"));
                }
                b.spend(retained_life.ports.len(), "dyn raw metadata")?;
                children.extend(retained_life.ports.iter().map(|p| &p.leaf));
                children.push(result.as_ref());
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                b.spend(name.len(), "dyn raw metadata")?;
                pay(b, &mut nodes, cap, fields.len())?;
                for (name, ty) in fields {
                    b.spend(name.len(), "dyn raw metadata")?;
                    children.push(ty);
                }
            }
            Type::Tuple(fields) => {
                pay(b, &mut nodes, cap, fields.len())?;
                children.extend(fields);
            }
            Type::Enum { name, variants } => {
                b.spend(name.len(), "dyn raw metadata")?;
                pay(b, &mut nodes, cap, variants.len())?;
                for (name, fields) in variants {
                    b.spend(name.len(), "dyn raw metadata")?;
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
                b.spend(name.len(), "dyn raw metadata")?;
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
            _ => return Err(limit_error("unrecognized dyn metadata type")),
        }
        if children.len() > cap.saturating_sub(nodes).saturating_sub(pending.len()) {
            return Err(limit_error("dyn metadata pending type count exceeded"));
        }
        b.spend(children.len(), "dyn raw metadata")?;
        for child in children {
            pending.push((child, depth + 1));
        }
    }
    Ok(())
}

/// Pay the entire public retained directory independently of its Context type.
/// A forged directory need not have the Context's size or paths.
pub(super) fn retained_shape(shape: &RetainedLifeShape, b: &mut Budget) -> CompileResult<()> {
    let cap = b.helper_nodes;
    let depth_cap = b.partial_depth;
    let mut nodes = 0usize;
    retained(b, &mut nodes, cap, depth_cap, shape)?;
    b.spend(shape.ports.len(), "dyn retained port inventory")?;
    for port in &shape.ports {
        ty(&port.leaf, b)?;
    }
    Ok(())
}

/// Full bounded declaration recipe traversal before clone or fingerprint.
pub(super) fn pattern(root: Option<&DynSelfPattern>, b: &mut Budget) -> CompileResult<()> {
    b.spend(1, "dyn recipe presence")?;
    if let Some(root) = root {
        recipe(root, b)?;
    }
    Ok(())
}

pub(super) fn guarantees(values: Option<&[DynGuarantee]>, b: &mut Budget) -> CompileResult<()> {
    let cap = b.helper_nodes;
    let depth = b.partial_depth;
    let mut nodes = 0usize;
    pay(b, &mut nodes, cap, 1)?;
    if let Some(values) = values {
        pay(b, &mut nodes, cap, values.len())?;
        for value in values {
            match value {
                DynGuarantee::StorageSources(binding) => {
                    path(b, &mut nodes, cap, depth, binding.position.path.len())?;
                    pay(b, &mut nodes, cap, binding.sources.len())?;
                    for atom in &binding.sources {
                        let source = crate::exact_storage_source(atom)?;
                        path(b, &mut nodes, cap, depth, source.path.len())?;
                    }
                }
                DynGuarantee::State {
                    position,
                    ty: value,
                    ..
                } => {
                    path(b, &mut nodes, cap, depth, position.path.len())?;
                    ty(value, b)?;
                }
                DynGuarantee::ManageResult => {}
            }
        }
    }
    Ok(())
}

/// This visitor only bounds raw structure before fingerprinting. Actual
/// declaration, substitution, origin and obligation proofs remain separate.
struct RecipeWork<'a> {
    b: &'a mut Budget,
    nodes: usize,
}
impl RecipeWork<'_> {
    fn count(&mut self, n: usize, depth: usize) -> CompileResult<()> {
        if depth > self.b.partial_depth.min(128) {
            return Err(limit_error("dynamic recipe metadata quota exceeded"));
        }
        self.nodes = self
            .nodes
            .checked_add(n)
            .ok_or_else(|| limit_error("dynamic recipe metadata quota exceeded"))?;
        if self.nodes > self.b.helper_nodes {
            return Err(limit_error("dynamic recipe metadata quota exceeded"));
        }
        self.b.spend(n, "dynamic declaration recipe inventory")
    }
}
impl DynRecipeVisitor for RecipeWork<'_> {
    type Error = Vec<zil_base::Diagnostic>;
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        self.count(1, depth)
    }
    fn rows(&mut self, n: usize, depth: usize) -> CompileResult<()> {
        self.count(n.saturating_add(1), depth)
    }
    fn ty(&mut self, value: &Type, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        ty_at(value, depth, self.b)
    }
    fn life(&mut self, value: &StorageLifeShape, depth: usize) -> CompileResult<()> {
        self.rows(value.binder_count as usize, depth)?;
        self.rows(
            value
                .slots
                .len()
                .saturating_add(value.retained.len())
                .saturating_add(value.nominal_arguments.len()),
            depth,
        )?;
        for row in &value.slots {
            self.rows(row.path.len(), depth.saturating_add(row.path.len()))?;
        }
        for row in &value.retained {
            self.rows(row.path.len(), depth.saturating_add(row.path.len()))?;
        }
        for row in &value.nominal_arguments {
            self.rows(row.path.len(), depth.saturating_add(row.path.len()))?;
        }
        Ok(())
    }
    fn identity(&mut self, value: &LifeDeclarationIdentity, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.rows(value.package.len(), depth)?;
        self.rows(
            value
                .module_path
                .len()
                .saturating_add(value.declaration_path.len())
                .saturating_add(value.generation_path.len()),
            depth,
        )?;
        for part in value.module_path.iter().chain(&value.declaration_path) {
            self.rows(part.len(), depth)?;
        }
        Ok(())
    }
    fn position(&mut self, value: &StoragePosition, depth: usize) -> CompileResult<()> {
        self.rows(value.path.len(), depth.saturating_add(value.path.len()))
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let Constant::WideString(units) = value {
            self.rows(units.len(), depth)?;
        }
        Ok(())
    }
    fn text(&mut self, value: &str, depth: usize) -> CompileResult<()> {
        self.rows(value.len(), depth)
    }
}
pub(super) fn recipe<T: DynRecipeMetadata>(value: &T, b: &mut Budget) -> CompileResult<()> {
    value.visit_dyn_metadata(&mut RecipeWork { b, nodes: 0 }, 0)
}
pub(super) fn recipe_declaration(value: &DynDeclaration, b: &mut Budget) -> CompileResult<()> {
    b.spend(5, "dynamic declaration recipe presence")?;
    for values in [&value.direct_supertraits] {
        if let Some(values) = values {
            for value in values {
                recipe(value, b)?;
            }
        }
    }
    if let Some(values) = &value.requirements {
        for value in values {
            recipe(value, b)?;
        }
    }
    if let Some(values) = &value.associated_definitions {
        for value in values {
            recipe(value, b)?;
        }
    }
    if let Some(values) = &value.method_signatures {
        for value in values {
            recipe(value, b)?;
        }
    }
    if let Some(values) = &value.method_definitions {
        for value in values {
            recipe(value, b)?;
        }
    }
    Ok(())
}
pub(super) fn recipe_table(value: &DynVtable, b: &mut Budget) -> CompileResult<()> {
    b.spend(2, "dynamic table recipe presence")?;
    if let Some(shape) = &value.self_life {
        DynRecipeVisitor::life(&mut RecipeWork { b, nodes: 0 }, shape, 0)?;
    }
    if let Some(values) = &value.implementation_witnesses {
        for value in values {
            recipe(value, b)?;
        }
    }
    Ok(())
}
