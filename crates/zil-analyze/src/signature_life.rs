//! Semantic class equations are not target origins or access paths.
use super::*;
#[path = "signature_ordinary_heads.rs"]
pub(super) mod ordinary_heads;

// The signature relation assigns no loan to a value-only Array. The module
// preflight separately validates every qualified child and its exact instance.
fn pure_array_value(ty: &Type) -> bool {
    if !matches!(ty, Type::Array { .. }) {
        return false;
    }
    pure_value_tree(ty)
}

// A complete class-free owning tree contributes no lifetime equation. This
// only classifies the known domain: canonical resource admission, moves and
// cleanup remain mandatory in the independent ownership consumers. Unknown
// shells, Address, Closure, Task and qualified children never become empty.
fn class_free_owned_tree(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            Type::Managed(payload)
                if matches!(
                    payload.as_ref(),
                    Type::Record { .. } | Type::ByteBuffer | Type::OwnedStore { .. }
                ) =>
            {
                pending.push(payload)
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty))
            }
            Type::Tuple(fields) => pending.extend(fields),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields)
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::Array { element, .. } | Type::OwnedStore { element } => pending.push(element),
            Type::ByteBuffer => {}
            Type::Unit | Type::Bool | Type::ForeignHandle | Type::CWideString => {}
            other if other.is_integer() || other.is_float() => {}
            _ => return false,
        }
    }
    true
}

fn pure_nominal_value(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) && pure_value_tree(ty)
}

fn pure_value_tree(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            Type::BorrowedStorage { storage, life } if life.slots.is_empty() => {
                pending.push(storage)
            }
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|(_, t)| t)),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64 => {}
            _ => return false,
        }
    }
    true
}

fn array_classes<'a>(
    ty: &'a Type,
    path: &mut Vec<LifeTypeProjection>,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32, &'a StorageLifeShape)>,
) {
    match ty {
        Type::BorrowedStorage { life, .. } => {
            for class in 0..life.binder_count {
                out.push((path.clone(), class, life));
            }
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)));
                array_classes(child, path, out);
                path.pop();
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            array_classes(element, path, out);
            path.pop();
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Record { fields, .. } => {
            for (index, (_, field)) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
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
                    array_classes(field, path, out);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

pub(super) fn charge(c: &StorageLifeContract, b: &mut Budget) -> CompileResult<()> {
    b.static_occurrences |= !c.static_bindings.is_empty();
    let rows = c
        .bindings
        .len()
        .saturating_add(c.classes.len())
        .saturating_add(c.static_bindings.len());
    b.spend(
        (c.fixed_binder_count as usize)
            .saturating_add(c.invocation_binder_count as usize)
            .saturating_add(rows.saturating_mul(rows))
            .saturating_add(rows.saturating_mul(2))
            .saturating_add(2),
        "signature relation row reservation",
    )?;
    for row in &c.bindings {
        b.spend(row.position.path.len(), "signature actual borrow path")?;
    }
    for position in &c.static_bindings {
        b.spend(position.path.len(), "signature constant borrow path")?;
    }
    for row in &c.classes {
        b.spend(row.position.path.len(), "signature semantic class path")?;
    }
    Ok(())
}

pub(super) fn outer(c: &StorageLifeContract, base: StorageBase) -> Option<StorageBinder> {
    c.bindings
        .iter()
        .find(|r| r.position.base == base && r.position.path.is_empty())
        .map(|r| r.binder)
}

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
) -> CompileResult<()> {
    relation_inner(
        parameters,
        None,
        result,
        c,
        false,
        &[],
        false,
        &[],
        None,
        &[],
        &[],
        &[],
    )
}
pub(super) fn function_relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
) -> CompileResult<()> {
    function_relation_inner(parameters, result, c, allow_callback, closures, false, &[])
}
fn function_relation_inner(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
    allow_byte_input: bool,
    interfaces: &[DynInterface],
) -> CompileResult<()> {
    function_relation_scopes_inner(
        parameters,
        result,
        c,
        allow_callback,
        closures,
        allow_byte_input,
        interfaces,
        &[],
    )
}
#[allow(clippy::too_many_arguments)]
fn function_relation_scopes_inner(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
    allow_byte_input: bool,
    interfaces: &[DynInterface],
    zero_resources: &[ZeroResourceRoot<'_>],
) -> CompileResult<()> {
    // Ordinary helpers may return one Task while accepting several references
    // in the same lifetime domain. The common relation checks every actual
    // input and result occurrence; constructor input/port counts do not apply.
    relation_inner(
        parameters,
        None,
        result,
        c,
        allow_callback,
        closures,
        allow_byte_input,
        interfaces,
        None,
        zero_resources,
        &[],
        &[],
    )
}

pub(super) fn closure_relation(
    closures: &[ClosureDefinition],
    interfaces: &[DynInterface],
    kind: CallableKind,
    parameters: &[Type],
    captures: &[Type],
    result: &Type,
    c: &StorageLifeContract,
) -> CompileResult<()> {
    relation_inner(
        parameters,
        Some(captures),
        result,
        c,
        true,
        closures,
        false,
        interfaces,
        Some(kind),
        &[],
        &[],
        &[],
    )
}
/// Module-aware resource domains do not confer an owning capture or call.
#[allow(clippy::too_many_arguments)]
pub(super) fn closure_relation_resource_scopes(
    module: &TypedModule,
    kind: CallableKind,
    parameters: &[Type],
    captures: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut roots = Vec::new();
    for (base, ty) in parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| (StorageBase::Parameter(index as u32), ty))
        .chain(
            captures
                .iter()
                .enumerate()
                .map(|(index, ty)| (StorageBase::Capture(index as u32), ty)),
        )
        .chain(std::iter::once((StorageBase::Result, result)))
    {
        if let Some(root) = resource_signature_root(module, base, ty, b)? {
            roots.push(root);
        }
    }
    relation_inner(
        parameters,
        Some(captures),
        result,
        c,
        true,
        &module.closures,
        false,
        &module.dyn_interfaces,
        Some(kind),
        &roots,
        &[],
        &[],
    )
}
pub(super) fn function_relation_in_module(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    module: &TypedModule,
    allow_byte_input: bool,
) -> CompileResult<()> {
    if !allow_byte_input && module.dyn_interfaces.is_empty() {
        return function_relation(parameters, result, c, allow_callback, &module.closures);
    }
    function_relation_inner(
        parameters,
        result,
        c,
        allow_callback,
        &module.closures,
        allow_byte_input,
        &module.dyn_interfaces,
    )
}

struct ZeroResourceRoot<'a> {
    base: StorageBase,
    whole_type: &'a Type,
}
fn store_resource_root<'a>(
    module: &TypedModule,
    base: StorageBase,
    ty: &'a Type,
    b: &mut Budget,
) -> CompileResult<Option<ZeroResourceRoot<'a>>> {
    b.spend(1, "resource signature complete root category")?;
    // A descriptor itself has a head class. An Address<Desc<...>> therefore
    // cannot claim a zero child domain by peeling both boundaries.
    let carrier = match ty {
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(_)) => inner.as_ref(),
        Type::Managed(_) | Type::Desc(_) => ty,
        _ => return Ok(None),
    };
    let (Type::Managed(payload) | Type::Desc(payload)) = carrier else {
        unreachable!()
    };
    if !matches!(payload.as_ref(), Type::OwnedStore { .. }) {
        return Ok(None);
    }
    crate::ownership_store::payload_definition(module, payload, b)?;
    if !crate::tasks::input_scope::known_zero(module, payload, b)? {
        return Ok(None);
    }
    b.type_work(ty, false, "actual zero Store signature occurrence")?;
    b.spend(1, "actual zero Store signature root")?;
    Ok(Some(ZeroResourceRoot {
        base,
        whole_type: ty,
    }))
}
fn resource_signature_root<'a>(
    module: &TypedModule,
    base: StorageBase,
    ty: &'a Type,
    b: &mut Budget,
) -> CompileResult<Option<ZeroResourceRoot<'a>>> {
    if let Some(root) = store_resource_root(module, base, ty, b)? {
        return Ok(Some(root));
    }
    let target = match ty {
        Type::Address { inner, .. } => inner.as_ref(),
        other => other,
    };
    if !matches!(
        target,
        Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
    ) || !crate::resources::sums::owned(target)
        || !crate::tasks::input_scope::known_zero(module, target, b)?
    {
        return Ok(None);
    }
    // Prove every possible arm and the full callback Product before recording
    // its zero free domain. This does not prove a move, target or cleanup edge.
    crate::resources::sums::admit(module, target, b)?;
    b.spend(1, "complete ordinary resource signature root")?;
    Ok(Some(ZeroResourceRoot {
        base,
        whole_type: ty,
    }))
}
fn opaque_resource(ty: &Type) -> Option<&Type> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let ty = if let Type::Managed(inner) | Type::Desc(inner) = ty {
        inner.as_ref()
    } else {
        ty
    };
    match ty {
        Type::Registration { .. } | Type::AsyncRegistration { .. } => Some(ty),
        Type::Task { retained_life, .. } if retained_life.binder_count == 0 => Some(ty),
        _ => None,
    }
}

/// Only a structural free-domain witness. Original parameter ordinals and
/// complete types stay intact; roots, state, cleanup and Transfer are separate.
#[allow(clippy::too_many_arguments)]
pub(super) fn function_relation_resource_scopes(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    module: &TypedModule,
    allow_byte_input: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut roots = Vec::new();
    let mut opaque_roots = false;
    for (base, ty) in parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| (StorageBase::Parameter(index as u32), ty))
        .chain(std::iter::once((StorageBase::Result, result)))
    {
        if let Some(root) = resource_signature_root(module, base, ty, b)? {
            roots.push(root);
        }
        if let Some(resource) = opaque_resource(ty) {
            if matches!(resource, Type::Task { .. }) {
                crate::tasks::require_closed_context(resource)?;
            }
            if crate::tasks::input_scope::domain(resource, b)?
                == crate::tasks::input_scope::Domain::Known(0)
            {
                b.type_work(ty, false, "actual zero-resource occurrence identity")?;
                roots.push(ZeroResourceRoot {
                    base,
                    whole_type: ty,
                });
                opaque_roots = true;
            }
        }
    }
    if opaque_roots && !c.static_bindings.is_empty() {
        return Err(bad(
            "opaque resource header requires its complete static occurrence consumer",
        ));
    }
    let nested = |ty: &Type| matches!(ty, Type::Address { inner, .. }
        if matches!(inner.as_ref(), Type::Address { .. } | Type::Desc(_)));
    let mut raw_shapes = Vec::new();
    if module.semantic.is_some() && (parameters.iter().any(nested) || nested(result)) {
        for function in &module.functions {
            b.spend(1, "nested reference actual signature owner")?;
            if !function.storage_life.as_ref().is_some_and(|actual| std::ptr::eq(actual, c)) { continue; }
            if function.parameters.len() != parameters.len() || function.return_type != *result {
                return Err(bad("nested reference signature changes its actual whole function"));
            }
            for (ordinal, (local, ty)) in function.parameters.iter().zip(parameters).enumerate() {
                if !nested(ty) { continue; }
                if function.locals.get(local.0 as usize).map(|row| &row.ty) != Some(ty) {
                    return Err(bad("nested reference signature changes its actual parameter Type"));
                }
                let life = crate::class_flow::raw_reference_shape(module, function, SemanticOperand::Local(*local), ty, b)?
                    .ok_or_else(|| bad("nested reference parameter omits its complete actual geometry"))?;
                b.spend(life.binder_count as usize, "nested reference complete signature coordinates")?;
                raw_shapes.push((StorageBase::Parameter(u32::try_from(ordinal).map_err(|_| bad("nested reference input ordinal overflow"))?), life));
            }
            if nested(result) {
                let initialized = crate::shared_initialization(function, b)?;
                let mut selected = None;
                for (block, live) in function.blocks.iter().zip(&initialized.reachable) {
                    if !*live { continue; }
                    let Terminator::Return(Some(value)) = block.terminator else { continue; };
                    let life = crate::class_flow::raw_reference_shape(module, function, SemanticOperand::Value(value), result, b)?
                        .ok_or_else(|| bad("nested reference result omits its complete actual geometry"))?;
                    crate::dyn_metadata::shape(&life, b)?;
                    if selected.as_ref().is_some_and(|old| old != &life) {
                        return Err(bad("nested reference Return alternatives change the complete local domain"));
                    }
                    selected = Some(life);
                }
                raw_shapes.push((StorageBase::Result, selected.ok_or_else(|| bad("nested reference signature has no complete actual Return geometry"))?));
            }
            break;
        }
    }
    let mut result_roles = Vec::new();
    if matches!(result, Type::Closure(_))
        && !b.semantic_reference_results.is_empty()
        && b.initialization
            .as_ref()
            .is_some_and(|cache| cache.module == module as *const TypedModule as usize)
    {
        b.spend(
            module.functions.len(),
            "original result-role header identity",
        )?;
        for function in &module.functions {
            if !function
                .storage_life
                .as_ref()
                .is_some_and(|header| std::ptr::eq(header, c))
            {
                continue;
            }
            b.type_work(result, true, "original result-role complete result")?;
            if function.return_type != *result || function.parameters.len() != parameters.len() {
                return Err(bad(
                    "original result role changes its actual function signature",
                ));
            }
            for (id, ty) in function.parameters.iter().zip(parameters) {
                b.type_work(ty, true, "original result-role complete input")?;
                if function
                    .locals
                    .get(id.0 as usize)
                    .filter(|local| local.id == *id)
                    .map(|local| &local.ty)
                    != Some(ty)
                {
                    return Err(bad("original result role changes an actual parameter"));
                }
            }
            b.spend(
                b.semantic_reference_results.len(),
                "original result-role complete directory",
            )?;
            for (owner, child, binder) in &b.semantic_reference_results {
                if *owner == function.id {
                    result_roles.push((*child, *binder));
                }
            }
            break;
        }
    }
    if !result_roles.is_empty() || !raw_shapes.is_empty() {
        b.spend(
            result_roles.len().saturating_add(c.classes.len()),
            "original result-role directory",
        )?;
        return relation_inner(
            parameters,
            None,
            result,
            c,
            allow_callback,
            &module.closures,
            allow_byte_input,
            &module.dyn_interfaces,
            None,
            &roots,
            &result_roles,
            &raw_shapes,
        );
    }
    if roots.is_empty() {
        return function_relation_in_module(
            parameters,
            result,
            c,
            allow_callback,
            module,
            allow_byte_input,
        );
    }
    function_relation_scopes_inner(
        parameters,
        result,
        c,
        allow_callback,
        &module.closures,
        allow_byte_input,
        &module.dyn_interfaces,
        &roots,
    )
}
#[allow(clippy::too_many_arguments)]
fn relation_inner(
    parameters: &[Type],
    captures: Option<&[Type]>,
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
    allow_byte_input: bool,
    interfaces: &[DynInterface],
    receiver_kind: Option<CallableKind>,
    zero_resources: &[ZeroResourceRoot<'_>],
    result_roles: &[(u32, StorageBinder)],
    raw_shapes: &[(StorageBase, StorageLifeShape)],
) -> CompileResult<()> {
    if c.classes
        .iter()
        .any(|row| row.position.base == StorageBase::Receiver)
    {
        return Err(bad("receiver head has no persistent class directory"));
    }
    let rows = c
        .bindings
        .len()
        .checked_add(c.classes.len())
        .ok_or_else(|| bad("signature row count overflow"))?;
    let domains = (c.fixed_binder_count as usize)
        .checked_add(c.invocation_binder_count as usize)
        .ok_or_else(|| bad("signature domain count overflow"))?;
    if domains > rows || captures.is_none() && c.fixed_binder_count as usize > c.classes.len() {
        return Err(bad("signature domain exceeds actual relation occurrences"));
    }
    let input_classes = c
        .classes
        .iter()
        .filter(|r| matches!(r.position.base, StorageBase::Parameter(_)))
        .count();
    if captures.is_none() && c.fixed_binder_count as usize > input_classes {
        return Err(bad("Fixed domain exceeds actual input class occurrences"));
    }
    struct Cursor<'a> {
        c: &'a StorageLifeContract,
        borrow: usize,
        constant: usize,
        class: usize,
        fixed: u32,
        invocation: u32,
        grounded: Vec<bool>,
        inputs: Option<(u32, u32)>,
        allow_callback: bool,
        allow_byte_input: bool,
        captured: bool,
        array_output_upper: Option<StorageBinder>,
        array_result_input: bool,
        closures: &'a [ClosureDefinition],
        interfaces: &'a [DynInterface],
        zero_resources: &'a [ZeroResourceRoot<'a>],
        result_roles: &'a [(u32, StorageBinder)],
        raw_shapes: &'a [(StorageBase, StorageLifeShape)],
    }
    impl Cursor<'_> {
        fn binder(&mut self, b: StorageBinder, retained: bool) -> CompileResult<()> {
            let (id, count, next, input) = match b {
                StorageBinder::Fixed(i) => (
                    i,
                    self.c.fixed_binder_count,
                    &mut self.fixed,
                    self.inputs.map(|x| x.0),
                ),
                StorageBinder::Invocation(i) => (
                    i,
                    self.c.invocation_binder_count,
                    &mut self.invocation,
                    self.inputs.map(|x| x.1),
                ),
            };
            if id >= count || id > *next || input.is_some_and(|n| id >= n) {
                return Err(bad("signature class is noncanonical or result-only"));
            }
            if id == *next {
                *next += 1;
            }
            if retained && let StorageBinder::Fixed(i) = b {
                self.grounded[i as usize] = true;
            }
            Ok(())
        }
        fn qualified_slots(
            &mut self,
            life: &zil_ir::StorageLifeShape,
            base: StorageBase,
            prefix: &[LifeTypeProjection],
        ) -> CompileResult<()> {
            for slot in &life.slots {
                if slot.binder >= life.binder_count {
                    return Err(bad(
                        "qualified physical slot leaves its complete class domain",
                    ));
                }
                let row = self
                    .c
                    .bindings
                    .get(self.borrow)
                    .ok_or_else(|| bad("qualified signature omits its actual physical slot"))?;
                let class = self
                    .c
                    .classes
                    .get(self.class + slot.binder as usize)
                    .ok_or_else(|| {
                        bad("qualified physical slot leaves its complete class domain")
                    })?;
                let path_matches = row.position.path.len() == prefix.len() + slot.path.len()
                    && prefix.iter().zip(&row.position.path).all(|(a, z)|
                        matches!((a, z), (LifeTypeProjection::Place(a), zil_ir::ContractProjection::Place(z)) if a == z))
                    && slot.path.iter().zip(&row.position.path[prefix.len()..]).all(|(a, z)|
                        matches!(z, zil_ir::ContractProjection::Place(z) if a == z));
                if row.position.base != base
                    || !path_matches
                    || class.position.base != base
                    || class.position.path != prefix
                    || class.position.class != slot.binder
                    || row.binder != class.binder
                {
                    return Err(bad(
                        "qualified physical slot changes its exact class relation",
                    ));
                }
                self.borrow += 1;
            }
            Ok(())
        }
        fn compound_child(
            &mut self,
            ty: &Type,
            base: StorageBase,
            path: &mut Vec<LifeTypeProjection>,
        ) -> CompileResult<()> {
            match ty {
                Type::Tuple(fields) => {
                    for (index, field) in fields.iter().enumerate() {
                        path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                            u32::try_from(index)
                                .map_err(|_| bad("compound signature ordinal overflow"))?,
                        )));
                        self.compound_child(field, base, path)?;
                        path.pop();
                    }
                }
                Type::Address { inner, mutable }
                    if matches!(inner.as_ref(), Type::Slice { element } if **element == Type::U8) =>
                {
                    let position = StoragePosition {
                        base,
                        path: path.iter().map(|step| match step {
                            LifeTypeProjection::Place(selector) => Ok(zil_ir::ContractProjection::Place(*selector)),
                            _ => Err(bad("compound address has a nonphysical occurrence path")),
                        }).collect::<CompileResult<Vec<_>>>()?,
                    };
                    if self.c.static_bindings.get(self.constant) == Some(&position) {
                        if *mutable {
                            return Err(bad("static compound occurrence must be an immutable address"));
                        }
                        self.constant += 1;
                    } else {
                        let row = self.c.bindings.get(self.borrow)
                            .ok_or_else(|| bad("compound signature omits its actual address occurrence"))?;
                        if row.position != position
                            || matches!(base, StorageBase::Capture(_))
                                && !matches!(row.binder, StorageBinder::Fixed(_))
                        {
                            return Err(bad("compound address changes its exact physical path or persistence"));
                        }
                        let binder = row.binder;
                        self.borrow += 1;
                        self.binder(binder, matches!(base, StorageBase::Capture(_)))?;
                    }
                }
                Type::BorrowedStorage { life, .. } => {
                    self.qualified_slots(life, base, path)?;
                    if life.binder_count as usize > self.c.classes.len().saturating_sub(self.class)
                    {
                        return Err(bad("compound signature omits child classes"));
                    }
                    for class in 0..life.binder_count {
                        let row = &self.c.classes[self.class];
                        if row.position.base != base
                            || row.position.path != *path
                            || row.position.class != class
                        {
                            return Err(bad(
                                "compound signature class differs from actual child scope",
                            ));
                        }
                        let binder = row.binder;
                        let capture = matches!(base, StorageBase::Capture(_));
                        if capture && !matches!(binder, StorageBinder::Fixed(_)) {
                            return Err(bad("captured compound child class must be Fixed"));
                        }
                        self.class += 1;
                        self.binder(binder, capture || !self.captured && self.inputs.is_none())?;
                    }
                }
                Type::Managed(payload)
                    if matches!(
                        payload.as_ref(),
                        Type::Record { .. } | Type::ByteBuffer | Type::OwnedStore { .. }
                    ) => {}
                other if other.is_copy_value() => {}
                _ => {
                    return Err(bad(
                        "compound signature child needs its complete value consumer",
                    ));
                }
            }
            Ok(())
        }
        fn ty(&mut self, ty: &Type, base: StorageBase) -> CompileResult<()> {
            if matches!(ty, Type::Desc(inner)
                if class_free_owned_tree(inner)
                    || self.zero_resources.iter().any(|root| root.base == base && root.whole_type == ty)
                    || matches!(inner.as_ref(), Type::Mutex { payload } if **payload == Type::I32))
            {
                // A complete closed weak head has one outer occurrence and
                // no child class domain. Consuming its exact header row does
                // not hold the owner or extend the descriptor's actual epoch.
                let row = self
                    .c
                    .bindings
                    .get(self.borrow)
                    .ok_or_else(|| bad("weak resource signature omits its outer occurrence"))?;
                if row.position.base != base
                    || !row.position.path.is_empty()
                    || matches!(base, StorageBase::Capture(_))
                        && !matches!(row.binder, StorageBinder::Fixed(_))
                {
                    return Err(bad(
                        "weak resource signature changes its exact outer position or persistence",
                    ));
                }
                let binder = row.binder;
                self.borrow += 1;
                self.binder(binder, matches!(base, StorageBase::Capture(_)))?;
                return Ok(());
            }
            if matches!(ty, Type::Tuple(_)) && !ty.is_copy_value() {
                return self.compound_child(ty, base, &mut Vec::new());
            }
            let capture = matches!(base, StorageBase::Capture(_));
            let (target, pointed) = if let Type::Address { inner, mutable } = ty {
                let position = StoragePosition {
                    base,
                    path: Vec::new(),
                };
                if self.c.static_bindings.get(self.constant) == Some(&position) {
                    if *mutable {
                        return Err(bad(
                            "static signature occurrence must be an immutable address",
                        ));
                    }
                    self.constant += 1;
                } else {
                    let row = self
                        .c
                        .bindings
                        .get(self.borrow)
                        .ok_or_else(|| bad("signature omits actual outer borrow"))?;
                    if row.position.base != base || !row.position.path.is_empty() {
                        return Err(bad(
                            "signature borrow position is flattened or noncanonical",
                        ));
                    }
                    self.borrow += 1;
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad(
                            "captured Address must retain a Fixed environment class",
                        ));
                    }
                    self.binder(row.binder, capture)?;
                }
                (inner.as_ref(), true)
            } else {
                (ty, false)
            };
            if pointed && matches!(target, Type::Address { .. } | Type::Desc(_)) {
                let shape = self.raw_shapes.iter().find(|(owner, _)| *owner == base)
                    .map(|(_, shape)| shape)
                    .ok_or_else(|| bad("nested signature requires its complete actual whole-reference geometry"))?;
                let [head] = shape.slots.as_slice() else { return Err(bad("nested signature changes its unique physical outer slot")); };
                if !head.path.is_empty() || head.binder >= shape.binder_count {
                    return Err(bad("nested signature outer slot leaves its complete domain"));
                }
                let outer = self.c.bindings.get(self.borrow.saturating_sub(1))
                    .filter(|row| row.position.base == base && row.position.path.is_empty())
                    .ok_or_else(|| bad("nested signature has no independently consumed outer borrow"))?;
                let mut images = vec![None; shape.binder_count as usize];
                images[head.binder as usize] = Some(outer.binder);
                for child in &shape.retained {
                    if child.path.first() != Some(&LifeTypeProjection::Pointee) || child.binder >= shape.binder_count {
                        return Err(bad("nested signature child leaves its complete Pointee domain"));
                    }
                    let row = self.c.classes.get(self.class)
                        .ok_or_else(|| bad("nested signature omits a complete intrinsic child class"))?;
                    if row.position.base != base || row.position.path != child.path || row.position.class != child.class {
                        return Err(bad("nested signature changes an exact child path or local class ordinal"));
                    }
                    if let Some(previous) = images[child.binder as usize]
                        && previous != row.binder {
                        return Err(bad("nested signature changes one original child's whole-class embedding"));
                    }
                    images[child.binder as usize] = Some(row.binder);
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured nested reference child is not Fixed"));
                    }
                    self.class += 1;
                    self.binder(row.binder, capture || !self.captured && self.inputs.is_none())?;
                }
                if images.iter().any(Option::is_none) { return Err(bad("nested signature omits an intrinsic or phantom coordinate")); }
                return Ok(());
            }
            if super::stored_flow::shared_array_value(target) {
                if capture {
                    return Err(bad("array captures require their own retained consumer"));
                }
                let Type::Array { length, .. } = target else {
                    unreachable!()
                };
                for index in 0..*length {
                    let row =
                        self.c.bindings.get(self.borrow).ok_or_else(|| {
                            bad("array signature omits an actual element binding")
                        })?;
                    if row.position.base != base
                        || row.position.path.as_slice()
                            != [ContractProjection::Place(PlaceProjection::Field(index))]
                    {
                        return Err(bad(
                            "array signature element binding differs from actual ordinal",
                        ));
                    }
                    if matches!(base, StorageBase::Parameter(_))
                        && Some(row.binder) == self.array_output_upper
                    {
                        self.array_result_input = true;
                    }
                    self.borrow += 1;
                    self.binder(row.binder, false)?;
                }
                return Ok(());
            }
            if self
                .zero_resources
                .iter()
                .any(|root| root.base == base && root.whole_type == ty)
            {
                return Ok(());
            }
            let task = match target {
                Type::Task { .. } => Some((target, None, false)),
                Type::Managed(inner) | Type::Desc(inner)
                    if matches!(inner.as_ref(), Type::Task { .. }) =>
                {
                    Some((inner.as_ref(), None, true))
                }
                Type::ErrorUnion {
                    result: success, ..
                } => match success.as_ref() {
                    Type::Managed(inner) | Type::Desc(inner)
                        if matches!(inner.as_ref(), Type::Task { .. }) =>
                    {
                        Some((inner.as_ref(), Some(PlaceProjection::ErrorArm(false)), true))
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some((task, arm, wrapped)) = task {
                super::tasks::require_closed_context(task)?;
                let Type::Task { retained_life, .. } = task else {
                    unreachable!()
                };
                // This is an actual complete Task shape, already checked by
                // require_closed_context. Zero data rows stay known empty;
                // an outer head Borrow was consumed independently above.
                let mut class_path = Vec::new();
                let mut borrow_path = Vec::new();
                if let Some(arm) = arm {
                    class_path.push(LifeTypeProjection::Place(arm));
                    borrow_path.push(ContractProjection::Place(arm));
                }
                if pointed {
                    class_path.push(LifeTypeProjection::Pointee);
                }
                if wrapped {
                    class_path.push(LifeTypeProjection::Pointee);
                }
                for (port, shape) in retained_life.ports.iter().enumerate() {
                    let row = self
                        .c
                        .bindings
                        .get(self.borrow)
                        .ok_or_else(|| bad("Task signature omits retained port"))?;
                    let mut path = borrow_path.clone();
                    path.push(ContractProjection::RetainedPort(port as u32));
                    if row.position.base != base || row.position.path != path {
                        return Err(bad("Task retained port relation differs from actual owner"));
                    }
                    let class_row = self
                        .c
                        .classes
                        .get(self.class + shape.binder as usize)
                        .ok_or_else(|| bad("Task retained class absent"))?;
                    if row.binder != class_row.binder {
                        return Err(bad("Task port and class bind different source lifetimes"));
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured Task port is not persistent Fixed"));
                    }
                    self.borrow += 1;
                    self.binder(row.binder, self.inputs.is_none())?;
                }
                for local in 0..retained_life.binder_count {
                    let row = self
                        .c
                        .classes
                        .get(self.class)
                        .ok_or_else(|| bad("Task signature omits retained class"))?;
                    if row.position.base != base
                        || row.position.path != class_path
                        || row.position.class != local
                    {
                        return Err(bad(
                            "Task retained class position differs from actual owner",
                        ));
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured Task class is not persistent Fixed"));
                    }
                    self.class += 1;
                    self.binder(row.binder, self.inputs.is_none())?;
                }
            } else if let Some((interface, arm)) = match target {
                Type::Dyn { interface } | Type::OwnedDyn { interface } => Some((*interface, None)),
                Type::ErrorUnion { error, result }
                    if **error == super::dyn_dispatch::create_error() =>
                {
                    match result.as_ref() {
                        Type::OwnedDyn { interface } => {
                            Some((*interface, Some(PlaceProjection::ErrorArm(false))))
                        }
                        _ => None,
                    }
                }
                _ => None,
            } {
                let item = self
                    .interfaces
                    .iter()
                    .find(|item| item.id == interface)
                    .ok_or_else(|| bad("dyn signature class selector has no actual interface"))?;
                for local in 0..item.retained_life.binder_count {
                    let row = self.c.classes.get(self.class).ok_or_else(|| {
                        bad("dyn signature omits its complete Context class domain")
                    })?;
                    let mut path = Vec::new();
                    if pointed {
                        path.push(LifeTypeProjection::Pointee);
                    }
                    if let Some(arm) = arm {
                        path.push(LifeTypeProjection::Place(arm));
                    }
                    let path = row.position.path == path;
                    if row.position.base != base || row.position.class != local || !path {
                        return Err(bad(
                            "dyn signature class must select its actual Context boundary",
                        ));
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured dyn Context class must remain Fixed"));
                    }
                    self.class += 1;
                    self.binder(
                        row.binder,
                        capture || !self.captured && self.inputs.is_none(),
                    )?;
                }
            } else if let Type::BorrowedStorage { storage, life } = target {
                if (!matches!(
                    storage.as_ref(),
                    Type::Record { .. } | Type::Package { .. } | Type::Tuple(_)
                ) || (!life.retained.is_empty()
                    && !matches!(storage.as_ref(), Type::Package { .. } | Type::Tuple(_))))
                    && !pure_nominal_value(target)
                    && !super::stored_flow::enum_address::signature(target)
                    && !super::stored_flow::option_address::signature(target)
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "signature qualified type needs its owning/nested consumer",
                        None,
                    )]);
                }
                if life.binder_count == 0
                    || life.binder_count as usize > self.c.classes.len().saturating_sub(self.class)
                {
                    return Err(bad("signature omits qualified class domain"));
                }
                if !pointed {
                    self.qualified_slots(life, base, &[])?;
                }
                for local in 0..life.binder_count {
                    let row = &self.c.classes[self.class];
                    if row.position.base != base
                        || row.position.class != local
                        || if pointed {
                            row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                        } else {
                            !row.position.path.is_empty()
                        }
                    {
                        return Err(bad(
                            "signature class must name the exact qualified type boundary",
                        ));
                    }
                    self.class += 1;
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured qualified class must be Fixed"));
                    }
                    self.binder(
                        row.binder,
                        capture || !self.captured && self.inputs.is_none(),
                    )?;
                }
            } else if pure_array_value(target)
                || (matches!(target, Type::ErrorUnion { .. })
                    && super::stored_flow::enum_address::signature(target))
            {
                let mut path = if pointed {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    Vec::new()
                };
                let mut classes = Vec::new();
                array_classes(target, &mut path, &mut classes);
                for (path, local, life) in classes {
                    if local == 0 && path.iter().all(|step| matches!(step, LifeTypeProjection::Place(_))) {
                        self.qualified_slots(life, base, &path)?;
                    }
                    let row = self
                        .c
                        .classes
                        .get(self.class)
                        .ok_or_else(|| bad("signature omits actual Array child class"))?;
                    if row.position.base != base
                        || row.position.path != path
                        || row.position.class != local
                    {
                        return Err(bad(
                            "signature Array child class differs from actual Element path",
                        ));
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured Array child class must be Fixed"));
                    }
                    self.class += 1;
                    self.binder(
                        row.binder,
                        capture || !self.captured && self.inputs.is_none(),
                    )?;
                }
            } else if self.allow_callback && matches!(target, Type::Closure(_)) {
                let Type::Closure(id) = target else {
                    unreachable!()
                };
                let definition = self
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("closure class selector has no actual definition"))?;
                if let Some(life) = &definition.storage_life {
                    let count = life.fixed_binder_count as usize;
                    if count > self.c.classes.len().saturating_sub(self.class) {
                        return Err(bad("signature omits actual closure free Fixed domain"));
                    }
                    for class in 0..count {
                        let row = &self.c.classes[self.class];
                        if row.position.base != base
                            || row.position.class as usize != class
                            || if pointed {
                                row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                            } else {
                                !row.position.path.is_empty()
                            }
                        {
                            return Err(bad(
                                "closure class selector must name its actual free Fixed domain",
                            ));
                        }
                        if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                            return Err(bad("captured child Closure class must be Fixed"));
                        }
                        self.class += 1;
                        let local_result = base == StorageBase::Result
                            && !pointed
                            && matches!(row.binder, StorageBinder::Invocation(_))
                            && self.result_roles.contains(&(class as u32, row.binder));
                        if local_result {
                            let inputs = self.inputs;
                            self.inputs = None;
                            self.binder(row.binder, false)?;
                            self.inputs = inputs;
                        } else {
                            self.binder(row.binder, self.inputs.is_none())?;
                        }
                    }
                } else {
                    return Err(bad(
                        "complete closure class directory is unknown, not empty",
                    ));
                }
            } else if pointed {
                if !(target.is_address_pointee()
                    || ty.is_shared_sync_cell_address()
                    // A resource head contributes the address's outer class,
                    // not a lifetime hidden inside its closed owning payload.
                    // Actual resource identity and CFG head access are checked
                    // independently by ownership and stored-flow.
                    || matches!(target, Type::Managed(_)) && class_free_owned_tree(target)
                    || matches!(target, Type::Package { .. }) && class_free_owned_tree(target)
                    // A borrowed Package manager may ground a Store window.
                    // Mutable input shape does not supply a target or loan;
                    // body provenance and exclusive authority are checked later.
                    || self.inputs.is_none()
                        && matches!(base, StorageBase::Parameter(_))
                        && matches!(ty, Type::Address { .. })
                        && matches!(target, Type::Package { .. })
                    || self.allow_callback && matches!(target, Type::Closure(_)))
                {
                    return Err(bad(format!(
                        "signature pointee is outside the implemented profile: {target:?}"
                    )));
                }
            } else if !(target.is_copy_value()
                || class_free_owned_tree(target)
                || pure_array_value(target)
                // A proved query's exact weak input has no semantic class.
                // It creates/grounds no row and is not an owning consumer.
                || self.allow_byte_input && self.inputs.is_none()
                    && matches!(base,StorageBase::Parameter(_))
                    && matches!(ty,Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer)
                // A direct owned field contributes no class/borrow position.
                // Capture admission and the independent resource kernel still
                // establish its finite payload, actual mode and responsibility.
                || capture && matches!(target, Type::Managed(_))
                // An actual helper's finite Managed companion is a carrier,
                // not a class-bearing input. Body/resource proofs still settle it.
                || self.allow_callback && self.inputs.is_none()
                    && matches!(base, StorageBase::Parameter(_))
                    && matches!(target, Type::Managed(payload)
                        if matches!(payload.as_ref(), Type::Record { fields, .. }
                            if fields.iter().all(|(_, ty)| super::stored_callables::scalar(ty))))
                || self.allow_callback
                    && self.inputs.is_none()
                    && matches!(target, Type::Closure(_)))
            {
                return Err(bad(format!(
                    "signature value needs its ownership consumer: {ty:?} at {base:?}"
                )));
            }
            Ok(())
        }
    }
    let mut cursor = Cursor {
        c,
        borrow: 0,
        constant: 0,
        class: 0,
        fixed: 0,
        invocation: 0,
        grounded: vec![false; c.fixed_binder_count as usize],
        inputs: None,
        allow_callback,
        allow_byte_input,
        captured: captures.is_some(),
        array_output_upper: match result {
            Type::Address { mutable: false, .. } => c
                .bindings
                .last()
                .filter(|row| {
                    row.position.base == StorageBase::Result && row.position.path.is_empty()
                })
                .map(|row| row.binder),
            _ => None,
        },
        array_result_input: false,
        closures,
        interfaces,
        zero_resources,
        result_roles,
        raw_shapes,
    };
    for (i, p) in parameters.iter().enumerate() {
        let i = u32::try_from(i).map_err(|_| bad("signature parameter ordinal overflow"))?;
        cursor.ty(p, StorageBase::Parameter(i))?;
    }
    if let Some(captures) = captures {
        for (i, capture) in captures.iter().enumerate() {
            let i = u32::try_from(i).map_err(|_| bad("capture ordinal overflow"))?;
            cursor.ty(capture, StorageBase::Capture(i))?;
        }
    }
    if let Some(row) = c.bindings.get(cursor.borrow)
        && row.position.base == StorageBase::Receiver
    {
        if !matches!(receiver_kind, Some(CallableKind::Fn | CallableKind::FnMut))
            || !row.position.path.is_empty()
            || !matches!(row.binder, StorageBinder::Invocation(_))
        {
            return Err(bad(
                "Receiver requires an actual non-consuming borrowed closure head",
            ));
        }
        if receiver_kind == Some(CallableKind::Fn)
            && matches!(result, Type::Address { mutable: true, .. })
            && c.bindings
                .iter()
                .any(|r| r.position.base == StorageBase::Result && r.binder == row.binder)
        {
            return Err(bad("shared receiver cannot grant an exclusive result view"));
        }
        cursor.borrow += 1;
        cursor.binder(row.binder, false)?;
    }
    cursor.inputs = Some((cursor.fixed, cursor.invocation));
    cursor.ty(result, StorageBase::Result)?;
    if cursor.borrow != c.bindings.len()
        || cursor.constant != c.static_bindings.len()
        || cursor.class != c.classes.len()
        || cursor.fixed != c.fixed_binder_count
        || cursor.invocation != c.invocation_binder_count
        || cursor.grounded.iter().any(|g| !g)
    {
        return Err(bad(
            "signature relation has missing/extra rows or ungrounded Fixed classes",
        ));
    }
    // A declared Address result still needs an actual authority-bearing input;
    // the body proof separately checks the exact returned target.
    if let Type::Address { mutable, inner } = result
        && !c
            .static_bindings
            .iter()
            .any(|position| position.base == StorageBase::Result && position.path.is_empty())
    {
        let out = outer(c, StorageBase::Result);
        let buffer_window =
            matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8);
        if !parameters.iter().enumerate().any(|(i, p)| {
            (matches!(p,Type::Address{mutable:m,..}if !mutable||*m)
                || (buffer_window
                    && matches!(p, Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer)))
                && outer(c, StorageBase::Parameter(i as u32)) == out
        }) && !zero_resources.iter().any(|root| {
            // The complete, checked Store head can ground a field view. Its
            // actual projected source and exclusive loan still come from the CFG.
            let Type::Desc(payload) = root.whole_type else {
                return false;
            };
            let Type::OwnedStore { element } = payload.as_ref() else {
                return false;
            };
            let eligible = if inner == element {
                !mutable
            } else {
                inner.is_copy_value()
            };
            eligible && matches!(root.base, StorageBase::Parameter(_)) && outer(c, root.base) == out
        }) && !c.classes.iter().any(|r| {
            let StorageBase::Parameter(i) = r.position.base else {
                return false;
            };
            Some(r.binder) == out
                && (!mutable
                    || matches!(
                        parameters.get(i as usize),
                        Some(Type::Address { mutable: true, .. })
                    ))
                && (!mutable || r.position.path.contains(&LifeTypeProjection::Pointee))
        }) && !(matches!(receiver_kind, Some(CallableKind::Fn | CallableKind::FnMut))
            && (!mutable || receiver_kind == Some(CallableKind::FnMut))
            && outer(c, StorageBase::Receiver).is_some_and(|receiver| Some(receiver) == out))
            && !captures.is_some_and(|captures| {
                captures.iter().enumerate().any(|(index, ty)| {
                    matches!(ty,Type::Address{mutable:available,..}if (!mutable||*available)
                    && outer(c,StorageBase::Capture(index as u32))==out)
                })
            })
            && !c.classes.iter().any(|row| {
                let StorageBase::Capture(index) = row.position.base else {
                    return false;
                };
                Some(row.binder) == out
                    && captures
                        .and_then(|captures| captures.get(index as usize))
                        .is_some_and(|ty| {
                            !mutable
                                || matches!(
                                    ty,
                                    Type::Address { mutable: true, .. }
                                        | Type::BorrowedStorage { .. }
                                )
                        })
            })
            && !cursor.array_result_input
        {
            return Err(bad(
                "signature Address result has no eligible declared input",
            ));
        }
    }
    Ok(())
}

/// Each equality required by the concrete target must hold in the pointer's
/// universally callable relation. Distinct variables are not disjointness.
pub(super) fn entails(required: &StorageLifeContract, supplied: &StorageLifeContract) -> bool {
    if required.bindings.len() != supplied.bindings.len()
        || required.static_bindings != supplied.static_bindings
        || required.classes.len() != supplied.classes.len()
    {
        return false;
    }
    if !required
        .bindings
        .iter()
        .zip(&supplied.bindings)
        .all(|(a, z)| a.position == z.position)
        || !required
            .classes
            .iter()
            .zip(&supplied.classes)
            .all(|(a, z)| a.position == z.position)
    {
        return false;
    }
    let rows = required
        .bindings
        .iter()
        .map(|r| r.binder)
        .chain(required.classes.iter().map(|r| r.binder));
    let actual = supplied
        .bindings
        .iter()
        .map(|r| r.binder)
        .chain(supplied.classes.iter().map(|r| r.binder));
    for (left, mapped) in rows.clone().zip(actual.clone()) {
        for (right, other) in rows.clone().zip(actual.clone()) {
            if left == right && mapped != other {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Tag".into(),
                fields: vec![("n".into(), Type::I32)],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![NominalLifeArgument {
                    path: vec![],
                    declaration: LifeDeclarationId {
                        module: zil_base::ModuleId(0),
                        declaration: zil_base::NodeId(1),
                        compiler: false,
                    },
                    argument: 0,
                    binder: 0,
                }],
            },
        }
    }
    fn address(t: Type) -> Type {
        Type::Address {
            mutable: false,
            inner: Box::new(t),
        }
    }
    fn relation2(class: u32) -> StorageLifeContract {
        StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: 0,
            invocation_binder_count: 2,
            bindings: (0..2)
                .map(|i| StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(i),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(i),
                })
                .collect(),
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Parameter(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: StorageBinder::Invocation(class),
            }],
        }
    }
    #[test]
    fn qualified_value_consumes_physical_slots_and_complete_classes() {
        let holder = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Holder".into(),
                fields: vec![("pointer".into(), address(Type::I32))],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![zil_ir::StorageLifeSlot {
                    path: vec![PlaceProjection::Field(0)],
                    binder: 0,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let mut header = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            static_bindings: vec![],
            bindings: vec![
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Result,
                        path: vec![zil_ir::ContractProjection::Place(PlaceProjection::Field(0))],
                    },
                    binder: StorageBinder::Invocation(0),
                },
            ],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Result,
                    path: vec![],
                    class: 0,
                },
                binder: StorageBinder::Invocation(0),
            }],
        };
        let check = |header: &StorageLifeContract| {
            function_relation(&[address(Type::I32)], &holder, header, false, &[])
        };
        check(&header).unwrap();
        header.bindings[1].position.path =
            vec![zil_ir::ContractProjection::Place(PlaceProjection::Field(1))];
        assert!(check(&header).is_err());
        header.bindings[1].position.path =
            vec![zil_ir::ContractProjection::Place(PlaceProjection::Field(0))];
        header.bindings[1].binder = StorageBinder::Invocation(1);
        assert!(check(&header).is_err());
        header.bindings.pop();
        assert!(check(&header).is_err());
        // A reference to the carrier has one outer physical occurrence and
        // the complete pointee class; it does not inline the carrier's fields.
        let pointed = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 2,
            static_bindings: vec![],
            bindings: vec![StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(0),
            }],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Parameter(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: StorageBinder::Invocation(1),
            }],
        };
        function_relation(&[address(holder)], &Type::Unit, &pointed, false, &[]).unwrap();
    }
    #[test]
    fn proved_local_result_role_is_exact_and_never_an_outer_address_permission() {
        let closure = ClosureDefinition {
            storage_life: Some(StorageLifeContract {
                fixed_binder_count: 1,
                invocation_binder_count: 0,
                bindings: vec![StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Capture(0),
                        path: vec![],
                    },
                    binder: StorageBinder::Fixed(0),
                }],
                static_bindings: vec![],
                classes: vec![],
            }),
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![address(Type::I32)],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        };
        let mut header = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 3,
            bindings: (0..2)
                .map(|index| StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(index),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(index),
                })
                .collect(),
            static_bindings: vec![],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Result,
                    path: vec![],
                    class: 0,
                },
                binder: StorageBinder::Invocation(2),
            }],
        };
        let parameters = [address(Type::I32), address(Type::I32)];
        let closures = [closure];
        let check =
            |header: &StorageLifeContract, result: &Type, roles: &[(u32, StorageBinder)]| {
                relation_inner(
                    &parameters,
                    None,
                    result,
                    header,
                    true,
                    &closures,
                    false,
                    &[],
                    None,
                    &[],
                    roles,
                    &[],
                )
            };
        assert!(check(&header, &Type::Closure(ClosureId(0)), &[]).is_err());
        assert!(
            check(
                &header,
                &Type::Closure(ClosureId(0)),
                &[(1, StorageBinder::Invocation(2))]
            )
            .is_err()
        );
        assert!(
            check(
                &header,
                &Type::Closure(ClosureId(0)),
                &[(0, StorageBinder::Invocation(1))]
            )
            .is_err()
        );
        check(
            &header,
            &Type::Closure(ClosureId(0)),
            &[(0, StorageBinder::Invocation(2))],
        )
        .unwrap();
        header.classes.clear();
        header.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            binder: StorageBinder::Invocation(2),
        });
        assert!(
            check(
                &header,
                &address(Type::I32),
                &[(0, StorageBinder::Invocation(2))]
            )
            .is_err()
        );
    }

    #[test]
    fn impossible_domain_is_rejected_before_allocating_claimed_class_storage() {
        let impossible = StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: u32::MAX,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
        };
        assert!(relation(&[], &Type::I32, &impossible).is_err());
    }
    #[test]
    fn captured_array_child_class_cannot_be_invocation_local() {
        let child = Type::Array {
            element: Box::new(tag()),
            length: 2,
        };
        let mut contract = StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            bindings: vec![],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Capture(0),
                    path: vec![LifeTypeProjection::Element],
                    class: 0,
                },
                binder: StorageBinder::Invocation(0),
            }],
        };
        assert!(
            closure_relation(
                &[],
                &[],
                CallableKind::Fn,
                &[],
                std::slice::from_ref(&child),
                &Type::Unit,
                &contract
            )
            .is_err()
        );
        contract.fixed_binder_count = 1;
        contract.invocation_binder_count = 0;
        contract.classes[0].binder = StorageBinder::Fixed(0);
        closure_relation(
            &[],
            &[],
            CallableKind::Fn,
            &[],
            &[child],
            &Type::Unit,
            &contract,
        )
        .unwrap();
    }
    #[test]
    fn qualified_class_is_joint_with_outer_inputs_and_never_a_physical_borrow_row() {
        let p = vec![address(tag()), address(Type::I32)];
        let one = relation2(0);
        let two = relation2(1);
        relation(&p, &Type::I32, &one).unwrap();
        relation(&p, &Type::I32, &two).unwrap();
        assert!(!entails(&one, &two));
        assert!(!entails(&two, &one));
        assert!(entails(&one, &one));
        let mut flat = one.clone();
        flat.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
            },
            binder: StorageBinder::Invocation(0),
        });
        assert!(relation(&p, &Type::I32, &flat).is_err());
        let mut omitted = one;
        omitted.classes.clear();
        assert!(relation(&p, &Type::I32, &omitted).is_err());
    }
}
