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

pub(crate) struct TypeEntry {
    // Retaining the owner makes every registered address request-stable.
    pub owner: Arc<Geometry>,
    pub raw_work: usize,
    pub raw_limits: (usize, usize),
    module: Option<usize>,
    pub nominal: Option<(usize, usize, usize)>, // module, type-depth limit, row
    type_id: usize,
}

pub(super) struct Inputs {
    module: Option<usize>,
    raw_limits: (usize, usize),
    types: BTreeSet<usize>,
    lives: BTreeSet<usize>,
    empty: StorageLifeShape,
    resolved: std::cell::RefCell<BTreeMap<(usize, usize), Arc<Geometry>>>,
    nominals: std::cell::RefCell<BTreeMap<usize, Arc<Geometry>>>,
}
impl Default for Inputs {
    fn default() -> Self {
        Self {
            module: None,
            raw_limits: (0, 0),
            types: BTreeSet::new(),
            lives: BTreeSet::new(),
            empty: value::empty(),
            resolved: std::cell::RefCell::new(BTreeMap::new()),
            nominals: std::cell::RefCell::new(BTreeMap::new()),
        }
    }
}
struct Register<'a> {
    inputs: &'a mut Inputs,
    budget: &'a mut Budget,
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
        self.budget.lookup(
            self.inputs.types.len(),
            "semantic geometry Type registration",
        )?;
        self.inputs.types.insert(value as *const Type as usize);
        Ok(())
    }
    fn life(&mut self, value: &StorageLifeShape, _depth: usize) -> CompileResult<()> {
        self.budget.lookup(
            self.inputs.lives.len(),
            "semantic geometry domain registration",
        )?;
        self.inputs
            .lives
            .insert(value as *const StorageLifeShape as usize);
        Ok(())
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
        // This existing structural visitor only records exact immutable roots.
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
                        budget
                            .lookup(inputs.lives.len(), "semantic geometry domain registration")?;
                        inputs
                            .lives
                            .insert(life as *const StorageLifeShape as usize);
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
            for ty in &closure.captures {
                inputs.register_type(ty, budget)?;
            }
        }
        for resource in &module.resources {
            inputs.register_type(&resource.payload, budget)?;
        }
        Ok(inputs)
    }
    fn register_type(&mut self, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
        budget.lookup(self.types.len(), "semantic geometry Type registration")?;
        self.types.insert(ty as *const Type as usize);
        if let Type::BorrowedStorage { life, .. } = ty {
            budget.lookup(self.lives.len(), "semantic geometry domain registration")?;
            self.lives.insert(life as *const StorageLifeShape as usize);
        }
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
    pub(super) fn resolved(
        &self,
        ty: &Type,
        life: &StorageLifeShape,
        budget: &mut Budget,
    ) -> CompileResult<Arc<Geometry>> {
        if self.module != budget.geometry_module {
            return Err(fail("geometry source changes its exact immutable module"));
        }
        let key = (
            ty as *const Type as usize,
            life as *const StorageLifeShape as usize,
        );
        // Only immutable input coordinates are ever published in this map.
        // A hit therefore already authenticates both roots and their limits;
        // do not repeat two source-membership and an owned-root tree search.
        if self.raw_limits == (budget.partial_depth, budget.helper_nodes) {
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
            if std::ptr::eq(life, &existing.life) {
                return Ok(existing);
            }
            crate::dyn_metadata::shape(life, budget)?;
            crate::dyn_metadata::shape(&existing.life, budget)?;
            if existing.life == *life {
                return Ok(existing);
            }
        }
        budget.lookup(self.types.len(), "semantic geometry Type source lookup")?;
        budget.lookup(self.lives.len(), "semantic geometry domain source lookup")?;
        let registered = self.raw_limits == (budget.partial_depth, budget.helper_nodes)
            && self.types.contains(&key.0)
            && (self.lives.contains(&key.1) || std::ptr::eq(life, &self.empty));
        let value = copy_geometry(ty, life, budget)?;
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
    let left = intern::owned(left, budget)?;
    let right = intern::owned(right, budget)?;
    Ok(left.geometry_id == right.geometry_id)
}
pub(crate) fn bounded_work(ty: &Type, budget: &mut Budget) -> CompileResult<Option<usize>> {
    Ok(registered_type(ty, budget)?.map(|(_, work)| work))
}
fn registered_type(ty: &Type, budget: &mut Budget) -> CompileResult<Option<(usize, usize)>> {
    budget.lookup(
        budget.geometry_types.len(),
        "semantic geometry bounded root query",
    )?;
    Ok(budget
        .geometry_types
        .get(&(ty as *const Type as usize))
        .filter(|entry| {
            std::ptr::eq(&entry.owner.ty, ty)
                && entry.raw_limits == (budget.partial_depth, budget.helper_nodes)
                && entry.module == budget.geometry_module
        })
        .map(|entry| (entry.type_id, entry.raw_work)))
}

/// Exact Type equality only. Cached roots are owned until this request ends;
/// class vectors, providers and permissions are not part of this catalogue.
pub(super) fn same_type(left: &Type, right: &Type, budget: &mut Budget) -> CompileResult<bool> {
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
        let registered_roots = budget.geometry_types.len();
        assert!(field_equal(&owner, None, 0, &temporary, &mut budget).unwrap());
        assert_eq!(budget.geometry_types.len(), registered_roots);
        assert_eq!(Arc::strong_count(&temporary), 1);
        // The same temporary address may later hold another complete Type;
        // it acquired no borrowed-pointer cache entry from this comparison.
        let changed = Arc::get_mut(&mut temporary).unwrap();
        changed.ty = Type::U32;
        changed.invalidate();
        assert!(!field_equal(&owner, None, 0, &temporary, &mut budget).unwrap());
        assert_eq!(budget.geometry_types.len(), registered_roots);

        let mut other_budget = Budget::new(&zil_base::Limits::default());
        let _ = copy_geometry(&Type::F64, &value::empty(), &mut other_budget).unwrap();
        let other_epoch = copy_geometry(&Type::U32, &value::empty(), &mut other_budget).unwrap();
        assert!(!field_equal(&owner, None, 0, &other_epoch, &mut budget).unwrap());

        let mut oversized = value::empty();
        oversized.binder_count = 1;
        oversized.slots.push(StorageLifeSlot {
            path: vec![LifeTypeProjection::Pointee; budget.partial_depth + 1],
            binder: 0,
            mode: StoredBorrowMode::SharedAddress,
        });
        let unbounded = Arc::new(Geometry::new(Type::I32, oversized));
        assert!(field_equal(&owner, None, 0, &unbounded, &mut budget).is_err());
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
    }
}
