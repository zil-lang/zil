//! Interpret a required callable signature in its own invocation namespace.
//! The resulting relation is local to this role: it never enters nominal
//! equality or changes the concrete closure's complete persistent domain.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};
use std::collections::BTreeMap;

impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn signature_value(
        &self,
        context: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        invocations: u32,
        bounds: &[u32],
        receiver: Option<&Class>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("callable role type exceeds its finite depth"));
        }
        match recipe {
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let class = match life {
                    SemanticLifeRecipe::Invocation(index) if *index < invocations => {
                        Class::Role(context.id, *index)
                    }
                    SemanticLifeRecipe::Invocation(_) => {
                        return Err(fail("callable role invocation exceeds its own signature"));
                    }
                    SemanticLifeRecipe::Bound { depth, index } => {
                        let level =
                            bounds
                                .len()
                                .checked_sub(*depth as usize + 1)
                                .ok_or_else(|| {
                                    fail("callable Bound leaves its protected quantifier scope")
                                })?;
                        if *index >= bounds[level] {
                            return Err(fail("callable Bound exceeds its own quantifier arity"));
                        }
                        b.spend(bounds.len(), WORK)?;
                        let base = bounds[..level]
                            .iter()
                            .try_fold(invocations, |offset, count| offset.checked_add(*count))
                            .and_then(|offset| offset.checked_add(*index))
                            .ok_or_else(|| fail("callable protected role namespace overflow"))?;
                        Class::Role(context.id, base)
                    }
                    SemanticLifeRecipe::OuterOccurrence {
                        operand: SemanticRecipeOperand::Receiver, path,
                        mode: StoredBorrowMode::SharedAddress,
                    } if path.is_empty() && !*mutable => receiver.cloned()
                        .ok_or_else(|| fail("Receiver occurrence leaves its callable result scope"))?,
                    _ => self.life(context, life, b, depth + 1)?,
                };
                let inner =
                    self.signature_value(context, inner, invocations, bounds, None, b, depth + 1)?;
                value::address(*mutable, class, inner, b)
            }
            SemanticTypeRecipe::Slice(element) => {
                let element =
                    self.signature_value(context, element, invocations, bounds, None, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "callable slice requires its complete element-domain embedding",
                    ));
                }
                Ok(value::scalar(Type::Slice {
                    element: Box::new(element.into_type(b)?),
                }))
            }
            SemanticTypeRecipe::Tuple(fields) if fields.is_empty() => Ok(value::scalar(Type::Unit)),
            SemanticTypeRecipe::Tuple(fields) => {
                let mut types = Vec::new();
                for field in fields {
                    let field =
                        self.signature_value(context, field, invocations, bounds, None, b, depth + 1)?;
                    if !field.classes.is_empty() {
                        return Err(unknown(
                            "callable tuple requires its complete child occurrence embedding",
                        ));
                    }
                    self.push_value(&mut types, field.into_type(b)?, b)?;
                }
                Ok(value::scalar(Type::Tuple(types)))
            }
            // Complete formal/opaque/nominal values keep their intrinsic N,
            // BS wrapper and all original phantom occurrences. Invocation is
            // handled only by this signature interpreter, never caller.life.
            _ => self.recipe_value(context, recipe, b, depth + 1),
        }
    }

    /// Relate a named life and the class of a rigid Type argument only at
    /// their real incoming calls. The declaration retains both coordinates.
    fn persistent_role_image(
        &self,
        context: &SemanticInstance,
        expected: &Class,
        actual: &Class,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        if self.capture_class_image(actual.clone(), b)?
            == self.capture_class_image(expected.clone(), b)?
        {
            return Ok(true);
        }
        let (Class::Formal(life_scope, life_ordinal), Class::TypeArgument(type_scope, type_ordinal, class))
            = (expected, actual) else { return Ok(false); };
        if *life_scope != context.id || *type_scope != context.id || context.parent.is_some() {
            return Ok(false);
        }
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(entry)) = context.target
        else { return Ok(false); };
        let mut proved = false;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if !matches!(site.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. }) {
                continue;
            }
            let SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After }
                = site.at else { continue; };
            let caller = self.inventory.instance(site.instance, b)?;
            let SemanticInstanceTarget::Function(target) = &caller.target else {
                return Err(fail("callable persistent role receipt has no actual caller"));
            };
            let function = self.inventory.function(target, b)?;
            let operation = function.blocks.get(block.0 as usize)
                .filter(|row| row.id == block)
                .and_then(|row| row.instructions.get(instruction as usize))
                .ok_or_else(|| fail("callable persistent role receipt leaves the actual CFG"))?;
            if !matches!(operation.kind,
                InstructionKind::Call { function: target, .. }
                | InstructionKind::SuspendCall { function: target, .. } if target == entry)
            {
                continue;
            }
            let (target, arguments) = self.original.call_arguments(
                caller, site.recipe, self.program, self.inventory, b,
            )?;
            if target != context.definition {
                return Err(fail("callable persistent role changes its original callee"));
            }
            self.operation_mixed_arguments(context, &arguments, b)?;
            let (Some(Argument::Life(life)), Some(Argument::Type(value))) = (
                arguments.get(*life_ordinal as usize), arguments.get(*type_ordinal as usize),
            ) else {
                return Err(fail("callable persistent role changes a full mixed argument kind"));
            };
            let actual = value.classes.get(*class as usize)
                .ok_or_else(|| fail("callable persistent role omits its actual intrinsic argument class"))?;
            if self.capture_class_image(actual.clone(), b)?
                != self.capture_class_image(life.clone(), b)?
            {
                return Ok(false);
            }
            proved = true;
        }
        Ok(proved)
    }

    #[allow(clippy::too_many_arguments)]
    fn role_occurrence(
        &self,
        context: &SemanticInstance,
        expected: &Class,
        actual: StorageBinder,
        subject: &Value,
        invocations: u32,
        allow_new_invocation: bool,
        relation: &mut BTreeMap<Class, StorageBinder>,
        reverse: &mut BTreeMap<StorageBinder, Class>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(relation.len().saturating_add(reverse.len() + 1), WORK)?;
        match actual {
            StorageBinder::Fixed(index) => {
                // A role cannot freshen or rename the closure's retained
                // class. Its complete subject supplies this exact image.
                let actual = subject.classes.get(index as usize)
                    .ok_or_else(|| fail("callable role leaves its complete Fixed domain"))?;
                if matches!(expected, Class::Role(..))
                    || !self.persistent_role_image(context, expected, actual, b)?
                {
                    b.spend(128, WORK)?;
                    let mut errors = unknown("callable role does not establish this persistent outer occurrence");
                    errors[0].notes.push(format!("original expected class {expected:?}, Fixed({index}) image {actual:?}"));
                    return Err(errors);
                }
                let binder = StorageBinder::Fixed(index);
                if matches!(relation.get(expected), Some(StorageBinder::Invocation(_)))
                    || reverse.get(&binder).is_some_and(|prior| prior != expected)
                {
                    return Err(fail("callable role changes an independent original lifetime relation"));
                }
                // Equal outside images can satisfy distinct Fixed positions.
                // Record their namespace without using one binder as proof of
                // another or permitting an Invocation to occupy this role.
                relation.entry(expected.clone()).or_insert(binder);
                reverse.insert(binder, expected.clone());
                return Ok(());
            }
            StorageBinder::Invocation(index) if index < invocations => {
                if !allow_new_invocation && !relation.contains_key(expected) {
                    return Err(unknown(
                        "callable result lifetime has no independently grounded input role",
                    ));
                }
            }
            _ => return Err(fail("callable role binder exceeds the actual entry domain")),
        }
        if relation.get(expected).is_some_and(|old| *old != actual)
            || reverse.get(&actual).is_some_and(|old| old != expected)
        {
            return Err(fail(
                "callable role changes an independent original lifetime relation",
            ));
        }
        relation.insert(expected.clone(), actual);
        reverse.insert(actual, expected.clone());
        Ok(())
    }

    pub(super) fn callable_signature(
        &self,
        context: &SemanticInstance,
        subject: &Value,
        definition: &ClosureDefinition,
        signature: &SemanticSignatureRecipe,
        kind: CallableKind,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.quantified_callable_signature(context, subject, definition, signature, kind, &[], b)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn quantified_callable_signature(
        &self,
        context: &SemanticInstance,
        subject: &Value,
        definition: &ClosureDefinition,
        signature: &SemanticSignatureRecipe,
        kind: CallableKind,
        enclosing_bounds: &[u32],
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(enclosing_bounds.len(), WORK)?;
        let mut bounds = enclosing_bounds.to_vec();
        if signature.bound_lifetimes != 0 {
            self.push_value(&mut bounds, signature.bound_lifetimes, b)?;
        }
        let actual = definition.storage_life.as_ref().ok_or_else(|| {
            unknown("callable role has no complete actual invocation and persistent inventory")
        })?;
        crate::signature_life::charge(actual, b)?;
        b.spend(actual.classes.len(), WORK)?;
        if actual.classes.iter().any(|row| row.position.base == StorageBase::Receiver) {
            return Err(fail("callable Receiver is not a persistent semantic class directory"));
        }
        if actual.fixed_binder_count as usize != subject.classes.len()
            || signature.parameters.len() != definition.parameters.len()
        {
            return Err(fail(
                "callable role loses the actual full domain or parameter arity",
            ));
        }
        let entry = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(definition.function), b)?;
        if !callable_entry_matches(kind, entry.call_kind, signature.call_kind) {
            return Err(fail(
                "actual callable changes its original fixed execution kind",
            ));
        }
        let declaration = self
            .inventory
            .definition(self.function_definition(definition.function, b)?, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature: original,
            ..
        } = &declaration.body
        else {
            return Err(fail(
                "actual callable entry omits its original full declaration",
            ));
        };
        if original.parameters.len() != signature.parameters.len()
            || signature.result_mode != original.result_mode
            || entry.result_mode != original.result_mode
        {
            return Err(unknown(
                "callable result and parameter modes need their complete original relation",
            ));
        }
        // This theorem is reproved from the actual closure relation, not from
        // a same-shaped entry or the proposed role's numeric binder labels.
        crate::stored_callables::check_role_relation(self.module, definition, actual, b)?;
        let mut parameters = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if parameter.mode != original.parameters[index].mode
                || !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy)
            {
                return Err(unknown(
                    "callable role requires its complete resource passing-mode proof",
                ));
            }
            let value = self.signature_value(
                context,
                &parameter.ty,
                signature.invocation_lifetimes,
                &bounds,
                None,
                b,
                0,
            )?;
            if parameter.mode == PassingMode::Copy
                && !self.structural_actual(&value.ty, original::Predicate::Copy, b, 0)?
            {
                return Err(fail(
                    "callable Copy parameter lacks its complete actual Copy proof",
                ));
            }
            self.push_value(&mut parameters, value, b)?;
        }
        let receiver = if original::receiver_result(signature, kind) {
            let ordinal = bounds.iter().try_fold(signature.invocation_lifetimes,
                |count, bound| count.checked_add(*bound))
                .ok_or_else(|| fail("callable Receiver role namespace overflow"))?;
            Some(Class::Role(context.id, ordinal))
        } else { None };
        if receiver.is_some() {
            self.original.receiver_sources(declaration.id, self.inventory, b)?
                .ok_or_else(|| fail("callable Receiver role has no complete original captured returns"))?;
        }
        let result = self.signature_value(
            context, &signature.result, signature.invocation_lifetimes,
            &bounds, receiver.as_ref(), b, 0,
        )?;
        let mut relation = BTreeMap::new();
        let mut reverse = BTreeMap::new();
        let mut used_bindings = 0usize;
        let mut used_classes = 0usize;
        let mut used_statics = 0usize;
        for (base, value, actual_type) in parameters
            .iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    StorageBase::Parameter(index as u32),
                    value,
                    &definition.parameters[index],
                )
            })
            .chain(std::iter::once((
                StorageBase::Result,
                &result,
                &definition.result,
            )))
        {
            if &value.ty != actual_type {
                return Err(fail(
                    "actual callable parameter or result changes the complete original type",
                ));
            }
            if base == StorageBase::Result && original::strings::is_byte_type(&value.ty)
                && empty(&value.life) && value.classes.is_empty()
            {
                // A complete Static result has no borrowed class slot to
                // enumerate. Prove its actual literal returns independently;
                // capture Static occurrences belong to the separate directory.
                self.static_byte_returns(entry, &value.ty, b)?;
                used_statics += 1;
            }
            for slot in &value.life.slots {
                let class = value
                    .classes
                    .get(slot.binder as usize)
                    .ok_or_else(|| fail("callable signature slot omits an original class"))?;
                let position = StoragePosition {
                    base,
                    path: slot
                        .path
                        .iter()
                        .copied()
                        .map(ContractProjection::Place)
                        .collect(),
                };
                b.spend(
                    position
                        .path
                        .len()
                        .saturating_add(actual.bindings.len() + actual.static_bindings.len()),
                    WORK,
                )?;
                let mut binding = None;
                let mut binding_count = 0usize;
                for row in &actual.bindings {
                    if row.position == position {
                        binding = Some(row.binder);
                        binding_count += 1;
                    }
                }
                let static_count = actual
                    .static_bindings
                    .iter()
                    .filter(|row| **row == position)
                    .count();
                if binding_count == 0
                    && static_count == 1
                    && base == StorageBase::Result
                    && slot.mode == StoredBorrowMode::SharedAddress
                {
                    // The actual static occurrence still owes the module's
                    // all-Ctor/Write/Return literal-root theorem. A role never
                    // turns its category upper into a literal fact.
                    used_statics += 1;
                    continue;
                }
                if binding_count != 1 || static_count != 0 {
                    return Err(fail(
                        "actual callable omits or duplicates an original borrow occurrence",
                    ));
                }
                let binding =
                    binding.ok_or_else(|| fail("callable occurrence lost its actual binding"))?;
                b.spend(actual.bindings.len(), WORK)?;
                let receiver_window = base == StorageBase::Result && slot.path.is_empty()
                    && receiver.as_ref() == Some(class);
                let receiver_input = receiver_window
                    && matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                    && actual.bindings.iter().any(|row| {
                        row.position.base == StorageBase::Receiver
                            && row.position.path.is_empty()
                            && row.binder == binding
                    });
                if receiver_window && matches!(binding, StorageBinder::Fixed(index)
                    if subject.classes.get(index as usize).is_some()) {
                    // The complete Fixed source survives; only its exposed
                    // result view is bounded by this signature's access window.
                    used_bindings += 1;
                    continue;
                }
                self.role_occurrence(
                    context,
                    class,
                    binding,
                    subject,
                    actual.invocation_binder_count,
                    base != StorageBase::Result || receiver_input,
                    &mut relation,
                    &mut reverse,
                    b,
                )?;
                used_bindings += 1;
            }
            // A Task's physical retained ports select its preserved opaque
            // classes. They are not ordinary inline address slots, and their
            // absence does not make the full logical domain empty.
            for (path, class) in self.task_ports(value, b)? {
                let image = value.classes.get(class as usize)
                    .ok_or_else(|| fail("callable Task port leaves its complete original domain"))?;
                let position = StoragePosition { base, path };
                let mut selected = None;
                for row in &actual.bindings {
                    b.spend(row.position.path.len().saturating_add(position.path.len() + 1), WORK)?;
                    if row.position == position && selected.replace(row.binder).is_some() {
                        return Err(fail("actual callable repeats an original Task retained port"));
                    }
                }
                b.spend(actual.static_bindings.len().saturating_mul(position.path.len() + 1), WORK)?;
                if actual.static_bindings.contains(&position) {
                    return Err(fail("actual callable replaces a Task retained port with a static category"));
                }
                let binder = selected.ok_or_else(||
                    fail("actual callable omits an original Task retained port"))?;
                b.spend(actual.bindings.len(), WORK)?;
                let receiver_input = matches!(image, Class::Role(_, index) if *index < signature.invocation_lifetimes)
                    && matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                    && actual.bindings.iter().any(|row| row.position.base == StorageBase::Receiver
                        && row.position.path.is_empty() && row.binder == binder);
                self.role_occurrence(context, image, binder, subject, actual.invocation_binder_count,
                    base != StorageBase::Result || receiver_input, &mut relation, &mut reverse, b)?;
                used_bindings += 1;
            }
            let mut occurrences = Vec::new();
            let task_domain = match &value.ty {
                Type::Task { .. } | Type::Managed(_) | Type::ErrorUnion { .. } =>
                    self.task_domain(&value.ty, b)?,
                _ => None,
            };
            if task_domain.as_ref().is_some_and(|domain| domain != &value.life) {
                return Err(fail("callable Task changes its complete original occurrence forest"));
            }
            if matches!(value.ty, Type::BorrowedStorage { .. } | Type::Closure(_))
                || task_domain.is_some() && value.life.retained.is_empty()
            {
                for (class, image) in value.classes.iter().enumerate() {
                    self.push_value(&mut occurrences, (Vec::new(), class as u32, image), b)?;
                }
            } else if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) || task_domain.is_some() {
                for row in &value.life.retained {
                    b.spend(row.path.len().saturating_add(1), WORK)?;
                    let image = value
                        .classes
                        .get(row.binder as usize)
                        .ok_or_else(|| fail("callable child occurrence omits its whole class"))?;
                    self.push_value(&mut occurrences, (row.path.clone(), row.class, image), b)?;
                }
            }
            for (path, class, image) in occurrences {
                b.spend(actual.classes.len().saturating_add(path.len()), WORK)?;
                let mut selected = None;
                for row in &actual.classes {
                    if row.position.base == base
                        && row.position.path == path
                        && row.position.class == class
                        && selected.replace(row.binder).is_some()
                    {
                        return Err(fail(
                            "actual callable repeats an intrinsic class occurrence",
                        ));
                    }
                }
                let binder = selected.ok_or_else(|| {
                    fail("actual callable omits a complete intrinsic class occurrence")
                })?;
                self.role_occurrence(
                    context,
                    image,
                    binder,
                    subject,
                    actual.invocation_binder_count,
                    base != StorageBase::Result,
                    &mut relation,
                    &mut reverse,
                    b,
                )?;
                used_classes += 1;
            }
        }
        for row in &actual.bindings {
            b.spend(row.position.path.len().saturating_add(1), WORK)?;
            if matches!(
                row.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                continue;
            }
            // Capture/Receiver obligations remain in the actual complete
            // inventory and are checked by the environment/entry theorem.
            used_bindings += 1;
        }
        for row in &actual.classes {
            b.spend(row.position.path.len().saturating_add(1), WORK)?;
            if !matches!(
                row.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                used_classes += 1;
            }
        }
        for position in &actual.static_bindings {
            b.spend(position.path.len().saturating_add(1), WORK)?;
            if !matches!(
                position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                used_statics += 1;
            }
        }
        if used_bindings != actual.bindings.len()
            || used_classes != actual.classes.len()
            || used_statics != actual.static_bindings.len()
        {
            b.spend(actual.bindings.len().saturating_add(actual.classes.len())
                .saturating_add(actual.static_bindings.len()).saturating_add(result.classes.len())
                .saturating_add(1).saturating_mul(128), WORK)?;
            let mut errors = fail(
                "actual callable has an unproved original role occurrence",
            );
            errors[0].notes.push(format!(
                "original role context {}:{} definition {}:{}, actual closure {} entry {}; used bindings/classes/statics {}/{}/{}, actual {}/{}/{}",
                context.id.module.0, context.id.index, context.definition.module.0, context.definition.index,
                definition.id.0, definition.function.0, used_bindings, used_classes, used_statics,
                actual.bindings.len(), actual.classes.len(), actual.static_bindings.len()));
            errors[0].notes.push(format!("complete original result classes {:?}; actual role header {:?}", result.classes, actual));
            return Err(errors);
        }
        Ok(())
    }

    pub(super) fn callable_requirement(
        &self,
        context: &SemanticInstance,
        ty: &SemanticTypeRecipe,
        kind: CallableKind,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.quantified_callable_requirement(context, ty, kind, signature, &[], b)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn quantified_callable_requirement(
        &self,
        context: &SemanticInstance,
        ty: &SemanticTypeRecipe,
        kind: CallableKind,
        signature: &SemanticSignatureRecipe,
        bounds: &[u32],
        b: &mut Budget,
    ) -> CompileResult<()> {
        let subject = self.recipe_value(context, ty, b, 0)?;
        let Type::Closure(id) = subject.ty else {
            return Err(unknown(
                "callable premise needs its complete actual callable inventory",
            ));
        };
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let definition = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .ok_or_else(|| fail("actual callable has another environment identity"))?;
        let allowed = match kind {
            CallableKind::FunctionPointer => false,
            CallableKind::Fn => definition.kind == CallableKind::Fn,
            CallableKind::FnMut => {
                matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut)
            }
            CallableKind::FnOnce => matches!(
                definition.kind,
                CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
            ),
        };
        if !allowed {
            return Err(fail(
                "actual callable does not support its original requested mode",
            ));
        }
        self.capture_directory(id, b)?;
        self.quantified_callable_signature(
            context, &subject, definition, signature, kind, bounds, b,
        )
    }
}
