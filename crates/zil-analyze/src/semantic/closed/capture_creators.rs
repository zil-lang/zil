//! Original lexical values are bound at each real environment construction.
//! Navigation need not fabricate an executable whole-name read in the child.
use super::*;
use crate::semantic::value::{Argument, Class, Value};
pub(super) struct CreatorBinding {
    pub(super) capture: u32,
    pub(super) lexical: Value,
    pub(super) stored: Value,
    pub(super) bound_operands: BTreeSet<ValueId>,
}
impl Check<'_, '_> {
    /// Evaluate only the proved original occurrence equations. The full
    /// original N, class directory and physical modes remain unchanged.
    pub(super) fn capture_class_value(
        &self,
        mut value: Value,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        b.spend(value.classes.len(), WORK)?;
        for class in &mut value.classes {
            *class = self.capture_class_image(class.clone(), b)?;
        }
        Ok(value)
    }
    /// Compare complete values after applying only exact creator-bound
    /// captured-head equations. No geometry, class or occurrence is omitted.
    pub(super) fn capture_values_equal(
        &self,
        left: &Value,
        right: &Value,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        if left.equal_paid(right, b)? {
            return Ok(true);
        }
        let left = self.capture_class_value(left.copy(b)?, b)?;
        let right = self.capture_class_value(right.copy(b)?, b)?;
        left.equal_paid(&right, b)
    }
    /// A call's complete ordered actuals stay in its original caller domain.
    /// Only the same independently checked creator equations may transport
    /// their class images; no actual kind, carrier or Type field is erased.
    pub(super) fn capture_arguments_equal(
        &self,
        left: &Argument,
        right: &Argument,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        match (left, right) {
            (Argument::Type(left), Argument::Type(right)) => {
                self.capture_values_equal(left, right, b)
            }
            (Argument::Life(left), Argument::Life(right)) => Ok(self
                .capture_class_image(left.clone(), b)?
                == self.capture_class_image(right.clone(), b)?),
            _ => super::traits::arguments_equal(
                std::slice::from_ref(left),
                std::slice::from_ref(right),
                b,
            ),
        }
    }
    pub(super) fn capture_class_image(
        &self,
        mut class: Class,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        let mut seen = BTreeSet::new();
        while let Class::Implicit(owner, ordinal) = class {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((owner, ordinal)) {
                return Err(fail("original captured-head class equations form a cycle"));
            }
            let instance = self.inventory.instance(owner, b)?;
            let mut selected = None;
            for row in &instance.header_embedding {
                b.spend(1, WORK)?;
                if !matches!(row.header, StorageBinder::Fixed(_))
                    || !matches!(row.actual, SemanticClassExpr::Implicit { scope: SemanticScopeRef::Ancestor(0), ordinal: actual } if actual == ordinal)
                {
                    continue;
                }
                if let Some(actual) =
                    self.captured_implicit_head(instance, row.header, ordinal, b)?
                {
                    if selected.as_ref().is_some_and(|old| old != &actual) {
                        return Err(fail(
                            "original captured-head class has competing creator equations",
                        ));
                    }
                    selected = Some(actual);
                }
            }
            let Some(actual) = selected else { break };
            if actual == class {
                break;
            }
            class = actual;
        }
        Ok(class)
    }
    /// Relate an original free-name borrow occurrence to the physical head
    /// saved by every actual creator. The original lexical value remains the
    /// complete pointee; this equation supplies no data or loan permission.
    pub(super) fn captured_implicit_head(
        &self,
        instance: &SemanticInstance,
        binder: StorageBinder,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<Option<Class>> {
        if !matches!(binder, StorageBinder::Fixed(_)) {
            return Ok(None);
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Ok(None);
        };
        let function = self.inventory.function(target, b)?;
        let Some(closure) = function
            .environment
            .and_then(|id| self.module.closures.get(id.0 as usize))
        else {
            return Ok(None);
        };
        if closure.function != function.id {
            return Err(fail("capture head changes its actual entry"));
        }
        let definition = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } = &definition.body
        else {
            return Ok(None);
        };
        let mut occurrence = None;
        for row in &definition.implicit_lifetimes {
            b.spend(row.occurrence_path.len().saturating_add(1), WORK)?;
            if row.ordinal == ordinal && occurrence.replace(row).is_some() {
                return Err(fail(
                    "capture head repeats its original implicit occurrence",
                ));
            }
        }
        let occurrence = occurrence
            .ok_or_else(|| fail("capture head omits its original implicit occurrence"))?;
        let mut lexical = None;
        for recipe in recipes {
            b.spend(
                recipe
                    .occurrence_path
                    .len()
                    .saturating_add(occurrence.occurrence_path.len())
                    .saturating_add(1),
                WORK,
            )?;
            if recipe.occurrence_path != occurrence.occurrence_path {
                continue;
            }
            let Some(capture) =
                self.original
                    .capture_recipe_ordinal(instance.definition, recipe, b)?
            else {
                continue;
            };
            if lexical.is_some_and(|old| old != capture) {
                return Err(fail(
                    "capture head occurrence selects several original lexical bindings",
                ));
            }
            lexical = Some(capture);
        }
        let Some(lexical) = lexical else {
            return Ok(None);
        };
        let creators = self.capture_creator_bindings(instance, closure, b)?;
        let Some(binding) = creators.get(&lexical) else {
            return Ok(None);
        };
        // A move capture already has its original complete class image. Only
        // a real extra physical Address head needs this occurrence equation.
        let Type::Address { inner, .. } = &binding.stored.ty else {
            return Ok(None);
        };
        if inner.as_ref() != &binding.lexical.ty
            || self.pointee(&binding.stored, b)? != binding.lexical
        {
            return Ok(None);
        }
        let [slot] = binding.stored.life.slots.as_slice() else {
            return Err(fail("capture head loses its exact outer slot"));
        };
        if !slot.path.is_empty() {
            return Err(fail("capture head changes its whole outer slot"));
        }
        let contract = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("capture head has no actual header"))?;
        let rows =
            self.capture_class_headers(contract, binding.capture, &binding.stored, slot.binder, b)?;
        let physical = rows
            .iter()
            .find(|(_, physical, _)| *physical)
            .map(|(_, _, binder)| *binder);
        if physical != Some(binder) {
            return Err(fail(
                "original capture head is attached to another physical header occurrence",
            ));
        }
        Ok(Some(
            binding
                .stored
                .classes
                .get(slot.binder as usize)
                .cloned()
                .ok_or_else(|| fail("capture head lacks its complete original class"))?,
        ))
    }

    pub(super) fn capture_creator_bindings(
        &self,
        instance: &SemanticInstance,
        closure: &ClosureDefinition,
        b: &mut Budget,
    ) -> CompileResult<BTreeMap<u32, CreatorBinding>> {
        let definition = self.inventory.definition(instance.definition, b)?;
        let parent = self.inventory.instance(
            instance
                .parent
                .ok_or_else(|| fail("captured environment has no real creator parent"))?,
            b,
        )?;
        let SemanticInstanceTarget::Function(target) = &parent.target else {
            return Err(fail(
                "captured environment parent is not an actual function",
            ));
        };
        let function = self.inventory.function(target, b)?;
        let parent_definition = self.inventory.definition(parent.definition, b)?;
        let source_index = self.inventory.original_index(parent_definition.id, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &parent_definition.body
        else {
            return Err(fail(
                "capture creator has no complete original function body",
            ));
        };
        let mut moved = None;
        for recipe in sites {
            b.spend(1, WORK)?;
            if !matches!(recipe.kind, SemanticSiteRecipeKind::Closure { definition: target, .. } if target == definition.id)
            {
                continue;
            }
            for link in site_nodes {
                b.spend(1, WORK)?;
                if link.site != recipe.id {
                    continue;
                }
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail(
                        "capture creator original role is not a closure expression",
                    ));
                };
                let CoreExprKind::Closure { move_capture, .. } =
                    &source_index.get_expr(node, b)?.kind
                else {
                    return Err(fail(
                        "capture creator original role changes its closure operation",
                    ));
                };
                if moved.replace(*move_capture).is_some() {
                    return Err(fail(
                        "capture creator has competing original ownership modes",
                    ));
                }
            }
        }
        let moved =
            moved.ok_or_else(|| fail("capture creator has no exact original closure mode"))?;
        let environment = instance
            .environment
            .as_ref()
            .ok_or_else(|| fail("captured environment has no sealed actual values"))?;
        let mut result = BTreeMap::new();
        for (ordinal, formal) in definition.environment.iter().enumerate() {
            b.spend(1, WORK)?;
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| fail("original environment ordinal overflow"))?;
            let Some(anchor) = self.original.capture_origin(definition.id, ordinal, b)? else {
                continue;
            };
            let Some(SemanticActual::Type(actual)) = environment.actuals.get(ordinal as usize)
            else {
                return Err(fail("runtime capture changes its original value kind"));
            };
            crate::dyn_metadata::recipe(&formal.source, b)?;
            let expected = self.resolved(parent, actual, b, 0)?;
            let mut selected = None;
            let mut stored = None;
            let mut bound_operands = BTreeSet::new();
            let mut creators = 0usize;
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    let InstructionKind::MakeClosure {
                        closure: id,
                        captures,
                    } = &instruction.kind
                    else {
                        continue;
                    };
                    if *id != closure.id {
                        continue;
                    }
                    creators += 1;
                    if captures.len() != closure.captures.len() {
                        return Err(fail(
                            "actual environment construction loses its whole capture vector",
                        ));
                    }
                    let mut this = None;
                    for (capture, operand) in captures.iter().enumerate() {
                        if !self.capture_matches(parent, function, *operand, anchor, b, 0)? {
                            continue;
                        }
                        operations::available_before(
                            function,
                            (block.id, index, SemanticSiteEdge::Before),
                            *operand,
                            b,
                        )?;
                        let value = self.ssa_value(parent, function, *operand, b, 0)?;
                        // The complete original category selects each exact
                        // physical leaf. Every actual constructor still proves
                        // that leaf back to a literal, including nested fields.
                        self.check_static_value_sources(&value, function, *operand, b)?;
                        if closure.captures.get(capture) != Some(&value.ty) {
                            return Err(fail("actual creator changes its physical capture type"));
                        }
                        let value = if value.equal_paid(&expected, b)?
                            || !moved
                                && matches!(&value.ty, Type::Address { inner, .. } if inner.as_ref() == &expected.ty)
                                && self.pointee(&value, b)?.equal_paid(&expected, b)?
                        {
                            value
                        } else if moved
                            && self.moved_reference_operand(
                                parent, function, *operand, &value, &expected, b,
                            )?
                            || !moved
                                && self.capture_reborrow(parent, function, *operand, value, &expected, b)?
                        {
                            // The closure stores the proved lexical bound.
                            // The actual source still has its own CFG loan.
                            b.lookup(bound_operands.len(), WORK)?;
                            bound_operands.insert(*operand);
                            expected.copy(b)?
                        } else {
                            return Err(fail(
                                "actual creator changes its captured lexical value or intrinsic class image",
                            ));
                        };
                        if stored.as_ref().is_some_and(|old| old != &value) {
                            return Err(fail(
                                "actual creators disagree on the complete stored capture head or child domain",
                            ));
                        }
                        stored = Some(value);
                        let capture = u32::try_from(capture)
                            .map_err(|_| fail("physical capture ordinal overflow"))?;
                        if this.replace(capture).is_some() {
                            return Err(fail(
                                "original captured binding occurs twice in one actual environment",
                            ));
                        }
                    }
                    let capture = this.ok_or_else(|| {
                        fail("actual creator omits its original captured lexical binding")
                    })?;
                    if selected.is_some_and(|old| old != capture) {
                        return Err(fail(
                            "actual creators disagree on the original environment binding",
                        ));
                    }
                    selected = Some(capture);
                }
            }
            if creators == 0 {
                return Err(fail(
                    "captured value has no actual environment construction in its real parent",
                ));
            }
            b.lookup(result.len(), WORK)?;
            result.insert(
                ordinal,
                CreatorBinding {
                    capture: selected
                        .ok_or_else(|| fail("captured value has no physical original operand"))?,
                    bound_operands,
                    lexical: expected,
                    stored: stored
                        .ok_or_else(|| fail("captured value has no complete stored head"))?,
                },
            );
        }
        Ok(result)
    }

    fn capture_reborrow(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        mut operand: ValueId,
        mut actual: Value,
        expected: &Value,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let Type::Address { mutable, .. } = &expected.ty else {
            return Ok(false);
        };
        let pointee = self.pointee(expected, b)?;
        for depth in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            if !geometry::same_type(&actual.ty, &expected.ty, b)?
                || !self.pointee(&actual, b)?.equal_paid(&pointee, b)?
            {
                return Ok(false);
            }
            if actual.equal_paid(expected, b)? {
                return Ok(true);
            }
            let (block, index, instruction) = self.defining(function, operand, b)?;
            let InstructionKind::ReborrowAddress {
                address,
                mutable: child_mutable,
            } = instruction.kind
            else {
                return Ok(false);
            };
            let mode = if *mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            };
            if child_mutable != *mutable
                || !matches!(actual.life.slots.as_slice(), [slot] if slot.path.is_empty()
                    && slot.mode == mode
                    && actual.classes.get(slot.binder as usize)
                        == Some(&Class::Reborrow(function.id, operand)))
            {
                return Ok(false);
            }
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                address,
                b,
            )?;
            operand = address;
            actual = self.ssa_value(instance, function, operand, b, depth + 1)?;
        }
        Err(unknown("captured reborrow exceeds its finite actual chain"))
    }

    fn capture_matches(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        value: ValueId,
        anchor: original::LocalAnchor,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "captured lexical source exceeds its finite actual chain",
            ));
        }
        let (_, _, instruction) = self.defining(function, value, b)?;
        if let InstructionKind::ReborrowAddress { address, .. } = instruction.kind {
            return self.capture_matches(instance, function, address, anchor, b, depth + 1);
        }
        match anchor {
            original::LocalAnchor::Environment(ordinal) => {
                let expected = self.environment_capture(instance, function, ordinal, b)?;
                Ok(
                    matches!(instruction.kind,InstructionKind::ReadCapture {index}|InstructionKind::TakeCapture {index}|InstructionKind::BorrowCapture {index,..} if index==expected),
                )
            }
            anchor => {
                let expected = self.anchored_local(instance, function, anchor, b)?;
                Ok(
                    matches!(instruction.kind,InstructionKind::Load(local)|InstructionKind::TakeLocal {local}|InstructionKind::BorrowLocal {local}|InstructionKind::BorrowLocalMut {local} if local==expected),
                )
            }
        }
    }
}
