//! Interpret a required callable signature in its own invocation namespace.
//! The resulting relation is local to this role: it never enters nominal
//! equality or changes the concrete closure's complete persistent domain.
use super::*;
use crate::semantic::value::{self, Class, Value};
use std::collections::BTreeMap;

impl Check<'_, '_> {
    fn signature_value(
        &self,
        context: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        invocations: u32,
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
                    _ => self.life(context, life, b, depth + 1)?,
                };
                let inner = self.signature_value(context, inner, invocations, b, depth + 1)?;
                value::address(*mutable, class, inner, b)
            }
            SemanticTypeRecipe::Slice(element) => {
                let element = self.signature_value(context, element, invocations, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "callable slice requires its complete element-domain embedding",
                    ));
                }
                Ok(value::scalar(Type::Slice {
                    element: Box::new(element.ty),
                }))
            }
            SemanticTypeRecipe::Tuple(fields) if fields.is_empty() => Ok(value::scalar(Type::Unit)),
            SemanticTypeRecipe::Tuple(fields) => {
                let mut types = Vec::new();
                for field in fields {
                    let field = self.signature_value(context, field, invocations, b, depth + 1)?;
                    if !field.classes.is_empty() {
                        return Err(unknown(
                            "callable tuple requires its complete child occurrence embedding",
                        ));
                    }
                    self.push_value(&mut types, field.ty, b)?;
                }
                Ok(value::scalar(Type::Tuple(types)))
            }
            // Complete formal/opaque/nominal values keep their intrinsic N,
            // BS wrapper and all original phantom occurrences. Invocation is
            // handled only by this signature interpreter, never caller.life.
            _ => self.recipe_value(context, recipe, b, depth + 1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn role_occurrence(
        &self,
        expected: &Class,
        actual: StorageBinder,
        subject: &Value,
        invocations: u32,
        relation: &mut BTreeMap<Class, StorageBinder>,
        reverse: &mut BTreeMap<StorageBinder, Class>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(relation.len().saturating_add(reverse.len() + 1), WORK)?;
        match actual {
            StorageBinder::Fixed(index) => {
                // A role cannot freshen or rename the closure's retained
                // class. Its complete subject supplies this exact image.
                if matches!(expected, Class::Role(..))
                    || subject.classes.get(index as usize) != Some(expected)
                {
                    return Err(unknown(
                        "callable role does not establish this persistent outer occurrence",
                    ));
                }
            }
            StorageBinder::Invocation(index) if index < invocations => {}
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

    fn callable_signature(
        &self,
        context: &SemanticInstance,
        subject: &Value,
        definition: &ClosureDefinition,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if signature.bound_lifetimes != 0 {
            return Err(unknown(
                "callable role requires its complete quantified skolem proof",
            ));
        }
        let actual = definition.storage_life.as_ref().ok_or_else(|| {
            unknown("callable role has no complete actual invocation and persistent inventory")
        })?;
        crate::signature_life::charge(actual, b)?;
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
        if entry.call_kind != signature.call_kind {
            return Err(fail(
                "actual callable changes its original fixed execution kind",
            ));
        }
        let actual_instance = self.function_instance(definition.function, b)?;
        let declaration = self.inventory.definition(actual_instance.definition, b)?;
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
            let value =
                self.signature_value(context, &parameter.ty, signature.invocation_lifetimes, b, 0)?;
            if parameter.mode == PassingMode::Copy
                && !original::nominal::predicate_actual(
                    &value.ty,
                    original::Predicate::Copy,
                    self.inventory,
                    b,
                    0,
                )?
            {
                return Err(fail(
                    "callable Copy parameter lacks its complete actual Copy proof",
                ));
            }
            self.push_value(&mut parameters, value, b)?;
        }
        let result = self.signature_value(
            context,
            &signature.result,
            signature.invocation_lifetimes,
            b,
            0,
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
                self.role_occurrence(
                    class,
                    binding.ok_or_else(|| fail("callable occurrence lost its actual binding"))?,
                    subject,
                    actual.invocation_binder_count,
                    &mut relation,
                    &mut reverse,
                    b,
                )?;
                used_bindings += 1;
            }
            let mut occurrences = Vec::new();
            if matches!(value.ty, Type::BorrowedStorage { .. } | Type::Closure(_)) {
                for (class, image) in value.classes.iter().enumerate() {
                    self.push_value(&mut occurrences, (Vec::new(), class as u32, image), b)?;
                }
            } else if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) {
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
                    image,
                    binder,
                    subject,
                    actual.invocation_binder_count,
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
            return Err(fail(
                "actual callable has an unproved original role occurrence",
            ));
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
        self.callable_signature(context, &subject, definition, signature, b)
    }
}
