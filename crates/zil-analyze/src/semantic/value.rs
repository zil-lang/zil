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
    // Every actual reborrow has its own outer occurrence. Its parent is read
    // from that exact instruction only when proving a particular call role.
    Reborrow(FunctionId, ValueId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Value {
    pub ty: Type,
    pub life: StorageLifeShape,
    pub classes: Vec<Class>,
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
fn put<T>(rows: &mut Vec<T>, value: T, b: &mut Budget) -> CompileResult<()> {
    b.spend(rows.len().saturating_add(1), WORK)?;
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
    Value {
        ty,
        life: empty(),
        classes: Vec::new(),
    }
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
    if inner.life.binder_count as usize != inner.classes.len() {
        return Err(fail("address child omits its complete intrinsic domain"));
    }
    let mut classes = Vec::new();
    let outer = intern(&mut classes, class, b)?;
    let mut retained = Vec::new();
    for (index, class) in inner.classes.into_iter().enumerate() {
        let binder = intern(&mut classes, class, b)?;
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
    b.spend(3, WORK)?;
    Ok(Value {
        ty: Type::Address {
            mutable,
            inner: Box::new(inner.ty),
        },
        life: StorageLifeShape {
            binder_count,
            slots: vec![StorageLifeSlot {
                path: Vec::new(),
                binder: outer,
                mode: if mutable {
                    StoredBorrowMode::ExclusiveAddress
                } else {
                    StoredBorrowMode::SharedAddress
                },
            }],
            // The child retains its own intrinsic shape and nominal identity
            // inside inner.ty. Only the explicit child-class embedding crosses
            // this Address boundary; no duplicate nominal or physical slots.
            retained,
            nominal_arguments: Vec::new(),
        },
        classes,
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
    Ok(Value {
        ty: inner.as_ref().clone(),
        life,
        classes,
    })
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
                put(&mut expected, LifeNominalActual::Lifetime(binder), b)?;
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
                crate::dyn_metadata::ty(&value.ty, b)?;
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
                    if value.classes.is_empty() {
                        LifeNominalActual::Type(value.ty.clone())
                    } else {
                        LifeNominalActual::QualifiedType {
                            ty: value.ty.clone(),
                            life: value.life.clone(),
                        }
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
                put(&mut expected, LifeNominalActual::Const(value.clone()), b)?;
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
        for slot in &field.life.slots {
            if !slot.path.is_empty() {
                return Err(unknown(
                    "nested nominal borrow field needs its complete typed projection embedding",
                ));
            }
            let class = field
                .classes
                .get(slot.binder as usize)
                .ok_or_else(|| fail("original field slot class is absent"))?;
            let binder = classes
                .iter()
                .position(|candidate| candidate == class)
                .ok_or_else(|| fail("original field has an unwitnessed persistent class"))?;
            put(
                &mut slots,
                StorageLifeSlot {
                    path: vec![path.clone()],
                    binder: u32::try_from(binder).map_err(|_| fail("field binder overflow"))?,
                    mode: slot.mode,
                },
                b,
            )?;
        }
        if !field.life.retained.is_empty() || !field.life.nominal_arguments.is_empty() {
            return Err(unknown(
                "nested nominal field needs its complete child class and phantom occurrence embedding",
            ));
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
    for instance in &inventory.module.life_instances {
        b.spend(
            instance.arguments.len().saturating_add(expected.len() + 1),
            WORK,
        )?;
        for argument in &instance.arguments {
            match argument {
                LifeNominalActual::Type(ty) => crate::dyn_metadata::ty(ty, b)?,
                LifeNominalActual::QualifiedType { ty, life } => {
                    crate::dyn_metadata::ty(ty, b)?;
                    crate::dyn_metadata::shape(life, b)?;
                }
                LifeNominalActual::Const(value) => {
                    b.spend(1, WORK)?;
                    if let Constant::WideString(units) = value {
                        b.spend(units.len(), WORK)?;
                    }
                }
                LifeNominalActual::Lifetime(_) => b.spend(1, WORK)?,
            }
        }
        if instance.declaration != *declaration || instance.arguments != expected {
            continue;
        }
        crate::dyn_metadata::ty(&instance.ty, b)?;
        let storage = match &instance.ty {
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
                actual.len() == fields.len()
                    && source_fields.iter().zip(actual).zip(fields).all(
                        |((source, (name, ty)), field)| source.name == *name && *ty == field.ty,
                    )
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
                for (source, (name, types)) in source_variants.iter().zip(actual) {
                    b.spend(types.len().saturating_add(name.len() + 1), WORK)?;
                    valid &= source.name == *name && source.fields.len() == types.len();
                    for ty in types {
                        valid &= fields.get(offset).is_some_and(|field| field.ty == *ty);
                        offset += 1;
                    }
                }
                valid && offset == fields.len()
            }
            _ => false,
        };
        if valid && selected.replace(instance).is_some() {
            return Err(fail(
                "nominal value has ambiguous complete physical instances",
            ));
        }
    }
    let instance = selected
        .ok_or_else(|| fail("nominal value has no exact original whole-domain instance"))?;
    b.spend(inventory.module.life_instances.len(), WORK)?;
    if inventory
        .module
        .life_instances
        .iter()
        .filter(|other| other.ty == instance.ty)
        .count()
        != 1
    {
        return Err(fail("nominal value has a nonunique exact type catalogue"));
    }
    crate::stored_flow::validate_nominal_metadata(inventory.module, instance, inventory.limits, b)?;
    Ok(Value {
        ty: instance.ty.clone(),
        life: shape,
        classes,
    })
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
        _ => Err(unknown(
            "semantic class expression needs its exact actual operand and scope relation",
        )),
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
    crate::dyn_metadata::ty(ty, b)?;
    crate::dyn_metadata::shape(life, b)?;
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
                Value {
                    ty: inner.as_ref().clone(),
                    life: child_life,
                    classes: local_classes,
                },
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
        ty: ty.clone(),
        life: life.clone(),
        classes: images,
    })
}
