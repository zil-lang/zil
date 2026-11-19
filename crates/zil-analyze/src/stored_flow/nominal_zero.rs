//! Complete zero persistent domains for plain nominal inventory entries.
//! This proof creates no Copy, ownership, address, loan or access authority.
use super::*;
use std::hash::{Hash, Hasher};

// Phantom Type arguments prove formation only. Runtime fields additionally
// require their actual canonical resource/creator inventory.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TypePurpose { Domain, Physical }

// This key only selects candidates. It deliberately does not hash child
// Types, domains or fields; every candidate still compares the complete Type.
fn candidate_hash(mut ty: &Type, b: &mut Budget) -> CompileResult<u64> {
    while let Type::BorrowedStorage { storage, .. } = ty {
        fee(b, 1)?;
        ty = storage;
    }
    let (kind, name) = match ty {
        Type::Record { name, .. } => (0u8, name),
        Type::Package { name, .. } => (1u8, name),
        Type::Enum { name, .. } => (2u8, name),
        _ => {
            return Err(failure(
                "zero nominal lookup requires an actual nominal type",
            ));
        }
    };
    b.spend(
        name.len().saturating_add(1),
        "zero nominal candidate identity hash",
    )?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hash);
    name.hash(&mut hash);
    Ok(hash.finish())
}

pub(super) fn validate(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    instance_zero_at(module, instance, limits, b, &mut Vec::new(), 0, TypePurpose::Physical)
}

pub(super) fn validate_type(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    type_zero_at(module, ty, limits, b, &mut Vec::new(), 0, TypePurpose::Physical)
}

pub(super) fn exact_index(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<usize> {
    let module_key = module as *const TypedModule as usize;
    let raw_work = crate::semantic::geometry::bounded_work(ty, b)?;
    let geometry_key = ty as *const Type as usize;
    if raw_work.is_some()
        && let Some(entry) = b.geometry_types.get(&geometry_key)
        && let Some((owner, depth, position)) = entry.nominal
        && owner == module_key
        && depth == limits.max_call_depth
    {
        return Ok(position);
    }
    // Bound the entire query before key selection, including unmatched input.
    // An exact immutable owned root already has a complete raw certificate.
    shape_work(ty, limits, b)?;
    let query_work = if let Some(work) = raw_work {
        work
    } else {
        let before = b.remaining();
        crate::dyn_metadata::ty(ty, b)?;
        before - b.remaining()
    };
    if b.nominal_zero_instances
        .as_ref()
        .is_none_or(|(owner, _)| *owner != module_key)
    {
        let mut index = BTreeMap::<u64, Vec<(usize, usize)>>::new();
        fee(b, module.life_instances.len())?;
        for (position, instance) in module.life_instances.iter().enumerate() {
            shape_work(&instance.ty, limits, b)?;
            let before = b.remaining();
            crate::dyn_metadata::ty(&instance.ty, b)?;
            let comparison_work = before - b.remaining();
            let hash = candidate_hash(&instance.ty, b)?;
            b.lookup(index.len(), "zero nominal exact identity index")?;
            fee(b, 1)?;
            index
                .entry(hash)
                .or_default()
                .push((position, comparison_work));
        }
        b.nominal_zero_instances = Some((module_key, index));
    }
    let hash = candidate_hash(ty, b)?;
    let (_, index) = b.nominal_zero_instances.as_ref().unwrap();
    let index_len = index.len();
    b.lookup(index_len, "zero nominal exact identity lookup")?;
    let width = b
        .nominal_zero_instances
        .as_ref()
        .unwrap()
        .1
        .get(&hash)
        .map_or(0, Vec::len);
    fee(b, width.saturating_mul(2))?;
    let candidates = b
        .nominal_zero_instances
        .as_ref()
        .unwrap()
        .1
        .get(&hash)
        .cloned()
        .unwrap_or_default();
    let mut selected = None;
    for (position, comparison_work) in candidates {
        let instance = &module.life_instances[position];
        // Immutable candidates were fully bounded when the directory formed.
        // Pay a complete paired Eq traversal, even if this comparison stops
        // early. The larger complete metadata tree bounds every compared node,
        // path, name and string; no successful domain/permission is reused.
        b.spend(
            query_work.max(comparison_work),
            "zero nominal complete identity comparison",
        )?;
        if instance.ty == *ty && selected.replace(position).is_some() {
            return Err(failure(
                "zero nominal type has ambiguous complete instances",
            ));
        }
    }
    let Some(position) = selected else {
        let mut error = failure("zero nominal type lacks its complete instance");
        let name = match ty {
            Type::Package { name, .. } | Type::Record { name, .. } | Type::Enum { name, .. } => name.as_str(),
            _ => "<qualified storage>",
        };
        b.spend(name.len().saturating_add(64), "missing nominal identity diagnostic")?;
        error[0].notes.push(format!("Expected an original nominal instance for {name}, but {width} candidates matched its identity bucket."));
        return Err(error);
    };
    if raw_work.is_some() {
        b.lookup(
            b.geometry_types.len(),
            "zero nominal immutable query publication",
        )?;
        if let Some(entry) = b.geometry_types.get_mut(&geometry_key) {
            entry.nominal = Some((module_key, limits.max_call_depth, position));
        }
    }
    Ok(position)
}

fn exact<'a>(
    module: &'a TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<&'a LifeNominalInstance> {
    Ok(&module.life_instances[exact_index(module, ty, limits, b)?])
}

fn empty(life: &StorageLifeShape) -> bool {
    life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
}

/// A catalogue row has one physical position. Only an actual element of this
/// slice may reuse its formation result, even if a detached row is equal.
fn position_of(rows: &[LifeNominalInstance], instance: &LifeNominalInstance) -> Option<usize> {
    let offset = std::ptr::from_ref(instance).addr().checked_sub(rows.as_ptr().addr())?;
    let position = offset / std::mem::size_of::<LifeNominalInstance>();
    rows.get(position)
        .filter(|row| std::ptr::eq(*row, instance))
        .map(|_| position)
}

fn instance_zero_at(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
    depth: usize,
    purpose: TypePurpose,
) -> CompileResult<()> {
    fee(b, active.len().saturating_add(1))?;
    if depth > limits.max_call_depth {
        return Err(failure("zero nominal domain exceeds the type depth limit"));
    }
    let key = instance as *const LifeNominalInstance as usize;
    if active.contains(&key) {
        return Err(failure("cyclic complete zero nominal domain"));
    }
    // A deep occurrence still runs its own depth-bounded proof. Reusing only
    // depth-zero successes cannot hide a longer current dependency path.
    let cache = if purpose == TypePurpose::Physical && depth == 0 && active.is_empty() {
        fee(b, 10)?;
        let root = position_of(&module.life_instances, instance);
        let phase = [
            b.semantic_zero_closures.len(),
            b.semantic_closures_proven.len(),
            b.semantic_declarations_proven.len(),
            b.semantic_dyn_tables_proven.len(),
            b.semantic_dyn_functions_proven.len(),
            limits.max_call_depth,
            b.partial_depth,
            b.helper_nodes,
        ];
        root.map(|root| (module as *const TypedModule as usize, phase, root))
    } else {
        None
    };
    if let Some((module, phase, root)) = cache {
        if b.nominal_zero_formed
            .as_ref()
            .is_none_or(|(owner, prior, _)| *owner != module || *prior != phase)
        {
            b.nominal_zero_formed = Some((module, phase, BTreeSet::new()));
        }
        let count = b.nominal_zero_formed.as_ref().unwrap().2.len();
        b.lookup(count, "complete zero nominal root formation lookup")?;
        if b.nominal_zero_formed.as_ref().unwrap().2.contains(&root) {
            return Ok(());
        }
    }
    crate::dyn_metadata::ty(&instance.ty, b)?;
    fee(b, 1)?;
    active.push(key);
    let storage = match &instance.ty {
        Type::BorrowedStorage { storage, life } if empty(life) => storage.as_ref(),
        Type::BorrowedStorage { .. } => {
            return Err(failure(
                "plain nominal depends on a nonzero qualified domain",
            ));
        }
        ty => ty,
    };
    fee(b, module.life_declarations.len())?;
    let declaration = module
        .life_declarations
        .iter()
        .find(|declaration| declaration.id == instance.declaration)
        .ok_or_else(|| failure("zero nominal declaration is absent"))?;
    identity_work(&declaration.identity, b)?;
    if !matches!(
        (declaration.kind, storage),
        (LifeNominalKind::Record, Type::Record { .. })
            | (LifeNominalKind::Resource, Type::Record { .. })
            | (LifeNominalKind::Package, Type::Package { .. })
            | (LifeNominalKind::Enum, Type::Enum { .. })
    ) {
        return Err(failure(
            "zero nominal declaration kind differs from its actual type",
        ));
    }
    if declaration.id.compiler {
        fee(b, 96)?;
        if matches!(instance.ty, Type::BorrowedStorage { .. }) {
            if !option_address::declaration(declaration)
                && !enum_address::poll_declaration(declaration)
                && !task_failure::declaration(declaration)
            {
                return Err(failure(
                    "zero nominal compiler declaration has no authentic complete role",
                ));
            }
            nominal::rows(module, instance, limits, b)?;
        } else {
            compiler_zero(declaration, instance, storage, b)?;
        }
    }
    fee(
        b,
        declaration
            .parameters
            .len()
            .saturating_add(instance.arguments.len()),
    )?;
    if declaration.parameters.len() != instance.arguments.len() {
        return Err(failure("zero nominal omits original mixed actual ordinals"));
    }
    // Phantom actuals participate even when no physical field mentions them.
    for (formal, actual) in declaration.parameters.iter().zip(&instance.arguments) {
        match (formal, actual) {
            (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => {
                type_zero_at(module, ty, limits, b, active, depth + 1, TypePurpose::Domain)?;
            }
            (LifeParameterKind::Type, LifeNominalActual::QualifiedType { ty, life }) => {
                fee(
                    b,
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len())
                        .saturating_add(1),
                )?;
                if !empty(life) {
                    return Err(failure("plain nominal has a nonzero qualified Type actual"));
                }
                type_zero_at(module, ty, limits, b, active, depth + 1, TypePurpose::Domain)?;
            }
            (LifeParameterKind::Const(carrier), LifeNominalActual::Const(value)) => {
                type_zero_at(module, carrier, limits, b, active, depth + 1, TypePurpose::Domain)?;
                b.type_work(carrier, false, "zero nominal constant carrier")?;
                if value.ty() != *carrier
                    || !matches!(
                        value,
                        Constant::Bool(_)
                            | Constant::I8(_)
                            | Constant::I16(_)
                            | Constant::I32(_)
                            | Constant::I64(_)
                            | Constant::Isize(_)
                            | Constant::U8(_)
                            | Constant::U16(_)
                            | Constant::U32(_)
                            | Constant::U64(_)
                            | Constant::Usize(_)
                    )
                {
                    return Err(failure("zero nominal const actual differs from its formal"));
                }
            }
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(_)) => {
                return Err(failure("plain nominal omits a persistent lifetime actual"));
            }
            _ => {
                return Err(failure(
                    "zero nominal actual kind differs from its full formal",
                ));
            }
        }
    }
    physical_zero_at(module, storage, limits, b, active, depth + 1, purpose)?;
    active.pop();
    if let Some((module, phase, root)) = cache {
        // No partial/negative/Pending result reaches this publication.
        if b.nominal_zero_formed
            .as_ref()
            .is_some_and(|(owner, prior, _)| *owner == module && *prior == phase)
        {
            let count = b.nominal_zero_formed.as_ref().unwrap().2.len();
            b.lookup(count, "complete zero nominal root formation insertion")?;
            b.nominal_zero_formed.as_mut().unwrap().2.insert(root);
        }
    }
    Ok(())
}

fn compiler_zero(
    declaration: &LifeNominalDeclaration,
    instance: &LifeNominalInstance,
    storage: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    if task_failure::declaration(declaration) {
        // The plain product is distinct from an enum. This only authenticates
        // its complete constructor; instance_zero still recursively proves
        // both original actuals and every physical child before publishing N0.
        return task_failure::plain_instance(declaration, instance, storage, b);
    }
    let Type::Enum { name, variants } = storage else {
        return Err(failure(
            "plain compiler nominal has no supported complete enum body",
        ));
    };
    fee(b, name.len().saturating_add(variants.len() + 1))?;
    if option_address::declaration(declaration) || enum_address::poll_declaration(declaration) {
        let child = match instance.arguments.as_slice() {
            [LifeNominalActual::Type(ty)] => ty,
            [LifeNominalActual::QualifiedType { ty, life }] if empty(life) => ty,
            _ => {
                return Err(failure(
                    "plain compiler enum loses its complete original Type actual",
                ));
            }
        };
        crate::dyn_metadata::ty(child, b)?;
        let (empty_name, value_name) = if declaration.id == zil_ir::builtin_life::OPTION {
            ("None", "Some")
        } else {
            ("Pending", "Ready")
        };
        if name.is_empty()
            || !matches!(variants.as_slice(),[(none,empty),(some,fields)] if none==empty_name && empty.is_empty() && some==value_name && matches!(fields.as_slice(),[payload] if payload==child))
        {
            return Err(failure(
                "plain compiler enum changes its canonical original variants or complete payload",
            ));
        }
        return Ok(());
    }
    let (source, physical, expected): (&str, &str, &[&str]) = match declaration.id {
        zil_ir::builtin_life::TASK_CREATE_ERROR => (
            "TaskCreateError",
            "@std.task:v1:TaskCreateError",
            &["Allocation"],
        ),
        zil_ir::builtin_life::TASK_POLL_STATE => (
            "TaskPollState",
            "@std.task:v1:TaskPollState",
            &["Complete", "Cancelled", "Invalid"],
        ),
        _ => {
            return Err(failure(
                "plain compiler nominal has no authentic sealed role",
            ));
        }
    };
    if declaration.kind != LifeNominalKind::Enum
        || !declaration.parameters.is_empty()
        || !instance.arguments.is_empty()
        || declaration.identity.package != "@zil.builtin"
        || declaration.identity.module_path != ["task"]
        || declaration.identity.declaration_path != ["enum", source]
        || !declaration.identity.generation_path.is_empty()
        || name != physical
        || variants.len() != expected.len()
    {
        return Err(failure(
            "plain Task enum changes its original role identity, formals or physical declaration",
        ));
    }
    for ((actual, fields), expected) in variants.iter().zip(expected) {
        fee(b, actual.len().saturating_add(fields.len() + 1))?;
        if actual.as_str() != *expected || !fields.is_empty() {
            return Err(failure(
                "plain Task enum changes its exact tag order or nullary payload",
            ));
        }
    }
    Ok(())
}

fn physical_zero_at(
    module: &TypedModule,
    storage: &Type,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
    depth: usize,
    purpose: TypePurpose,
) -> CompileResult<()> {
    fee(b, 1)?;
    match storage {
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fee(b, fields.len())?;
            for (_, ty) in fields {
                type_zero_at(module, ty, limits, b, active, depth, purpose)?;
            }
        }
        Type::Enum { variants, .. } => {
            fee(b, variants.len())?;
            for (_, fields) in variants {
                fee(b, fields.len())?;
                for ty in fields {
                    type_zero_at(module, ty, limits, b, active, depth, purpose)?;
                }
            }
        }
        _ => return Err(failure("zero nominal has no complete physical body")),
    }
    Ok(())
}

fn type_zero_at(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
    depth: usize,
    purpose: TypePurpose,
) -> CompileResult<()> {
    fee(b, 1)?;
    if depth > limits.max_call_depth {
        return Err(failure(
            "zero nominal dependency exceeds the type depth limit",
        ));
    }
    // Prepay complete recursive identity work, including opaque/retained data,
    // before any structural comparison or directory lookup.
    if crate::semantic::geometry::bounded_work(ty, b)?.is_none() {
        crate::dyn_metadata::ty(ty, b)?;
    }
    // This is intrinsic-domain formation, before runtime interface/permission
    // qualification. The bounded visitor above pays the full type identity;
    // type_work would incorrectly require already published Dyn interfaces.
    if crate::dyn_dispatch::is_create_error(ty) {
        return Ok(());
    }
    match ty {
        Type::Unit
        | Type::Bool
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::Isize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ByteBuffer
        | Type::SyncCell(_) => Ok(()),
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } => instance_zero_at(
            module,
            exact(module, ty, limits, b)?,
            limits,
            b,
            active,
            depth,
            purpose,
        ),
        Type::BorrowedStorage { storage, life } => {
            if !empty(life) {
                return Err(failure("plain nominal contains a nonzero qualified domain"));
            }
            if matches!(
                storage.as_ref(),
                Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
            ) {
                instance_zero_at(
                    module,
                    exact(module, ty, limits, b)?,
                    limits,
                    b,
                    active,
                    depth,
                    purpose,
                )
            } else {
                physical(module, ty, b)?;
                type_zero_at(module, storage, limits, b, active, depth + 1, purpose)
            }
        }
        Type::Tuple(fields) => {
            fee(b, fields.len())?;
            for ty in fields {
                type_zero_at(module, ty, limits, b, active, depth + 1, purpose)?;
            }
            Ok(())
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::OwnedStore { element }
        | Type::Managed(element)
        | Type::Mutex { payload: element } => {
            type_zero_at(module, element, limits, b, active, depth + 1, purpose)
        }
        Type::ErrorUnion { error, result } => {
            type_zero_at(module, error, limits, b, active, depth + 1, purpose)?;
            type_zero_at(module, result, limits, b, active, depth + 1, purpose)
        }
        Type::BorrowedFunctionPointer { parameters, result, life } => {
            crate::signature_life::charge(life, b)?;
            crate::borrowed_fp::relation(parameters, result, life)?;
            fee(b, parameters.len().saturating_add(1))?;
            for occurrence in parameters.iter().chain(std::iter::once(result.as_ref())) {
                let data = match occurrence {
                    Type::Address { inner, .. } => inner.as_ref(),
                    value => value,
                };
                // Invocation binders belong to this code signature, never
                // to the stored pointer value. The full data shape is still
                // required beyond each separately checked signature head.
                type_zero_at(module, data, limits, b, active, depth + 1, purpose)?;
            }
            Ok(())
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            fee(b, parameters.len())?;
            for ty in parameters {
                type_zero_at(module, ty, limits, b, active, depth + 1, purpose)?;
            }
            type_zero_at(module, result, limits, b, active, depth + 1, purpose)
        }
        Type::Address { .. } | Type::Desc(_) => Err(failure(
            "plain nominal contains an outer persistent borrow occurrence",
        )),
        Type::Task { result, .. } => {
            if purpose == TypePurpose::Physical {
                crate::tasks::semantic_resource_scope(module, ty, limits, b)
            } else {
                crate::tasks::semantic_type_domain_zero(ty, b)?;
                // The complete child formation is independent of the Task
                // directory, including any nested nominal/phantom children.
                type_zero_at(module, result, limits, b, active, depth + 1, TypePurpose::Domain)
            }
        },
        Type::Closure(id) => {
            b.lookup(
                b.semantic_zero_closures.len(),
                "completed zero closure domain",
            )?;
            if b.semantic_zero_closures.contains(id) {
                Ok(())
            } else {
                // Original formation can precede publication of the completed
                // closure directory. Reconstruct every stored child instead;
                // Some(0) alone is never enough. Original environment binding,
                // actual captures and callable access remain separate gates.
                fee(b, module.closures.len())?;
                let closure = module
                    .closures
                    .get(id.0 as usize)
                    .filter(|closure| closure.id == *id)
                    .ok_or_else(|| failure("zero closure has no exact actual environment"))?;
                let Some(header) = closure.storage_life.as_ref() else {
                    return Err(failure("zero closure omits its complete class directory"));
                };
                if header.fixed_binder_count != 0 {
                    return Err(failure("zero closure retains a nonzero fixed class domain"));
                }
                if !closure.lending_captures.is_empty() {
                    // A returned view may depend on the fresh invocation even
                    // when every value stored in this environment has N=0.
                    // Require the actual matching entry. Its access, return
                    // provenance and live loans are checked by the call proof.
                    let entry = module.functions.get(closure.function.0 as usize)
                        .filter(|f| f.id == closure.function && f.environment == Some(*id))
                        .ok_or_else(|| failure("zero lending closure lacks its actual entry"))?;
                    crate::signature_life::charge(header, b)?;
                    if let Some(actual) = &entry.storage_life {
                        crate::signature_life::charge(actual, b)?;
                    }
                    if entry.storage_life.as_ref() != Some(header)
                        || !crate::stored_callables::result_is_receiver(closure)
                    {
                        return Err(failure("zero lending closure lacks its matched invocation result bound"));
                    }
                }
                fee(b, closure.captures.len())?;
                for capture in &closure.captures {
                    type_zero_at(module, capture, limits, b, active, depth + 1, purpose)?;
                }
                Ok(())
            }
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            // Only the intrinsic domain is reconstructed here. This neither
            // publishes an admitted interface nor proves provider, access,
            // ownership or cleanup; all runtime consumers still require those.
            fee(b, module.dyn_interfaces.len())?;
            let mut selected = None;
            for candidate in &module.dyn_interfaces {
                if candidate.id == *interface && selected.replace(candidate).is_some() {
                    return Err(failure("zero Dyn has ambiguous actual interfaces"));
                }
            }
            let interface = selected.ok_or_else(|| failure("zero Dyn has no actual interface"))?;
            crate::dyn_metadata::recipe(&interface.application, b)?;
            crate::dyn_metadata::retained_shape(&interface.retained_life, b)?;
            if interface.context.is_some()
                || interface.retained_life.binder_count != 0
                || !interface.retained_life.ports.is_empty()
                || !interface.retained_life.retained.is_empty()
                || !interface.retained_life.nominal_arguments.is_empty()
                || !interface.application.actuals.is_empty()
                || !interface.inherited.is_empty()
                || !interface.associated_types.is_empty()
                || !interface.associated_constants.is_empty()
            {
                return Err(failure(
                    "zero Dyn requires its complete original Context and mixed domain",
                ));
            }
            fee(b, module.dyn_declarations.len())?;
            let mut declaration = None;
            for candidate in &module.dyn_declarations {
                if candidate.id == interface.application.declaration
                    && declaration.replace(candidate).is_some()
                {
                    return Err(failure("zero Dyn has ambiguous original declarations"));
                }
            }
            let declaration =
                declaration.ok_or_else(|| failure("zero Dyn has no original declaration"))?;
            crate::dyn_metadata::recipe_declaration(declaration, b)?;
            if declaration.kind != DynDeclarationKind::Trait || !declaration.parameters.is_empty() {
                return Err(failure(
                    "zero Dyn changes its original trait kind or complete arity",
                ));
            }
            Ok(())
        }
        Type::Registration { environment } | Type::AsyncRegistration { environment } => {
            // The environment determines the intrinsic domain. The provider,
            // live protocol phase and synchronous/asynchronous teardown are
            // independently reconstructed by registration's actual consumer.
            if !matches!(environment.as_ref(), Type::Closure(_)) {
                return Err(failure(
                    "zero registration has no concrete callback environment",
                ));
            }
            type_zero_at(module, environment, limits, b, active, depth + 1, purpose)
        }
        // Their hidden class domains need the completed original/opaque
        // consumer. Neither an empty port list nor Some(0) proves this case.
        Type::Opaque { .. }
        | Type::TaskI32
        | Type::MutexGuard { .. }
        | Type::Named(_)
        | Type::Unsupported(_) => Err(failure(
            "plain nominal dependency has no complete zero-domain proof",
        )),
    }
}

#[cfg(test)]
mod positions {
    use super::*;

    #[test]
    fn only_the_actual_catalogue_row_has_its_position() {
        let prototype = LifeNominalInstance {
            declaration: zil_ir::builtin_life::OPTION,
            ty: Type::I32,
            arguments: Vec::new(),
        };
        let rows = vec![prototype.clone(); 3];
        for (position, row) in rows.iter().enumerate() {
            assert_eq!(position_of(&rows, row), Some(position));
        }
        assert_eq!(position_of(&rows, &prototype), None);
        assert_eq!(position_of(&rows[1..], &rows[0]), None);
        assert_eq!(position_of(&rows[..2], &rows[2]), None);
        assert_eq!(position_of(&[], &rows[0]), None);
    }
}
