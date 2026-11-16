//! Actual erased receivers and slot boundaries. The original interface scheme
//! is solved before this module sees a Typed slot or a concrete provider.
use super::*;
use crate::semantics::original::{IntrinsicCall, IntrinsicKind};

fn receiver_origin(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    mut value: ValueId,
    original: ValueId,
) -> R<bool> {
    let mut seen = std::collections::BTreeSet::new();
    while value != original {
        world.pay(seen.len().saturating_add(1))?;
        if seen.len() >= world.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dynamic receiver reborrow depth exceeded",
                None,
            ));
        }
        if !seen.insert(value) {
            return Err(bad("dynamic receiver contains a cyclic actual SSA origin"));
        }
        let mut producer = None;
        for block in &function.blocks {
            world.pay(block.instructions.len())?;
            for operation in &block.instructions {
                if operation.result == Some(value) && producer.replace(&operation.kind).is_some() {
                    return Err(bad("dynamic receiver repeats an actual SSA producer"));
                }
            }
        }
        match producer {
            Some(InstructionKind::ReborrowAddress { address, .. }) => value = *address,
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn borrow(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    local: LocalId,
    mutable: bool,
    ty: &Type,
    image: SemanticClassExpr,
    source: &SemanticTypeValue,
) -> R<SharedType> {
    let source = match World::type_of(source)? {
        Type::OwnedDyn { .. } => {
            if mutable
                && !function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| bad("dynamic owner local is absent"))?
                    .mutable
            {
                return Err(bad("exclusive dynamic borrow has an immutable owner local"));
            }
            world.copy(source)?
        }
        Type::Address {
            mutable: parent,
            inner,
        } if matches!(inner.as_ref(), Type::OwnedDyn { .. }) => {
            if mutable && !*parent {
                return Err(bad(
                    "exclusive dynamic borrow has only a shared parent address",
                ));
            }
            world.pointee(source)?
        }
        _ => {
            return Err(bad(
                "dynamic owner borrow reads another actual carrier kind",
            ));
        }
    };
    let SemanticTypeValue::Resolved {
        ty: Type::OwnedDyn { interface },
        life,
        classes,
    } = world.owned(source)?
    else {
        return Err(bad("dynamic owner borrow has no complete retained domain"));
    };
    if !matches!(ty,Type::Address{mutable:actual,inner} if *actual==mutable && inner.as_ref()==&Type::Dyn{interface})
    {
        return Err(bad(
            "dynamic owner borrow changes its interface or receiver mode",
        ));
    }
    let inner = SemanticTypeValue::Resolved {
        ty: Type::Dyn { interface },
        life,
        classes,
    };
    world.address(ty, image, &inner)
}

/// Bind only the fresh outer receiver role. The Before observation is the
/// initialized owner; its complete Context domain is preserved by `borrow`.
pub(super) fn seal_borrow(
    world: &mut World<'_, '_>,
    original: &SemanticDefinition,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    local: LocalId,
    image: &SemanticClassExpr,
    implicit: &mut BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad("dynamic receiver borrow has no original body"));
    };
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("dynamic receiver borrow has no original Gamma"))?;
    let SemanticClassExpr::OuterOccurrence {
        operand: SemanticOperand::Value(result),
        path,
        ..
    } = image
    else {
        return Err(bad(
            "dynamic receiver borrow has no fresh actual result occurrence",
        ));
    };
    if !path.is_empty() {
        return Err(bad(
            "dynamic receiver borrow invents an inner outer occurrence",
        ));
    }
    world.pay(function.blocks.len())?;
    let operation = function
        .blocks
        .iter()
        .find(|row| row.id == block)
        .and_then(|row| row.instructions.get(index))
        .ok_or_else(|| bad("dynamic receiver borrow leaves its actual CFG"))?;
    if operation.result != Some(*result)
        || !matches!(operation.kind, InstructionKind::BorrowDynOwned { local: actual, .. } if actual == local)
    {
        return Err(bad(
            "dynamic receiver role is not its actual BorrowDynOwned producer",
        ));
    }
    let mut role = None;
    world.pay(sites.len())?;
    for call_site in sites {
        let Some(call) = proof.intrinsics.get(&(original.id, call_site.recipe)) else {
            continue;
        };
        let IntrinsicKind::DynCall { receiver_node, .. } = &call.kind else {
            continue;
        };
        if !matches!(
            call_site.action,
            SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
        ) {
            continue;
        }
        let SemanticSiteAddress::Typed {
            block: call_block,
            instruction,
            ..
        } = call_site.at
        else {
            continue;
        };
        world.pay(function.blocks.len())?;
        let operation = function
            .blocks
            .iter()
            .find(|row| row.id == call_block)
            .and_then(|row| row.instructions.get(instruction as usize))
            .ok_or_else(|| bad("dynamic call occurrence leaves its actual CFG"))?;
        let InstructionKind::CallDynSlot { callee, .. } = operation.kind else {
            continue;
        };
        if !receiver_origin(world, function, callee, *result)? {
            continue;
        }
        world.pay(sites.len().saturating_add(proof.intrinsics.len()))?;
        for site in sites {
            let owner_local = match &site.action {
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Local(target),
                    ..
                } => Some(*target),
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Parameter(parameter),
                    ..
                } => function.parameters.get(*parameter as usize).copied(),
                _ => None,
            };
            if owner_local != Some(local) {
                continue;
            }
            world.pay(site_nodes.len())?;
            let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
            let Some(binding) = nodes.next() else {
                continue;
            };
            if nodes.next().is_some() {
                return Err(bad(
                    "dynamic receiver observation repeats its original source",
                ));
            }
            if binding.node != SemanticOriginalNode::Expression(*receiver_node) {
                continue;
            }
            let recipe = world.original_recipe(original, site.recipe)?;
            let Some(SemanticTypeRecipe::Address {
                lifetime:
                    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    }),
                ..
            }) = call.parameters.first()
            else {
                continue;
            };
            world.pay(original.implicit_lifetimes.len())?;
            if !original.implicit_lifetimes.iter().any(|entry| {
                entry.ordinal == *ordinal && entry.occurrence_path == recipe.occurrence_path
            }) {
                return Err(bad(
                    "dynamic receiver outer role differs from its original borrow occurrence",
                ));
            }
            if role.is_some_and(|previous| previous != *ordinal) {
                return Err(bad("dynamic receiver borrow has ambiguous original roles"));
            }
            role = Some(*ordinal);
        }
    }
    let ordinal = if let Some(ordinal) = role {
        ordinal
    } else {
        let mut diagnostic =
            bad("dynamic receiver borrow has no complete original occurrence seal");
        world.pay(128)?;
        diagnostic.notes.push(format!("original {:?}; actual function {:?}, block {:?}, instruction {}, local {:?}, result {:?}", original.id, function.id, block, index, local, result));
        for site in sites {
            world.pay(
                site_nodes
                    .len()
                    .saturating_add(proof.intrinsics.len())
                    .saturating_add(64),
            )?;
            let nodes: Vec<_> = site_nodes
                .iter()
                .filter(|row| row.site == site.recipe)
                .map(|row| row.node)
                .collect();
            if let SemanticSiteAction::TypeUse { subject, .. } = &site.action {
                crate::dyn_metadata::semantic_operand_snapshot(
                    subject,
                    world.limits,
                    world.left,
                    1,
                )?;
                diagnostic.notes.push(format!(
                    "receiver type-use site {:?} nodes {:?}, at {:?}, subject {:?}",
                    site.recipe, nodes, site.at, subject
                ));
            }
            if let Some(call) = proof.intrinsics.get(&(original.id, site.recipe)) {
                if let IntrinsicKind::DynCall { receiver_node, .. } = &call.kind {
                    if let Some(parameter) = call.parameters.first() {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            parameter,
                            world.limits,
                            world.left,
                            1,
                        )?;
                    }
                    diagnostic.notes.push(format!("original dynamic call site {:?}, receiver node {:?}, parameter {:?}, actual {:?}, action {:?}", site.recipe, receiver_node, call.parameters.first(), site.at, std::mem::discriminant(&site.action)));
                }
            }
        }
        for row in &function.blocks {
            world.pay(row.instructions.len())?;
            for (index, operation) in row.instructions.iter().enumerate() {
                let cost = match &operation.kind {
                    InstructionKind::CallDynSlot { arguments, .. } => {
                        Some(arguments.len().saturating_add(1))
                    }
                    InstructionKind::BorrowDynOwned { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::Store { .. } => Some(1),
                    _ => None,
                };
                if let Some(cost) = cost {
                    world.pay(cost.saturating_mul(64))?;
                    diagnostic.notes.push(format!("actual receiver path block {:?}, instruction {}, result {:?}, operation {:?}", row.id, index, operation.result, operation.kind));
                }
            }
        }
        return Err(diagnostic);
    };
    world.pay(implicit.len().saturating_add(1))?;
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    if let Some(old) = implicit.insert(ordinal, image.clone()) {
        if old != *image {
            return Err(bad(
                "one dynamic receiver occurrence selects different actual heads",
            ));
        }
    }
    Ok(())
}
fn insert(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    map: &mut BTreeMap<StorageBinder, SemanticClassExpr>,
    binder: StorageBinder,
    image: &SemanticClassExpr,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    world.pay(map.len().saturating_add(1))?;
    if let Some(old) = map.get(&binder) {
        return world.same_class(scope, old, image, implicit);
    }
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    map.insert(binder, image.clone());
    Ok(())
}
fn mixed(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    expected: &SemanticActual,
    actual: &DynGenericActual,
    map: &BTreeMap<StorageBinder, SemanticClassExpr>,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(expected, world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
    match (expected, actual) {
        (
            SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
            DynGenericActual::Type(target),
        ) if ty == target && life.binder_count == 0 && classes.is_empty() => Ok(()),
        (
            SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
            DynGenericActual::QualifiedType(target),
        ) if ty == &target.ty && life == &target.life && classes.len() == target.classes.len() => {
            world.pay(classes.len())?;
            for (image, binder) in classes.iter().zip(&target.classes) {
                let expected = map
                    .get(binder)
                    .ok_or_else(|| bad("dynamic Type actual uses an unbound full class role"))?;
                world.same_class(scope, image, expected, implicit)?;
            }
            Ok(())
        }
        (
            SemanticActual::Const {
                ty: SemanticTypeValue::Resolved { ty, life, classes },
                value,
            },
            DynGenericActual::Const(target),
        ) if ty == &target.ty()
            && life.binder_count == 0
            && classes.is_empty()
            && value == target =>
        {
            Ok(())
        }
        (SemanticActual::Life(image), DynGenericActual::Life(binder)) => {
            let target = map
                .get(binder)
                .ok_or_else(|| bad("dynamic Life actual uses an unbound original role"))?;
            world.same_class(scope, image, target, implicit)
        }
        _ => Err(bad(
            "dynamic slot changes its complete original mixed actual",
        )),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    call: &IntrinsicCall,
    site: &SemanticSite,
    instruction: &InstructionKind,
    parameters: &[SharedType],
    result: &SemanticTypeValue,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let IntrinsicKind::DynCall {
        receiver_node: _,
        declaration,
        parent,
        member,
        receiver_mode,
        parameter_modes,
        result_mode,
        call_kind,
    } = &call.kind
    else {
        return Err(bad("dynamic bridge receives another original scheme"));
    };
    let InstructionKind::CallDynSlot {
        interface, slot, ..
    } = instruction
    else {
        return Err(bad("dynamic original call has another actual opcode"));
    };
    world.pay(world.module.dyn_interfaces.len())?;
    let mut interfaces = world
        .module
        .dyn_interfaces
        .iter()
        .filter(|row| row.id == *interface);
    let table = interfaces
        .next()
        .ok_or_else(|| bad("dynamic slot has no actual interface"))?;
    if interfaces.next().is_some() {
        return Err(bad("dynamic actual interface identity repeats"));
    }
    let target = table
        .slots
        .get(*slot as usize)
        .ok_or_else(|| bad("dynamic actual slot ordinal is absent"))?;
    crate::dyn_metadata::actual_slot_snapshot(target, world.limits, world.left, 1)?;
    if target.receiver.mode != *receiver_mode
        || target.parameter_modes != *parameter_modes
        || target.result_mode != *result_mode
        || target.call_kind != *call_kind
        || parameters.len() != target.parameters.len().saturating_add(1)
        || World::type_of(result)? != &target.result
    {
        return Err(bad(
            "dynamic actual slot changes its complete original ABI or modes",
        ));
    }
    let receiver = parameters
        .first()
        .ok_or_else(|| bad("dynamic scheme omits its receiver"))?;
    if !matches!(World::type_of(receiver)?,Type::Address{mutable,inner} if *mutable==target.receiver.mutable&&inner.as_ref()==&Type::Dyn{interface:*interface})
    {
        return Err(bad(
            "dynamic actual receiver changes its complete original interface or access",
        ));
    }
    for (parameter, ty) in parameters.iter().skip(1).zip(&target.parameters) {
        if World::type_of(parameter)? != ty {
            return Err(bad(
                "dynamic slot changes a complete original parameter type",
            ));
        }
    }
    let context = world.pointee(receiver)?;
    let SemanticTypeValue::Resolved {
        classes: context, ..
    } = context.as_ref()
    else {
        return Err(bad("dynamic receiver Context remains unresolved"));
    };
    let header = target
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("dynamic slot omits its complete original lifetime header"))?;
    if header.fixed_binder_count as usize != context.len() {
        return Err(bad(
            "dynamic header truncates the entire receiver Context domain",
        ));
    }
    let mut bindings = Vec::new();
    let mut classes = Vec::new();
    let mut statics = Vec::new();
    world.pay(parameters.len().saturating_add(context.len()))?;
    for (ordinal, parameter) in parameters.iter().enumerate() {
        world.header_value(
            parameter,
            StorageBase::Parameter(ordinal as u32),
            &mut bindings,
            &mut classes,
            &mut statics,
        )?;
    }
    world.header_value(
        result,
        StorageBase::Result,
        &mut bindings,
        &mut classes,
        &mut statics,
    )?;
    if bindings.len() != header.bindings.len()
        || classes.len() != header.classes.len()
        || statics.len() != header.static_bindings.len()
    {
        return Err(bad(
            "dynamic slot header omits an original outer or intrinsic class occurrence",
        ));
    }
    let mut map = BTreeMap::new();
    for (ordinal, image) in context.iter().enumerate() {
        insert(
            world,
            instance.id,
            &mut map,
            StorageBinder::Fixed(ordinal as u32),
            image,
            implicit,
        )?;
    }
    let binding_paths = bindings.iter().fold(0usize, |n, (position, _)| {
        n.saturating_add(position.path.len().saturating_add(1))
    });
    let class_paths = classes.iter().fold(0usize, |n, (position, _)| {
        n.saturating_add(position.path.len().saturating_add(1))
    });
    let actual_binding_paths = header.bindings.iter().fold(0usize, |n, row| {
        n.saturating_add(row.position.path.len().saturating_add(1))
    });
    let actual_class_paths = header.classes.iter().fold(0usize, |n, row| {
        n.saturating_add(row.position.path.len().saturating_add(1))
    });
    world.pay(
        binding_paths
            .saturating_mul(header.bindings.len())
            .saturating_add(actual_binding_paths.saturating_mul(bindings.len()))
            .saturating_add(class_paths.saturating_mul(header.classes.len()))
            .saturating_add(actual_class_paths.saturating_mul(classes.len())),
    )?;
    // Input occurrences establish invocation images. Outputs may only consume
    // that mapping; an output claim never anchors its own class proof.
    for outputs in [false, true] {
        world.pay(
            header
                .bindings
                .len()
                .saturating_mul(bindings.len())
                .saturating_add(header.classes.len().saturating_mul(classes.len())),
        )?;
        for row in &header.bindings {
            if (row.position.base == StorageBase::Result) != outputs {
                continue;
            }
            if !matches!(
                row.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                return Err(bad(
                    "dynamic header reads an invented capture or receiver scope",
                ));
            }
            let mut expected = bindings
                .iter()
                .filter(|(position, _)| position == &row.position);
            let (_, image) = expected
                .next()
                .ok_or_else(|| bad("dynamic header invents an original outer occurrence"))?;
            if expected.next().is_some() {
                return Err(bad("dynamic original outer occurrence is ambiguous"));
            }
            match row.binder {
                StorageBinder::Fixed(index) if index >= header.fixed_binder_count => {
                    return Err(bad("dynamic Fixed binder exceeds complete Context"));
                }
                StorageBinder::Invocation(index) if index >= header.invocation_binder_count => {
                    return Err(bad("dynamic Invocation binder exceeds its original scope"));
                }
                _ => {}
            }
            if outputs {
                let target = map.get(&row.binder).ok_or_else(|| {
                    pending(
                        "dynamic result class lacks a complete original input or Context source",
                    )
                })?;
                world.same_class(instance.id, image, target, implicit)?;
            } else {
                insert(world, instance.id, &mut map, row.binder, image, implicit)?;
            }
        }
        for row in &header.classes {
            if (row.position.base == StorageBase::Result) != outputs {
                continue;
            }
            if !matches!(
                row.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                return Err(bad("dynamic intrinsic class reads an invented scope"));
            }
            let mut expected = classes
                .iter()
                .filter(|(position, _)| position == &row.position);
            let (_, image) = expected
                .next()
                .ok_or_else(|| bad("dynamic header invents a complete intrinsic occurrence"))?;
            if expected.next().is_some() {
                return Err(bad("dynamic intrinsic occurrence is ambiguous"));
            }
            match row.binder {
                StorageBinder::Fixed(index) if index >= header.fixed_binder_count => {
                    return Err(bad("dynamic intrinsic Fixed binder exceeds Context"));
                }
                StorageBinder::Invocation(index) if index >= header.invocation_binder_count => {
                    return Err(bad("dynamic intrinsic Invocation binder exceeds scope"));
                }
                _ => {}
            }
            if outputs {
                let target = map.get(&row.binder).ok_or_else(|| {
                    pending("dynamic result intrinsic class lacks its complete original source")
                })?;
                world.same_class(instance.id, image, target, implicit)?;
            } else {
                insert(world, instance.id, &mut map, row.binder, image, implicit)?;
            }
        }
    }
    world.pay(
        statics
            .iter()
            .fold(0usize, |n, position| {
                n.saturating_add(position.path.len().saturating_add(1))
            })
            .saturating_mul(header.static_bindings.len())
            .saturating_add(
                header
                    .static_bindings
                    .iter()
                    .fold(0usize, |n, position| {
                        n.saturating_add(position.path.len().saturating_add(1))
                    })
                    .saturating_mul(statics.len()),
            ),
    )?;
    for position in &statics {
        if header
            .static_bindings
            .iter()
            .filter(|candidate| *candidate == position)
            .count()
            != 1
        {
            return Err(bad(
                "dynamic slot changes a complete original static occurrence",
            ));
        }
    }
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("dynamic call loses its checked original mixed solution"))?;
    let recipes = proof
        .calls
        .get(&(instance.definition, site.recipe))
        .ok_or_else(|| bad("dynamic call has no original mixed solution"))?;
    if recipes.len() != target.actuals.len() {
        return Err(bad("dynamic slot omits full method actuals"));
    }
    world.pay(recipes.len())?;
    for (recipe, actual) in recipes.iter().zip(&target.actuals) {
        let expected = super::super::materialize::actual_in_view(
            world.program,
            &world.view,
            world.ledger,
            instance.id,
            recipe,
            world.limits,
            world.left,
        )?;
        mixed(world, instance.id, &expected, actual, &map, implicit)?;
    }
    super::super::materialize::dynamic::call_slot(
        world.program,
        world.module,
        world.ledger,
        *interface,
        *slot,
        *parent,
        *declaration,
        *member,
        world.limits,
        world.left,
        parameters,
        result,
    )?;
    Ok(())
}

impl World<'_, '_> {
    /// A real constant-source SSA or a checked local role retaining that
    /// source can fill a shorter shared field. No type/nominal class equality
    /// follows from this directed outer relation.
    pub(super) fn constructor_field_source(
        &mut self,
        scope: SemanticInstanceId,
        operand: ValueId,
        actual: &SemanticTypeValue,
        expected: &SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        self.pay(1)?;
        if matches!(Self::type_of(actual)?, Type::Address { mutable: false, .. })
            && matches!(
                Self::type_of(expected)?,
                Type::Address { mutable: false, .. }
            )
        {
            let constant = matches!(
                self.value_role_sources.get(&operand),
                Some(roles::Source::Static)
            ) || matches!(roles::outer(self, actual)?, roles::Source::Static);
            if constant {
                let a = self.pointee(actual)?;
                let b = self.pointee(expected)?;
                return self.same(scope, &a, &b, implicit);
            }
        }
        self.same(scope, actual, expected, implicit)
    }
}

impl World<'_, '_> {
    pub(super) fn check_owned_provider(
        &mut self,
        interface: DynInterfaceId,
        table: DynVtableId,
        source: &Type,
        route: DynOwnedStorage,
    ) -> R<()> {
        super::super::materialize::dynamic::owned_provider(self, interface, table, source, route)
    }

    /// Interpret the actual entry's full P/R in its real rigid original scope
    /// before a Context contract can project any of those type operands.
    pub(in crate::semantics) fn dynamic_provider_signature(
        &mut self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        method: &SemanticDefinition,
        declaration: &SemanticDefinition,
        table: &DynVtable,
        interface: &DynInterface,
        slot: &DynSlot,
    ) -> R<(Vec<SharedType>, SharedType)> {
        {
            let ledger = self.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("Dyn provider signature has no original Gamma"))?;
            self.pay(1)?;
            if !proof.dyn_provider_conformance(method.id, declaration.id) {
                return Err(bad(
                    "Dyn provider signature lacks its original quantified declaration conformance",
                ));
            }
        }
        if instance.definition != method.id
            || !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function.id)
        {
            return Err(bad(
                "Dyn provider signature changes its real original entry",
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &method.body else {
            return Err(bad(
                "Dyn provider signature omits its complete original body",
            ));
        };
        if signature.parameters.len() != function.parameters.len()
            || function.parameters.len() != slot.parameters.len().saturating_add(1)
            || signature.call_kind != function.call_kind
            || signature.result_mode != function.result_mode
            || slot.parameter_modes.len() != slot.parameters.len()
        {
            return Err(bad(
                "Dyn provider signature omits a complete original parameter",
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature: declared,
            ..
        } = &declaration.body
        else {
            return Err(bad(
                "Dyn provider declaration has no complete original signature",
            ));
        };
        if declared.parameters.len() != signature.parameters.len()
            || signature.parameters.first().map(|p| p.mode) != Some(slot.receiver.mode)
            || declared.parameters.first().map(|p| p.mode) != Some(slot.receiver.mode)
            || signature
                .parameters
                .iter()
                .skip(1)
                .zip(&slot.parameter_modes)
                .any(|(parameter, mode)| parameter.mode != *mode)
        {
            return Err(bad("Dyn provider changes complete original passing modes"));
        }
        let mut parameters = Vec::new();
        for (ordinal, local) in function.parameters.iter().enumerate() {
            let local = function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("Dyn provider parameter local is absent"))?;
            let value = self.parameter_value(instance.id, ordinal, &local.ty)?;
            if Self::type_of(&value)? != &local.ty
                || ordinal != 0 && slot.parameters.get(ordinal - 1) != Some(Self::type_of(&value)?)
            {
                return Err(bad(
                    "Dyn provider parameter changes its independently interpreted original Type",
                ));
            }
            if ordinal == 0
                && !matches!(Self::type_of(&value)?,Type::Address {mutable,..}if *mutable==slot.receiver.mutable)
            {
                return Err(bad(
                    "Dyn provider changes its original receiver access mode",
                ));
            }
            self.pay(
                1 + if parameters.len() == parameters.capacity() {
                    parameters.len()
                } else {
                    0
                },
            )?;
            if parameters.len() >= self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "Dyn original signature exceeds node limit",
                    None,
                ));
            }
            parameters.try_reserve(1).map_err(|_| {
                Diagnostic::limit(
                    Phase::Types,
                    "Dyn original signature allocation failed",
                    None,
                )
            })?;
            parameters.push(value);
        }
        let result = self.result_value(instance.id, &function.return_type)?;
        if Self::type_of(&result)? != &function.return_type
            || Self::type_of(&result)? != &slot.result
        {
            return Err(bad(
                "Dyn provider result changes its independently interpreted original Type",
            ));
        }
        self.check_header(instance, function, signature)?;
        self.dynamic_provider_header(function, table, interface, slot)?;
        Ok((parameters, result))
    }

    fn dynamic_provider_header(
        &mut self,
        function: &TypedFunction,
        table: &DynVtable,
        interface: &DynInterface,
        slot: &DynSlot,
    ) -> R<()> {
        let source = function
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Dyn provider has no complete original header"))?;
        let target = slot
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Dyn slot has no complete original header"))?;
        let frame = function
            .dyn_instance
            .as_ref()
            .ok_or_else(|| bad("Dyn provider has no exact original dynamic frame"))?;
        if target.fixed_binder_count != interface.retained_life.binder_count
            || frame.declaration_actuals.len() != table.actuals.len()
            || frame.method_actuals.len() != slot.actuals.len()
        {
            return Err(bad(
                "Dyn provider header truncates a complete lexical or Context domain",
            ));
        }
        crate::dyn_metadata::actual_slot_snapshot(slot, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(frame, self.limits, self.left, 1)?;
        if frame.receiver.ty != table.self_type
            || table.self_life.as_ref() != Some(&frame.receiver.life)
        {
            return Err(bad(
                "Dyn provider header changes independently derived complete Self",
            ));
        }
        let mut fixed = BTreeMap::new();
        let mut bind = |world: &mut World<'_, '_>,
                        local: StorageBinder,
                        image: StorageBinder,
                        persistent: bool|
         -> R<()> {
            world.pay(fixed.len().saturating_add(1))?;
            if !matches!(local,StorageBinder::Fixed(index)if index<frame.scope.fixed_binder_count)
                || matches!(image,StorageBinder::Fixed(index)if index>=target.fixed_binder_count)
                || matches!(image,StorageBinder::Invocation(index)if persistent||index>=target.invocation_binder_count)
            {
                return Err(bad("Dyn provider changes a rigid lexical/Context class"));
            }
            if fixed.insert(local, image).is_some_and(|old| old != image) {
                return Err(bad(
                    "Dyn provider has contradictory original-to-Context images",
                ));
            }
            Ok(())
        };
        for (persistent, local, context) in [
            (
                true,
                frame.declaration_actuals.as_slice(),
                table.actuals.as_slice(),
            ),
            (
                false,
                frame.method_actuals.as_slice(),
                slot.actuals.as_slice(),
            ),
        ] {
            for (a, b) in local.iter().zip(context) {
                crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
                self.pay(1)?;
                match (a, b) {
                    (DynGenericActual::Life(a), DynGenericActual::Life(b)) => {
                        bind(self, *a, *b, persistent)?
                    }
                    (DynGenericActual::QualifiedType(a), DynGenericActual::QualifiedType(b))
                        if a.ty == b.ty
                            && a.life == b.life
                            && a.classes.len() == b.classes.len()
                            && a.classes.len() == a.life.binder_count as usize =>
                    {
                        for (a, b) in a.classes.iter().zip(&b.classes) {
                            bind(self, *a, *b, persistent)?;
                        }
                    }
                    (DynGenericActual::Type(a), DynGenericActual::Type(b)) if a == b => {}
                    (DynGenericActual::Const(a), DynGenericActual::Const(b)) if a == b => {}
                    _ => return Err(bad("Dyn provider header changes a complete mixed actual")),
                }
            }
        }
        if frame.receiver.life.binder_count as usize != frame.receiver.classes.len()
            || frame.receiver.classes.len() != table.self_classes.len()
        {
            return Err(bad(
                "Dyn provider header truncates the complete original Self",
            ));
        }
        for (a, b) in frame.receiver.classes.iter().zip(&table.self_classes) {
            bind(self, *a, *b, true)?;
        }
        drop(bind);
        let outer_count = source
            .classes
            .iter()
            .filter(|row| {
                !(row.position.base == StorageBase::Parameter(0)
                    && row.position.path == [LifeTypeProjection::Pointee])
            })
            .count();
        if target.bindings.len() != source.bindings.len()
            || target.classes.len()
                != outer_count.saturating_add(interface.retained_life.binder_count as usize)
            || target.static_bindings != source.static_bindings
        {
            return Err(bad(
                "Dyn contextual slot changes complete original header occurrences",
            ));
        }
        for class in 0..interface.retained_life.binder_count {
            let position = SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class,
            };
            self.pay(target.classes.iter().fold(0usize, |cost, row| {
                cost.saturating_add(
                    row.position
                        .path
                        .len()
                        .saturating_add(position.path.len())
                        .saturating_add(1),
                )
            }))?;
            let mut rows = target.classes.iter().filter(|row| row.position == position);
            let row = rows
                .next()
                .ok_or_else(|| bad("Dyn header omits a full Context class"))?;
            if rows.next().is_some() || row.binder != StorageBinder::Fixed(class) {
                return Err(bad("Dyn header changes a complete Context class identity"));
            }
        }
        let mut invocation = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        for outputs in [false, true] {
            for (position, binder) in source
                .bindings
                .iter()
                .map(|row| (&row.position, row.binder))
                .map(|(p, b)| (EitherHeader::Borrow(p), b))
                .chain(
                    source
                        .classes
                        .iter()
                        .filter(|row| {
                            !(row.position.base == StorageBase::Parameter(0)
                                && row.position.path == [LifeTypeProjection::Pointee])
                        })
                        .map(|row| (EitherHeader::Class(&row.position), row.binder)),
                )
            {
                let base = match position {
                    EitherHeader::Borrow(p) => p.base,
                    EitherHeader::Class(p) => p.base,
                };
                if (base == StorageBase::Result) != outputs {
                    continue;
                }
                if !matches!(base, StorageBase::Parameter(_) | StorageBase::Result) {
                    return Err(bad("Dyn method header invents a captured scope"));
                }
                self.pay(frame.header_embedding.len())?;
                let mut images = frame
                    .header_embedding
                    .iter()
                    .filter(|row| row.from == binder);
                let local = images
                    .next()
                    .ok_or_else(|| bad("Dyn provider header omits its real lexical image"))?
                    .to;
                if images.next().is_some() {
                    return Err(bad("Dyn provider header repeats its lexical image"));
                }
                let image = match position {
                    EitherHeader::Borrow(p) => {
                        self.pay(target.bindings.iter().fold(0usize, |cost, row| {
                            cost.saturating_add(
                                row.position
                                    .path
                                    .len()
                                    .saturating_add(p.path.len())
                                    .saturating_add(1),
                            )
                        }))?;
                        let mut rows = target.bindings.iter().filter(|row| row.position == *p);
                        let image = rows
                            .next()
                            .ok_or_else(|| bad("Dyn slot omits an original borrow occurrence"))?
                            .binder;
                        if rows.next().is_some() {
                            return Err(bad("Dyn slot repeats an original borrow occurrence"));
                        }
                        image
                    }
                    EitherHeader::Class(p) => {
                        self.pay(target.classes.iter().fold(0usize, |cost, row| {
                            cost.saturating_add(
                                row.position
                                    .path
                                    .len()
                                    .saturating_add(p.path.len())
                                    .saturating_add(1),
                            )
                        }))?;
                        let mut rows = target.classes.iter().filter(|row| row.position == *p);
                        let image = rows
                            .next()
                            .ok_or_else(|| bad("Dyn slot omits an original intrinsic occurrence"))?
                            .binder;
                        if rows.next().is_some() {
                            return Err(bad("Dyn slot repeats an original intrinsic occurrence"));
                        }
                        image
                    }
                };
                if matches!(image,StorageBinder::Fixed(i)if i>=target.fixed_binder_count)
                    || matches!(image,StorageBinder::Invocation(i)if i>=target.invocation_binder_count)
                {
                    return Err(bad("Dyn slot binder leaves its complete original domain"));
                }
                match local {
                    StorageBinder::Fixed(_) => {
                        self.pay(fixed.len().saturating_add(1))?;
                        if fixed.get(&local) != Some(&image) {
                            return Err(bad("Dyn slot changes an original Fixed class image"));
                        }
                    }
                    StorageBinder::Invocation(_) => {
                        self.pay(invocation.len().saturating_add(1))?;
                        if outputs {
                            if invocation.get(&local) != Some(&image) {
                                return Err(bad(
                                    "Dyn result has no complete original input invocation image",
                                ));
                            }
                        } else if invocation
                            .insert(local, image)
                            .is_some_and(|old| old != image)
                        {
                            return Err(bad("Dyn input invocation has contradictory images"));
                        }
                    }
                }
                if let StorageBinder::Invocation(i) = image {
                    self.pay(used.len().saturating_add(1))?;
                    used.insert(i);
                }
            }
        }
        if used.len() != target.invocation_binder_count as usize {
            return Err(bad("Dyn slot adds an ungrounded invocation class"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum EitherHeader<'a> {
    Borrow(&'a StoragePosition),
    Class(&'a SignatureLifeClass),
}
