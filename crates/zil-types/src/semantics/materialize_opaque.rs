//! Explicit original-parameter result boundaries. The original caller supplies
//! the actual recipes; the actual emitted call selects its real callee entry.
//! Callee-local regions are deliberately not transported by this pass.
use super::*;

#[derive(PartialEq, Eq)]
struct ClosedContext {
    definition: SemanticDefinitionId,
    arguments: Vec<SemanticActual>,
    environment: Vec<SemanticActual>,
}

impl Eval<'_, '_, '_> {
    /// Evidence for an already rejected boundary, never a source of Type or
    /// runtime facts. Charge every complete recipe before allocating Debug.
    pub(super) fn opaque_boundary_evidence(
        &mut self, caller: SemanticInstanceId, recipe: &SemanticTypeRecipe,
        mut diagnostic: Diagnostic, depth: usize,
    ) -> R<Diagnostic> {
        self.node(depth)?;
        let SemanticTypeRecipe::Opaque { definition, .. } = recipe else {
            return Ok(diagnostic);
        };
        crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
        diagnostic.notes.push(format!("rejected original opaque application in {caller:?}: {recipe:?}"));
        self.fee(self.input.instances.len())?;
        let ledger = self.ledger;
        for original in &self.input.instances {
            if original.definition != *definition { continue; }
            let actual = self.instance(original.id)?;
            self.fee(actual.actuals.len().saturating_add(8))?;
            for argument in &actual.actuals {
                crate::dyn_metadata::semantic_operand_snapshot(argument, self.limits, self.left, 1)?;
            }
            diagnostic.notes.push(format!(
                "opaque defining candidate {:?}: definition {:?}, parent {:?}, target {:?}, lifetime {:?}, actuals {:?}",
                actual.id, actual.definition, actual.parent, actual.target,
                actual.lifetime_instantiation, actual.actuals,
            ));
            if let Some(context) = ledger.original.context() {
                match context.instance(actual.id, self.left) {
                    Ok(source) => {
                        self.fee(source.actuals.len().saturating_add(4))?;
                        for argument in &source.actuals {
                            crate::dyn_metadata::semantic_operand_snapshot(argument, self.limits, self.left, 1)?;
                        }
                        diagnostic.notes.push(format!(
                            "immutable candidate source {:?}: target {:?}, actuals {:?}",
                            source.id, source.target, source.actuals,
                        ));
                    }
                    Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => return Err(error),
                    Err(error) => diagnostic.notes.push(format!(
                        "immutable candidate source {:?} unavailable: {}", actual.id, error.message,
                    )),
                }
            }
            let request = {
                let requests = ledger.instance_requests.borrow();
                self.fee(requests.len().saturating_add(1).ilog2() as usize + 1)?;
                requests.get(&actual.id).copied()
            };
            diagnostic.notes.push(format!("exact emitted candidate request {:?}: {request:?}", actual.id));
            if let Some((source, call, _)) = request {
                let owner = self.instance(source)?.definition;
                let proof = ledger.original.borrow();
                let proof = proof.as_ref().ok_or_else(|| bad("opaque boundary evidence has no original proof"))?;
                for ordinal in 0..actual.actuals.len() {
                    match proof.call_actual(owner, call, ordinal as u32, self.limits, self.left) {
                        Ok(argument) => {
                            crate::dyn_metadata::semantic_operand_snapshot(&argument, self.limits, self.left, 1)?;
                            diagnostic.notes.push(format!(
                                "candidate original actual {owner:?}/{call:?}/full ordinal {ordinal}: {argument:?}",
                            ));
                        }
                        Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => return Err(error),
                        Err(error) => diagnostic.notes.push(format!(
                            "candidate original actual {owner:?}/{call:?}/full ordinal {ordinal} unavailable: {}", error.message,
                        )),
                    }
                }
            }
        }
        Ok(diagnostic)
    }
    /// An independently proved closed original Type application is separate
    /// from runtime value provenance. Interpret every exact defining world;
    /// the physical hint never chooses a creator or an actual argument.
    pub(super) fn closed_opaque_definition(
        &mut self,
        caller: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        self.node(depth)?;
        let owner = self.instance(caller)?.definition;
        let Some((definition, result_recipe, generic)) = ({
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("closed opaque lacks its original all-return proof"))?;
            if let Some((definition, result)) = proof.closed_opaque_result(
                self.input, recipe, self.limits, self.left,
            )? { Some((definition, result, false)) }
            else { proof.closed_opaque_application(self.input, owner, recipe, self.limits, self.left)?
                .map(|(definition, result)| (definition, result, true)) }
        }) else {
            return Ok(None);
        };
        let mut expected_actuals = Vec::new();
        let application_arguments = match recipe {
            SemanticTypeRecipe::Opaque { arguments, .. } => arguments.as_slice(),
            _ => unreachable!(),
        };
        if generic {
            let arguments = application_arguments;
            self.fee(arguments.len().saturating_mul(
                std::mem::size_of::<SemanticActual>().saturating_add(1),
            ))?;
            expected_actuals = Vec::with_capacity(arguments.len());
            for argument in arguments {
                expected_actuals.push(self.recipe_actual(caller, argument, depth + 1)?);
            }
        }
        self.fee(self.input.instances.len())?;
        let mut result: Option<(SemanticTypeValue, ClosedContext)> = None;
        for original in &self.input.instances {
            let instance = self.instance(original.id)?;
            if instance.definition != definition {
                continue;
            }
            if !matches!(
                instance.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))
            ) {
                continue;
            }
            if generic {
                if !self.closed_application_source_matches(
                    caller, instance.id, application_arguments, depth + 1,
                )? { continue; }
                if !self.closed_application_actuals_match(instance, &expected_actuals, depth + 1)? {
                    continue;
                }
            } else if !instance.actuals.is_empty() {
                return Err(bad("closed opaque defining world adds original mixed actuals"));
            }
            if instance.parent.is_some()
                || instance.environment.is_some()
                || instance.self_type.is_some()
                || instance.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
            {
                return Err(bad(
                    "closed opaque defining world changes its complete original lexical context",
                ));
            }
            let value = self.ty(instance.id, &result_recipe, None, depth + 1)?;
            let context =
                self.closed_result_context(instance.id, &result_recipe, &value, depth + 1)?;
            if let Some((previous, previous_context)) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(
                    previous,
                    self.limits,
                    self.left,
                    1,
                )?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                self.charge_closed_context(Some(previous_context))?;
                self.charge_closed_context(Some(&context))?;
                if previous != &value || previous_context != &context {
                    return Err(bad(
                        "closed opaque defining worlds disagree in complete result type or original environment",
                    ));
                }
            } else {
                result = Some((value, context));
            }
        }
        let Some((value, _)) = result else {
            return Ok(None);
        };
        if let Some(hint) = hint {
            let SemanticTypeValue::Resolved { ty, .. } = &value else {
                return Err(bad("closed opaque result remains unresolved"));
            };
            inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            if ty != hint {
                return Err(bad(
                    "closed opaque original defining type differs from actual emitted type",
                ));
            }
        }
        Ok(Some(value))
    }
    fn closed_application_actuals_match(
        &mut self, candidate: &SemanticInstance, expected: &[SemanticActual], depth: usize,
    ) -> R<bool> {
        self.node(depth)?;
        self.fee(candidate.actuals.len().saturating_add(expected.len()))?;
        if candidate.actuals.len() != expected.len() { return Ok(false); }
        for (ordinal, (claim, expected)) in candidate.actuals.iter().zip(expected).enumerate() {
            // CoreInferred is resolved through its own genuine original
            // scope/call/formal receipt. There is no invented current callsite.
            let actual = self.holder_actual(candidate.id, ordinal as u32, claim, depth + 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(expected, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
            if &actual != expected { return Ok(false); }
        }
        Ok(true)
    }
    fn opaque_actuals_match(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        candidate: &SemanticInstance,
        expected: &[SemanticActual],
        depth: usize,
    ) -> R<bool> {
        self.fee(candidate.actuals.len().saturating_add(expected.len()))?;
        if candidate.actuals.len() != expected.len() {
            return Ok(false);
        }
        for (ordinal, (claim, expected)) in candidate.actuals.iter().zip(expected).enumerate() {
            if let SemanticActual::CoreInferred {
                scope,
                call: actual,
                formal,
            } = claim
                && (*scope != caller || *actual != call || *formal != ordinal as u32)
            {
                return Ok(false);
            }
            let actual = self.holder_actual(candidate.id, ordinal as u32, claim, depth + 1)?;
            match (expected, actual) {
                (SemanticActual::Life(expected), SemanticActual::Life(actual)) => {
                    if matches!(claim, SemanticActual::CoreInferred { .. }) {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            expected,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &actual,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if expected != &actual {
                            return Ok(false);
                        }
                    } else {
                        if candidate.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
                        {
                            return Err(unknown(
                                "opaque result requires its actual rigid callee Life domain",
                            ));
                        }
                        let canonical = self.canonical_formal(
                            candidate.id,
                            &SemanticFormalRef {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: ordinal as u32,
                            },
                        )?;
                        if actual != canonical {
                            return Ok(false);
                        }
                    }
                }
                (
                    SemanticActual::Type(SemanticTypeValue::Resolved {
                        ty: expected,
                        life: expected_life,
                        classes: expected_classes,
                    }),
                    SemanticActual::Type(SemanticTypeValue::Resolved {
                        ty: actual,
                        life: actual_life,
                        classes: actual_classes,
                    }),
                ) => {
                    inference::charge_type_snapshot(
                        expected,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    inference::charge_type_snapshot(
                        &actual,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let _ = self.copy_shape(expected_life)?;
                    let _ = self.copy_shape(&actual_life)?;
                    if expected != &actual
                        || expected_life != &actual_life
                        || expected_classes.len() != expected_life.binder_count as usize
                        || actual_classes.len() != actual_life.binder_count as usize
                    {
                        return Ok(false);
                    }
                    self.fee(actual_classes.len())?;
                    for (class, image) in actual_classes.iter().enumerate() {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            image,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        let canonical = SemanticClassExpr::IntrinsicClass {
                            operand: SemanticOperand::TypeArgument {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: ordinal as u32,
                            },
                            path: Vec::new(),
                            class: class as u32,
                        };
                        let expected = &expected_classes[class];
                        crate::dyn_metadata::semantic_operand_snapshot(
                            expected,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if image != &canonical && image != expected {
                            return Ok(false);
                        }
                    }
                }
                (SemanticActual::Const { .. }, actual @ SemanticActual::Const { .. }) => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        expected,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if expected != &actual {
                        return Ok(false);
                    }
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
    fn explicit_result_images(
        &mut self,
        value: &mut SemanticTypeValue,
        actuals: &[SemanticActual],
        caller: SemanticInstanceId,
        candidate: &SemanticInstance,
        boundary: &super::super::original::CalleeResultBoundary,
        output: ValueId,
        depth: usize,
    ) -> R<()> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(bad("callee result retains an unresolved whole type"));
        };
        self.fee(classes.len())?;
        if classes.len() != life.binder_count as usize {
            return Err(bad("callee result omits an intrinsic class"));
        }
        let mut input_images = BTreeMap::new();
        let function = self.function(candidate.id)?;
        let needs_boundary = classes.iter().any(|image| match image {
            SemanticClassExpr::Formal {
                scope: SemanticScopeRef::Ancestor(0),
                ..
            } => false,
            SemanticClassExpr::IntrinsicClass {
                operand:
                    SemanticOperand::TypeArgument {
                        scope: SemanticScopeRef::Ancestor(0),
                        ..
                    },
                path,
                ..
            } if path.is_empty() => false,
            _ => true,
        });
        if needs_boundary && let Some(header) = &function.storage_life {
            if function.parameters.len() != boundary.parameters.len() {
                return Err(bad(
                    "opaque result changes its complete original input arity",
                ));
            }
            let mut bindings = Vec::new();
            let mut intrinsic = Vec::new();
            let mut statics = Vec::new();
            for (index, parameter) in boundary.parameters.iter().enumerate() {
                let local = function
                    .locals
                    .get(function.parameters[index].0 as usize)
                    .ok_or_else(|| bad("opaque result input has no actual local"))?;
                let input = match self.captured_call_type(
                    caller,
                    boundary.call,
                    &parameter.ty,
                    &local.ty,
                    depth + 1,
                )? {
                    Some(value) => value,
                    None => self.ty(caller, &parameter.ty, Some(&local.ty), depth + 1)?,
                };
                super::super::creator_headers::rows::append(
                    &input,
                    StorageBase::Parameter(index as u32),
                    &mut bindings,
                    &mut intrinsic,
                    &mut statics,
                    self.limits,
                    self.left,
                )?;
            }
            for row in &header.bindings {
                if !matches!(row.position.base, StorageBase::Parameter(_)) {
                    continue;
                }
                self.fee(bindings.len())?;
                let mut selected = bindings
                    .iter()
                    .filter(|(position, _)| position == &row.position);
                let (_, image) = selected
                    .next()
                    .ok_or_else(|| bad("opaque input header adds a physical occurrence"))?;
                if selected.next().is_some() {
                    return Err(bad("opaque input physical occurrence is ambiguous"));
                }
                self.insert_result_input(&mut input_images, row.binder, image)?;
            }
            for row in &header.classes {
                if !matches!(row.position.base, StorageBase::Parameter(_)) {
                    continue;
                }
                self.fee(intrinsic.len())?;
                let mut selected = intrinsic
                    .iter()
                    .filter(|(position, _)| position == &row.position);
                let (_, image) = selected
                    .next()
                    .ok_or_else(|| bad("opaque input header adds an intrinsic occurrence"))?;
                if selected.next().is_some() {
                    return Err(bad("opaque input intrinsic occurrence is ambiguous"));
                }
                self.insert_result_input(&mut input_images, row.binder, image)?;
            }
        }
        for (class_index, image) in classes.iter_mut().enumerate() {
            let local_image;
            let selected = match image {
                SemanticClassExpr::Formal {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                } => {
                    let Some(SemanticActual::Life(value)) = actuals.get(*ordinal as usize) else {
                        return Err(bad("callee result Life selects another mixed actual kind"));
                    };
                    value
                }
                SemanticClassExpr::IntrinsicClass {
                    operand:
                        SemanticOperand::TypeArgument {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal,
                        },
                    path,
                    class,
                } if path.is_empty() => {
                    let Some(SemanticActual::Type(SemanticTypeValue::Resolved {
                        life,
                        classes,
                        ..
                    })) = actuals.get(*ordinal as usize)
                    else {
                        return Err(bad("callee result class selects another mixed Type actual"));
                    };
                    if classes.len() != life.binder_count as usize {
                        return Err(bad("callee whole Type actual omits an intrinsic class"));
                    }
                    classes.get(*class as usize).ok_or_else(|| {
                        bad("callee result exceeds the full intrinsic Type domain")
                    })?
                }
                _ => {
                    // The actual result header connects this complete child
                    // class to a real input occurrence of this exact call.
                    // It supplies a proposal only: original headers and every
                    // actual call/return equation are checked independently.
                    let header = function.storage_life.as_ref().ok_or_else(|| {
                        unknown("opaque result requires its complete actual input/result header")
                    })?;
                    self.fee(header.classes.len())?;
                    let mut rows = header.classes.iter().filter(|row| {
                        row.position.base == StorageBase::Result
                            && row.position.path.is_empty()
                            && row.position.class as usize == class_index
                    });
                    let Some(row) = rows.next() else {
                        if candidate.environment.is_none()
                            && let Type::Closure(id) = stored::physical(ty)
                        {
                            self.pending_header = Some(*id);
                            return Err(Diagnostic::new(
                                Phase::Types,
                                zil_base::ResultCategory::Unsupported,
                                PENDING_HEADER,
                                "opaque result awaits its complete original creator input class relation",
                                None,
                            ));
                        }
                        return Err(unknown("opaque result has no complete original input class relation"));
                    };
                    if rows.next().is_some() {
                        return Err(bad("opaque result class relation is ambiguous"));
                    }
                    if let Some(input) = input_images.get(&row.binder) {
                        input
                    } else {
                        let StorageBinder::Invocation(ordinal) = row.binder else {
                            return Err(unknown(
                                "opaque captured Fixed result lacks its sealed input boundary",
                            ));
                        };
                        if ordinal >= header.invocation_binder_count
                            || !matches!(
                                &self.definition(candidate.definition)?.body,
                                SemanticDefinitionBody::FunctionWithBody {
                                    body: SemanticOriginalFunctionBody::Defined(_),
                                    ..
                                }
                            )
                        {
                            return Err(bad(
                                "opaque local result class lacks its complete original body domain",
                            ));
                        }
                        // The complete class of this exact emitted result is
                        // fresh to the call. Its input alternatives remain in
                        // the independent return-source/loan proof; no input
                        // images are identified by this coordinate.
                        local_image = SemanticClassExpr::IntrinsicClass {
                            operand: SemanticOperand::Value(output),
                            path: Vec::new(),
                            class: class_index as u32,
                        };
                        &local_image
                    }
                }
            };
            crate::dyn_metadata::semantic_operand_snapshot(selected, self.limits, self.left, 1)?;
            *image = selected.clone();
        }
        Ok(())
    }
    fn insert_result_input(
        &mut self,
        images: &mut BTreeMap<StorageBinder, SemanticClassExpr>,
        binder: StorageBinder,
        image: &SemanticClassExpr,
    ) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        if let Some(old) = images.get(&binder) {
            crate::dyn_metadata::semantic_operand_snapshot(old, self.limits, self.left, 1)?;
            if old != image {
                return Err(bad(
                    "opaque input class changes its complete original caller image",
                ));
            }
        } else {
            images.insert(binder, image.clone());
        }
        Ok(())
    }
    fn charge_closed_context(&mut self, context: Option<&ClosedContext>) -> R<()> {
        if let Some(context) = context {
            self.fee(
                context
                    .arguments
                    .len()
                    .saturating_add(context.environment.len()),
            )?;
            for actual in context.arguments.iter().chain(&context.environment) {
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            }
        }
        Ok(())
    }

    fn closed_context_actual(
        &mut self,
        creator: SemanticInstanceId,
        recipe: &SemanticActualRecipe,
        value: &SemanticActual,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        let (ty, shape) = match value {
            SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes })
            | SemanticActual::Const {
                ty: SemanticTypeValue::Resolved { ty, life, classes },
                ..
            } => {
                if !classes.is_empty() {
                    return Err(unknown(
                        "multiple opaque origins retain a free actual class",
                    ));
                }
                (ty, life)
            }
            _ => {
                return Err(unknown(
                    "multiple opaque origins require a completely closed actual context",
                ));
            }
        };
        let _ = self.copy_shape(shape)?;
        if shape.binder_count != 0
            || !shape.slots.is_empty()
            || !shape.retained.is_empty()
            || !shape.nominal_arguments.is_empty()
        {
            return Err(unknown(
                "multiple opaque origins retain a nonempty actual domain",
            ));
        }
        // Static is an original source property, not N=0 of an erased
        // pointer. Keep the view's exact Type and prove its whole child zero;
        // ordinary addresses still require their actual free class domain.
        let persistent = if let Type::Address { mutable: false, inner } = ty {
            if self.source_static_outer(creator, recipe, depth + 1)? {
                inner.as_ref()
            } else {
                ty
            }
        } else {
            ty
        };
        self.module.ordinary_zero(
            persistent,
            self.limits,
            self.left,
            &self.ledger.ordinary_stats,
        )?;
        Ok(())
    }

    fn closed_result_context(
        &mut self,
        creator: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        value: &SemanticTypeValue,
        depth: usize,
    ) -> R<ClosedContext> {
        self.node(depth)?;
        let SemanticTypeRecipe::Closure {
            definition,
            arguments,
            environment,
        } = recipe
        else {
            return Err(unknown(
                "multiple opaque origins need their complete original closure result",
            ));
        };
        let SemanticTypeValue::Resolved {
            ty: Type::Closure(id),
            life,
            classes,
        } = value
        else {
            return Err(bad(
                "closed original closure result has another actual carrier",
            ));
        };
        let _ = self.copy_shape(life)?;
        if life.binder_count != 0
            || !classes.is_empty()
            || !life.slots.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(unknown(
                "multiple opaque origins have a free actual result domain",
            ));
        }
        self.module.ordinary_zero(
            &Type::Closure(*id),
            self.limits,
            self.left,
            &self.ledger.ordinary_stats,
        )?;
        self.fee(
            self.module
                .closures
                .len()
                .saturating_add(self.input.instances.len()),
        )?;
        let closure = self
            .module
            .closures
            .iter()
            .find(|row| row.id == *id)
            .ok_or_else(|| bad("closed result has no actual closure definition"))?;
        let mut child = None;
        for original in &self.input.instances {
            let row = self.instance(original.id)?;
            if row.parent == Some(creator)
                && row.definition == *definition
                && row.target
                    == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(
                        closure.function,
                    ))
                && matches!(&row.environment, Some(SemanticEnvironmentInstance {
                    closure: SemanticClosureTarget::Typed(actual), ..
                }) if actual == id)
            {
                if child.replace(row).is_some() {
                    return Err(bad("closed result has ambiguous creator environments"));
                }
            }
        }
        let child = child
            .ok_or_else(|| bad("closed result lacks its actual creator environment"))?;
        let actual_environment = &child
            .environment
            .as_ref()
            .expect("selected environment")
            .actuals;
        self.fee(arguments.len().saturating_add(environment.len()))?;
        if arguments.len() != child.actuals.len() || environment.len() != actual_environment.len() {
            return Err(bad("closed result omits a complete mixed context"));
        }
        let mut checked_arguments = Vec::new();
        for (ordinal, (recipe, claim)) in arguments.iter().zip(&child.actuals).enumerate() {
            let expected = self.recipe_actual(creator, recipe, depth + 1)?;
            let actual = self.holder_actual(child.id, ordinal as u32, claim, depth + 1)?;
            self.closed_context_actual(creator, recipe, &expected, depth + 1)?;
            self.closed_context_actual(creator, recipe, &actual, depth + 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
            if expected != actual {
                return Err(bad("closed result changes its original mixed actual"));
            }
            checked_arguments.push(expected);
        }
        let mut checked_environment = Vec::new();
        for (recipe, claim) in environment.iter().zip(actual_environment) {
            let expected = self.recipe_actual(creator, recipe, depth + 1)?;
            let actual = self.actual(creator, claim, depth + 1)?;
            self.closed_context_actual(creator, recipe, &expected, depth + 1)?;
            self.closed_context_actual(creator, recipe, &actual, depth + 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
            if expected != actual {
                return Err(bad("closed result changes its original sealed environment"));
            }
            checked_environment.push(expected);
        }
        Ok(ClosedContext {
            definition: *definition,
            arguments: checked_arguments,
            environment: checked_environment,
        })
    }

    pub(super) fn multiple_inferred_actual(
        &mut self,
        proof: &super::super::original::Proof,
        caller: SemanticInstanceId,
        owner: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        depth: usize,
    ) -> R<Option<SemanticActual>> {
        let input = self.input;
        let directory = self.module;
        let frozen = self.ledger.frozen.as_deref();
        let boundaries = proof.call_actual_boundaries(
            owner, call, formal, self.limits, self.left,
            |left| {
                if directory.find_source_definition(input, owner, left)?.is_some() {
                    return Ok(input);
                }
                frozen.map(|source| &source.semantic).ok_or_else(|| {
                    bad("inferred opaque origins lost their original definition forest")
                })
            },
        )?;
        let Some(boundaries) = boundaries else {
            return Ok(None);
        };
        self.fee(boundaries.len())?;
        if boundaries.len() < 2 {
            return Err(bad(
                "multiple opaque origins omit their full original source set",
            ));
        }
        let mut result: Option<(SemanticTypeValue, Option<ClosedContext>)> = None;
        for boundary in &boundaries {
            let (value, context) =
                self.actual_result_boundary(caller, boundary, None, true, depth + 1)?;
            if let Some((prior, prior_context)) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                self.charge_closed_context(prior_context.as_ref())?;
                self.charge_closed_context(context.as_ref())?;
                if prior != &value || prior_context != &context {
                    return Err(bad(
                        "original opaque origins disagree on their complete actual result and context",
                    ));
                }
            } else {
                result = Some((value, context));
            }
        }
        let (value, _) = result.ok_or_else(|| bad("inferred opaque source set is empty"))?;
        Ok(Some(SemanticActual::Type(value)))
    }

    pub(super) fn original_result_boundary(
        &mut self,
        scope: SemanticInstanceId,
        selector: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        let caller = self.ancestor(scope, selector, depth + 1)?;
        let caller_definition = self.instance(caller)?.definition;
        let boundary = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("opaque result lacks its checked original caller"))?;
            proof.call_result_boundary(
                caller_definition,
                caller_definition,
                source,
                self.limits,
                self.left,
            )?
        };
        let Some(boundary) = boundary else {
            return Ok(None);
        };
        if scope != caller {
            return Err(unknown(
                "ancestor opaque result requires its actual sealed environment boundary",
            ));
        }
        self.actual_result_boundary(caller, &boundary, hint, false, depth + 1)
            .map(|(value, _)| Some(value))
    }

    pub(super) fn forwarded_opaque_result(
        &mut self,
        caller: SemanticInstanceId,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        self.node(depth)?;
        let owner = self.instance(caller)?.definition;
        let boundaries = {
            let proof = self.ledger.original.borrow();
            proof.as_ref().ok_or_else(|| bad("forwarded opaque result lacks its original proof"))?
                .forwarded_opaque_boundaries(owner, self.limits, self.left)?
        };
        let Some(boundaries) = boundaries else { return Ok(None); };
        let mut result = None;
        for boundary in &boundaries {
            let (value, _) = self.actual_result_boundary(caller, boundary, hint, false, depth + 1)?;
            if let Some(previous) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(previous, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                if previous != &value {
                    return Err(bad("forwarded opaque returns disagree on their complete original result"));
                }
            } else {
                result = Some(value);
            }
        }
        Ok(result)
    }

    /// During actual Task preparation, an inner call can already be emitted
    /// while the original semantic directory still uses Core addresses. Use
    /// that completed occurrence's ledger receipt; publication independently
    /// relocates and checks the same call again.
    pub(super) fn actual_call_point(
        &mut self,
        caller: SemanticInstanceId,
        site: &SemanticSite,
        depth: usize,
    ) -> R<(BlockId, u32)> {
        self.node(depth)?;
        if site.instance != caller {
            return Err(bad("result call boundary changes its actual caller instance"));
        }
        let expression = match site.at {
            SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After } =>
                return Ok((block, instruction)),
            SemanticSiteAddress::Core { expression, occurrence: 0, role: SemanticEmissionRole::Call }
                if matches!(site.action, SemanticSiteAction::Call { .. }
                    | SemanticSiteAction::CoreBodyCall | SemanticSiteAction::CallableCall { .. }) => expression,
            _ => return Err(bad("result call is not its actual post-call occurrence")),
        };
        let function = self.function(caller)?;
        let ledger = self.ledger;
        let row = ledger.completed_expression(function.id, expression, self.left, function.span)?;
        if row.callee.is_none() {
            return Err(bad("result occurrence is not a completed actual call expression"));
        }
        self.fee(row.instructions.len())?;
        let mut found = None;
        for point in &row.instructions {
            self.fee(function.blocks.len())?;
            let instruction = function.blocks.iter().find(|block| block.id == point.block)
                .and_then(|block| block.instructions.get(point.instruction))
                .ok_or_else(|| bad("completed result receipt leaves its actual emitted CFG"))?;
            if instruction.result != row.value || !matches!(instruction.kind,
                InstructionKind::Call { .. } | InstructionKind::SuspendCall { .. }
                    | InstructionKind::ForeignCall { .. }) {
                continue;
            }
            let index = u32::try_from(point.instruction)
                .map_err(|_| bad("completed call instruction ordinal overflows"))?;
            if found.replace((point.block, index)).is_some() {
                return Err(bad("completed result occurrence has multiple actual call producers"));
            }
        }
        found.ok_or_else(|| bad("completed result occurrence has no exact direct-call producer"))
    }

    fn actual_result_boundary(
        &mut self,
        caller: SemanticInstanceId,
        boundary: &super::super::original::CalleeResultBoundary,
        hint: Option<&Type>,
        closed: bool,
        depth: usize,
    ) -> R<(SemanticTypeValue, Option<ClosedContext>)> {
        self.node(depth)?;
        if boundary.caller != self.instance(caller)?.definition {
            return Err(bad("opaque result changes its original caller definition"));
        }
        let function = self.function(caller)?;
        self.fee(self.input.sites.len())?;
        let mut sites = self
            .input
            .sites
            .iter()
            .filter(|site| site.instance == caller && site.recipe == boundary.call);
        let site = sites
            .next()
            .ok_or_else(|| bad("original result call has no actual emitted site"))?;
        if sites.next().is_some() {
            return Err(bad("original result call has multiple actual sites"));
        }
        let (block, instruction) = self.actual_call_point(caller, site, depth + 1)?;
        self.fee(function.blocks.len())?;
        let op = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .and_then(|row| row.instructions.get(instruction as usize))
            .ok_or_else(|| bad("opaque result call leaves its actual emitted CFG"))?;
        let (InstructionKind::Call {
            function: target, ..
        }
        | InstructionKind::SuspendCall {
            function: target, ..
        }
        | InstructionKind::ForeignCall {
            function: target, ..
        }) = &op.kind
        else {
            return Err(unknown(
                "opaque result needs its exact original direct-call target",
            ));
        };
        let output = op
            .result
            .ok_or_else(|| bad("opaque result call has no actual result SSA"))?;
        let actual_result = function
            .values
            .get(output.0 as usize)
            .ok_or_else(|| bad("opaque result call has no actual complete result SSA"))?;
        if let Some(hint) = hint {
            inference::charge_type_snapshot(
                actual_result,
                Span::default(),
                self.limits,
                self.left,
                1,
            )?;
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            if hint != actual_result {
                let mut diagnostic = bad(
                    "original result alias changes the actual call's whole result type",
                );
                diagnostic.notes.push(format!(
                    "original result alias boundary: caller {caller:?}, original call {:?}, callee {:?}, actual target {target:?}, block {block:?}, instruction {instruction}, result {output:?}",
                    boundary.call, boundary.callee
                ));
                diagnostic.notes.push(format!("actual alias type: {hint:?}"));
                diagnostic.notes.push(format!("actual call result type: {actual_result:?}"));
                return Err(diagnostic);
            }
        }
        let hint = Some(actual_result);
        let mut actuals = Vec::new();
        self.fee(boundary.actuals.len())?;
        for (ordinal, recipe) in boundary.actuals.iter().enumerate() {
            actuals.push(self.call_recipe_actual(
                caller,
                boundary.call,
                ordinal as u32,
                None,
                recipe,
                depth + 1,
            )?);
        }
        self.fee(self.input.instances.len())?;
        let mut result: Option<(SemanticTypeValue, Option<ClosedContext>)> = None;
        for original in &self.input.instances {
            let candidate = self.instance(original.id)?;
            if candidate.definition != boundary.callee
                || candidate.target
                    != SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(*target))
            {
                continue;
            }
            if !self.opaque_actuals_match(caller, boundary.call, candidate, &actuals, depth + 1)? {
                continue;
            }
            let forwarded = if matches!(boundary.result, SemanticTypeRecipe::Opaque { .. }) {
                self.forwarded_opaque_result(candidate.id, hint, depth + 1)?
            } else { None };
            let mut value = match forwarded {
                Some(value) => value,
                None => self.ty(candidate.id, &boundary.result, hint, depth + 1)?,
            };
            self.explicit_result_images(
                &mut value,
                &actuals,
                caller,
                candidate,
                boundary,
                output,
                depth + 1,
            )?;
            let context = if closed {
                Some(self.closed_result_context(
                    candidate.id,
                    &boundary.result,
                    &value,
                    depth + 1,
                )?)
            } else {
                None
            };
            if let Some((prior, prior_context)) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                self.charge_closed_context(prior_context.as_ref())?;
                self.charge_closed_context(context.as_ref())?;
                if prior != &value || prior_context != &context {
                    return Err(bad(
                        "actual callee contexts disagree on the complete original result",
                    ));
                }
            } else {
                result = Some((value, context));
            }
        }
        result
            .ok_or_else(|| bad("opaque result call has no complete matching actual callee context"))
    }
}

/// Evaluate only the result of this exact original and emitted call. This is
/// an untrusted whole-type proposal; World still proves the actual call/return
/// class flow independently of these result images.
pub(crate) fn call_result_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    site: SemanticSiteId,
    hint: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeValue> {
    let mut eval = Eval {
        input: program,
        module: &ActualDirectory::from_module_at(module, "opaque call_result_at"),
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    };
    let owner = eval.instance(scope)?.definition;
    let original = eval.definition(owner)?;
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad(
            "opaque call result lacks its complete original operation body",
        ));
    };
    eval.fee(site_nodes.len())?;
    let mut found = site_nodes.iter().filter(|row| row.site == site);
    let row = found
        .next()
        .ok_or_else(|| bad("opaque call result has no original node relationship"))?;
    if found.next().is_some() {
        return Err(bad(
            "opaque result site has multiple original operation nodes",
        ));
    }
    let SemanticOriginalNode::Expression(node) = row.node else {
        return Err(bad(
            "opaque result site does not select an original expression",
        ));
    };
    eval.original_result_boundary(
        scope,
        SemanticScopeRef::Ancestor(0),
        &SemanticOriginalTypeSource::Expression(node),
        Some(hint),
        0,
    )?
    .ok_or_else(|| bad("original call has no complete opaque result boundary"))
}
