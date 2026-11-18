//! Complete ghost type values. Classes identify semantic occurrences only;
//! this module creates no target, loan, initialized value or cleanup ability.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Class {
    Formal(SemanticInstanceId, u32),
    Implicit(SemanticInstanceId, u32),
    Invocation(SemanticInstanceId, u32),
    TypeArgument(SemanticInstanceId, u32, u32),
    Nominal(u32, u32),
    // Ephemeral canonical-shape atoms; never produced by a public class
    // expression and never transported as an original semantic identity.
    Local(u32),
    // Signature-local invocation symbols, never an enclosing function's
    // Invocation domain and never a premise for nominal equality.
    Role(SemanticInstanceId, u32),
    // Every actual borrow/reborrow producer has its own outer occurrence. Its
    // source is read only from that exact instruction at a real consumer.
    Reborrow(FunctionId, ValueId),
    // Exact caller/original-call/callee-declaration role. Actual SSA selection
    // must separately match that immutable call receipt.
    CallResult(
        SemanticInstanceId,
        zil_base::NodeId,
        SemanticDefinitionId,
        u32,
    ),
}
#[derive(Clone, Debug)]
pub(super) struct Value {
    geometry: std::sync::Arc<geometry::Geometry>,
    pub classes: Vec<Class>,
}
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        (std::sync::Arc::ptr_eq(&self.geometry, &other.geometry) || self.geometry == other.geometry)
            && self.classes == other.classes
    }
}
impl Eq for Value {}
impl std::ops::Deref for Value {
    type Target = geometry::Geometry;
    fn deref(&self) -> &Self::Target {
        &self.geometry
    }
}
impl Value {
    /// Select the exact original child geometry from a fully reconstructed
    /// class-free Tuple. No source, initialization or permission is produced.
    pub(super) fn empty_tuple_child(&self, field: u32, b: &mut Budget) -> CompileResult<Self> {
        if !empty_shape(&self.life) || !self.classes.is_empty()
            || !matches!(self.ty, Type::Tuple(_)) {
            return Err(fail("structural child requires its complete zero-domain Tuple"));
        }
        let child = geometry::project(&self.geometry,
            geometry::Child::Field(None, field as usize), &empty(), b)?;
        Ok(Self::from_geometry(child, Vec::new()))
    }
    pub(super) fn check_geometry(&self, b: &mut Budget) -> CompileResult<()> {
        geometry::check_geometry(&self.geometry, b)
    }
    pub(super) fn from_geometry(
        geometry: std::sync::Arc<geometry::Geometry>,
        classes: Vec<Class>,
    ) -> Self {
        Self { geometry, classes }
    }
    pub(super) fn new(ty: Type, life: StorageLifeShape, classes: Vec<Class>) -> Self {
        Self {
            geometry: std::sync::Arc::new(geometry::Geometry::new(ty, life)),
            classes,
        }
    }
    #[track_caller]
    pub(super) fn into_parts(
        self,
        b: &mut Budget,
    ) -> CompileResult<(Type, StorageLifeShape, Vec<Class>)> {
        let geometry = match std::sync::Arc::try_unwrap(self.geometry) {
            Ok(geometry) => geometry,
            Err(geometry) => {
                crate::dyn_metadata::ty(&geometry.ty, b)?;
                crate::dyn_metadata::shape(&geometry.life, b)?;
                geometry.as_ref().clone()
            }
        };
        Ok((geometry.ty, geometry.life, self.classes))
    }
    #[track_caller]
    pub(super) fn into_type(self, b: &mut Budget) -> CompileResult<Type> {
        match std::sync::Arc::try_unwrap(self.geometry) {
            Ok(geometry) => Ok(geometry.ty),
            Err(geometry) => {
                crate::dyn_metadata::ty(&geometry.ty, b)?;
                Ok(geometry.ty.clone())
            }
        }
    }
    pub(super) fn geometry_mut(
        &mut self,
        b: &mut Budget,
    ) -> CompileResult<&mut geometry::Geometry> {
        b.spend(1, WORK)?;
        if std::sync::Arc::strong_count(&self.geometry) != 1 {
            crate::dyn_metadata::ty(&self.geometry.ty, b)?;
            crate::dyn_metadata::shape(&self.geometry.life, b)?;
        }
        let geometry = std::sync::Arc::make_mut(&mut self.geometry);
        geometry.invalidate();
        Ok(geometry)
    }
    pub(super) fn field_equal(
        &self,
        owner: &std::sync::Arc<geometry::Geometry>,
        variant: Option<usize>,
        field: usize,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        geometry::field_equal(owner, variant, field, &self.geometry, b)
    }
    pub(super) fn equal_paid(&self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.classes
                .len()
                .saturating_add(other.classes.len())
                .saturating_add(1),
            WORK,
        )?;
        if self.classes != other.classes {
            return Ok(false);
        }
        geometry::same_geometry(&self.geometry, &other.geometry, b)
    }
    pub(super) fn with_classes(&self, classes: Vec<Class>, b: &mut Budget) -> CompileResult<Self> {
        b.spend(1, WORK)?;
        Ok(Self {
            geometry: std::sync::Arc::clone(&self.geometry),
            classes,
        })
    }
    pub(super) fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(self.classes.len().saturating_add(1), WORK)?;
        Ok(self.clone())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Argument {
    Type(Value),
    Const { carrier: Type, value: Constant },
    Life(Class),
}
fn unknown(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::unsupported(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
#[track_caller]
fn put<T>(rows: &mut Vec<T>, value: T, b: &mut Budget) -> CompileResult<()> {
    let relocation = if rows.len() == rows.capacity() {
        rows.len()
    } else {
        0
    };
    b.spend(relocation.saturating_add(1), WORK)?;
    rows.try_reserve(1).map_err(|_| {
        vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,
            "semantic complete-value allocation failed",
            None,
        )]
    })?;
    rows.push(value);
    Ok(())
}
pub(super) fn empty() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: Vec::new(),
        retained: Vec::new(),
        nominal_arguments: Vec::new(),
    }
}
pub(super) fn scalar(ty: Type) -> Value {
    Value::new(ty, empty(), Vec::new())
}
pub(super) fn borrowed_scalar(
    ty: &Type,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Value> {
    Ok(Value::from_geometry(
        inventory.geometry.type_value(ty, None, b)?,
        Vec::new(),
    ))
}
pub(super) fn borrowed_full(
    ty: &Type,
    life: &StorageLifeShape,
    classes: Vec<Class>,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Value> {
    Ok(Value::from_geometry(
        inventory.geometry.resolved(ty, life, b)?,
        classes,
    ))
}
pub(super) fn unary(kind: geometry::Unary, inner: Value, b: &mut Budget) -> CompileResult<Value> {
    crate::dyn_metadata::shape(&inner.life, b)?;
    let life = inner.life.clone();
    let geometry = geometry::construct(
        geometry::Constructor::Unary(kind),
        &[&inner.geometry],
        life,
        b,
    )?;
    Ok(Value::from_geometry(geometry, inner.classes))
}
/// A resource carrier preserves every child coordinate. It has no physical
/// borrow slots of its own; these type paths grant no loan or cleanup right.
pub(super) fn resource(kind: geometry::Unary, inner: Value, b: &mut Budget) -> CompileResult<Value> {
    let prefix = match kind {
        geometry::Unary::OwnedStore => LifeTypeProjection::Element,
        geometry::Unary::Managed | geometry::Unary::Mutex
        | geometry::Unary::Registration | geometry::Unary::AsyncRegistration => LifeTypeProjection::Pointee,
        _ => return Err(fail("resource domain transport changes its constructor family")),
    };
    let life = inherited_domain(&inner.life, prefix, b)?;
    if inner.classes.len() != inner.life.binder_count as usize {
        return Err(fail("resource child omits a complete intrinsic class"));
    }
    let geometry = geometry::construct(geometry::Constructor::Unary(kind), &[&inner.geometry], life, b)?;
    Ok(Value::from_geometry(geometry, inner.classes))
}
fn inherited_domain(child: &StorageLifeShape, prefix: LifeTypeProjection, b: &mut Budget) -> CompileResult<StorageLifeShape> {
    crate::dyn_metadata::shape(child, b)?;
    let mut slots = Vec::new();
    if let LifeTypeProjection::Place(selector) = prefix {
        for slot in &child.slots {
            if slot.binder >= child.binder_count {
                return Err(fail("inline child slot leaves its complete class domain"));
            }
            let mut path = Vec::new();
            put(&mut path, selector, b)?;
            for step in &slot.path { put(&mut path, *step, b)?; }
            put(&mut slots, StorageLifeSlot { path, binder: slot.binder, mode: slot.mode }, b)?;
        }
    }
    let mut retained = Vec::new();
    for class in 0..child.binder_count {
        put(&mut retained, StorageRetainedBinding { path: vec![prefix.clone()], class, binder: class }, b)?;
    }
    for row in &child.retained {
        if row.binder >= child.binder_count { return Err(fail("resource retained child leaves its complete domain")); }
        let mut path = Vec::new();
        put(&mut path, prefix.clone(), b)?;
        for step in &row.path { put(&mut path, step.clone(), b)?; }
        put(&mut retained, StorageRetainedBinding { path, class: row.class, binder: row.binder }, b)?;
    }
    let mut nominal_arguments = Vec::new();
    for row in &child.nominal_arguments {
        if row.binder >= child.binder_count { return Err(fail("resource phantom child leaves its complete domain")); }
        let mut path = Vec::new();
        put(&mut path, prefix.clone(), b)?;
        for step in &row.path { put(&mut path, step.clone(), b)?; }
        put(&mut nominal_arguments, NominalLifeArgument { path, declaration: row.declaration,
            argument: row.argument, binder: row.binder }, b)?;
    }
    Ok(StorageLifeShape { binder_count: child.binder_count, slots, retained, nominal_arguments })
}
/// An inline sum carries the complete checked domain of its qualified success
/// child across either tag. The declaration's zero-domain error is independent.
pub(super) fn qualified_sum_domain(
    ty: &Type, inventory: &Inventory<'_>, b: &mut Budget,
) -> CompileResult<Option<StorageLifeShape>> {
    let Type::ErrorUnion { error, result } = ty else { return Ok(None) };
    let Type::BorrowedStorage { life, .. } = result.as_ref() else { return Ok(None) };
    if !crate::stored_flow::enum_address::signature(result) { return Ok(None); }
    crate::stored_flow::validate_plain_nominal_zero(inventory.module, error, inventory.limits, b)?;
    inherited_domain(life, LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)), b).map(Some)
}
/// Reconstruct the whole opaque Task domain from its complete type inventory,
/// retaining phantom classes even when no physical port names them.
pub(super) fn task_domain(ty: &Type, b: &mut Budget) -> CompileResult<Option<StorageLifeShape>> {
    b.spend(1, WORK)?;
    match ty {
        Type::Task { retained_life, .. } => {
            crate::dyn_metadata::retained_shape(retained_life, b)?;
            for port in &retained_life.ports {
                b.spend(1, WORK)?;
                if port.binder >= retained_life.binder_count { return Err(fail("Task physical port exceeds its complete domain")); }
            }
            for row in &retained_life.retained {
                b.spend(1, WORK)?;
                if row.binder >= retained_life.binder_count { return Err(fail("Task retained row exceeds its complete domain")); }
            }
            for row in &retained_life.nominal_arguments {
                b.spend(1, WORK)?;
                if row.binder >= retained_life.binder_count { return Err(fail("Task phantom row exceeds its complete domain")); }
            }
            Ok(Some(StorageLifeShape { binder_count: retained_life.binder_count, ..empty() }))
        }
        Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { .. }) => {
            let child = task_domain(inner, b)?.ok_or_else(|| fail("Task owner changes its child family"))?;
            Ok(Some(inherited_domain(&child, LifeTypeProjection::Pointee, b)?))
        }
        Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { .. })) => {
            let child = task_domain(result, b)?.ok_or_else(|| fail("Task result carrier changes its child family"))?;
            Ok(Some(inherited_domain(&child, LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)), b)?))
        }
        _ => Ok(None),
    }
}
pub(super) fn tuple(fields: Vec<Value>, b: &mut Budget) -> CompileResult<Value> {
    b.spend(fields.len().saturating_add(1), WORK)?;
    if fields.iter().any(|field| !empty_shape(&field.life) || !field.classes.is_empty()) {
        return qualified_tuple(fields, b);
    }
    let mut children = Vec::new();
    children
        .try_reserve_exact(fields.len())
        .map_err(|_| fail("semantic tuple geometry allocation failed"))?;
    for field in &fields {
        children.push(&field.geometry);
    }
    let geometry = geometry::construct(geometry::Constructor::Tuple, &children, empty(), b)?;
    Ok(Value::from_geometry(geometry, Vec::new()))
}

fn qualified_tuple(fields: Vec<Value>, b: &mut Budget) -> CompileResult<Value> {
    let mut storage = Vec::new();
    let mut classes = Vec::new();
    let mut life = empty();
    for (index, field) in fields.into_iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| fail("qualified Tuple field ordinal overflow"))?;
        let selector = PlaceProjection::Field(index);
        let prefix = LifeTypeProjection::Place(selector);
        let (ty, child, images) = field.into_parts(b)?;
        if child.binder_count as usize != images.len() {
            return Err(fail("qualified Tuple field omits its complete class domain"));
        }
        let mut embedding = Vec::new();
        let mut distinct = BTreeSet::new();
        for class in images {
            let mut binder = intern(&mut classes, class.clone(), b)?;
            b.lookup(distinct.len(), WORK)?;
            if !distinct.insert(binder) {
                binder = u32::try_from(classes.len()).map_err(|_| fail("Tuple class domain overflow"))?;
                put(&mut classes, class, b)?;
                b.lookup(distinct.len(), WORK)?;
                distinct.insert(binder);
            }
            put(&mut embedding, binder, b)?;
        }
        let parent = |binder: u32| embedding.get(binder as usize).copied()
            .ok_or_else(|| fail("Tuple field occurrence leaves its complete class domain"));
        let boundary = matches!(ty, Type::BorrowedStorage { .. } | Type::Closure(_)
            | Type::MutexGuard { .. } | Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. });
        if boundary {
            for (class, binder) in embedding.iter().copied().enumerate() {
                put(&mut life.retained, StorageRetainedBinding {
                    path: vec![prefix],
                    class: u32::try_from(class).map_err(|_| fail("Tuple child class overflow"))?,
                    binder,
                }, b)?;
            }
        } else {
            for row in child.retained {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                let mut path = vec![prefix];
                path.extend(row.path);
                put(&mut life.retained, StorageRetainedBinding {
                    path, class: row.class, binder: parent(row.binder)?,
                }, b)?;
            }
        }
        for row in child.slots {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut path = vec![selector];
            path.extend(row.path);
            put(&mut life.slots, StorageLifeSlot { path, binder: parent(row.binder)?, mode: row.mode }, b)?;
        }
        for row in child.nominal_arguments {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut path = vec![prefix];
            path.extend(row.path);
            put(&mut life.nominal_arguments, NominalLifeArgument {
                path, declaration: row.declaration, argument: row.argument, binder: parent(row.binder)?,
            }, b)?;
        }
        put(&mut storage, ty, b)?;
    }
    life.binder_count = u32::try_from(classes.len()).map_err(|_| fail("Tuple class domain overflow"))?;
    crate::dyn_metadata::shape(&life, b)?;
    let ty = Type::BorrowedStorage { storage: Box::new(Type::Tuple(storage)), life: life.clone() };
    Ok(Value::new(ty, life, classes))
}
/// A sum retains the entire success domain even while its error arm is
/// active. This whole-child directory is a type map, never an active-tag proof.
pub(super) fn error_union(error: Value, result: Value, b: &mut Budget) -> CompileResult<Value> {
    if !error.classes.is_empty() || error.life != empty() {
        return Err(unknown(
            "ErrorUnion needs its complete nonzero error-arm domain",
        ));
    }
    if result.classes.len() != result.life.binder_count as usize {
        return Err(fail("ErrorUnion success omits a complete intrinsic class"));
    }
    let life = inherited_domain(&result.life,
        LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)), b)?;
    let geometry = geometry::construct(
        geometry::Constructor::ErrorUnion,
        &[&error.geometry, &result.geometry],
        life,
        b,
    )?;
    Ok(Value::from_geometry(geometry, result.classes))
}
pub(super) fn intern(rows: &mut Vec<Class>, class: Class, b: &mut Budget) -> CompileResult<u32> {
    b.spend(rows.len().saturating_add(1), WORK)?;
    if let Some(index) = rows.iter().position(|old| old == &class) {
        return u32::try_from(index).map_err(|_| fail("semantic class ordinal overflow"));
    }
    let index = u32::try_from(rows.len()).map_err(|_| fail("semantic class domain overflow"))?;
    put(rows, class, b)?;
    Ok(index)
}
pub(super) fn address(
    mutable: bool,
    class: Class,
    inner: Value,
    b: &mut Budget,
) -> CompileResult<Value> {
    reference(
        geometry::Unary::Address(mutable),
        if mutable {
            StoredBorrowMode::ExclusiveAddress
        } else {
            StoredBorrowMode::SharedAddress
        },
        class,
        inner,
        b,
    )
}
pub(super) fn description(class: Class, inner: Value, b: &mut Budget) -> CompileResult<Value> {
    reference(
        geometry::Unary::Desc,
        StoredBorrowMode::WritableDescription,
        class,
        inner,
        b,
    )
}
fn reference(
    kind: geometry::Unary,
    mode: StoredBorrowMode,
    class: Class,
    inner: Value,
    b: &mut Budget,
) -> CompileResult<Value> {
    if inner.life.binder_count as usize != inner.classes.len() {
        return Err(fail("address child omits its complete intrinsic domain"));
    }
    let mut classes = Vec::new();
    let outer = intern(&mut classes, class, b)?;
    let mut retained = Vec::new();
    let mut child_binders = Vec::new();
    for (index, class) in inner.classes.iter().enumerate() {
        let binder = intern(&mut classes, class.clone(), b)?;
        put(&mut child_binders, binder, b)?;
        put(
            &mut retained,
            StorageRetainedBinding {
                path: vec![LifeTypeProjection::Pointee],
                class: u32::try_from(index)
                    .map_err(|_| fail("address child class ordinal overflow"))?,
                binder,
            },
            b,
        )?;
    }
    // Transport the complete child forest. Direct Pointee rows above retain
    // every intrinsic class, including phantom ones; descendants keep their
    // own ordered paths and complete nominal argument ordinals as well.
    for row in &inner.life.retained {
        let mut path = Vec::new();
        put(&mut path, LifeTypeProjection::Pointee, b)?;
        for step in &row.path { put(&mut path, step.clone(), b)?; }
        let binder = *child_binders.get(row.binder as usize)
            .ok_or_else(|| fail("address child retained row leaves its intrinsic domain"))?;
        put(&mut retained, StorageRetainedBinding { path, class: row.class, binder }, b)?;
    }
    let mut nominal_arguments = Vec::new();
    for row in &inner.life.nominal_arguments {
        let mut path = Vec::new();
        put(&mut path, LifeTypeProjection::Pointee, b)?;
        for step in &row.path { put(&mut path, step.clone(), b)?; }
        let binder = *child_binders.get(row.binder as usize)
            .ok_or_else(|| fail("address child nominal row leaves its intrinsic domain"))?;
        put(&mut nominal_arguments, NominalLifeArgument {
            path, declaration: row.declaration, argument: row.argument, binder,
        }, b)?;
    }
    let binder_count =
        u32::try_from(classes.len()).map_err(|_| fail("address complete domain overflow"))?;
    b.spend(1, WORK)?; // The fresh outer occurrence row is still allocated.
    let geometry = geometry::construct(
        geometry::Constructor::Unary(kind),
        &[&inner.geometry],
        StorageLifeShape {
            binder_count,
            slots: vec![StorageLifeSlot {
                path: Vec::new(),
                binder: outer,
                mode,
            }],

            retained,
            nominal_arguments,
        },
        b,
    )?;
    Ok(Value::from_geometry(geometry, classes))
}

pub(super) fn dyn_domain(
    ty: &Type,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<StorageLifeShape> {
    let (Type::Dyn { interface: id } | Type::OwnedDyn { interface: id }) = ty else {
        return Err(fail("opaque Dyn domain query selects another constructor"));
    };
    let mut selected = None;
    for interface in &inventory.module.dyn_interfaces {
        b.spend(1, WORK)?;
        if interface.id == *id && selected.replace(interface).is_some() {
            return Err(fail(
                "opaque Dyn domain has competing actual interface identities",
            ));
        }
    }
    let interface =
        selected.ok_or_else(|| fail("opaque Dyn domain has no complete actual interface"))?;
    crate::dyn_metadata::retained_shape(&interface.retained_life, b)?;
    Ok(StorageLifeShape {
        binder_count: interface.retained_life.binder_count,
        slots: Vec::new(),
        retained: interface.retained_life.retained.clone(),
        nominal_arguments: interface.retained_life.nominal_arguments.clone(),
    })
}
/// Check only the complete physical forest with fresh binder-coordinate
/// symbols. This supplies no actual class equation or original premise.
pub(super) fn check_address_domain(
    ty: &Type, life: &StorageLifeShape, inventory: &Inventory<'_>, b: &mut Budget,
) -> CompileResult<()> {
    let geometry = inventory.geometry.resolved(ty, life, b)?;
    let mut classes = Vec::new();
    for binder in 0..life.binder_count { put(&mut classes, Class::Local(binder), b)?; }
    pointee(&Value::from_geometry(geometry, classes), inventory, b)?;
    Ok(())
}
// A raw nested reference does not carry a second StorageLifeShape in its
// Type. Recover that exact local shape from the complete parent forest. This
// only reconstructs coordinates; no class image, loan or lifetime is invented.
fn nested_reference_shape(
    child: &Type, parent: &StorageLifeShape, b: &mut Budget,
) -> CompileResult<StorageLifeShape> {
    let mode = match child {
        Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
        Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
        Type::Desc(_) => StoredBorrowMode::WritableDescription,
        _ => return Err(fail("nested reference shape selects another physical constructor")),
    };
    let mut direct = BTreeMap::new();
    for row in &parent.retained {
        b.spend(row.path.len().saturating_add(1), WORK)?;
        if row.path.as_slice() != [LifeTypeProjection::Pointee] { continue; }
        if row.binder >= parent.binder_count || row.class as usize >= b.helper_nodes {
            return Err(fail("nested reference child coordinate is outside its full domain"));
        }
        b.lookup(direct.len(), WORK)?;
        if direct.insert(row.class, row.binder).is_some() {
            return Err(fail("nested reference repeats a complete child class coordinate"));
        }
    }
    let count = u32::try_from(direct.len()).map_err(|_| fail("nested reference child domain overflows"))?;
    let mut reverse = BTreeMap::new();
    for (ordinal, (class, parent)) in direct.iter().enumerate() {
        b.spend(1, WORK)?;
        if *class as usize != ordinal { return Err(fail("nested reference child domain omits an original class")); }
        b.lookup(reverse.len(), WORK)?;
        match reverse.entry(*parent) {
            std::collections::btree_map::Entry::Vacant(entry) => { entry.insert(Some(*class)); }
            std::collections::btree_map::Entry::Occupied(mut entry) => { entry.insert(None); }
        }
    }
    let binder = |parent: u32, b: &mut Budget| -> CompileResult<u32> {
        b.lookup(reverse.len(), WORK)?;
        reverse.get(&parent).copied().flatten().ok_or_else(||
            fail("nested reference child forest has no unique complete local class coordinate"))
    };
    let mut retained = Vec::new();
    for row in &parent.retained {
        b.spend(row.path.len().saturating_add(1), WORK)?;
        let Some(path) = row.path.strip_prefix(&[LifeTypeProjection::Pointee]) else { continue; };
        if path.is_empty() { continue; }
        let mut selected = Vec::new();
        for step in path { put(&mut selected, step.clone(), b)?; }
        put(&mut retained, StorageRetainedBinding { path: selected, class: row.class,
            binder: binder(row.binder, b)? }, b)?;
    }
    let mut nominal_arguments = Vec::new();
    for row in &parent.nominal_arguments {
        b.spend(row.path.len().saturating_add(1), WORK)?;
        let Some(path) = row.path.strip_prefix(&[LifeTypeProjection::Pointee]) else { continue; };
        let mut selected = Vec::new();
        for step in path { put(&mut selected, step.clone(), b)?; }
        put(&mut nominal_arguments, NominalLifeArgument { path: selected,
            declaration: row.declaration, argument: row.argument, binder: binder(row.binder, b)? }, b)?;
    }
    if count == 0 {
        if !super::original::strings::is_byte_type(child) || !retained.is_empty() || !nominal_arguments.is_empty() {
            return Err(fail("nested raw reference omits its real outer class occurrence"));
        }
        return Ok(empty());
    }
    let mut slots = Vec::new();
    put(&mut slots, StorageLifeSlot { path: Vec::new(), binder: 0, mode }, b)?;
    Ok(StorageLifeShape { binder_count: count, slots, retained, nominal_arguments })
}

pub(super) fn pointee(
    input: &Value,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Value> {
    let Type::Address { inner, .. } = &input.ty else {
        return Err(fail("whole pointee query is not an Address"));
    };
    reference_pointee(input, inner, inventory, b)
}

/// Recover the actual qualified domain, including all inherited occurrences,
/// before publishing just its root classes in the signature.
pub(super) fn qualified_reference_header(
    input: &Value, inventory: &Inventory<'_>, b: &mut Budget,
) -> CompileResult<bool> {
    let (Type::Address { inner, .. } | Type::Desc(inner)) = &input.ty else { return Ok(false) };
    if !matches!(inner.as_ref(), Type::BorrowedStorage { .. }) { return Ok(false); }
    reference_pointee(input, inner, inventory, b)?;
    Ok(true)
}

fn reference_pointee(
    input: &Value, inner: &Type, inventory: &Inventory<'_>, b: &mut Budget,
) -> CompileResult<Value> {
    geometry::retain_child(&input.geometry, geometry::Child::Inner, b)?;
    let life = match inner {
        Type::Dyn { .. } | Type::OwnedDyn { .. } => dyn_domain(inner, inventory, b)?,
        Type::MutexGuard { .. } => mutex_guard_domain(inner, b)?,
        Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) =>
            task_domain(inner, b)?.ok_or_else(|| fail("Task pointee has no complete child domain"))?,
        Type::BorrowedStorage { life, .. } => {
            crate::dyn_metadata::shape(life, b)?;
            life.clone()
        }
        Type::Closure(id) => {
            b.spend(inventory.module.closures.len().saturating_add(1), WORK)?;
            let declaration = inventory
                .module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.id == *id)
                .ok_or_else(|| fail("whole pointee has no actual closure inventory"))?;
            StorageLifeShape {
                binder_count: declaration
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("whole pointee closure scope is unknown"))?
                    .fixed_binder_count,
                slots: Vec::new(),
                retained: Vec::new(),
                nominal_arguments: Vec::new(),
            }
        }
        Type::Address { .. } | Type::Desc(_) => nested_reference_shape(inner, &input.life, b)?,
        _ => {
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                inner,
                inventory.limits,
                b,
            )?;
            empty()
        }
    };
    if input.classes.len() != input.life.binder_count as usize {
        return Err(fail("whole address omits its complete class image"));
    }
    let mut classes = Vec::new();
    let mut local_classes = Vec::new();
    for index in 0..life.binder_count {
        let mut selected = None;
        for row in &input.life.retained {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.as_slice() == [LifeTypeProjection::Pointee]
                && row.class == index
                && selected.replace(row.binder).is_some()
            {
                return Err(fail("whole pointee repeats a child class occurrence"));
            }
        }
        let binder = selected.ok_or_else(|| fail("whole pointee omits a child occurrence"))?;
        let class = input
            .classes
            .get(binder as usize)
            .ok_or_else(|| fail("whole pointee class is out of range"))?;
        put(&mut classes, class.clone(), b)?;
        put(&mut local_classes, Class::Local(binder), b)?;
    }
    let geometry = geometry::project(&input.geometry, geometry::Child::Inner, &life, b)?;
    if !(super::original::strings::is_byte_type(&input.ty) && empty_shape(&input.life) && input.classes.is_empty()) {
        let [slot] = input.life.slots.as_slice() else { return Err(fail("whole address has no unique outer slot")); };
        let target = Value::from_geometry(std::sync::Arc::clone(&geometry), local_classes);
        let expected = match input.ty {
            Type::Address { mutable, .. } => address(mutable, Class::Local(slot.binder), target, b)?,
            Type::Desc(_) => description(Class::Local(slot.binder), target, b)?,
            _ => return Err(fail("whole reference has no actual pointer constructor")),
        };
        if expected.life != input.life {
            return Err(fail("whole address changes its complete Pointee class or nominal forest"));
        }
    }
    Ok(Value::from_geometry(geometry, classes))
}

fn mutex_guard_domain(ty: &Type, b: &mut Budget) -> CompileResult<StorageLifeShape> {
    crate::dyn_metadata::ty(ty, b)?;
    let Type::MutexGuard {
        payload,
        retained_life,
    } = ty
    else {
        return Err(fail(
            "intrinsic guard domain selects another type constructor",
        ));
    };
    if payload.as_ref() != &Type::I32
        || retained_life.binder_count != 1
        || !retained_life.retained.is_empty()
        || !retained_life.nominal_arguments.is_empty()
        || !matches!(retained_life.ports.as_slice(), [port]
            if port.leaf == (Type::Mutex { payload: payload.clone() })
                && port.mode == StoredBorrowMode::SharedAddress && port.binder == 0)
    {
        return Err(fail(
            "semantic Mutex guard changes its complete sealed retained inventory",
        ));
    }
    Ok(StorageLifeShape {
        binder_count: 1,
        slots: Vec::new(),
        retained: Vec::new(),
        nominal_arguments: Vec::new(),
    })
}

// Borrow the current full actuals while matching the immutable catalogue.
// Building a temporary public LifeNominalActual used to deep-clone each Type
// solely to compare it and then discard it. No semantic image is cached here.
enum ExpectedActual<'a> {
    Lifetime(u32),
    Type {
        ty: &'a Type,
        life: Option<&'a StorageLifeShape>,
    },
    Value(&'a Value),
    Const(&'a Constant),
}
fn empty_shape(life: &StorageLifeShape) -> bool {
    life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
}
fn actual_equal(actual: &LifeNominalActual, expected: &ExpectedActual<'_>) -> bool {
    if let ExpectedActual::Value(value) = expected {
        return actual_equal(
            actual,
            &ExpectedActual::Type {
                ty: &value.ty,
                life: (!value.classes.is_empty()).then_some(&value.life),
            },
        );
    }
    match (actual, expected) {
        (LifeNominalActual::Lifetime(actual), ExpectedActual::Lifetime(expected)) => {
            actual == expected
        }
        (LifeNominalActual::Const(actual), ExpectedActual::Const(expected)) => actual == *expected,
        (LifeNominalActual::Type(actual), ExpectedActual::Type { ty, life }) => {
            actual == *ty && life.is_none_or(empty_shape)
        }
        (
            LifeNominalActual::QualifiedType {
                ty: actual,
                life: actual_life,
            },
            ExpectedActual::Type { ty, life },
        ) => {
            actual == *ty
                && match life {
                    Some(expected) => actual_life == *expected,
                    None => empty_shape(actual_life),
                }
        }
        _ => false,
    }
}

fn actual_equal_paid(
    actual: &LifeNominalActual,
    expected: &ExpectedActual<'_>,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<bool> {
    let (ty, life) = match actual {
        LifeNominalActual::Type(ty) => (ty, None),
        LifeNominalActual::QualifiedType { ty, life } => (ty, Some(life)),
        _ => return Ok(actual_equal(actual, expected)),
    };
    let (expected_ty, expected_life, expected_owner) = match expected {
        ExpectedActual::Type { ty, life } => (*ty, *life, None),
        ExpectedActual::Value(value) => (
            &value.ty,
            (!value.classes.is_empty()).then_some(&value.life),
            Some(&value.geometry),
        ),
        _ => return Ok(false),
    };
    let geometry = inventory.geometry.type_value(ty, life, b)?;
    let expected = match expected_owner {
        Some(owner) if expected_life.is_some() || empty_shape(&owner.life) => {
            b.spend(1, "semantic geometry shared result")?;
            std::sync::Arc::clone(owner)
        }
        _ => inventory
            .geometry
            .type_value(expected_ty, expected_life, b)?,
    };
    geometry::same_geometry(&geometry, &expected, b)
}

fn physical_nominal_domain<'a>(
    source: &SemanticDefinition,
    declared: &LifeNominalDeclaration,
    declaration: &LifeDeclarationId,
    body: &SemanticNominalBody,
    args: &'a [Argument],
    fields: &[Value],
    b: &mut Budget,
) -> CompileResult<(Vec<Class>, Vec<ExpectedActual<'a>>, StorageLifeShape)> {
    let mut classes = Vec::new();
    let mut nominal_rows = Vec::new();
    let mut retained = Vec::new();
    let mut expected = Vec::new();
    for (ordinal, ((formal, kind), argument)) in source
        .parameters
        .iter()
        .zip(&declared.parameters)
        .zip(args)
        .enumerate()
    {
        let ordinal = u32::try_from(ordinal).map_err(|_| fail("nominal mixed ordinal overflow"))?;
        match (formal, kind, argument) {
            (SemanticFormal::Lifetime, LifeParameterKind::Lifetime, Argument::Life(class)) => {
                let binder = intern(&mut classes, class.clone(), b)?;
                put(&mut expected, ExpectedActual::Lifetime(binder), b)?;
                put(
                    &mut nominal_rows,
                    NominalLifeArgument {
                        path: Vec::new(),
                        declaration: *declaration,
                        argument: ordinal,
                        binder,
                    },
                    b,
                )?;
            }
            (SemanticFormal::Type { .. }, LifeParameterKind::Type, Argument::Type(value)) => {
                for (child, class) in value.classes.iter().enumerate() {
                    let binder = intern(&mut classes, class.clone(), b)?;
                    put(
                        &mut retained,
                        StorageRetainedBinding {
                            path: vec![LifeTypeProjection::TypeArgument(ordinal)],
                            class: u32::try_from(child)
                                .map_err(|_| fail("nominal child class overflow"))?,
                            binder,
                        },
                        b,
                    )?;
                }
                put(
                    &mut expected,
                    ExpectedActual::Value(value),
                    b,
                )?;
            }
            (
                SemanticFormal::Const { .. },
                LifeParameterKind::Const(ty),
                Argument::Const { carrier, value },
            ) if ty == carrier && value.ty() == *carrier => {
                crate::dyn_metadata::ty(carrier, b)?;
                put(&mut expected, ExpectedActual::Const(value), b)?;
            }
            _ => {
                return Err(fail(
                    "nominal value changes an original mixed argument kind or carrier",
                ));
            }
        }
    }
    // Fields can add no unnamed persistent classes. Their external class
    // occurrences must already be witnessed by the declaration's full actuals.
    let mut slots = Vec::new();
    let mut offset = 0usize;
    let mut check_field = |field: &Value,
                           path: PlaceProjection,
                           b: &mut Budget|
     -> CompileResult<()> {
        if field.life.binder_count as usize != field.classes.len() {
            return Err(fail(
                "nominal field omits its complete intrinsic class domain",
            ));
        }
        if matches!(&field.ty,Type::BorrowedStorage {life,..} if life != &field.life) {
            return Err(fail(
                "nested qualified field changes its complete intrinsic shape",
            ));
        }
        if matches!(&field.ty,Type::BorrowedStorage {life,..} if life == &field.life) {
            // A nested nominal/qualified boundary keeps its own full rows.
            // The parent maps each intrinsic child class once, including
            // phantom classes, rather than flattening those rows twice.
            for (child, class) in field.classes.iter().enumerate() {
                b.spend(classes.len().saturating_add(1), WORK)?;
                let binder = classes
                    .iter()
                    .position(|candidate| candidate == class)
                    .ok_or_else(|| fail("nested nominal field has an unwitnessed class"))?;
                put(
                    &mut retained,
                    StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Place(path)],
                        class: u32::try_from(child)
                            .map_err(|_| fail("nested field class ordinal overflow"))?,
                        binder: u32::try_from(binder)
                            .map_err(|_| fail("nested field binder overflow"))?,
                    },
                    b,
                )?;
            }
            return Ok(());
        }
        for slot in &field.life.slots {
            b.spend(slot.path.len().saturating_add(classes.len() + 1), WORK)?;
            let class = field
                .classes
                .get(slot.binder as usize)
                .ok_or_else(|| fail("original field slot class is absent"))?;
            let binder = classes
                .iter()
                .position(|candidate| candidate == class)
                .ok_or_else(|| fail("original field has an unwitnessed persistent class"))?;
            let mut physical = Vec::new();
            put(&mut physical, path, b)?;
            for step in &slot.path {
                put(&mut physical, *step, b)?;
            }
            put(
                &mut slots,
                StorageLifeSlot {
                    path: physical,
                    binder: u32::try_from(binder).map_err(|_| fail("field binder overflow"))?,
                    mode: slot.mode,
                },
                b,
            )?;
        }
        for row in &field.life.retained {
            let class = field
                .classes
                .get(row.binder as usize)
                .ok_or_else(|| fail("nested field retained class is absent"))?;
            b.spend(classes.len().saturating_add(1), WORK)?;
            let binder = classes
                .iter()
                .position(|candidate| candidate == class)
                .ok_or_else(|| fail("nested field retained occurrence has no original actual"))?;
            let mut prefix = Vec::new();
            put(&mut prefix, LifeTypeProjection::Place(path), b)?;
            for step in &row.path {
                put(&mut prefix, step.clone(), b)?;
            }
            put(
                &mut retained,
                StorageRetainedBinding {
                    path: prefix,
                    class: row.class,
                    binder: u32::try_from(binder)
                        .map_err(|_| fail("nested retained binder overflow"))?,
                },
                b,
            )?;
        }
        for row in &field.life.nominal_arguments {
            let class = field
                .classes
                .get(row.binder as usize)
                .ok_or_else(|| fail("nested field nominal class is absent"))?;
            b.spend(classes.len().saturating_add(1), WORK)?;
            let binder = classes
                .iter()
                .position(|candidate| candidate == class)
                .ok_or_else(|| fail("nested field nominal occurrence has no original actual"))?;
            let mut prefix = Vec::new();
            put(&mut prefix, LifeTypeProjection::Place(path), b)?;
            for step in &row.path {
                put(&mut prefix, step.clone(), b)?;
            }
            put(
                &mut nominal_rows,
                NominalLifeArgument {
                    path: prefix,
                    declaration: row.declaration,
                    argument: row.argument,
                    binder: u32::try_from(binder)
                        .map_err(|_| fail("nested nominal binder overflow"))?,
                },
                b,
            )?;
        }
        Ok(())
    };
    match body {
        SemanticNominalBody::Product {
            fields: source_fields,
        } => {
            if fields.len() != source_fields.len() {
                return Err(fail("nominal value changes its full product arity"));
            }
            for (index, field) in fields.iter().enumerate() {
                check_field(
                    field,
                    PlaceProjection::Field(
                        u32::try_from(index).map_err(|_| fail("nominal field index overflow"))?,
                    ),
                    b,
                )?;
            }
        }
        SemanticNominalBody::Enum { variants } => {
            for (variant, item) in variants.iter().enumerate() {
                for index in 0..item.fields.len() {
                    let field = fields
                        .get(offset)
                        .ok_or_else(|| fail("nominal enum value omits a payload"))?;
                    check_field(
                        field,
                        PlaceProjection::Variant(VariantProjection {
                            variant: u32::try_from(variant)
                                .map_err(|_| fail("variant overflow"))?,
                            index: u32::try_from(index)
                                .map_err(|_| fail("variant field overflow"))?,
                        }),
                        b,
                    )?;
                    offset += 1;
                }
            }
        }
    }
    if matches!(body, SemanticNominalBody::Enum { .. }) && offset != fields.len() {
        return Err(fail("nominal enum value adds a payload"));
    }
    let shape = StorageLifeShape {
        binder_count: u32::try_from(classes.len())
            .map_err(|_| fail("nominal class count overflow"))?,
        slots,
        retained,
        nominal_arguments: nominal_rows,
    };
    Ok((classes, expected, shape))
}

/// A Type argument owns its intrinsic directory even when another argument
/// maps to the same outer lifetime. Build in local coordinates first. Only the
/// final permutation applies these coordinates to the actual class images.
fn qualified_nominal_domain<'a>(
    source: &SemanticDefinition,
    declared: &LifeNominalDeclaration,
    declaration: LifeDeclarationId,
    body: &SemanticNominalBody,
    args: &'a [Argument],
    fields: &[Value],
    program: &SemanticModule,
    original: &original::Checked,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<(Vec<Class>, Vec<ExpectedActual<'a>>, StorageLifeShape)> {
    let mut classes = Vec::new();
    let mut coordinates = Vec::new();
    let mut expected = Vec::new();
    let mut named = Vec::new();
    let mut shape = empty();
    for (index, ((formal, kind), arg)) in source.parameters.iter().zip(&declared.parameters).zip(args).enumerate() {
        let index = u32::try_from(index).map_err(|_| fail("nominal argument ordinal overflow"))?;
        let (coordinate, actual) = match (formal, kind, arg) {
            (SemanticFormal::Type { .. }, LifeParameterKind::Type, Argument::Type(value)) => {
                let mut local = Vec::new();
                for class in &value.classes {
                    let index = u32::try_from(classes.len()).map_err(|_| fail("nominal class domain overflow"))?;
                    put(&mut classes, class.clone(), b)?;
                    put(&mut local, Class::Local(index), b)?;
                }
                inherit_qualified_rows(&mut shape, value, &local, LifeTypeProjection::TypeArgument(index), b)?;
                (Argument::Type(value.with_classes(local, b)?), ExpectedActual::Value(value))
            }
            (SemanticFormal::Lifetime, LifeParameterKind::Lifetime, Argument::Life(image)) => {
                b.spend(named.len() + 1, WORK)?;
                let binder = match named.iter().find(|(prev, _)| *prev == image) {
                    Some((_, binder)) => *binder,
                    None => {
                        let binder = u32::try_from(classes.len()).map_err(|_| fail("nominal lifetime ordinal overflow"))?;
                        put(&mut named, (image, binder), b)?;
                        put(&mut classes, image.clone(), b)?;
                        binder
                    }
                };
                put(&mut shape.nominal_arguments, NominalLifeArgument {
                    path: Vec::new(), declaration, argument: index, binder,
                }, b)?;
                (Argument::Life(Class::Local(binder)), ExpectedActual::Lifetime(binder))
            }
            (SemanticFormal::Const { .. }, LifeParameterKind::Const(ty), Argument::Const { carrier, value })
                if ty == carrier && value.ty() == *carrier => {
                crate::dyn_metadata::ty(carrier, b)?;
                if let Constant::WideString(units) = value { b.spend(units.len(), WORK)?; }
                (arg.clone(), ExpectedActual::Const(value))
            }
            _ => return Err(fail("qualified nominal changes an original mixed argument")),
        };
        put(&mut coordinates, coordinate, b)?;
        put(&mut expected, actual, b)?;
    }
    let mut inputs = fields.iter();
    let mut field = |recipe: &SemanticTypeRecipe, path: PlaceProjection, b: &mut Budget| -> CompileResult<()> {
        let input = inputs.next().ok_or_else(|| fail("qualified nominal omits an original field"))?;
        if empty_shape(&input.life) && input.classes.is_empty() {
            return Ok(());
        }
        let located = closed::coordinate_field(source.id, &coordinates, recipe, program, inventory, original, b, depth + 1)?;
        if !geometry::same_geometry(&input.geometry, &located.geometry, b)? || located.classes.len() != input.classes.len() {
            return Err(fail("qualified field changes its declared intrinsic geometry"));
        }
        b.spend(input.classes.len(), WORK)?;
        for (coordinate, image) in located.classes.iter().zip(&input.classes) {
            let Class::Local(index) = coordinate else {
                return Err(fail("qualified field class has no original argument coordinate"));
            };
            if classes.get(*index as usize) != Some(image) {
                return Err(fail("qualified field changes the image of its original coordinate"));
            }
        }
        inherit_qualified_rows(&mut shape, &located, &located.classes, LifeTypeProjection::Place(path), b)
    };
    match body {
        SemanticNominalBody::Product { fields } => {
            for (index, orig) in fields.iter().enumerate() {
                let index = u32::try_from(index).map_err(|_| fail("nominal field ordinal overflow"))?;
                field(&orig.ty, PlaceProjection::Field(index), b)?;
            }
        }
        SemanticNominalBody::Enum { variants } => {
            for (variant, orig) in variants.iter().enumerate() {
                let variant = u32::try_from(variant).map_err(|_| fail("nominal variant ordinal overflow"))?;
                for (index, recipe) in orig.fields.iter().enumerate() {
                    let index = u32::try_from(index).map_err(|_| fail("nominal payload ordinal overflow"))?;
                    field(recipe, PlaceProjection::Variant(VariantProjection { variant, index }), b)?;
                }
            }
        }
    }
    if inputs.next().is_some() { return Err(fail("qualified nominal adds a field")); }
    shape.binder_count = u32::try_from(classes.len()).map_err(|_| fail("nominal class domain overflow"))?;
    let count = shape.slots.len().saturating_add(shape.retained.len()).saturating_add(shape.nominal_arguments.len());
    b.spend(count, WORK)?;
    let width = shape.slots.iter().map(|r| r.path.len()).chain(shape.retained.iter().map(|r| r.path.len()))
        .chain(shape.nominal_arguments.iter().map(|r| r.path.len())).max().unwrap_or(0).saturating_add(1);
    b.spend(count.saturating_mul(width).saturating_mul(count.saturating_add(1).ilog2() as usize + 1), WORK)?;
    shape.slots.sort_unstable();
    shape.retained.sort_unstable();
    shape.nominal_arguments.sort_unstable();
    b.spend(classes.len().saturating_add(count).saturating_add(expected.len()), WORK)?;
    let mut reordered = Vec::new();
    let mut permutation = vec![None; classes.len()];
    for binder in shape.nominal_arguments.iter_mut().map(|r| &mut r.binder)
        .chain(shape.slots.iter_mut().map(|r| &mut r.binder))
        .chain(shape.retained.iter_mut().map(|r| &mut r.binder)) {
        let pos = *binder as usize;
        let mapped = permutation.get_mut(pos).ok_or_else(|| fail("qualified row leaves its original domain"))?;
        *binder = match *mapped {
            Some(index) => index,
            None => {
                let index = reordered.len() as u32;
                put(&mut reordered, classes[pos].clone(), b)?;
                *mapped = Some(index);
                index
            }
        };
    }
    if reordered.len() != classes.len() { return Err(fail("qualified nominal omits an original coordinate")); }
    for actual in &mut expected {
        if let ExpectedActual::Lifetime(binder) = actual {
            *binder = permutation[*binder as usize].ok_or_else(|| fail("named nominal lifetime is unrepresented"))?;
        }
    }
    Ok((reordered, expected, shape))
}

fn inherit_qualified_rows(parent: &mut StorageLifeShape, value: &Value,
    coordinates: &[Class], prefix: LifeTypeProjection, b: &mut Budget) -> CompileResult<()> {
    let child = &value.life;
    if child.binder_count as usize != coordinates.len() {
        return Err(fail("qualified child omits its intrinsic class domain"));
    }
    let mapped = |index: u32| match coordinates.get(index as usize) {
        Some(Class::Local(binder)) => Ok(*binder),
        _ => Err(fail("qualified child row lacks a declaration-local coordinate")),
    };
    // A physical root address spells its class in the slot itself. The
    // corresponding type argument still needs its separate retained entry.
    let direct_address = matches!(prefix, LifeTypeProjection::Place(_))
        && matches!(value.ty, Type::Address { .. })
        && child.binder_count == 1
        && matches!(child.slots.as_slice(), [slot] if slot.path.is_empty() && slot.binder == 0)
        && child.retained.is_empty() && child.nominal_arguments.is_empty();
    if !direct_address {
        for class in 0..child.binder_count {
            put(&mut parent.retained, StorageRetainedBinding { path: vec![prefix], class, binder: mapped(class)? }, b)?;
        }
    }
    let prefixed = |tail: &[LifeTypeProjection], b: &mut Budget| -> CompileResult<Vec<LifeTypeProjection>> {
        b.spend(tail.len().saturating_add(1), WORK)?;
        let mut path = Vec::with_capacity(tail.len().saturating_add(1));
        path.push(prefix);
        path.extend_from_slice(tail);
        Ok(path)
    };
    for row in &child.retained {
        put(&mut parent.retained, StorageRetainedBinding {
            path: prefixed(&row.path, b)?, class: row.class, binder: mapped(row.binder)?,
        }, b)?;
    }
    for row in &child.nominal_arguments {
        put(&mut parent.nominal_arguments, NominalLifeArgument {
            path: prefixed(&row.path, b)?, declaration: row.declaration, argument: row.argument, binder: mapped(row.binder)?,
        }, b)?;
    }
    if let LifeTypeProjection::Place(field) = prefix {
        for row in &child.slots {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut path = Vec::with_capacity(row.path.len().saturating_add(1));
            path.push(field);
            path.extend_from_slice(&row.path);
            put(&mut parent.slots, StorageLifeSlot { path, mode: row.mode, binder: mapped(row.binder)? }, b)?;
        }
    }
    Ok(())
}

/// Reconstruct a nominal from the original whole argument vector. A local
/// binder is selected by its actual occurrence, never a caller's numeric ID.
pub(super) fn nominal(
    definition: SemanticDefinitionId,
    args: &[Argument],
    fields: &[Value],
    program: &SemanticModule,
    original: &original::Checked,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(unknown("semantic nominal value exceeds its complete depth"));
    }
    let source = inventory.definition(definition, b)?;
    let SemanticDefinitionBody::Nominal {
        declaration,
        kind,
        body,
    } = &source.body
    else {
        return Err(fail("semantic nominal value selects a nonnominal original"));
    };
    if source.parameters.len() != args.len() {
        return Err(fail("semantic nominal value omits mixed arguments"));
    }
    let mut declared = None;
    for item in inventory.nominal_declarations(*declaration, b)? {
        b.spend(1, WORK)?;
        if item.id == *declaration && declared.replace(item).is_some() {
            return Err(fail("nominal declaration identity is duplicated"));
        }
    }
    let declared =
        declared.ok_or_else(|| fail("semantic nominal value has no real declaration inventory"))?;
    if declared.identity != source.identity
        || declared.kind != *kind
        || declared.parameters.len() != args.len()
    {
        return Err(fail(
            "nominal value changes original identity, kind or arity",
        ));
    }
    let (classes, expected, shape) = if args.iter().any(|arg| {
        matches!(arg, Argument::Type(value) if !value.classes.is_empty())
    }) {
        qualified_nominal_domain(source, declared, *declaration, body, args, fields,
            program, original, inventory, b, depth + 1)?
    } else {
        physical_nominal_domain(source, declared, declaration, body, args, fields, b)?
    };
    let mut selected = None;
    for (position, instance) in inventory.nominal_instances(*declaration, b)? {
        b.spend(1, WORK)?;
        if instance.declaration != *declaration {
            continue;
        }
        b.spend(
            instance.arguments.len().saturating_add(expected.len() + 1),
            WORK,
        )?;
        for argument in &instance.arguments {
            match argument {
                LifeNominalActual::Type(_) | LifeNominalActual::QualifiedType { .. } => {}
                LifeNominalActual::Const(value) => {
                    b.spend(1, WORK)?;
                    if let Constant::WideString(units) = value {
                        b.spend(units.len(), WORK)?;
                    }
                }
                LifeNominalActual::Lifetime(_) => b.spend(1, WORK)?,
            }
        }
        // A complete zero Type domain has two explicit inventory spellings:
        // Type(T) and QualifiedType(T, empty).  Neither spelling changes T or
        // supplies a class.  Nonzero domains still require the exact shape.
        let mut arguments_equal = instance.arguments.len() == expected.len();
        for (actual, expected) in instance.arguments.iter().zip(&expected) {
            arguments_equal &= actual_equal_paid(actual, expected, inventory, b)?;
        }
        if !arguments_equal {
            continue;
        }
        let actual_geometry = inventory.geometry.nominal(position, inventory.module, b)?;
        crate::dyn_metadata::shape(&actual_geometry.life, b)?;
        crate::dyn_metadata::shape(&shape, b)?;
        let storage = match &actual_geometry.ty {
            Type::BorrowedStorage { storage, life } if *life == shape => storage.as_ref(),
            Type::BorrowedStorage { .. } => continue,
            ty if classes.is_empty()
                && shape.slots.is_empty()
                && shape.retained.is_empty()
                && shape.nominal_arguments.is_empty() =>
            {
                ty
            }
            _ => continue,
        };
        let valid = match (kind, body, storage) {
            (
                LifeNominalKind::Record | LifeNominalKind::Resource,
                SemanticNominalBody::Product {
                    fields: source_fields,
                },
                Type::Record { fields: actual, .. },
            )
            | (
                LifeNominalKind::Package,
                SemanticNominalBody::Product {
                    fields: source_fields,
                },
                Type::Package { fields: actual, .. },
            ) => {
                b.spend(source_fields.len().saturating_add(actual.len()), WORK)?;
                let mut valid = actual.len() == fields.len();
                for (ordinal, ((source, (name, _)), field)) in
                    source_fields.iter().zip(actual).zip(fields).enumerate()
                {
                    b.spend(
                        source
                            .name
                            .len()
                            .saturating_add(name.len())
                            .saturating_add(1),
                        WORK,
                    )?;
                    valid &= source.name == *name;
                    valid &= field.field_equal(&actual_geometry, None, ordinal, b)?;
                }
                valid
            }
            (
                LifeNominalKind::Enum,
                SemanticNominalBody::Enum {
                    variants: source_variants,
                },
                Type::Enum {
                    variants: actual, ..
                },
            ) => {
                let mut offset = 0usize;
                let mut valid = source_variants.len() == actual.len();
                for (variant, (source, (name, types))) in
                    source_variants.iter().zip(actual).enumerate()
                {
                    b.spend(types.len().saturating_add(name.len() + 1), WORK)?;
                    valid &= source.name == *name && source.fields.len() == types.len();
                    for (ordinal, _) in types.iter().enumerate() {
                        valid &= match fields.get(offset) {
                            Some(field) => {
                                field.field_equal(&actual_geometry, Some(variant), ordinal, b)?
                            }
                            None => false,
                        };
                        offset += 1;
                    }
                }
                valid && offset == fields.len()
            }
            _ => false,
        };
        if valid && selected.replace((position, instance)).is_some() {
            return Err(fail(
                "nominal value has ambiguous complete physical instances",
            ));
        }
    }
    let (position, instance) = selected
        .ok_or_else(|| fail("nominal value has no exact original whole-domain instance"))?;
    let geometry = inventory.geometry.nominal(position, inventory.module, b)?;
    if crate::stored_flow::nominal_instance_index(
        inventory.module,
        &geometry.ty,
        inventory.limits,
        b,
    )? != position
    {
        return Err(fail("nominal value has a nonunique exact type catalogue"));
    }
    crate::stored_flow::validate_nominal_metadata(inventory.module, instance, inventory.limits, b)?;
    // The generated current domain was independently matched above. Sharing
    // its immutable layout never shares the freshly generated class vector.
    if geometry.life != shape {
        return Err(fail(
            "nominal geometry differs from the current complete domain",
        ));
    }
    Ok(Value { geometry, classes })
}

pub(super) fn ancestor<'a>(
    mut instance: &'a SemanticInstance,
    scope: SemanticScopeRef,
    inventory: &Inventory<'a>,
    b: &mut Budget,
) -> CompileResult<&'a SemanticInstance> {
    let SemanticScopeRef::Ancestor(depth) = scope;
    if depth as usize > b.partial_depth {
        return Err(unknown("semantic value lexical depth exceeds its bound"));
    }
    for _ in 0..depth {
        instance = inventory.instance(
            instance
                .parent
                .ok_or_else(|| fail("semantic class has no actual lexical parent"))?,
            b,
        )?;
    }
    Ok(instance)
}
fn description_producer(
    function: &TypedFunction,
    module: &TypedModule,
    producer: &InstructionKind,
    payload: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    let local = |id: LocalId| {
        function.locals.get(id.0 as usize).filter(|declaration| declaration.id == id)
            .map(|declaration| &declaration.ty)
            .ok_or_else(|| fail("fresh description occurrence names no actual local"))
    };
    let owner = match *producer {
        InstructionKind::DescribeLocal { local: root } => local(root)?,
        InstructionKind::DescribeCapture { index } => {
            function.environment.and_then(|id| module.closures.get(id.0 as usize)
                .filter(|closure| closure.id == id && closure.function == function.id))
                .and_then(|closure| closure.captures.get(index as usize))
                .ok_or_else(|| fail("fresh description occurrence names no actual capture"))?
        }
        InstructionKind::DescribePackageField { local: root, index } => {
            let root = local(root)?;
            crate::dyn_metadata::ty(root, b)?;
            let Type::Package { fields, .. } = crate::resources::package_storage(root) else {
                return Err(fail("fresh description occurrence has no actual Package owner"));
            };
            &fields.get(index as usize)
                .ok_or_else(|| fail("fresh description occurrence names no actual Package field"))?.1
        }
        InstructionKind::DescribeAddress { address } => {
            let Some(Type::Address { inner, .. }) = function.values.get(address.0 as usize) else {
                return Err(fail("fresh description occurrence has no actual parent address"));
            };
            inner
        }
        _ => return Err(fail("outer description occurrence has no actual fresh description producer")),
    };
    crate::dyn_metadata::ty(owner, b)?;
    if !matches!(owner, Type::Managed(actual) if actual.as_ref() == payload) {
        return Err(fail("fresh description occurrence changes its actual complete managed payload"));
    }
    Ok(())
}

pub(super) fn class(
    instance: &SemanticInstance,
    expression: &SemanticClassExpr,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Class> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(unknown("semantic class relation exceeds its finite depth"));
    }
    match expression {
        SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(id),
            path,
            mode,
        } if path.is_empty() => {
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                return Err(fail(
                    "actual outer occurrence has no executing function world",
                ));
            };
            let function = inventory.function(target, b)?;
            let ty = function
                .values
                .get(id.0 as usize)
                .ok_or_else(|| fail("actual outer occurrence names no SSA value"))?;
            crate::dyn_metadata::ty(ty, b)?;
            let actual_mode = match ty {
                Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                Type::Desc(_) => StoredBorrowMode::WritableDescription,
                _ => return Err(fail("fresh outer occurrence has no actual Address or Desc head")),
            };
            if *mode != actual_mode {
                return Err(fail(
                    "fresh outer occurrence changes its actual producer mode",
                ));
            }
            let mut producer = None;
            for block in &function.blocks {
                b.spend(block.instructions.len().saturating_add(1), WORK)?;
                for instruction in &block.instructions {
                    if instruction.result == Some(*id)
                        && producer.replace(&instruction.kind).is_some()
                    {
                        return Err(fail(
                            "fresh outer occurrence has competing actual producers",
                        ));
                    }
                }
            }
            let producer = producer.ok_or_else(|| fail("fresh outer occurrence has no actual producer"))?;
            if let Type::Desc(payload) = ty {
                description_producer(function, inventory.module, producer, payload, b)?;
                return Ok(Class::Reborrow(function.id, *id));
            }
            let Type::Address { mutable, inner } = ty else {
                unreachable!("actual address mode");
            };
            match producer {
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    let expected_mutable = matches!(producer, InstructionKind::BorrowLocalMut { .. });
                    let local = function
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| fail("fresh outer occurrence names no actual local"))?;
                    crate::dyn_metadata::ty(&local.ty, b)?;
                    if *mutable != expected_mutable || local.ty != **inner {
                        return Err(fail(
                            "fresh outer occurrence changes its actual complete local head",
                        ));
                    }
                }
                InstructionKind::BorrowCapture { index, mutable: output } => {
                    let closure = function.environment
                        .and_then(|id| inventory.module.closures.get(id.0 as usize))
                        .filter(|closure| closure.function == function.id)
                        .ok_or_else(|| fail("fresh capture occurrence has another actual environment"))?;
                    let captured = closure.captures.get(*index as usize)
                        .ok_or_else(|| fail("fresh capture occurrence leaves its actual capture directory"))?;
                    crate::dyn_metadata::ty(captured, b)?;
                    if output != mutable || captured != inner.as_ref()
                        || *output && closure.kind == CallableKind::Fn
                    {
                        return Err(fail("fresh capture occurrence changes its complete child type or environment mode"));
                    }
                }
                InstructionKind::ReborrowAddress {
                    address,
                    mutable: output,
                } => {
                    let parent = function.values.get(address.0 as usize).ok_or_else(|| {
                        fail("fresh reborrow occurrence has no actual parent SSA")
                    })?;
                    crate::dyn_metadata::ty(parent, b)?;
                    if !matches!(parent, Type::Address { mutable: parent_mutable, inner: parent_inner }
                        if parent_inner == inner && (!*output || *parent_mutable))
                        || output != mutable
                    {
                        return Err(fail(
                            "fresh reborrow occurrence changes its actual parent type or strengthens its mode",
                        ));
                    }
                }
                _ => {
                    return Err(fail(
                        "outer occurrence does not name an actual fresh borrow producer",
                    ));
                }
            }
            // This class transports no initialized data, target, parent loan,
            // mutability permission or cleanup ability. Those actual ledgers
            // still check every creator, access and exit independently.
            Ok(Class::Reborrow(function.id, *id))
        }
        SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Environment(ordinal),
            path,
            mode,
        } if path.is_empty() => {
            let definition = inventory.definition(instance.definition, b)?;
            if !matches!(
                definition
                    .environment
                    .get(*ordinal as usize)
                    .map(|row| &row.kind),
                Some(SemanticFormal::Type { .. })
            ) {
                return Err(fail(
                    "outer environment occurrence changes its original Type slot kind",
                ));
            }
            let parent = inventory.instance(
                instance
                    .parent
                    .ok_or_else(|| fail("outer environment occurrence lacks its real creator"))?,
                b,
            )?;
            let Some(SemanticActual::Type(actual)) = instance
                .environment
                .as_ref()
                .and_then(|env| env.actuals.get(*ordinal as usize))
            else {
                return Err(fail(
                    "outer environment occurrence has no sealed actual Type",
                ));
            };
            let source = resolved(parent, actual, inventory, b, depth + 1)?;
            let mut selected = None;
            for slot in &source.life.slots {
                b.spend(slot.path.len().saturating_add(1), WORK)?;
                if slot.path.is_empty()
                    && slot.mode == *mode
                    && selected.replace(slot.binder).is_some()
                {
                    return Err(fail(
                        "outer environment occurrence has several root slot identities",
                    ));
                }
            }
            source
                .classes
                .get(selected.ok_or_else(|| {
                    fail("outer environment occurrence has no exact typed root slot")
                })? as usize)
                .cloned()
                .ok_or_else(|| fail("outer environment occurrence loses its complete source class"))
        }
        SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::TypeArgument { scope, ordinal },
            path,
            mode,
        } if path.is_empty() => {
            let context = ancestor(instance, *scope, inventory, b)?;
            if !matches!(
                inventory
                    .definition(context.definition, b)?
                    .parameters
                    .get(*ordinal as usize),
                Some(SemanticFormal::Type { .. })
            ) {
                return Err(fail(
                    "outer TypeArgument occurrence changes its full formal kind",
                ));
            }
            let Some(SemanticActual::Type(actual)) = context.actuals.get(*ordinal as usize) else {
                return Err(fail(
                    "outer TypeArgument occurrence omits its complete actual",
                ));
            };
            let value = resolved(context, actual, inventory, b, depth + 1)?;
            let mut selected = None;
            for slot in &value.life.slots {
                b.spend(slot.path.len().saturating_add(1), WORK)?;
                if slot.path.is_empty()
                    && slot.mode == *mode
                    && selected.replace(slot.binder).is_some()
                {
                    return Err(fail("outer TypeArgument repeats its root occurrence"));
                }
            }
            Ok(Class::TypeArgument(
                context.id,
                *ordinal,
                selected.ok_or_else(|| fail("outer TypeArgument has no exact typed root slot"))?,
            ))
        }
        SemanticClassExpr::Formal { scope, ordinal } => {
            let context = ancestor(instance, *scope, inventory, b)?;
            if !matches!(
                inventory
                    .definition(context.definition, b)?
                    .parameters
                    .get(*ordinal as usize),
                Some(SemanticFormal::Lifetime)
            ) {
                return Err(fail(
                    "semantic class Formal changes its declared mixed kind",
                ));
            }
            if !matches!(context.actuals.get(*ordinal as usize),Some(SemanticActual::Life(SemanticClassExpr::Formal {scope:SemanticScopeRef::Ancestor(0),ordinal:actual})) if *actual==*ordinal)
            {
                return Err(unknown(
                    "semantic lifetime actual needs a complete independent rigid partition context",
                ));
            }
            Ok(Class::Formal(context.id, *ordinal))
        }
        SemanticClassExpr::Implicit { scope, ordinal } => {
            let context = ancestor(instance, *scope, inventory, b)?;
            b.spend(1, WORK)?;
            if !inventory
                .definition(context.definition, b)?
                .implicit_lifetimes
                .get(*ordinal as usize)
                .is_some_and(|row| row.ordinal == *ordinal)
            {
                return Err(fail(
                    "semantic implicit class has no real declaration occurrence",
                ));
            }
            Ok(Class::Implicit(context.id, *ordinal))
        }
        SemanticClassExpr::Environment { scope, ordinal } => {
            let context = ancestor(instance, *scope, inventory, b)?;
            let definition = inventory.definition(context.definition, b)?;
            if !matches!(
                definition
                    .environment
                    .get(*ordinal as usize)
                    .map(|formal| &formal.kind),
                Some(SemanticFormal::Lifetime)
            ) {
                return Err(fail("semantic Life environment changes its original kind"));
            }
            let parent = inventory.instance(
                context
                    .parent
                    .ok_or_else(|| fail("semantic environment has no real creator"))?,
                b,
            )?;
            let Some(SemanticActual::Life(actual)) = context
                .environment
                .as_ref()
                .and_then(|environment| environment.actuals.get(*ordinal as usize))
            else {
                return Err(fail("semantic Life environment slot is absent"));
            };
            class(parent, actual, inventory, b, depth + 1)
        }
        SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::TypeArgument { scope, ordinal },
            path,
            class: child,
        } if path.is_empty() => {
            let context = ancestor(instance, *scope, inventory, b)?;
            if !matches!(
                inventory
                    .definition(context.definition, b)?
                    .parameters
                    .get(*ordinal as usize),
                Some(SemanticFormal::Type { .. })
            ) {
                return Err(fail(
                    "semantic TypeArgument class changes its original mixed kind",
                ));
            }
            let Some(SemanticActual::Type(SemanticTypeValue::Resolved { life, .. })) =
                context.actuals.get(*ordinal as usize)
            else {
                return Err(fail(
                    "semantic TypeArgument class has no complete actual type",
                ));
            };
            crate::dyn_metadata::shape(life, b)?;
            if *child >= life.binder_count {
                return Err(fail("semantic TypeArgument class is out of range"));
            }
            Ok(Class::TypeArgument(context.id, *ordinal, *child))
        }
        SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Environment(ordinal),
            path,
            class: child,
        } if path.is_empty() => {
            let definition = inventory.definition(instance.definition, b)?;
            if !matches!(
                definition
                    .environment
                    .get(*ordinal as usize)
                    .map(|formal| &formal.kind),
                Some(SemanticFormal::Type { .. })
            ) {
                return Err(fail(
                    "intrinsic environment class changes its original kind",
                ));
            }
            let parent = inventory.instance(
                instance
                    .parent
                    .ok_or_else(|| fail("intrinsic environment class lacks its real creator"))?,
                b,
            )?;
            let Some(SemanticActual::Type(actual)) = instance
                .environment
                .as_ref()
                .and_then(|environment| environment.actuals.get(*ordinal as usize))
            else {
                return Err(fail(
                    "intrinsic environment class omits its complete actual",
                ));
            };
            resolved(parent, actual, inventory, b, depth + 1)?
                .classes
                .get(*child as usize)
                .cloned()
                .ok_or_else(|| fail("intrinsic environment class is absent"))
        }
        _ => {
            crate::dyn_metadata::recipe(expression, b)?;
            let mut errors = unknown(
                "semantic class expression needs its exact actual operand and scope relation",
            );
            errors[0].notes.push(format!(
                "actual semantic instance {}:{}, original definition {}:{}, unresolved class {expression:?}",
                instance.id.module.0, instance.id.index,
                instance.definition.module.0, instance.definition.index,
            ));
            Err(errors)
        }
    }
}
/// Build a flat invocation-only code type from independently checked outer
/// occurrences. The physical relation check handles exact order and coverage.
pub(super) fn invocation_code(
    inputs: Vec<(Type, Option<u32>)>,
    (result, result_class): (Type, Option<u32>),
    b: &mut Budget,
) -> CompileResult<Value> {
    b.spend(inputs.len().saturating_add(1), WORK)?;
    let mut parameters = Vec::with_capacity(inputs.len());
    let mut bindings = Vec::new();
    let mut count = 0u32;
    for (ordinal, (ty, class)) in inputs.into_iter().enumerate() {
        if let Some(class) = class {
            let base = StorageBase::Parameter(u32::try_from(ordinal)
                .map_err(|_| fail("borrowed code parameter ordinal overflow"))?);
            bindings.push(StorageLifeBinding {
                position: StoragePosition { base, path: Vec::new() },
                binder: StorageBinder::Invocation(class),
            });
            count = count.max(class.checked_add(1)
                .ok_or_else(|| fail("borrowed code class ordinal overflow"))?);
        }
        parameters.push(ty);
    }
    if let Some(class) = result_class {
        if class >= count {
            return Err(fail("borrowed code result selects no complete input class"));
        }
        bindings.push(StorageLifeBinding {
            position: StoragePosition { base: StorageBase::Result, path: Vec::new() },
            binder: StorageBinder::Invocation(class),
        });
    }
    if count == 0 {
        return Err(fail("borrowed code omits its invocation domain"));
    }
    let life = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: count,
        bindings,
        static_bindings: Vec::new(),
        classes: Vec::new(),
    };
    crate::signature_life::charge(&life, b)?;
    crate::borrowed_fp::relation(&parameters, &result, &life)?;
    Ok(scalar(Type::BorrowedFunctionPointer { parameters, result: Box::new(result), life }))
}

pub(super) fn resolved(
    instance: &SemanticInstance,
    input: &SemanticTypeValue,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    let SemanticTypeValue::Resolved { ty, life, classes } = input else {
        return Err(fail("Core type remains in an executable semantic value"));
    };
    resolved_parts(instance, ty, life, classes, inventory, b, depth)
}
fn resolved_parts(
    instance: &SemanticInstance,
    ty: &Type,
    life: &StorageLifeShape,
    classes: &[SemanticClassExpr],
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(unknown(
            "complete semantic type value exceeds its finite depth",
        ));
    }
    let geometry = inventory.geometry.resolved(ty, life, b)?;
    let ty = &geometry.ty;
    let life = &geometry.life;
    if classes.len() != life.binder_count as usize {
        return Err(fail(
            "complete semantic type value omits an intrinsic class",
        ));
    }
    // Publish only physical child Type identities before independent domain
    // qualification recursively visits them. No child class image is stored.
    match ty {
        Type::Address { .. } | Type::Desc(_) => {
            geometry::retain_child(&geometry, geometry::Child::Inner, b)?;
        }
        Type::ErrorUnion { .. } => {
            geometry::retain_child(&geometry, geometry::Child::UnionError, b)?;
            geometry::retain_child(&geometry, geometry::Child::UnionResult, b)?;
        }
        _ => {}
    }
    match ty {
        Type::ErrorUnion { .. } if let Some(expected) = qualified_sum_domain(ty, inventory, b)? => {
            if expected != *life { return Err(fail("semantic tagged sum changes its complete success domain")); }
        }
        Type::Task { .. } | Type::Managed(_) | Type::ErrorUnion { .. }
            if task_domain(ty, b)?.is_some() => {
                let expected = task_domain(ty, b)?.ok_or_else(|| fail("Task domain vanished"))?;
                if expected != *life { return Err(fail("semantic Task carrier changes its complete child-domain forest")); }
                if let Type::ErrorUnion { error, .. } = ty {
                    crate::stored_flow::validate_plain_nominal_zero(inventory.module, error, inventory.limits, b)?;
                }
            }
        Type::Address { .. }
            if super::original::strings::is_byte_type(ty)
                && life.binder_count == 0
                && life.slots.is_empty()
                && life.retained.is_empty()
                && life.nominal_arguments.is_empty() => {}
        Type::Tuple(fields) if empty_shape(life) => {
            // Resolve the complete structural value using the same leaf
            // rules as a standalone value. A class-free shared byte leaf is
            // a Static obligation, not a source proof: the original creator,
            // every actual constructor/write and return still establish its
            // literal provenance. The nominal type-only query below cannot
            // reconstruct that obligation from a raw nested Address.
            for (index, _) in fields.iter().enumerate() {
                b.spend(1, WORK)?;
                let child = geometry::project(
                    &geometry,
                    geometry::Child::Field(None, index),
                    &empty(),
                    b,
                )?;
                resolved_parts(
                    instance, &child.ty, &child.life, &[], inventory, b, depth + 1,
                )?;
            }
        }
        Type::BorrowedStorage { life: actual, .. } if actual == life => {}
        Type::BorrowedStorage { .. } => {
            return Err(fail(
                "semantic whole type and embedded lifetime shape differ",
            ));
        }
        Type::Dyn { .. } | Type::OwnedDyn { .. } => {
            if dyn_domain(ty, inventory, b)? != *life {
                return Err(fail(
                    "semantic Dyn value changes its complete Context domain",
                ));
            }
        }
        Type::ErrorUnion { error, result }
            if matches!(result.as_ref(), Type::Dyn { .. } | Type::OwnedDyn { .. }) =>
        {
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                error,
                inventory.limits,
                b,
            )?;
            let child = dyn_domain(result, inventory, b)?;
            let mut locals = Vec::new();
            for index in 0..child.binder_count {
                put(&mut locals, Class::Local(index), b)?;
            }
            let error = geometry::project(&geometry, geometry::Child::UnionError, &empty(), b)?;
            let result = geometry::project(&geometry, geometry::Child::UnionResult, &child, b)?;
            let expected = error_union(
                Value::from_geometry(error, Vec::new()),
                Value::from_geometry(result, locals),
                b,
            )?;
            if expected.life != *life {
                return Err(fail(
                    "semantic ErrorUnion changes its complete Dyn child domain",
                ));
            }
        }
        Type::Closure(id) => {
            let definition = inventory
                .module
                .closures
                .get(id.0 as usize)
                .filter(|definition| definition.id == *id)
                .ok_or_else(|| fail("semantic closure type has another actual definition"))?;
            let count = definition
                .storage_life
                .as_ref()
                .ok_or_else(|| {
                    unknown("semantic closure has no complete intrinsic class inventory")
                })?
                .fixed_binder_count;
            if life.binder_count != count
                || !life.slots.is_empty()
                || !life.retained.is_empty()
                || !life.nominal_arguments.is_empty()
            {
                return Err(fail(
                    "opaque semantic closure domain differs from its complete inventory",
                ));
            }
        }
        Type::MutexGuard { .. } => {
            if mutex_guard_domain(ty, b)? != *life {
                return Err(fail(
                    "semantic Mutex guard changes its opaque intrinsic domain",
                ));
            }
        }
        Type::Desc(inner) => {
            // This is only the whole weak occurrence shape. The actual
            // descriptor origin, epoch and cleanup remain independent facts.
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                inner,
                inventory.limits,
                b,
            )?;
            if life.binder_count != 1
                || classes.len() != 1
                || !matches!(life.slots.as_slice(), [slot] if slot.path.is_empty()
                    && slot.binder == 0 && slot.mode == StoredBorrowMode::WritableDescription)
                || !life.retained.is_empty()
                || !life.nominal_arguments.is_empty()
            {
                return Err(fail(
                    "semantic description changes its complete weak outer occurrence",
                ));
            }
        }
        Type::Address { mutable, inner } => {
            if life.slots.len() != 1
                || !life.slots[0].path.is_empty()
                || life.slots[0].binder != 0
                || life.slots[0].mode
                    != if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    }
            {
                return Err(fail("semantic address loses its exact outer occurrence"));
            }
            let child_life = match inner.as_ref() {
                Type::Dyn { .. } | Type::OwnedDyn { .. } => dyn_domain(inner, inventory, b)?,
                Type::MutexGuard { .. } => mutex_guard_domain(inner, b)?,
        Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) =>
            task_domain(inner, b)?.ok_or_else(|| fail("Task pointee has no complete child domain"))?,
                Type::BorrowedStorage { life, .. } => {
                    crate::dyn_metadata::shape(life, b)?;
                    life.clone()
                }
                Type::Closure(id) => {
                    b.spend(inventory.module.closures.len().saturating_add(1), WORK)?;
                    let declaration = inventory
                        .module
                        .closures
                        .get(id.0 as usize)
                        .filter(|declaration| declaration.id == *id)
                        .ok_or_else(|| fail("address child has no actual closure inventory"))?;
                    let count = declaration
                        .storage_life
                        .as_ref()
                        .ok_or_else(|| unknown("address child closure domain is unknown"))?
                        .fixed_binder_count;
                    StorageLifeShape {
                        binder_count: count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    }
                }
                Type::Address { .. } | Type::Desc(_) => nested_reference_shape(inner, life, b)?,
                _ => {
                    crate::stored_flow::validate_plain_nominal_zero(
                        inventory.module,
                        inner,
                        inventory.limits,
                        b,
                    )?;
                    empty()
                }
            };
            let mut child_expressions = Vec::new();
            let mut local_classes = Vec::new();
            for child in 0..child_life.binder_count {
                let mut selected = None;
                for row in &life.retained {
                    b.spend(row.path.len().saturating_add(1), WORK)?;
                    if row.path.as_slice() == [LifeTypeProjection::Pointee] && row.class == child {
                        if selected.replace(row.binder).is_some() {
                            return Err(fail(
                                "semantic address repeats a Pointee class occurrence",
                            ));
                        }
                    }
                }
                let binder = selected
                    .ok_or_else(|| fail("semantic address omits a Pointee class occurrence"))?;
                let expression = classes.get(binder as usize).ok_or_else(|| {
                    fail("semantic Pointee embedding is outside its whole domain")
                })?;
                crate::dyn_metadata::recipe(expression, b)?;
                put(&mut child_expressions, expression.clone(), b)?;
                put(&mut local_classes, Class::Local(binder), b)?;
            }
            let child = geometry::project(&geometry, geometry::Child::Inner, &child_life, b)?;
            // Recursively validate the complete child with its actual fresh
            // expressions. Only immutable physical geometry is shared.
            resolved_parts(
                instance, &child.ty, &child.life, &child_expressions,
                inventory, b, depth + 1,
            )?;
            let expected = address(
                *mutable,
                Class::Local(0),
                Value::from_geometry(child, local_classes),
                b,
            )?;
            if expected.life != *life {
                return Err(fail(
                    "semantic address changes its complete canonical Pointee embedding",
                ));
            }
        }
        _ if life.binder_count == 0
            && life.slots.is_empty()
            && life.retained.is_empty()
            && life.nominal_arguments.is_empty() =>
        {
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                ty,
                inventory.limits,
                b,
            )?;
        }
        _ => {
            return Err(unknown(
                "semantic whole type requires its complete intrinsic domain proof",
            ));
        }
    }
    let mut images = Vec::new();
    for expression in classes {
        let image = class(instance, expression, inventory, b, depth + 1)?;
        put(&mut images, image, b)?;
    }
    Ok(Value {
        geometry,
        classes: images,
    })
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    #[test]
    fn sum_preserves_unused_success_classes_and_whole_child_identity() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let success = Value::new(
            Type::Closure(ClosureId(0)),
            StorageLifeShape {
                binder_count: 2,
                slots: Vec::new(),
                retained: Vec::new(),
                nominal_arguments: Vec::new(),
            },
            vec![Class::Local(7), Class::Local(9)],
        );
        let sum = error_union(scalar(Type::U8), success, &mut budget).unwrap();
        assert_eq!(sum.classes, vec![Class::Local(7), Class::Local(9)]);
        assert_eq!(sum.life.binder_count, 2);
        assert_eq!(sum.life.retained.len(), 2);
        for (index, row) in sum.life.retained.iter().enumerate() {
            assert_eq!(
                row.path,
                vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))]
            );
            assert_eq!(row.class, index as u32);
            assert_eq!(row.binder, index as u32);
        }
        let missing = Value::new(
            Type::Closure(ClosureId(0)),
            StorageLifeShape {
                binder_count: 2,
                slots: Vec::new(),
                retained: Vec::new(),
                nominal_arguments: Vec::new(),
            },
            vec![Class::Local(7)],
        );
        assert!(error_union(scalar(Type::U8), missing, &mut budget).is_err());
    }

    #[test]
    fn shared_layout_keeps_independent_complete_class_images() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let first = address(false, Class::Local(0), scalar(Type::I32), &mut budget).unwrap();
        let other = first
            .with_classes(vec![Class::Local(1)], &mut budget)
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(&first.geometry, &other.geometry));
        assert_ne!(first, other);
        assert_eq!(first, first.copy(&mut budget).unwrap());

        // The same physical child Type can carry a different whole intrinsic
        // domain and current image. Constructor reuse must rebuild the full
        // outer/child partition rather than returning the old geometry.
        let child = |class| {
            Value::new(
                Type::Closure(ClosureId(0)),
                StorageLifeShape {
                    binder_count: 1,
                    slots: Vec::new(),
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                },
                vec![class],
            )
        };
        let merged = address(false, Class::Local(0), child(Class::Local(0)), &mut budget).unwrap();
        let separate =
            address(false, Class::Local(0), child(Class::Local(1)), &mut budget).unwrap();
        assert_eq!(merged.ty, separate.ty);
        assert_eq!(merged.life.binder_count, 1);
        assert_eq!(separate.life.binder_count, 2);
        assert_eq!(merged.classes, vec![Class::Local(0)]);
        assert_eq!(separate.classes, vec![Class::Local(0), Class::Local(1)]);
        assert!(!std::sync::Arc::ptr_eq(
            &merged.geometry,
            &separate.geometry
        ));
    }

    #[test]
    fn nested_raw_reference_reconstructs_the_whole_child_without_class_images() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let first = address(false, Class::Local(7), scalar(Type::I32), &mut budget).unwrap();
        let middle = address(true, Class::Local(9), first, &mut budget).unwrap();
        let outer = address(false, Class::Local(11), middle.copy(&mut budget).unwrap(), &mut budget).unwrap();
        assert_eq!(nested_reference_shape(&middle.ty, &outer.life, &mut budget).unwrap(), middle.life);
        let coincident = outer.with_classes(vec![Class::Local(3); outer.classes.len()], &mut budget).unwrap();
        assert_eq!(nested_reference_shape(&middle.ty, &coincident.life, &mut budget).unwrap(), middle.life);
        assert_eq!(middle.life.binder_count, 2);
        assert_eq!(middle.life.slots[0].mode, StoredBorrowMode::ExclusiveAddress);
        let mut missing = outer.life.clone();
        missing.retained.retain(|row| !(row.path.as_slice() == [LifeTypeProjection::Pointee] && row.class == 1));
        assert!(nested_reference_shape(&middle.ty, &missing, &mut budget).is_err());
        let mut repeated = outer.life.clone();
        repeated.retained.push(repeated.retained[0].clone());
        assert!(nested_reference_shape(&middle.ty, &repeated, &mut budget).is_err());
    }

    #[test]
    fn address_transports_complete_child_forest_and_phantom_actuals() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let declaration = LifeDeclarationId {
            module: zil_base::ModuleId(0), declaration: zil_base::NodeId(7), compiler: false,
        };
        let child_life = StorageLifeShape {
            binder_count: 2,
            slots: vec![StorageLifeSlot { path: vec![PlaceProjection::Field(0)], binder: 0,
                mode: StoredBorrowMode::SharedAddress }],
            retained: vec![StorageRetainedBinding { path: vec![LifeTypeProjection::Place(PlaceProjection::Field(1))], class: 0, binder: 0 }],
            nominal_arguments: vec![
                NominalLifeArgument { path: Vec::new(), declaration, argument: 2, binder: 0 },
                NominalLifeArgument { path: Vec::new(), declaration, argument: 4, binder: 1 },
            ],
        };
        let child = Value::new(Type::BorrowedStorage {
            storage: Box::new(Type::Record { name: "Holder".into(), fields: vec![
                ("pointer".into(), Type::Address { mutable: false, inner: Box::new(Type::I32) }),
                ("callback".into(), Type::Closure(ClosureId(0))),
            ] }), life: child_life.clone(),
        }, child_life.clone(), vec![Class::Local(7), Class::Local(9)]);
        let parent = address(false, Class::Local(3), child.copy(&mut budget).unwrap(), &mut budget).unwrap();
        assert_eq!(parent.life.binder_count, 3);
        assert_eq!(parent.life.slots.len(), 1);
        assert!(parent.life.slots[0].path.is_empty());
        assert_eq!(parent.life.retained, vec![
            StorageRetainedBinding { path: vec![LifeTypeProjection::Pointee], class: 0, binder: 1 },
            StorageRetainedBinding { path: vec![LifeTypeProjection::Pointee], class: 1, binder: 2 },
            StorageRetainedBinding { path: vec![LifeTypeProjection::Pointee, LifeTypeProjection::Place(PlaceProjection::Field(1))], class: 0, binder: 1 },
        ]);
        assert_eq!(parent.life.nominal_arguments, vec![
            NominalLifeArgument { path: vec![LifeTypeProjection::Pointee], declaration, argument: 2, binder: 1 },
            NominalLifeArgument { path: vec![LifeTypeProjection::Pointee], declaration, argument: 4, binder: 2 },
        ]);
        let coincident = parent.with_classes(vec![Class::Local(3); 3], &mut budget).unwrap();
        assert_eq!(coincident.life, parent.life);
        assert_eq!(coincident.life.binder_count, 3);
        let mut missing = child.copy(&mut budget).unwrap();
        missing.classes.pop();
        assert!(address(false, Class::Local(3), missing, &mut budget).is_err());
        let mut dangling = child;
        dangling.geometry_mut(&mut budget).unwrap().life.nominal_arguments[1].binder = 2;
        assert!(address(false, Class::Local(3), dangling, &mut budget).is_err());
    }

    #[test]
    fn borrowed_nominal_actuals_keep_full_type_and_phantom_domain_equality() {
        let ty = Type::I32;
        let expected = ExpectedActual::Type {
            ty: &ty,
            life: None,
        };
        assert!(actual_equal(&LifeNominalActual::Type(Type::I32), &expected));
        assert!(!actual_equal(
            &LifeNominalActual::Type(Type::U32),
            &expected
        ));
        assert!(actual_equal(
            &LifeNominalActual::QualifiedType {
                ty: Type::I32,
                life: empty()
            },
            &expected
        ));
        let mut phantom = empty();
        phantom.binder_count = 1;
        assert!(!actual_equal(
            &LifeNominalActual::QualifiedType {
                ty: Type::I32,
                life: phantom
            },
            &expected
        ));
    }

    #[test]
    fn changing_shared_geometry_preserves_the_original_layout_and_classes() {
        let mut budget = Budget::new(&zil_base::Limits::default());
        let first = address(false, Class::Local(0), scalar(Type::I32), &mut budget).unwrap();
        let mut other = first.copy(&mut budget).unwrap();
        let diagnostic = format!("{first:?}");
        assert!(first.equal_paid(&other, &mut budget).unwrap());
        assert_eq!(diagnostic, format!("{first:?}"));
        let different_classes = first
            .with_classes(vec![Class::Local(1)], &mut budget)
            .unwrap();
        assert!(!first.equal_paid(&different_classes, &mut budget).unwrap());
        let before = budget.remaining();
        let geometry = other.geometry_mut(&mut budget).unwrap();
        let Type::Address { mutable, .. } = &mut geometry.ty else {
            panic!()
        };
        *mutable = true;
        geometry.life.slots[0].mode = StoredBorrowMode::ExclusiveAddress;
        assert!(budget.remaining() < before);
        assert!(matches!(first.ty, Type::Address { mutable: false, .. }));
        assert!(matches!(other.ty, Type::Address { mutable: true, .. }));
        assert_eq!(first.classes, other.classes);
        assert_ne!(first, other);
        assert!(!first.equal_paid(&other, &mut budget).unwrap());
    }
}
