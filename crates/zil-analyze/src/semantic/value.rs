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
pub(super) fn tuple(fields: Vec<Value>, b: &mut Budget) -> CompileResult<Value> {
    b.spend(fields.len().saturating_add(1), WORK)?;
    let mut children = Vec::new();
    children
        .try_reserve_exact(fields.len())
        .map_err(|_| fail("semantic tuple geometry allocation failed"))?;
    for field in &fields {
        if !empty_shape(&field.life) || !field.classes.is_empty() {
            return Err(fail("plain tuple geometry loses a complete child domain"));
        }
        children.push(&field.geometry);
    }
    let geometry = geometry::construct(geometry::Constructor::Tuple, &children, empty(), b)?;
    Ok(Value::from_geometry(geometry, Vec::new()))
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
    let mut retained = Vec::new();
    for child in 0..result.life.binder_count {
        put(
            &mut retained,
            StorageRetainedBinding {
                path: vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))],
                class: child,
                binder: child,
            },
            b,
        )?;
    }
    let geometry = geometry::construct(
        geometry::Constructor::ErrorUnion,
        &[&error.geometry, &result.geometry],
        StorageLifeShape {
            binder_count: u32::try_from(result.classes.len())
                .map_err(|_| fail("ErrorUnion complete domain overflows"))?,
            slots: Vec::new(),
            retained,
            nominal_arguments: Vec::new(),
        },
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
    for (index, class) in inner.classes.iter().enumerate() {
        let binder = intern(&mut classes, class.clone(), b)?;
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
            nominal_arguments: Vec::new(),
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
pub(super) fn pointee(
    input: &Value,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Value> {
    let Type::Address { inner, .. } = &input.ty else {
        return Err(fail("whole pointee query is not an Address"));
    };
    let life = match inner.as_ref() {
        Type::Dyn { .. } | Type::OwnedDyn { .. } => dyn_domain(inner, inventory, b)?,
        Type::MutexGuard { .. } => mutex_guard_domain(inner, b)?,
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
        Type::Address { .. } | Type::Desc(_) => {
            return Err(unknown(
                "nested raw pointee needs its complete intrinsic shape",
            ));
        }
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
    if input.life.retained.len() != life.binder_count as usize {
        return Err(fail("whole pointee omits complete child class occurrences"));
    }
    let mut classes = Vec::new();
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
    }
    crate::dyn_metadata::ty(inner, b)?;
    Ok(Value::new(inner.as_ref().clone(), life, classes))
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
    Const(&'a Constant),
}
fn empty_shape(life: &StorageLifeShape) -> bool {
    life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
}
fn actual_equal(actual: &LifeNominalActual, expected: &ExpectedActual<'_>) -> bool {
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
    let ExpectedActual::Type {
        ty: expected_ty,
        life: expected_life,
    } = expected
    else {
        return Ok(false);
    };
    let geometry = inventory.geometry.type_value(ty, life, b)?;
    let expected = inventory
        .geometry
        .type_value(expected_ty, *expected_life, b)?;
    geometry::same_geometry(&geometry, &expected, b)
}

/// Reconstruct a nominal from the original whole argument vector. A local
/// binder is selected by its actual occurrence, never a caller's numeric ID.
pub(super) fn nominal(
    definition: SemanticDefinitionId,
    args: &[Argument],
    fields: &[Value],
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
    for item in &inventory.module.life_declarations {
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
                    ExpectedActual::Type {
                        ty: &value.ty,
                        life: (!value.classes.is_empty()).then_some(&value.life),
                    },
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
    let mut selected = None;
    for (position, instance) in inventory.module.life_instances.iter().enumerate() {
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
            let Type::Address { mutable, inner } = ty else {
                return Err(fail(
                    "fresh outer occurrence is not an actual address producer",
                ));
            };
            if *mode
                != if *mutable {
                    StoredBorrowMode::ExclusiveAddress
                } else {
                    StoredBorrowMode::SharedAddress
                }
            {
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
            match producer.ok_or_else(|| fail("fresh outer occurrence has no actual producer"))? {
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    let expected_mutable =
                        matches!(producer, Some(InstructionKind::BorrowLocalMut { .. }));
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
            let count = inventory
                .definition(context.definition, b)?
                .implicit_lifetimes
                .len();
            b.spend(count, WORK)?;
            if !inventory
                .definition(context.definition, b)?
                .implicit_lifetimes
                .iter()
                .any(|row| row.ordinal == *ordinal)
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
pub(super) fn resolved(
    instance: &SemanticInstance,
    input: &SemanticTypeValue,
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
    let SemanticTypeValue::Resolved { ty, life, classes } = input else {
        return Err(fail("Core type remains in an executable semantic value"));
    };
    let geometry = inventory.geometry.resolved(ty, life, b)?;
    let ty = &geometry.ty;
    let life = &geometry.life;
    if classes.len() != life.binder_count as usize {
        return Err(fail(
            "complete semantic type value omits an intrinsic class",
        ));
    }
    match ty {
        Type::Address { .. }
            if super::original::strings::is_byte_type(ty)
                && life.binder_count == 0
                && life.slots.is_empty()
                && life.retained.is_empty()
                && life.nominal_arguments.is_empty() => {}
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
            crate::dyn_metadata::ty(error, b)?;
            crate::dyn_metadata::ty(result, b)?;
            let expected = error_union(
                scalar(error.as_ref().clone()),
                Value::new(result.as_ref().clone(), child, locals),
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
                || !life.nominal_arguments.is_empty()
            {
                return Err(fail("semantic address loses its exact outer occurrence"));
            }
            let child_life = match inner.as_ref() {
                Type::Dyn { .. } | Type::OwnedDyn { .. } => dyn_domain(inner, inventory, b)?,
                Type::MutexGuard { .. } => mutex_guard_domain(inner, b)?,
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
                Type::Address { .. } | Type::Desc(_) => {
                    return Err(unknown(
                        "nested raw reference child requires its independently reconstructed whole local shape",
                    ));
                }
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
            if life.retained.len() != child_life.binder_count as usize {
                return Err(fail(
                    "semantic address omits or duplicates a Pointee class occurrence",
                ));
            }
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
            crate::dyn_metadata::ty(inner, b)?;
            crate::dyn_metadata::shape(&child_life, b)?;
            let child = SemanticTypeValue::Resolved {
                ty: inner.as_ref().clone(),
                life: child_life.clone(),
                classes: child_expressions,
            };
            // Recursively validate the child's intrinsic domain. External image
            // equality does not shrink that domain or its phantom inventory.
            resolved(instance, &child, inventory, b, depth + 1)?;
            crate::dyn_metadata::ty(inner, b)?;
            let expected = address(
                *mutable,
                Class::Local(0),
                Value::new(inner.as_ref().clone(), child_life, local_classes),
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
