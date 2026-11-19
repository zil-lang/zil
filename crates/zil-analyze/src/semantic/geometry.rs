//! Shared immutable Type/lifetime geometry. Semantic class images are never
//! stored here; every value carries its own current complete class vector.
use super::*;
use std::sync::Arc;
#[path = "geometry_intern.rs"]
mod intern;

pub(crate) struct Geometry {
    pub ty: Type,
    pub life: StorageLifeShape,
    receipt: std::sync::OnceLock<intern::Receipt>,
}
impl Geometry {
    pub(super) fn new(ty: Type, life: StorageLifeShape) -> Self {
        Self {
            ty,
            life,
            receipt: std::sync::OnceLock::new(),
        }
    }
    pub(super) fn invalidate(&mut self) {
        let _ = self.receipt.take();
    }
}
impl Clone for Geometry {
    fn clone(&self) -> Self {
        Self::new(self.ty.clone(), self.life.clone())
    }
}
impl std::fmt::Debug for Geometry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Geometry")
            .field("ty", &self.ty)
            .field("life", &self.life)
            .finish()
    }
}
impl PartialEq for Geometry {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty && self.life == other.life
    }
}
impl Eq for Geometry {}
pub(crate) use intern::{GeometryRecord, TypeRecord};

// A physical Type selector only. The caller separately reconstructs every
// complete child domain and current class image before making a Value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Child {
    Inner,
    UnionError,
    UnionResult,
    Field(Option<usize>, usize),
}
pub(super) fn retain_child(
    owner: &Arc<Geometry>,
    child: Child,
    budget: &mut Budget,
) -> CompileResult<()> {
    intern::retain_child(owner, child, budget)
}
pub(super) fn project(
    owner: &Arc<Geometry>,
    child: Child,
    life: &StorageLifeShape,
    budget: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    intern::project(owner, child, life, budget)
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Unary {
    Address(bool),
    Managed,
    Desc,
    OwnedStore,
    Mutex,
    Registration,
    AsyncRegistration,
    Slice,
    Array(u32),
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Constructor {
    Unary(Unary),
    Tuple,
    ErrorUnion,
}
pub(super) fn construct(
    kind: Constructor,
    children: &[&Arc<Geometry>],
    life: StorageLifeShape,
    budget: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    intern::construct(kind, children, life, budget)
}

pub(crate) struct TypeEntry {
    // Retaining the owner makes every registered address request-stable.
    pub owner: Arc<Geometry>,
    pub raw_work: usize,
    pub raw_limits: (usize, usize),
    module: Option<usize>,
    pub nominal: Option<(usize, usize, usize)>, // module, type-depth limit, row
    type_id: usize,
    coordinate: intern::Node,
    // For an immutable input root, the map key addresses the actual source
    // rather than the retained equal Type. Only its original published input
    // directory authenticates that address. This attests no lifetime image.
    input: Option<Arc<BTreeSet<usize>>>,
}

pub(super) struct Inputs {
    module: Option<usize>,
    raw_limits: (usize, usize),
    types: Arc<BTreeSet<usize>>,
    lives: BTreeSet<usize>,
    empty: StorageLifeShape,
    scalars: std::cell::RefCell<Vec<Arc<Geometry>>>,
    resolved: std::cell::RefCell<BTreeMap<(usize, Option<usize>), Arc<Geometry>>>,
    // Exact immutable input Type coordinates. The owner's actual first domain
    // was fully bounded; it is not a domain premise for a later request.
    type_roots: std::cell::RefCell<BTreeMap<usize, Arc<Geometry>>>,
    nominals: std::cell::RefCell<BTreeMap<usize, Arc<Geometry>>>,
}
impl Default for Inputs {
    fn default() -> Self {
        Self {
            module: None,
            raw_limits: (0, 0),
            types: Arc::default(),
            lives: BTreeSet::new(),
            empty: value::empty(),
            scalars: std::cell::RefCell::default(),
            resolved: std::cell::RefCell::new(BTreeMap::new()),
            type_roots: std::cell::RefCell::new(BTreeMap::new()),
            nominals: std::cell::RefCell::new(BTreeMap::new()),
        }
    }
}
struct Register<'a> {
    inputs: &'a mut Inputs,
    budget: &'a mut Budget,
}
fn registration_limit(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::limit(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
impl DynRecipeVisitor for Register<'_> {
    type Error = Vec<zil_base::Diagnostic>;
    fn node(&mut self, _depth: usize) -> CompileResult<()> {
        self.budget.spend(1, "semantic geometry input directory")
    }
    fn rows(&mut self, n: usize, _depth: usize) -> CompileResult<()> {
        self.budget
            .spend(n.saturating_add(1), "semantic geometry input directory")
    }
    fn ty(&mut self, value: &Type, _depth: usize) -> CompileResult<()> {
        self.inputs.register_type(value, self.budget)
    }
    fn life(&mut self, value: &StorageLifeShape, _depth: usize) -> CompileResult<()> {
        self.inputs.register_life(value, self.budget)
    }
    fn identity(&mut self, _: &LifeDeclarationIdentity, depth: usize) -> CompileResult<()> {
        self.node(depth)
    }
    fn position(&mut self, _: &StoragePosition, depth: usize) -> CompileResult<()> {
        self.node(depth)
    }
    fn constant(&mut self, _: &Constant, depth: usize) -> CompileResult<()> {
        self.node(depth)
    }
    fn text(&mut self, _: &str, depth: usize) -> CompileResult<()> {
        self.node(depth)
    }
}
impl Inputs {
    pub(super) fn new(
        module: &TypedModule,
        program: &SemanticModule,
        budget: &mut Budget,
    ) -> CompileResult<Self> {
        // The complete raw metadata/depth preflight has already succeeded.
        // The recipe visitor exposes Type roots; registration follows every
        // existing immutable child and embedded domain below each such root.
        budget.geometry_module = Some(module as *const TypedModule as usize);
        let mut inputs = Self {
            module: Some(module as *const TypedModule as usize),
            raw_limits: (budget.partial_depth, budget.helper_nodes),
            ..Self::default()
        };
        program.visit_dyn_metadata(
            &mut Register {
                inputs: &mut inputs,
                budget,
            },
            0,
        )?;
        for instance in &module.life_instances {
            inputs.register_type(&instance.ty, budget)?;
            for argument in &instance.arguments {
                match argument {
                    LifeNominalActual::Type(ty) => inputs.register_type(ty, budget)?,
                    LifeNominalActual::QualifiedType { ty, life } => {
                        inputs.register_type(ty, budget)?;
                        inputs.register_life(life, budget)?;
                    }
                    _ => budget.spend(1, "semantic geometry input directory")?,
                }
            }
        }
        for function in &module.functions {
            inputs.register_type(&function.return_type, budget)?;
            for ty in &function.values {
                inputs.register_type(ty, budget)?;
            }
            for local in &function.locals {
                inputs.register_type(&local.ty, budget)?;
            }
        }
        for closure in &module.closures {
            for ty in closure
                .captures
                .iter()
                .chain(&closure.parameters)
                .chain(std::iter::once(&closure.result))
            {
                inputs.register_type(ty, budget)?;
            }
        }
        for resource in &module.resources {
            inputs.register_type(&resource.payload, budget)?;
        }
        if module
            .semantic
            .as_ref()
            .is_some_and(|actual| std::ptr::eq(actual, program))
        {
            budget.spend(1, "immutable input type directory")?;
            budget.input_type_coordinates = Some((
                module as *const TypedModule as usize,
                Arc::clone(&inputs.types),
            ));
        }
        Ok(inputs)
    }
    fn register_type(&mut self, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
        let mut pending = Vec::new();
        Self::register_child(&mut pending, ty, 0, budget)?;
        let mut nodes = 0usize;
        while let Some((ty, depth)) = pending.pop() {
            budget.spend(1, "semantic geometry input directory")?;
            nodes = nodes
                .checked_add(1)
                .ok_or_else(|| registration_limit("semantic geometry input node count overflow"))?;
            if depth > budget.partial_depth || nodes > budget.helper_nodes {
                return Err(registration_limit(
                    "semantic geometry input exceeds metadata profile",
                ));
            }
            budget.lookup(self.types.len(), "semantic geometry Type registration")?;
            // Registration finishes before the directory is shared with other passes.
            if !Arc::get_mut(&mut self.types)
                .ok_or_else(|| fail("geometry input directory was already published"))?
                .insert(ty as *const Type as usize)
            {
                // Every published input root was traversed by this same walk.
                continue;
            }
            let child_depth = depth.saturating_add(1);
            match ty {
                Type::BorrowedStorage { storage, life } => {
                    self.register_life(life, budget)?;
                    Self::register_child(&mut pending, storage, child_depth, budget)?;
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
                | Type::Opaque { hidden: inner, .. } => {
                    Self::register_child(&mut pending, inner, child_depth, budget)?
                }
                Type::Task {
                    result,
                    retained_life,
                }
                | Type::MutexGuard {
                    payload: result,
                    retained_life,
                } => {
                    Self::register_child(&mut pending, result, child_depth, budget)?;
                    for port in &retained_life.ports {
                        Self::register_child(&mut pending, &port.leaf, child_depth, budget)?;
                    }
                }
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    for (_, child) in fields {
                        Self::register_child(&mut pending, child, child_depth, budget)?;
                    }
                }
                Type::Tuple(fields) => {
                    for child in fields {
                        Self::register_child(&mut pending, child, child_depth, budget)?;
                    }
                }
                Type::Enum { variants, .. } => {
                    for (_, fields) in variants {
                        budget.spend(1, "semantic geometry input directory")?;
                        for child in fields {
                            Self::register_child(&mut pending, child, child_depth, budget)?;
                        }
                    }
                }
                Type::ErrorUnion { error, result } => {
                    Self::register_child(&mut pending, error, child_depth, budget)?;
                    Self::register_child(&mut pending, result, child_depth, budget)?;
                }
                Type::FunctionPointer { parameters, result }
                | Type::BorrowedFunctionPointer {
                    parameters, result, ..
                }
                | Type::ForeignCallback { parameters, result } => {
                    Self::register_child(&mut pending, result, child_depth, budget)?;
                    for child in parameters {
                        Self::register_child(&mut pending, child, child_depth, budget)?;
                    }
                }
                Type::Dyn { .. }
                | Type::OwnedDyn { .. }
                | Type::Unit
                | Type::Bool
                | Type::I32
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::I8
                | Type::I16
                | Type::Isize
                | Type::ByteBuffer
                | Type::ForeignHandle
                | Type::SyncCell(_)
                | Type::TaskI32
                | Type::CWideString
                | Type::Named(_)
                | Type::Closure(_)
                | Type::Unsupported(_) => {}
            }
        }
        Ok(())
    }
    fn register_life(&mut self, life: &StorageLifeShape, budget: &mut Budget) -> CompileResult<()> {
        budget.lookup(self.lives.len(), "semantic geometry domain registration")?;
        self.lives.insert(life as *const StorageLifeShape as usize);
        Ok(())
    }
    fn register_child<'t>(
        pending: &mut Vec<(&'t Type, usize)>,
        ty: &'t Type,
        depth: usize,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        if pending.len() >= budget.helper_nodes {
            return Err(registration_limit(
                "semantic geometry input pending count exceeded",
            ));
        }
        let relocation = if pending.len() == pending.capacity() {
            pending.len()
        } else {
            0
        };
        budget.spend(
            relocation.saturating_add(1),
            "semantic geometry input pending allocation",
        )?;
        pending
            .try_reserve(1)
            .map_err(|_| registration_limit("semantic geometry input allocation failed"))?;
        pending.push((ty, depth));
        Ok(())
    }
    pub(super) fn type_value(
        &self,
        ty: &Type,
        life: Option<&StorageLifeShape>,
        budget: &mut Budget,
    ) -> CompileResult<Arc<Geometry>> {
        self.resolved(ty, life.unwrap_or(&self.empty), budget)
    }
    pub(super) fn check_type(&self, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
        // Flat atoms are cheaper to walk than to query in the input directory.
        if ty.is_b0_scalar()
            || matches!(
                ty,
                Type::ByteBuffer
                    | Type::ForeignHandle
                    | Type::SyncCell(_)
                    | Type::TaskI32
                    | Type::CWideString
                    | Type::Closure(_)
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
            )
        {
            return crate::dyn_metadata::ty(ty, budget);
        }
        if self.module != budget.geometry_module {
            return Err(fail("geometry source changes its exact immutable module"));
        }
        if self.raw_limits == (budget.partial_depth, budget.helper_nodes) {
            budget.lookup(
                self.type_roots.borrow().len(),
                "semantic geometry input Type bounds",
            )?;
            if let Some(owner) = self.type_roots.borrow().get(&(ty as *const Type as usize)) {
                intern::owned(owner, budget)?;
                return Ok(());
            }
        }
        crate::dyn_metadata::ty(ty, budget)
    }
    pub(super) fn resolved(
        &self,
        ty: &Type,
        life: &StorageLifeShape,
        budget: &mut Budget,
    ) -> CompileResult<Arc<Geometry>> {
        if self.module != budget.geometry_module {
            return Err(fail("geometry source changes its exact immutable module"));
        }
        // The empty domain has no leaves, classes or nominal arguments. Its
        // identity survives moves of Inputs and fresh empty values at call sites.
        let empty_domain = life == &self.empty;
        let key = (
            ty as *const Type as usize,
            (!empty_domain).then_some(life as *const StorageLifeShape as usize),
        );
        // Only immutable input coordinates are ever published in this map.
        // A hit therefore already authenticates both roots and their limits;
        // do not repeat two source-membership and an owned-root tree search.
        if self.raw_limits == (budget.partial_depth, budget.helper_nodes) {
            // Empty-domain scalar values have no declaration, member position,
            // loan or cleanup obligation. The same scalar from any source
            // coordinate has exactly the same immutable representation.
            if ty.is_b0_scalar() && empty_domain {
                let mut values = self.scalars.borrow_mut();
                for value in values.iter() {
                    budget.spend(1, "semantic scalar lookup")?;
                    if &value.ty == ty {
                        return Ok(Arc::clone(value));
                    }
                }
                let value = intern::borrowed(ty, life, budget)?;
                let relocation = if values.len() == values.capacity() {
                    values.len()
                } else {
                    0
                };
                budget.spend(1 + relocation, "semantic scalar publication")?;
                values.push(Arc::clone(&value));
                return Ok(value);
            }
            budget.lookup(
                self.resolved.borrow().len(),
                "semantic geometry source lookup",
            )?;
            if let Some(value) = self.resolved.borrow().get(&key) {
                budget.spend(1, "semantic geometry shared result")?;
                return Ok(Arc::clone(value));
            }
        }
        // Current values may already point into an owned immutable geometry.
        // Keep that exact owner; this is neither an address-only borrowed cache
        // nor an inference about any current class vector.
        budget.lookup(
            budget.geometry_types.len(),
            "semantic geometry owned root lookup",
        )?;
        budget.spend(1, "semantic geometry shared result")?;
        let existing = budget
            .geometry_types
            .get(&key.0)
            .filter(|entry| {
                std::ptr::eq(&entry.owner.ty, ty)
                    && entry.raw_limits == (budget.partial_depth, budget.helper_nodes)
                    && entry.module == budget.geometry_module
            })
            .map(|entry| Arc::clone(&entry.owner));
        if let Some(existing) = existing {
            if std::ptr::eq(life, &existing.life) || (empty_domain && existing.life == self.empty) {
                return Ok(existing);
            }
            return intern::with_life(&existing, life, budget);
        }
        budget.lookup(self.types.len(), "semantic geometry Type source lookup")?;
        budget.lookup(self.lives.len(), "semantic geometry domain source lookup")?;
        let registered_type = self.raw_limits == (budget.partial_depth, budget.helper_nodes)
            && self.types.contains(&key.0);
        let registered =
            registered_type && key.1.is_none_or(|address| self.lives.contains(&address));
        let type_root = if registered_type {
            budget.lookup(
                self.type_roots.borrow().len(),
                "semantic geometry input Type snapshot",
            )?;
            budget.spend(1, "semantic geometry shared result")?;
            self.type_roots.borrow().get(&key.0).cloned()
        } else {
            None
        };
        let value = if let Some(owner) = type_root {
            intern::with_life(&owner, life, budget)?
        } else {
            let value = copy_geometry(ty, life, budget)?;
            if registered_type {
                self.remember_input_type(ty, &value, budget)?;
                budget.lookup(
                    self.type_roots.borrow().len(),
                    "semantic geometry input Type publication",
                )?;
                budget.spend(1, "semantic geometry shared result")?;
                self.type_roots
                    .borrow_mut()
                    .insert(key.0, Arc::clone(&value));
            }
            value
        };
        if registered {
            budget.lookup(
                self.resolved.borrow().len(),
                "semantic geometry source publication",
            )?;
            budget.spend(1, "semantic geometry shared result")?;
            retain(&value, budget)?;
            self.resolved.borrow_mut().insert(key, Arc::clone(&value));
        }
        Ok(value)
    }
    fn remember_input_type(
        &self,
        ty: &Type,
        owner: &Arc<Geometry>,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        // The caller has checked membership in self.types, then bounded and
        // compared this actual Type in the request-local interner.
        let Some((module, roots)) = &budget.input_type_coordinates else {
            return Ok(());
        };
        if Some(*module) != self.module || !Arc::ptr_eq(roots, &self.types) {
            return Ok(());
        }
        let receipt = intern::owned(owner, budget)?;
        let key = ty as *const Type as usize;
        budget.lookup(
            budget.geometry_types.len(),
            "immutable input Type receipt publication",
        )?;
        budget.spend(2, "immutable input Type retained proof")?;
        budget.geometry_types.insert(
            key,
            TypeEntry {
                owner: Arc::clone(owner),
                raw_work: receipt.raw_work,
                raw_limits: self.raw_limits,
                module: self.module,
                nominal: None,
                type_id: receipt.type_id,
                coordinate: intern::Node::Root,
                input: Some(Arc::clone(&self.types)),
            },
        );
        Ok(())
    }
    pub(super) fn nominal(
        &self,
        position: usize,
        module: &TypedModule,
        budget: &mut Budget,
    ) -> CompileResult<Arc<Geometry>> {
        if self.module != Some(module as *const TypedModule as usize) {
            return Err(fail("nominal geometry changes its exact immutable module"));
        }
        let same_limits = self.raw_limits == (budget.partial_depth, budget.helper_nodes);
        budget.lookup(
            self.nominals.borrow().len(),
            "semantic nominal geometry lookup",
        )?;
        if same_limits && let Some(value) = self.nominals.borrow().get(&position) {
            budget.spend(1, "semantic geometry shared result")?;
            return Ok(Arc::clone(value));
        }
        let instance = module
            .life_instances
            .get(position)
            .ok_or_else(|| fail("nominal geometry has no exact registered row"))?;
        let empty = value::empty();
        let life = match &instance.ty {
            Type::BorrowedStorage { life, .. } => life,
            _ => &empty,
        };
        let value = copy_geometry(&instance.ty, life, budget)?;
        budget.lookup(
            self.nominals.borrow().len(),
            "semantic nominal geometry publication",
        )?;
        budget.spend(1, "semantic geometry shared result")?;
        retain(&value, budget)?;
        if same_limits {
            self.nominals
                .borrow_mut()
                .insert(position, Arc::clone(&value));
        }
        Ok(value)
    }
}
fn copy_geometry(
    ty: &Type,
    life: &StorageLifeShape,
    budget: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    intern::borrowed(ty, life, budget)
}
fn retain(value: &Arc<Geometry>, budget: &mut Budget) -> CompileResult<()> {
    intern::owned(value, budget)?;
    Ok(())
}
pub(super) fn same_geometry(
    left: &Arc<Geometry>,
    right: &Arc<Geometry>,
    budget: &mut Budget,
) -> CompileResult<bool> {
    Ok(identity(left, budget)? == identity(right, budget)?)
}
pub(super) fn identity(owner: &Arc<Geometry>, budget: &mut Budget) -> CompileResult<usize> {
    Ok(intern::owned(owner, budget)?.geometry_id)
}
pub(crate) fn type_identity(ty: &Type, budget: &mut Budget) -> CompileResult<usize> {
    intern::type_identity(ty, budget)
}
pub(super) fn check_geometry(owner: &Arc<Geometry>, budget: &mut Budget) -> CompileResult<()> {
    intern::owned(owner, budget)?;
    Ok(())
}
pub(super) fn retain_payload(
    parent: &Type,
    payload: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    intern::retain_payload(parent, payload, budget)
}
pub(crate) fn bounded_work(ty: &Type, budget: &mut Budget) -> CompileResult<Option<usize>> {
    Ok(registered_type(ty, budget)?.map(|(_, work)| work))
}
fn registered_type(ty: &Type, budget: &mut Budget) -> CompileResult<Option<(usize, usize)>> {
    budget.lookup(
        budget.geometry_types.len(),
        "semantic geometry bounded root query",
    )?;
    let entry = budget
        .geometry_types
        .get(&(ty as *const Type as usize))
        .filter(|entry| {
            entry.raw_limits == (budget.partial_depth, budget.helper_nodes)
                && entry.module == budget.geometry_module
        });
    let Some(entry) = entry else {
        return Ok(None);
    };
    let exact = match &entry.input {
        Some(roots) => budget
            .input_type_coordinates
            .as_ref()
            .is_some_and(|(module, current)| {
                Some(*module) == entry.module && Arc::ptr_eq(current, roots)
            }),
        None => std::ptr::eq(intern::node(&entry.owner, entry.coordinate)?, ty),
    };
    Ok(exact.then_some((entry.type_id, entry.raw_work)))
}

/// Exact Type equality only. Cached roots are owned until this request ends;
/// class vectors, providers and permissions are not part of this catalogue.
pub(super) fn same_type(left: &Type, right: &Type, budget: &mut Budget) -> CompileResult<bool> {
    if left.is_b0_scalar() && right.is_b0_scalar() {
        budget.spend(1, "semantic scalar comparison")?;
        return Ok(left == right);
    }
    let left_entry = registered_type(left, budget)?;
    let right_entry = registered_type(right, budget)?;
    if let (Some((left, _)), Some((right, _))) = (left_entry, right_entry) {
        return Ok(left == right);
    }
    let left_work = left_entry.map(|(_, work)| work);
    let right_work = right_entry.map(|(_, work)| work);
    let left_work = match left_work {
        Some(work) => work,
        None => {
            let before = budget.remaining();
            crate::dyn_metadata::ty(left, budget)?;
            before - budget.remaining()
        }
    };
    let right_work = match right_work {
        Some(work) => work,
        None => {
            let before = budget.remaining();
            crate::dyn_metadata::ty(right, budget)?;
            before - budget.remaining()
        }
    };
    budget.spend(
        left_work.max(right_work),
        "complete Type equality comparison",
    )?;
    let equal = left == right;
    Ok(equal)
}

/// The coordinate selects a real child of the immutable physical nominal.
/// A success says only that this complete child Type equals the current Type.
pub(super) fn field_equal(
    owner: &Arc<Geometry>,
    variant: Option<usize>,
    field: usize,
    expected: &Arc<Geometry>,
    budget: &mut Budget,
) -> CompileResult<bool> {
    intern::field_equal(owner, variant, field, expected, budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_inputs_reuse_only_complete_structural_identity() {
        let inputs = Inputs::default();
        let mut budget = Budget::new(&zil_base::Limits::default());
        let mut ty = Type::I32;
        let life = value::empty();
        let first = inputs.resolved(&ty, &life, &mut budget).unwrap();
        let second = inputs.resolved(&ty, &life, &mut budget).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(inputs.resolved.borrow().is_empty());
        assert!(inputs.type_roots.borrow().is_empty());
        assert!(
            !budget
                .geometry_types
                .contains_key(&(&ty as *const Type as usize))
        );
        ty = Type::U32;
        let changed = inputs.resolved(&ty, &life, &mut budget).unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        assert_eq!(first.ty, Type::I32);
        assert_eq!(changed.ty, Type::U32);
        assert!(inputs.type_roots.borrow().is_empty());

        // A registered immutable Type can be reused with a freshly supplied
        // complete domain. The snapshot keeps its actual first domain rather
        // than creating an artificial N0 value for the Type directory.
        let input_type = Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(0))),
        };
        let mut registered = Inputs {
            raw_limits: (budget.partial_depth, budget.helper_nodes),
            ..Inputs::default()
        };
        registered.register_type(&input_type, &mut budget).unwrap();
        let distinct = StorageLifeShape {
            binder_count: 2,
            slots: vec![StorageLifeSlot {
                path: Vec::new(),
                binder: 0,
                mode: StoredBorrowMode::SharedAddress,
            }],
            retained: vec![StorageRetainedBinding {
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
                binder: 1,
            }],
            nominal_arguments: Vec::new(),
        };
        let mut merged = distinct.clone();
        merged.binder_count = 1;
        merged.retained[0].binder = 0;
        let separate = registered
            .resolved(&input_type, &distinct, &mut budget)
            .unwrap();
        let together = registered
            .resolved(&input_type, &merged, &mut budget)
            .unwrap();
        assert_eq!(separate.ty, together.ty);
        assert_eq!(separate.life, distinct);
        assert_eq!(together.life, merged);
        assert!(!Arc::ptr_eq(&separate, &together));
        assert_eq!(registered.type_roots.borrow().len(), 1);
        assert_eq!(
            registered.type_roots.borrow().values().next().unwrap().life,
            distinct
        );
        assert!(registered.resolved.borrow().is_empty());
        let before_move = registered
            .resolved(&input_type, &value::empty(), &mut budget)
            .unwrap();
        let moved = Box::new(registered);
        let after_move = moved
            .resolved(&input_type, &value::empty(), &mut budget)
            .unwrap();
        assert!(Arc::ptr_eq(&before_move, &after_move));
        assert!(!Arc::ptr_eq(&before_move, &together));
        assert_eq!(moved.resolved.borrow().len(), 1);
        budget.partial_depth = 0;
        assert!(moved.resolved(&input_type, &merged, &mut budget).is_err());
    }
    #[test]
    fn complete_type_receipts_require_the_published_input_coordinate_and_limits() {
        let mut b = Budget::new(&zil_base::Limits::default());
        let ty = Type::Tuple(vec![Type::I32, Type::U64]);
        let other = Type::Tuple(vec![Type::U32, Type::U64]);
        let scope = 17;
        let mut inputs = Inputs {
            module: Some(scope),
            raw_limits: (b.partial_depth, b.helper_nodes),
            ..Inputs::default()
        };
        inputs.register_type(&ty, &mut b).unwrap();
        inputs.register_type(&other, &mut b).unwrap();
        b.geometry_module = inputs.module;
        b.input_type_coordinates = Some((scope, Arc::clone(&inputs.types)));
        let actual = inputs.type_value(&ty, None, &mut b).unwrap();
        let sibling = inputs.type_value(&other, None, &mut b).unwrap();
        let first = registered_type(&ty, &mut b).unwrap().unwrap().0;
        assert_eq!(first, intern::owned(&actual, &mut b).unwrap().type_id);
        assert_ne!(first, intern::owned(&sibling, &mut b).unwrap().type_id);
        assert!(registered_type(&ty.clone(), &mut b).unwrap().is_none());

        let limits = b.partial_depth;
        b.partial_depth = limits - 1;
        assert!(registered_type(&ty, &mut b).unwrap().is_none());
        b.partial_depth = limits;
        b.input_type_coordinates = Some((scope, Arc::new((*inputs.types).clone())));
        assert!(registered_type(&ty, &mut b).unwrap().is_none());
        assert!(registered_type(&actual.ty, &mut b).unwrap().is_some());
    }
    #[test]
    fn physical_field_equalities_keep_complete_coordinate_and_type_identity() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let owner = copy_geometry(
            &Type::Record {
                name: "Pair".into(),
                fields: vec![("first".into(), Type::I32), ("second".into(), Type::U32)],
            },
            &value::empty(),
            &mut budget,
        )
        .unwrap();
        let expected = copy_geometry(&Type::I32, &value::empty(), &mut budget).unwrap();
        retain(&owner, &mut budget).unwrap();
        retain(&expected, &mut budget).unwrap();
        assert!(field_equal(&owner, None, 0, &expected, &mut budget).unwrap());
        assert!(!field_equal(&owner, None, 1, &expected, &mut budget).unwrap());
        assert!(field_equal(&owner, None, 0, &expected, &mut budget).unwrap());
        assert!(field_equal(&owner, None, 2, &expected, &mut budget).is_err());
        let mut temporary = Arc::new(Geometry::new(Type::I32, value::empty()));
        assert!(field_equal(&owner, None, 0, &temporary, &mut budget).unwrap());
        // A comparison now retains this real immutable owner, not an address
        // of a caller-owned temporary. Mutation must create another owner.
        assert!(Arc::strong_count(&temporary) > 1);
        let original = Arc::clone(&temporary);
        let changed = Arc::make_mut(&mut temporary);
        changed.ty = Type::U32;
        changed.invalidate();
        assert!(!Arc::ptr_eq(&original, &temporary));
        assert_eq!(original.ty, Type::I32);
        assert!(!field_equal(&owner, None, 0, &temporary, &mut budget).unwrap());
        assert!(field_equal(&owner, None, 0, &original, &mut budget).unwrap());

        let mut other_budget = Budget::new(&zil_base::Limits::default());
        let _ = copy_geometry(&Type::F64, &value::empty(), &mut other_budget).unwrap();
        let other_epoch = copy_geometry(&Type::U32, &value::empty(), &mut other_budget).unwrap();
        assert!(!field_equal(&owner, None, 0, &other_epoch, &mut budget).unwrap());

        let mut oversized = value::empty();
        oversized.binder_count = 1;
        oversized.slots.push(StorageLifeSlot {
            path: vec![zil_ir::PlaceProjection::Field(0); budget.partial_depth + 1],
            binder: 0,
            mode: StoredBorrowMode::SharedAddress,
        });
        let unbounded = Arc::new(Geometry::new(Type::I32, oversized));
        assert!(field_equal(&owner, None, 0, &unbounded, &mut budget).is_err());
    }

    #[test]
    fn payload_identity_is_shared_only_after_checking_each_actual_parent() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let store = |element| Type::Managed(Box::new(Type::OwnedStore { element: Box::new(element) }));
        let values = [Type::I64, Type::I64, Type::U64].map(|element|
            Arc::new(Geometry::new(store(element), value::empty())));
        let mut identities = Vec::new();
        for owner in &values {
            retain(owner, &mut budget).unwrap();
            let Type::Managed(payload) = &owner.ty else { unreachable!() };
            retain_payload(&owner.ty, payload, &mut budget).unwrap();
            identities.push(registered_type(payload, &mut budget).unwrap().unwrap().0);
        }
        assert_eq!(identities[0], identities[1]);
        assert_ne!(identities[0], identities[2]);
        let Type::Managed(other) = &values[1].ty else { unreachable!() };
        assert!(retain_payload(&values[0].ty, other, &mut budget).is_err());
        assert_eq!(budget.geometry_child_types.len(), 2);
    }

    #[test]
    fn immutable_raw_bounds_do_not_cross_changed_request_limits() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let geometry = copy_geometry(
            &Type::Array {
                element: Box::new(Type::I32),
                length: 2,
            },
            &value::empty(),
            &mut budget,
        )
        .unwrap();
        retain(&geometry, &mut budget).unwrap();
        assert!(bounded_work(&geometry.ty, &mut budget).unwrap().is_some());
        budget.partial_depth = 0;
        assert!(bounded_work(&geometry.ty, &mut budget).unwrap().is_none());
        assert!(crate::dyn_metadata::ty(&geometry.ty, &mut budget).is_err());

        // A constructor alias built at the old profile does not authenticate
        // its deeper parent under a newly reduced depth limit either.
        budget.partial_depth = zil_base::Limits::default().max_call_depth;
        let child = copy_geometry(&Type::I32, &value::empty(), &mut budget).unwrap();
        construct(
            Constructor::Unary(Unary::Address(false)),
            &[&child],
            value::empty(),
            &mut budget,
        )
        .unwrap();
        budget.partial_depth = 0;
        assert!(
            construct(
                Constructor::Unary(Unary::Address(false)),
                &[&child],
                value::empty(),
                &mut budget
            )
            .is_err()
        );
    }
}
