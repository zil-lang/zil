//! Check original recipes against each actual operation. Scalar equality and
//! captured-head relations never erase intrinsic classes or grant memory access.
use super::*;

impl Check<'_, '_> {
    fn affine_field_initializer(
        &self, instance: &SemanticInstance, site: SemanticSiteId, local: LocalId,
        ty: &Type, point: &Point<'_>, b: &mut Budget,
    ) -> CompileResult<()> {
        if !matches!(ty, Type::Package { .. } | Type::Enum { .. } | Type::Tuple(_) | Type::ErrorUnion { .. })
            && !matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. })) {
            return Ok(());
        }
        let source = self.inventory.original_index(instance.definition, b)?;
        let Some(SemanticOriginalNode::Statement(binding)) = source.site_node(site, b)? else { return Ok(()); };
        let CoreStatementKind::Binding { initializer: Some(initializer), .. } = &source.get_stmt(binding, b)?.kind
            else { return Ok(()); };
        let transparent = |mut expression: &'_ SemanticOriginalExpr, b: &mut Budget| -> CompileResult<zil_base::NodeId> {
            while let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. }
                | CoreExprKind::Unary { op: UnaryOp::Give, operand: inner } = &expression.kind {
                b.spend(1, WORK)?;
                expression = inner;
            }
            Ok(expression.id)
        };
        let expression = source.get_expr(transparent(initializer, b)?, b)?;
        let CoreExprKind::Member { object, member, .. } = &expression.kind else { return Ok(()); };
        let Some(anchor) = self.original.place_read_anchor(instance.definition, transparent(object, b)?, b)?
            else { return Ok(()); };
        let parent = self.anchored_local(instance, point.function, anchor, b)?;
        let Some(Type::Package { fields, .. }) = point.function.locals.get(parent.0 as usize).map(|local| crate::resources::package_storage(&local.ty))
            else { return Ok(()); };
        crate::dyn_metadata::ty(ty, b)?;
        if ty.is_copy_value() { return Ok(()); }
        let Some(InstructionKind::Store { local: target, value }) = point.block.instructions.get(point.instruction + 1).map(|row| &row.kind)
            else { return Err(fail("affine field declaration lacks its initial storage transfer")); };
        let (_, _, actual) = self.defining(point.function, *value, b)?;
        let InstructionKind::TakePackageField { local: root, index } = actual.kind else {
            return Err(fail("affine field initializer lacks its actual consuming field transfer"));
        };
        let Some((field, child)) = fields.get(index as usize) else { return Err(fail("affine field transfer exceeds its actual parent")); };
        b.spend(field.len().saturating_add(member.len()), WORK)?;
        if *target != local || root != parent || field != member || !geometry::same_type(child, ty, b)? {
            return Err(fail("affine field initializer selects another original owner or field"));
        }
        Ok(())
    }

    fn scalar_call_inputs(
        &self, instance: &SemanticInstance, function: &TypedFunction, site: SemanticSiteId,
        arguments: &[ValueId], parameters: &[&Type], b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(parameters.len(), WORK)?;
        if !parameters.iter().any(|ty| ty.is_b0_scalar()) { return Ok(()); }
        let original = self.inventory.original_index(instance.definition, b)?;
        let Some(SemanticOriginalNode::Expression(node)) = original.site_node(site, b)? else {
            return Err(fail("scalar argument lacks its original call occurrence"));
        };
        let CoreExprKind::Call { arguments: inputs, .. } = &original.get_expr(node, b)?.kind else {
            return Err(fail("scalar argument selects another original operation"));
        };
        if inputs.len() != arguments.len() {
            return Err(fail("static call changes its original scalar argument ordinals"));
        }
        for (ordinal, ((source, &operand), ty)) in inputs.iter().zip(arguments).zip(parameters).enumerate() {
            if !ty.is_b0_scalar() { continue; }
            let initializer = if matches!(source.kind, CoreExprKind::Name(_)) {
                self.original.prepared_initializer(instance.definition, source.id, b)?
            } else { None };
            let source = match initializer {
                Some(node) => original.get_expr(node, b)?,
                None => source,
            };
            if let Some(emitted) = self.original_subject_value(instance, function, source.id, b)? {
                if (emitted == operand || self.operation_operand_matches(function, operand, emitted, b)?)
                    && self.scalar_call_projection(instance, function, source, emitted, b)?
                {
                    continue;
                }
            } else if self.copy_expression_source(instance, function, source, operand, b, 0)? {
                continue;
            } else if initializer.is_some() {
                let mut proven = true;
                for value in self.call_operand_origins(function, operand, b)? {
                    if !self.copy_expression_source(instance, function, source, value, b, 0)? {
                        proven = false;
                        break;
                    }
                }
                if proven { continue; }
            }
            let (block, index, producer) = self.defining(function, operand, b)?;
            let mut diagnostic = fail(&format!("actual scalar Call argument selects another original expression {:?}, SSA {:?}", source.id, operand));
            diagnostic[0].span = Some(source.span);
            diagnostic[0].notes.push(format!(
                "Call argument {ordinal} requires the value of original {:?} in {:?}; actual {:?} is produced in {:?}, block {} instruction {index} at {:?}.",
                source.id, instance.definition, operand, function.id, block.0, producer.span,
            ));
            return Err(diagnostic);
        }
        Ok(())
    }

    fn foreign_projected_arguments(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        site: SemanticSiteId, arguments: &[ValueId], parameters: &[&Type], b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(parameters.len(), WORK)?;
        if !parameters.contains(&&Type::ForeignHandle) { return Ok(()); }
        let original = self.inventory.original_index(instance.definition, b)?;
        let Some(SemanticOriginalNode::Expression(node)) = original.site_node(site, b)? else {
            return Err(fail("foreign argument lacks its original Call occurrence"));
        };
        let CoreExprKind::Call { arguments: inputs, .. } = &original.get_expr(node, b)?.kind else {
            return Err(fail("foreign argument selects another original operation"));
        };
        if arguments.len() != inputs.len() {
            return Err(fail("foreign call changes its exact original argument arity"));
        }
        for ((source, actual), ty) in inputs.iter().zip(arguments).zip(parameters) {
            if **ty != Type::ForeignHandle { continue; }
            if !self.foreign_resource_member(instance, source, b)? { continue; }
            for origin in self.call_operand_origins(function, *actual, b)? {
                if !self.copy_expression_source(instance, function, source, origin, b, 0)? {
                    return Err(fail("foreign handle argument selects another original resource field or parent"));
                }
            }
        }
        Ok(())
    }

    fn foreign_resource_member(
        &self, instance: &SemanticInstance, mut expression: &SemanticOriginalExpr, b: &mut Budget,
    ) -> CompileResult<bool> {
        loop {
            b.spend(1, WORK)?;
            expression = match &expression.kind {
                CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => inner,
                CoreExprKind::Member { object, .. } => {
                    let parent = self.original.source_type(instance, SemanticScopeRef::Ancestor(0),
                        &SemanticOriginalTypeSource::Expression(object.id), self.program, self.inventory, b)?;
                    if matches!(parent, Type::Managed(_) | Type::Desc(_)) { return Ok(true); }
                    object
                }
                _ => return Ok(false),
            };
        }
    }

    fn resumable_enum_operands(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        node: zil_base::NodeId,
        ty: &Type,
        fields: &[ValueId],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if fields.is_empty() || function.call_kind != CallKind::Suspend
            || !crate::tasks::contains_kind(self.module, ty, b, false)?
        {
            return Ok(());
        }
        let source = self.inventory.original_index(instance.definition, b)?.get_expr(node, b)?;
        let CoreExprKind::Call { arguments, .. } = &source.kind else {
            return Err(fail("resumable enum payload belongs to another original operation"));
        };
        b.spend(arguments.len(), WORK)?;
        if arguments.len() != fields.len() {
            return Err(fail("resumable enum changes its complete original payload occurrences"));
        }
        for (argument, &field) in arguments.iter().zip(fields) {
            self.resumable_enum_operand(instance, function, argument, field, b)?;
        }
        Ok(())
    }

    fn resumable_enum_operand(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        mut expression: &SemanticOriginalExpr,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let written = expression.id;
        loop {
            b.spend(1, WORK)?;
            expression = match &expression.kind {
                CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. }
                | CoreExprKind::Unary { op: UnaryOp::Give, operand: inner } => inner,
                _ => break,
            };
        }
        if let Some(anchor) = self.original.local_read_anchor(instance.definition, expression.id, b)? {
            let expected = self.anchored_local(instance, function, anchor, b)?;
            let (block, index, producer) = self.defining(function, value, b)?;
            if let InstructionKind::TakeLocal { local } = producer.kind {
                self.prepared_owner_source(function, expected, local, (block, index), b)
                    .map_err(|mut errors| {
                        errors[0].notes.push("resumable enum field selects another original owning binder".into());
                        errors
                    })?;
                return Ok(());
            }
            if matches!(producer.kind, InstructionKind::Load(local) if local == expected) {
                return Ok(());
            }
            return Err(fail("resumable enum field selects another original binder"));
        }
        for node in [written, expression.id] {
            if let Some(source) = self.original_subject_value(instance, function, node, b)?
                && self.operation_operand_matches(function, value, source, b)?
            {
                return Ok(());
            }
        }
        Err(fail("resumable enum field lacks its exact original operand occurrence"))
    }
}

struct Point<'a> {
    function: &'a TypedFunction,
    block: &'a BasicBlock,
    instruction: usize,
    edge: SemanticSiteEdge,
    initialization: &'a crate::Initialization,
}

pub(super) fn available_before(
    function: &TypedFunction,
    at: (BlockId, usize, SemanticSiteEdge),
    value: ValueId,
    b: &mut Budget,
) -> CompileResult<()> {
    let initialization = crate::shared_initialization(function, b)?;
    let block = function
        .blocks
        .get(at.0.0 as usize)
        .filter(|block| block.id == at.0)
        .ok_or_else(|| fail("implicit read has no actual emission boundary"))?;
    Point {
        function,
        block,
        instruction: at.1,
        edge: at.2,
        initialization: &initialization,
    }
    .value(value, b)?;
    Ok(())
}

/// Candidate occurrence selection uses the same paid dominance relation as
/// mandatory operands. An unavailable candidate is a mismatch; malformed CFG
/// identities and resource exhaustion remain errors.
pub(super) fn value_available_before(function: &TypedFunction,
    at: (BlockId, usize, SemanticSiteEdge), value: ValueId, b: &mut Budget)
    -> CompileResult<bool> {
    let initialization = crate::shared_initialization(function, b)?;
    let block = function.blocks.get(at.0.0 as usize)
        .filter(|block| block.id == at.0 && at.1 <= block.instructions.len())
        .ok_or_else(|| fail("candidate occurrence has no actual emission boundary"))?;
    Ok(Point { function, block, instruction: at.1, edge: at.2,
        initialization: &initialization }.unavailable(value, b)?.is_none())
}

pub(super) fn initialized_local_before(
    function: &TypedFunction,
    at: (BlockId, usize),
    local: LocalId,
    b: &mut Budget,
) -> CompileResult<()> {
    let initialization = crate::shared_initialization(function, b)?;
    let block = function.blocks.get(at.0.0 as usize)
        .filter(|block| block.id == at.0 && at.1 <= block.instructions.len())
        .ok_or_else(|| fail("resource source has no exact initialized local boundary"))?;
    Point { function, block, instruction: at.1, edge: SemanticSiteEdge::Before,
        initialization: &initialization }.local(local, b)?;
    Ok(())
}

impl Point<'_> {
    fn local(&self, local: LocalId, b: &mut Budget) -> CompileResult<&Type> {
        b.spend(1, WORK)?;
        let declaration = self
            .function
            .locals
            .get(local.0 as usize)
            .filter(|declaration| declaration.id == local)
            .ok_or_else(|| fail("semantic operand selects an absent actual local"))?;
        let plan = self
            .initialization
            .blocks
            .get(self.block.id.0 as usize)
            .ok_or_else(|| fail("semantic local has no actual initialization state"))?;
        b.lookup(plan.initialized_in.len(), WORK)?;
        let mut initialized = plan.initialized_in.contains(&local);
        let end = self
            .instruction
            .saturating_add(usize::from(self.edge == SemanticSiteEdge::After));
        for instruction in self.block.instructions.iter().take(end) {
            b.spend(1, WORK)?;
            match &instruction.kind {
                InstructionKind::UnpackVariant {
                    local: source,
                    fields,
                    ..
                }
                | InstructionKind::UnpackTuple {
                    local: source,
                    fields,
                } => {
                    b.spend(fields.len(), WORK)?;
                    if *source == local {
                        initialized = false;
                    }
                    if fields.contains(&local) {
                        initialized = true;
                    }
                }
                kind => match crate::incremental::Operation::of(kind) {
                    crate::incremental::Operation::Store(actual) if actual == local => {
                        initialized = true
                    }
                    crate::incremental::Operation::Take(actual)
                    | crate::incremental::Operation::Kill(actual)
                        if actual == local =>
                    {
                        initialized = false
                    }
                    _ => {}
                },
            }
        }
        if !initialized {
            return Err(fail(
                "semantic local operand has no initialized occupant at its actual site",
            ));
        }
        Ok(&declaration.ty)
    }

    fn value(&self, value: ValueId, b: &mut Budget) -> CompileResult<&Type> {
        let ty = self.function.values.get(value.0 as usize)
            .ok_or_else(|| fail("semantic operand selects an absent actual SSA value"))?;
        if let Some(reason) = self.unavailable(value, b)? { return Err(fail(reason)); }
        Ok(ty)
    }
    fn unavailable(&self, value: ValueId, b: &mut Budget) -> CompileResult<Option<&'static str>> {
        if self.function.values.get(value.0 as usize).is_none() {
            return Err(fail("semantic operand selects an absent actual SSA value"));
        }
        let definition = crate::shared_definition(self.function, value, b)?;
        let (block, ordinal) = (definition.block, definition.instruction);
        if block == self.block.id {
            if ordinal < self.instruction
                || (ordinal == self.instruction && self.edge == SemanticSiteEdge::After)
            {
                return Ok(None);
            }
            return Ok(Some("semantic SSA operand is read before its actual definition"));
        }
        // An actual path to this site which avoids the definition disproves
        // dominance. Walk real CFG edges; an asserted type is not a def/use edge.
        b.spend(1, WORK)?;
        let mut seen = BTreeSet::new();
        let mut pending = vec![self.function.entry];
        while let Some(current) = pending.pop() {
            b.spend(1, WORK)?;
            if current == block {
                continue;
            }
            b.lookup(seen.len(), WORK)?;
            if !seen.insert(current) {
                continue;
            }
            if current == self.block.id {
                return Ok(Some("semantic SSA operand does not dominate its actual use"));
            }
            let actual = self
                .function
                .blocks
                .get(current.0 as usize)
                .filter(|actual| actual.id == current)
                .ok_or_else(|| fail("semantic SSA dominance encounters an invalid actual block"))?;
            let edges = match actual.terminator {
                Terminator::Jump(next) => [Some(next), None],
                Terminator::Branch { then_block, else_block, .. } => [Some(then_block), Some(else_block)],
                _ => continue,
            };
            let count = 1 + usize::from(edges[1].is_some());
            let relocation = if pending.capacity() - pending.len() < count { pending.len() } else { 0 };
            b.spend(relocation.saturating_add(count), WORK)?;
            pending.try_reserve(count).map_err(|_| fail("semantic dominance worklist allocation failed"))?;
            pending.extend(edges.into_iter().flatten());
        }
        if !self
            .initialization
            .reachable
            .get(block.0 as usize)
            .copied()
            .unwrap_or(false)
        {
            return Ok(Some("semantic SSA definition is not on an actual reachable path"));
        }
        Ok(None)
    }
}

impl Check<'_, '_> {
    fn ffi_literal_occurrence(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        subject: &SemanticOperand,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let Some(original) = self.original.ffi_literal_at(instance, site, self.inventory, b)? else {
            return Ok(());
        };
        let SemanticOperand::Value(output) = subject else {
            return Err(fail("original foreign constant has no emitted SSA"));
        };
        let (_, _, instruction) = self.defining(function, *output, b)?;
        let InstructionKind::Constant(actual) = &instruction.kind else {
            return Err(fail(
                "original foreign constant is replaced by another actual operation",
            ));
        };
        if let (Constant::WideString(original), Constant::WideString(actual)) = (original, actual) {
            b.spend(original.len().saturating_add(actual.len()), WORK)?;
        }
        if original != actual {
            return Err(fail(
                "actual foreign constant changes the original literal payload",
            ));
        }
        Ok(())
    }

    fn inputs(
        &self,
        arguments: &[ValueId],
        point: &Point<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let before = Point {
            function: point.function,
            block: point.block,
            instruction: point.instruction,
            edge: SemanticSiteEdge::Before,
            initialization: point.initialization,
        };
        b.spend(arguments.len(), WORK)?;
        for value in arguments {
            before.value(*value, b)?;
        }
        Ok(())
    }

    fn image_operands(
        &self,
        instance: &SemanticInstance,
        value: &SemanticTypeValue,
        point: &Point<'_>,
        result: Option<ValueId>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let SemanticTypeValue::Resolved { classes, .. } = value else {
            return Err(fail("class image retains an unresolved Core type"));
        };
        b.spend(classes.len(), WORK)?;
        for expression in classes {
            if let SemanticClassExpr::OuterOccurrence { operand, .. }
            | SemanticClassExpr::IntrinsicClass { operand, .. } = expression
            {
                match operand {
                    SemanticOperand::Value(value) if Some(*value) == result => {}
                    SemanticOperand::Value(value) => {
                        point.value(*value, b)?;
                    }
                    SemanticOperand::Local(local) => {
                        point.local(*local, b)?;
                    }
                    SemanticOperand::Parameter(parameter) => {
                        let local = point.function.parameters.get(*parameter as usize)
                            .ok_or_else(|| fail("parameter class image exceeds the actual input table"))?;
                        point.local(*local, b)?;
                        // Reuse the original signature/actual selector proof;
                        // an initialized input is not its own class image.
                        if self.parameter_address_claim(instance, value, b, 0)?.is_none() {
                            return Err(unknown("parameter class image needs its complete whole-value selector transport"));
                        }
                    }
                    SemanticOperand::Environment(_) | SemanticOperand::TypeArgument { .. } => {
                        // This selector reads a complete checked logical type
                        // in the real creator/formal scope; it is not a head
                        // permission. The subject below is still matched to
                        // its actual ReadCapture/SSA/initialized occupant, and
                        // every creator separately proves its real captures.
                        crate::semantic::value::class(instance, expression, self.inventory, b, 0)?;
                    }
                    _ => {
                        return Err(unknown(
                            "capture class image needs its actual emitted value or initialized occupant",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn operand<'p>(
        &'p self,
        instance: &'p SemanticInstance,
        operand: &SemanticOperand,
        point: &'p Point<'_>,
        b: &mut Budget,
    ) -> CompileResult<&'p Type> {
        b.spend(1, WORK)?;
        let ty = match operand {
            SemanticOperand::Value(value) => point.value(*value, b)?,
            SemanticOperand::Local(local) => point.local(*local, b)?,
            SemanticOperand::Parameter(index) => {
                let local = point
                    .function
                    .parameters
                    .get(*index as usize)
                    .ok_or_else(|| {
                        fail("semantic parameter operand is outside its original header")
                    })?;
                point.local(*local, b)?
            }
            SemanticOperand::Capture(index) => {
                let closure = point
                    .function
                    .environment
                    .ok_or_else(|| fail("semantic capture operand has no actual environment"))?;
                self.module
                    .closures
                    .get(closure.0 as usize)
                    .filter(|definition| definition.id == closure)
                    .and_then(|definition| definition.captures.get(*index as usize))
                    .ok_or_else(|| {
                        fail("semantic capture operand is outside its actual environment")
                    })?
            }
            SemanticOperand::TypeArgument { scope, ordinal } => {
                let context = self.ancestor(instance, *scope, b)?;
                return match context.actuals.get(*ordinal as usize) {
                    Some(SemanticActual::Type(value)) => self.value_type_ref(value, b),
                    _ => Err(fail(
                        "semantic type argument operand changes its original mixed kind",
                    )),
                };
            }
            SemanticOperand::Environment(ordinal) => {
                return match instance
                    .environment
                    .as_ref()
                    .and_then(|environment| environment.actuals.get(*ordinal as usize))
                {
                    Some(SemanticActual::Type(value)) => self.value_type_ref(value, b),
                    _ => Err(fail("semantic environment operand has no sealed type slot")),
                };
            }
            _ => {
                return Err(unknown(
                    "semantic receiver or unresolved operand needs its complete actual context",
                ));
            }
        };
        // instances() admitted every physical local/SSA type. Address roots
        // retain their stricter observation rule, and captures are a separate
        // directory. The point above still proves this occurrence is live.
        if matches!(ty, Type::Address { .. }) || matches!(operand, SemanticOperand::Capture(_)) {
            self.ordinary_type(ty, b)?;
        }
        Ok(ty)
    }

    pub(super) fn result(
        &self,
        instruction: &Instruction,
        function: &TypedFunction,
        expected: &Type,
    ) -> CompileResult<()> {
        match (instruction.result, expected) {
            (None, Type::Unit) => Ok(()),
            (Some(value), _) if function.values.get(value.0 as usize) == Some(expected) => Ok(()),
            _ => Err(fail(
                "actual call or constructor result differs from its original instantiated type",
            )),
        }
    }

    fn arguments<'a>(
        &self,
        function: &TypedFunction,
        arguments: &[ValueId],
        expected: impl ExactSizeIterator<Item = &'a Type>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(arguments.len().saturating_add(expected.len()), WORK)?;
        if arguments.len() != expected.len() {
            return Err(fail("actual call changes its original argument arity"));
        }
        for (actual, expected) in arguments.iter().zip(expected) {
            let Some(operand) = function.values.get(actual.0 as usize) else {
                return Err(fail("actual call argument is absent from its function"));
            };
            if !geometry::same_type(operand, expected, b)? {
                return Err(fail(
                    "actual call argument differs from the complete instantiated parameter",
                ));
            }
        }
        Ok(())
    }

    fn closed_call_targets(
        &self, callee: FunctionId, definition: SemanticDefinitionId, formals: &[SemanticFormal],
        arguments: &[value::Argument], b: &mut Budget,
    ) -> CompileResult<bool> {
        if arguments.is_empty() { return Ok(false); }
        b.spend(arguments.len(), WORK)?;
        let mut expected = Vec::with_capacity(arguments.len());
        for (formal, argument) in formals.iter().zip(arguments) {
            let (SemanticFormal::Type { .. }, value::Argument::Type(value)) = (formal, argument) else { return Ok(false); };
            let Some(id) = value.closed_geometry_identity(b)? else { return Ok(false); };
            expected.push(id);
        }
        let key = (callee, definition);
        b.lookup(self.closed_call_targets.borrow().len(), WORK)?;
        if let Some(contract) = self.closed_call_targets.borrow().get(&key) {
            b.spend(expected.len(), WORK)?;
            return Ok(contract.as_ref() == Some(&expected));
        }
        let worlds = self.function_worlds(callee, b)?;
        let mut common: Option<Vec<usize>> = None;
        let mut closed = !worlds.is_empty();
        'worlds: for target in worlds {
            b.spend(1, WORK)?;
            if target.definition != definition || target.actuals.len() != formals.len()
                || target.environment.is_some() { closed = false; break; }
            b.spend(target.actuals.len(), WORK)?;
            let mut actual = Vec::with_capacity(target.actuals.len());
            for argument in &target.actuals {
                let SemanticActual::Type(input @ SemanticTypeValue::Resolved { life, classes, .. }) = argument else {
                    closed = false; break 'worlds;
                };
                if !empty(life) || !classes.is_empty() { closed = false; break 'worlds; }
                let value = self.resolved(target, input, b, 0)?;
                let Some(id) = value.closed_geometry_identity(b)? else { closed = false; break 'worlds; };
                actual.push(id);
            }
            if let Some(common) = &common {
                b.spend(actual.len(), WORK)?;
                if *common != actual { closed = false; break; }
            } else {
                common = Some(actual);
            }
        }
        if !closed { common = None; }
        b.spend(expected.len(), WORK)?;
        let matched = common.as_ref() == Some(&expected);
        b.lookup(self.closed_call_targets.borrow().len(), WORK)?;
        self.closed_call_targets.borrow_mut().insert(key, common);
        Ok(matched)
    }

    pub(super) fn body_call_actual(
        &self,
        target: &SemanticInstance,
        ordinal: usize,
        formal: &SemanticFormal,
        expected: &crate::semantic::value::Argument,
        actual: &crate::semantic::value::Argument,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        use crate::semantic::value::{Argument, Class};
        b.spend(1, WORK)?;
        let ordinal =
            u32::try_from(ordinal).map_err(|_| fail("original call mixed ordinal overflows"))?;
        Ok(match (formal, expected, actual) {
            (
                SemanticFormal::Lifetime,
                Argument::Life(_),
                Argument::Life(Class::Formal(owner, index)),
            ) => *owner == target.id && *index == ordinal,
            (SemanticFormal::Type { .. }, Argument::Type(expected), Argument::Type(actual)) => {
                // The target is checked in its own rigid declaration frame.
                // Reconstruct every intrinsic coordinate, including phantom
                // classes; caller images never become target premises.
                b.spend(
                    expected.classes.len().saturating_add(actual.classes.len()),
                    WORK,
                )?;
                if expected.classes.len() != expected.life.binder_count as usize
                    || actual.classes.len() != expected.classes.len()
                {
                    return Ok(false);
                }
                let mut classes = Vec::new();
                for child in 0..expected.life.binder_count {
                    self.push_value(
                        &mut classes,
                        Class::TypeArgument(target.id, ordinal, child),
                        b,
                    )?;
                }
                let rigid = expected.with_classes(classes, b)?;
                actual.equal_paid(&rigid, b)?
            }
            (SemanticFormal::Const { .. }, Argument::Const { .. }, Argument::Const { .. }) => {
                self.capture_arguments_equal(expected, actual, b)?
            }
            _ => false,
        })
    }

    fn original_claim(
        &self,
        instance: &SemanticInstance,
        claim: &SemanticTypeValue,
        original: &crate::semantic::value::Value,
        b: &mut Budget,
    ) -> CompileResult<value::Value> {
        let actual = self.resolved(instance, claim, b, 0)?;
        if self.capture_values_equal(&actual, original, b)? {
            return Ok(actual);
        }
        crate::dyn_metadata::recipe(claim, b)?;
        crate::dyn_metadata::ty(&original.ty, b)?;
        crate::dyn_metadata::shape(&original.life, b)?;
        b.spend(
            original
                .classes
                .len()
                .saturating_add(actual.classes.len())
                .saturating_mul(128)
                .saturating_add(256),
            WORK,
        )?;
        let mut errors = fail("actual semantic claim changes its complete original class value");
        errors[0].notes.push(format!("independent original value {original:?}; independently resolved actual claim {actual:?}; input claim {claim:?}"));
        Err(errors)
    }

    fn operation(
        &self,
        instance: &SemanticInstance,
        site: &SemanticSite,
        recipe: &SemanticSiteRecipe,
        point: &Point<'_>,
        ct_trees: &mut BTreeMap<(BlockId, usize), usize>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let instruction = point.block.instructions.get(point.instruction);
        match (&recipe.kind, &site.action) {
            (
                SemanticSiteRecipeKind::Construct { result: recipe },
                SemanticSiteAction::Construct {
                    result: SemanticOperand::Value(result),
                    value,
                },
            ) => {
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                let expected = &original_value.ty;
                let prepared_claim = if self.uses_full_value(&original_value.ty, b)? {
                    Some(self.original_claim(instance, value, &original_value, b)?)
                } else { None };

                self.image_operands(instance, value, point, Some(*result), b)?;
                if !geometry::same_type(self.captured_value(
                    instance,
                    value,
                    prepared_claim,
                    &SemanticOperand::Value(*result),
                    (point.block.id, point.instruction, SemanticSiteEdge::After),
                    false,
                    b,
                )?.ty(), expected, b)?
                    || instruction.and_then(|actual| actual.result) != Some(*result)
                {
                    return Err(fail(
                        "actual construction changes its original whole type or result",
                    ));
                }
                self.result(
                    instruction.ok_or_else(|| fail("actual construction instruction is absent"))?,
                    point.function,
                    expected,
                )?;
                if let Some(constructor) = self.original.enum_constructor(
                    instance, site.recipe, self.program, self.inventory, b,
                )? {
                    let Some(InstructionKind::MakeVariant { variant, fields }) =
                        instruction.map(|instruction| &instruction.kind)
                    else {
                        return Err(fail("original enum construction changes its actual opcode"));
                    };
                    if constructor.result.ty != *expected {
                        return Err(fail("actual enum formation changes its original declaration result"));
                    }
                    if *variant != constructor.variant || fields.len() != constructor.payload.len() {
                        return Err(fail("actual enum formation changes its original variant or payload arity"));
                    }
                    self.inputs(fields, point, b)?;
                    self.resumable_enum_operands(instance, point.function, constructor.node, &constructor.result.ty, fields, b)?;
                    for (actual, expected) in fields.iter().zip(&constructor.payload) {
                        b.spend(1, WORK)?;
                        if point.function.values.get(actual.0 as usize) != Some(&expected.ty) {
                            return Err(fail("actual enum formation changes its complete original payload type"));
                        }
                        if self.uses_full_value(&expected.ty, b)? {
                            let value = self.ssa_value(instance, point.function, *actual, b, 0)?;
                            if !self.capture_values_equal(&value, expected, b)? {
                                return Err(fail("actual enum formation changes its original payload class occurrence"));
                            }
                        }
                    }
                }
            }
            (
                SemanticSiteRecipeKind::TypeUse { expected: recipe },
                SemanticSiteAction::TypeUse { subject, expected },
            ) => {
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                let expected_type = &original_value.ty;
                if let SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Expression(node),
                } = recipe
                    && let Some(read) = self.original.resource_field_source(
                        instance, *node, self.program, self.inventory, b,
                    )?
                {
                    self.checked_resource_field_read(instance, point.function, subject, &read, b)?;
                }
                if let SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Expression(node),
                } = recipe
                    && let Some(transfer) = self.original.prepared_owner_transfer(
                        instance, *node, self.program, self.inventory, b, 0,
                    )?
                    && matches!(transfer.root.ty, Type::Tuple(_))
                {
                    let SemanticOperand::Value(value) = subject else {
                        return Err(fail("original tuple move has no actual consuming SSA result"));
                    };
                    self.checked_owned_place(instance, point.function, *value, &transfer, b)?;
                }
                if super::ct_data::product(expected_type) && self.ct_data_recipe(instance, recipe, b)? {
                    let SemanticOperand::Value(value) = subject else {
                        return Err(fail("reified compile-time data has no actual result value"));
                    };
                    let start = self.ct_data_value(
                        point.function, (point.block.id, point.instruction, point.edge),
                        *value, &original_value, b,
                    )?;
                    let block = point.block.id;
                    let end = point.instruction;
                    b.lookup(ct_trees.len(), WORK)?;
                    b.lookup(ct_trees.len(), WORK)?;
                    if ct_trees.range((block, start)..(block, end)).next().is_some()
                        || ct_trees.range(..(block, start)).next_back()
                            .is_some_and(|(&(owner, _), &prior_end)| owner == block && prior_end > start)
                    {
                        return Err(fail("original compile-time results reuse an actual data producer"));
                    }
                    ct_trees.insert((block, start), end);
                }
                // A logical local use crosses the private shell only through
                // this original binding's real declaration/initializer witness.
                let actual_operand = if let SemanticOperand::Local(local) = subject
                    && matches!(expected_type, Type::Closure(_) | Type::Managed(_))
                    && let Some(slot) = self.binding_slot(instance, point.function, *local, b)? {
                    ObservedType::Reconstructed(self.persistent_slot_value(instance, point.function, &slot,
                        (point.block.id, point.instruction + usize::from(point.edge == SemanticSiteEdge::After)), b, 0)?)
                } else { ObservedType::Physical(self.operand(instance, subject, point, b)?) };
                let actual_operand = actual_operand.ty();
                let same_operand = geometry::same_type(actual_operand, expected_type, b)?;
                if !same_operand
                    && let Some((whole, error)) = self.original.return_injection(
                        instance,
                        site.recipe,
                        self.program,
                        self.inventory,
                        b,
                    )?
                {
                    let SemanticOperand::Value(output) = subject else {
                        return Err(fail("implicit return injection has no actual result SSA"));
                    };
                    let (block, index, producer) = self.defining(point.function, *output, b)?;
                    let InstructionKind::MakeErrorUnion { is_error, value } = producer.kind else {
                        return Err(fail(
                            "implicit return injection is replaced by another actual opcode",
                        ));
                    };
                    if is_error != error
                        || !geometry::same_type(actual_operand, &whole.ty, b)?
                        || !self.capture_values_equal(
                            &self.resolved(instance, expected, b, 0)?, &whole, b,
                        )?
                        || point.function.values.get(value.0 as usize) != Some(&original_value.ty)
                    {
                        return Err(fail(
                            "implicit return injection changes its original union, arm or complete payload",
                        ));
                    }
                    operations::available_before(
                        point.function,
                        (block, index, SemanticSiteEdge::Before),
                        value,
                        b,
                    )?;
                    if self.uses_full_value(&original_value.ty, b)?
                        && !self.capture_values_equal(
                            &self.ssa_value(instance, point.function, value, b, 0)?,
                            &original_value, b,
                        )?
                    {
                        return Err(fail(
                            "implicit return injection changes its original payload classes",
                        ));
                    }
                    self.image_operands(instance, expected, point, None, b)?;
                    self.function_reference(
                        instance,
                        site.recipe,
                        point.function,
                        &SemanticOperand::Value(value),
                        &original_value.ty,
                        b,
                    )?;
                    if let Some(bytes) =
                        self.original
                            .string_literal_at(instance, site.recipe, self.inventory, b)?
                    {
                        let (_, _, inner) = self.defining(point.function, value, b)?;
                        let InstructionKind::StaticUtf8 { bytes: actual } = &inner.kind else {
                            return Err(fail(
                                "implicit return injection replaces its original literal payload",
                            ));
                        };
                        b.spend(bytes.len().saturating_add(actual.len()), WORK)?;
                        if actual.as_slice() != bytes {
                            return Err(fail(
                                "implicit return injection changes original literal bytes",
                            ));
                        }
                    }
                    self.ffi_literal_occurrence(
                        instance,
                        site.recipe,
                        point.function,
                        &SemanticOperand::Value(value),
                        b,
                    )?;
                    if let Some(constructor) = self.original.enum_constructor(
                        instance,
                        site.recipe,
                        self.program,
                        self.inventory,
                        b,
                    )? && constructor.value_member
                    {
                        let (_, _, inner) = self.defining(point.function, value, b)?;
                        if !constructor.payload.is_empty()
                            || constructor.result.ty != original_value.ty
                            || !matches!(&inner.kind, InstructionKind::MakeVariant { variant, fields } if *variant == constructor.variant && fields.is_empty())
                        {
                            return Err(fail(
                                "implicit return injection replaces its original variant payload",
                            ));
                        }
                    }
                    return Ok(());
                }
                let prepared_claim = if self.uses_full_value(&original_value.ty, b)? {
                    Some(self.original_claim(instance, expected, &original_value, b)?)
                } else { None };

                self.image_operands(instance, expected, point, None, b)?;
                self.function_reference(
                    instance,
                    site.recipe,
                    point.function,
                    subject,
                    expected_type,
                    b,
                )?;
                if let Some(constructor) = self.original.enum_constructor(
                    instance,
                    site.recipe,
                    self.program,
                    self.inventory,
                    b,
                )? && constructor.value_member
                {
                    let SemanticOperand::Value(value) = subject else {
                        return Err(fail("original enum member has no emitted SSA"));
                    };
                    let (_, _, actual) = self.defining(point.function, *value, b)?;
                    if !constructor.payload.is_empty()
                        || &constructor.result.ty != expected_type
                        || !matches!(&actual.kind, InstructionKind::MakeVariant { variant, fields }
                            if *variant == constructor.variant && fields.is_empty())
                    {
                        return Err(fail(
                            "actual enum member changes its original declaration, variant or payload",
                        ));
                    }
                }
                if let Some(bytes) =
                    self.original
                        .string_literal_at(instance, site.recipe, self.inventory, b)?
                {
                    let SemanticOperand::Value(value) = subject else {
                        return Err(fail("original string literal has no emitted SSA"));
                    };
                    let (_, _, literal) = self.defining(point.function, *value, b)?;
                    let InstructionKind::StaticUtf8 { bytes: actual } = &literal.kind else {
                        return Err(fail(
                            "original string literal is replaced by another actual operation",
                        ));
                    };
                    b.spend(bytes.len().saturating_add(actual.len() + 1), WORK)?;
                    if actual.as_slice() != bytes {
                        return Err(fail(
                            "actual UTF8 bytes differ from the complete original literal",
                        ));
                    }
                }
                self.ffi_literal_occurrence(instance, site.recipe, point.function, subject, b)?;
                let captured = self.captured_value(
                    instance,
                    expected,
                    prepared_claim,
                    subject,
                    (point.block.id, point.instruction, point.edge),
                    false,
                    b,
                )?;
                let captured = captured.ty();
                if !same_operand || !geometry::same_type(expected_type, captured, b)? {
                    crate::dyn_metadata::ty(actual_operand, b)?;
                    crate::dyn_metadata::ty(captured, b)?;
                    crate::dyn_metadata::recipe(expected, b)?;
                    b.spend(
                        original_value
                            .classes
                            .len()
                            .saturating_mul(128)
                            .saturating_add(256),
                        WORK,
                    )?;
                    let mut errors = fail(
                        "actual type-use occupant differs from its complete original ascription",
                    );
                    errors[0].notes.push(format!("original whole value {original_value:?}; emitted claim {expected:?}; captured operand type {captured:?}; actual operand type {actual_operand:?}"));
                    return Err(errors);
                }
            }
            (
                SemanticSiteRecipeKind::DeclareLocal { expected: recipe },
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                },
            ) => {
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                let expected_type = &original_value.ty;
                let prepared_claim = if self.uses_full_value(&original_value.ty, b)? {
                    Some(self.original_claim(instance, expected, &original_value, b)?)
                } else { None };

                if let Some(slot) = self.binding_slot(instance, point.function, *local, b)? {
                    if slot.declaration() != (point.block.id, point.instruction)
                        || point.edge != SemanticSiteEdge::After
                        || !slot.logical().equal_paid(&original_value, b)? {
                        return Err(fail("persistent declaration changes its exact original boundary or whole value"));
                    }
                    let actual = self.ssa_value(instance, point.function, slot.initial(), b, 0)?;
                    if !actual.equal_paid(&original_value, b)? {
                        return Err(fail("persistent declaration initializer changes its complete original child"));
                    }
                    // The original declaration is logical; its private
                    // shell is initialized by the witnessed immediate Store.
                    let images = self.image_heads(instance, expected,
                        (slot.initial_store().0, slot.initial_store().1, SemanticSiteEdge::After), b)?;
                    self.match_heads(instance, &images, &SemanticOperand::Value(slot.initial()),
                        (slot.initial_store().0, slot.initial_store().1, SemanticSiteEdge::Before), b)?;
                    return Ok(());
                }
                self.image_operands(instance, expected, point, None, b)?;
                if !geometry::same_type(self.captured_value(
                    instance,
                    expected,
                    prepared_claim,
                    &SemanticOperand::Local(*local),
                    (point.block.id, point.instruction, point.edge),
                    true,
                    b,
                )?.ty(), expected_type, b)?
                    || point
                        .function
                        .locals
                        .get(local.0 as usize)
                        .filter(|actual| actual.id == *local)
                        .map(|actual| &actual.ty)
                        != Some(expected_type)
                    || !matches!(instruction.map(|actual| &actual.kind), Some(InstructionKind::StorageLive(actual)) if actual == local)
                {
                    return Err(fail(
                        "actual local declaration changes its complete original constraint",
                    ));
                }
                self.affine_field_initializer(instance, site.recipe, *local, expected_type, point, b)?;
                self.check_reference_declaration(instance, point.function, *local, b)?;
            }
            (SemanticSiteRecipeKind::OriginalCall, SemanticSiteAction::BodyCall { actuals }) => {
                if self
                    .code_operation(
                        instance,
                        site.recipe,
                        point.function,
                        point.block.id,
                        point.instruction,
                        actuals,
                        b,
                    )?
                    .is_some()
                {
                    return Ok(());
                }
                if self
                    .trait_operation(
                        instance,
                        site.recipe,
                        point.function,
                        point.block.id,
                        point.instruction,
                        actuals,
                        b,
                    )?
                    .is_some()
                {
                    return Ok(());
                }
                if self
                    .abstract_operation(
                        instance,
                        point.function,
                        point.block.id,
                        point.instruction,
                        b,
                    )?
                    .is_some()
                {
                    return Ok(());
                }
                if let Some(constructor) = self.original.enum_constructor(
                    instance,
                    site.recipe,
                    self.program,
                    self.inventory,
                    b,
                )? {
                    let instruction = instruction.ok_or_else(|| {
                        fail("original enum construction has no actual instruction")
                    })?;
                    let InstructionKind::MakeVariant { variant, fields } = &instruction.kind else {
                        return Err(fail(
                            "original enum constructor is replaced by another opcode",
                        ));
                    };
                    if *variant != constructor.variant
                        || fields.len() != constructor.payload.len()
                        || actuals.len() != constructor.arguments.len()
                    {
                        return Err(fail(
                            "actual enum constructor changes its original member or full arity",
                        ));
                    }
                    self.inputs(fields, point, b)?;
                    self.resumable_enum_operands(instance, point.function, constructor.node, &constructor.result.ty, fields, b)?;
                    for (actual, expected) in fields.iter().zip(&constructor.payload) {
                        b.spend(1, WORK)?;
                        if point.function.values.get(actual.0 as usize) != Some(&expected.ty) {
                            return Err(fail(
                                "actual enum payload differs from its complete original member type",
                            ));
                        }
                        if self.uses_full_value(&expected.ty, b)? {
                            let value = self.ssa_value(instance, point.function, *actual, b, 0)?;
                            if !self.capture_values_equal(&value, expected, b)? {
                                return Err(fail(
                                    "actual enum payload changes its original intrinsic class occurrence",
                                ));
                            }
                        }
                    }
                    for (actual, expected) in actuals.iter().zip(&constructor.arguments) {
                        if !self.capture_arguments_equal(
                            &self.actual_value(instance, actual, b, 0)?, expected, b,
                        )? {
                            return Err(fail(
                                "actual enum construction changes its ordered original mixed arguments",
                            ));
                        }
                    }
                    self.result(instruction, point.function, &constructor.result.ty)?;
                    return Ok(());
                }
                if let Some(intrinsic) =
                    self.original
                        .intrinsic(instance, site.recipe, self.inventory, b)?
                {
                    match intrinsic {
                        original::sealed::Intrinsic::TakeStored => {
                            let Some(op) = instruction else { return Err(fail("move_out lacks its actual instruction")); };
                            let InstructionKind::LoadStoredBorrow { storage, path, mode: StoredBorrowLoad::Take } = &op.kind
                                else { return Err(fail("move_out must consume the actual declared stored slot")); };
                            let source = self.inventory.original_index(instance.definition, b)?;
                            let Some(SemanticOriginalNode::Expression(node)) = source.site_node(site.recipe, b)? else {
                                return Err(fail("move_out lacks its original call"));
                            };
                            let CoreExprKind::Call { arguments, .. } = &source.get_expr(node, b)?.kind else {
                                return Err(fail("move_out changes its original call operation"));
                            };
                            let [field] = arguments.as_slice() else { return Err(fail("move_out changes its original field arity")); };
                            if !actuals.is_empty() || !self.original_stored_slot(instance, point.function, *storage, path, field, b)? {
                                return Err(fail("move_out changes its complete original stored field"));
                            }
                            let result = op.result.ok_or_else(|| fail("move_out has no actual result"))?;
                            let selected = self.ssa_value(instance, point.function, result, b, 0)?;
                            let expected = self.original.intrinsic_output(instance, site.recipe, self.program, self.inventory, b)?;
                            if !selected.equal_paid(&expected, b)? { return Err(fail("move_out changes its complete original stored lifetime")); }
                            return Ok(());
                        }
                        original::sealed::Intrinsic::OwnedDyn => {
                            let output = self.dyn_owned_operation(
                                instance,
                                site.recipe,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual owning Dyn construction has no instruction")
                                })?,
                                point.function,
                                &output.ty,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::CCallback => {
                            let outputs = self.original.intrinsic_inputs(
                                instance,
                                site.recipe,
                                self.program,
                                self.inventory,
                                b,
                            )?;
                            let [output] = outputs.as_slice() else {
                                return Err(fail(
                                    "original callback conversion loses its complete output scheme",
                                ));
                            };
                            if !actuals.is_empty()
                                || !matches!(output.ty, Type::ForeignCallback { .. })
                                || !empty(&output.life)
                                || !output.classes.is_empty()
                            {
                                return Err(fail(
                                    "actual callback conversion changes its original fixed ABI scheme",
                                ));
                            }
                            let instruction = instruction.ok_or_else(|| {
                                fail("original callback conversion has no actual instruction")
                            })?;
                            let result = instruction.result.ok_or_else(|| {
                                fail("actual callback conversion has no code SSA")
                            })?;
                            self.function_reference(
                                instance,
                                site.recipe,
                                point.function,
                                &SemanticOperand::Value(result),
                                &output.ty,
                                b,
                            )?;
                            self.result(instruction, point.function, &output.ty)?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::StoreNew
                        | original::sealed::Intrinsic::StoreReserve
                        | original::sealed::Intrinsic::StorePush
                        | original::sealed::Intrinsic::StorePop
                        | original::sealed::Intrinsic::StoreLen
                        | original::sealed::Intrinsic::StoreCapacity
                        | original::sealed::Intrinsic::StoreAt
                        | original::sealed::Intrinsic::StoreAtMut => {
                            self.store_operation(
                                instance,
                                site.recipe,
                                intrinsic,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::TaskYield => {
                            let instruction = instruction.ok_or_else(|| {
                                fail("original task_yield has no actual instruction")
                            })?;
                            if !actuals.is_empty()
                                || point.function.call_kind != CallKind::Suspend
                                || !matches!(instruction.kind, InstructionKind::TaskYield)
                            {
                                return Err(fail(
                                    "actual operation changes the original task_yield scheme or Suspend ABI",
                                ));
                            }
                            self.result(instruction, point.function, &Type::Unit)?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::TaskAwait
                        | original::sealed::Intrinsic::TaskCreate
                        | original::sealed::Intrinsic::TaskCreateOwned
                        | original::sealed::Intrinsic::TaskPoll
                        | original::sealed::Intrinsic::TaskCancel
                        | original::sealed::Intrinsic::TaskPollClose
                        | original::sealed::Intrinsic::TaskFinish) => {
                            let output = self.task_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual Task operation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::MutexCreate
                        | original::sealed::Intrinsic::MutexLock
                        | original::sealed::Intrinsic::MutexValue
                        | original::sealed::Intrinsic::SyncCellScope(_)
                        | original::sealed::Intrinsic::SyncCellOp(..)
                        | original::sealed::Intrinsic::SyncCellCompareExchange
                        | original::sealed::Intrinsic::ThreadScope) => {
                            let output = self.mutex_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual synchronization operation has no instruction")
                                })?,
                                point.function,
                                &output.ty,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::BytesNew
                        | original::sealed::Intrinsic::BytesReserve
                        | original::sealed::Intrinsic::BytesPush
                        | original::sealed::Intrinsic::BytesTruncate
                        | original::sealed::Intrinsic::BytesLen
                        | original::sealed::Intrinsic::BytesView { .. }
                        | original::sealed::Intrinsic::BytesCapacity) => {
                            let output = self.buffer_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual ByteBuffer operation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::ScopedForeignCallback => {
                            let instruction = instruction.ok_or_else(|| {
                                fail("actual scoped callback has no instruction")
                            })?;
                            self.scoped_foreign_callback_operation(
                                instance, site.recipe, point.function, point.block.id,
                                point.instruction, instruction, actuals, b,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::Register
                        | original::sealed::Intrinsic::RegisterAsync) => {
                            let output = self.registration_start_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual registration installation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::AsyncRequest
                        | original::sealed::Intrinsic::AsyncPoll
                        | original::sealed::Intrinsic::AsyncFinish
                        | original::sealed::Intrinsic::AsyncFinishChecked
                        | original::sealed::Intrinsic::AsyncValid
                        | original::sealed::Intrinsic::RegistrationValid
                        | original::sealed::Intrinsic::TaskValid
                        | original::sealed::Intrinsic::RegistrationClose) => {
                            let output = self.registration_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual registration operation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::SliceLength => {
                            self.slice_length_operation(
                                instance,
                                site.recipe,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::NumericConvert(number) => {
                            let instruction = instruction.ok_or_else(|| {
                                fail("original conversion has no actual instruction")
                            })?;
                            let (value, target, integer) = match &instruction.kind {
                                InstructionKind::ConvertNumber { value, target } => {
                                    (value, target, false)
                                }
                                InstructionKind::ConvertInteger { value, target } => {
                                    (value, target, true)
                                }
                                _ => {
                                    return Err(fail(
                                        "original numeric conversion is replaced by another actual opcode",
                                    ));
                                }
                            };
                            if !actuals.is_empty() || *target != number.ty() {
                                return Err(fail(
                                    "actual numeric conversion changes its closed target or generic arity",
                                ));
                            }
                            self.inputs(&[*value], point, b)?;
                            let expected = self.original.intrinsic_inputs(
                                instance,
                                site.recipe,
                                self.program,
                                self.inventory,
                                b,
                            )?;
                            if expected.len() != 1
                                || point.function.values.get(value.0 as usize)
                                    != Some(&expected[0].ty)
                            {
                                return Err(fail(
                                    "actual numeric input differs from its checked original numeric carrier",
                                ));
                            }
                            let input =
                                point.function.values.get(value.0 as usize).ok_or_else(|| {
                                    fail("actual numeric conversion input is absent")
                                })?;
                            if !matches!(
                                input,
                                Type::I8
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
                            ) {
                                return Err(fail(
                                    "actual numeric conversion input violates its original numeric premise",
                                ));
                            }
                            if integer
                                && (matches!(input, Type::F32 | Type::F64)
                                    || matches!(target, Type::F32 | Type::F64))
                            {
                                return Err(fail(
                                    "actual integer conversion changes the original mathematical carrier family",
                                ));
                            }
                            self.result(instruction, point.function, &number.ty())?;
                            return Ok(());
                        }
                        _ => {
                            return Err(unknown(
                                "actual core operation lacks its complete original scheme proof",
                            ));
                        }
                    }
                }
                let (original, closure, expected_actuals, operation) =
                    self.original
                        .call(instance, site.recipe, self.program, self.inventory, b)?;
                let instruction = instruction
                    .ok_or_else(|| fail("original BodyCall has no actual instruction"))?;
                let (callee, arguments, parameters, result) = match &instruction.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    }
                    | InstructionKind::SuspendCall {
                        function,
                        arguments,
                    }
                    | InstructionKind::ForeignCall {
                        function,
                        arguments,
                        ..
                    } if !closure => {
                        let callee = self
                            .module
                            .functions
                            .get(function.0 as usize)
                            .filter(|callee| callee.id == *function)
                            .ok_or_else(|| fail("actual original callee function is absent"))?;
                        let declaration = self.inventory.definition(original, b)?;
                        let SemanticDefinitionBody::FunctionWithBody {
                            signature, body, ..
                        } = &declaration.body
                        else {
                            return Err(fail(
                                "actual static call has no independently checked original scheme",
                            ));
                        };
                        let valid = match (&instruction.kind, body) {
                            (
                                InstructionKind::ForeignCall { .. },
                                SemanticOriginalFunctionBody::External,
                            ) => {
                                signature.call_kind == CallKind::Direct
                                    && callee.call_kind == CallKind::Direct
                                    && callee.foreign.is_some()
                            }
                            (
                                InstructionKind::SuspendCall { .. },
                                SemanticOriginalFunctionBody::Defined(_),
                            ) => {
                                signature.call_kind == CallKind::Suspend
                                    && callee.call_kind == CallKind::Suspend
                                    && callee.foreign.is_none()
                                    && point.function.call_kind == CallKind::Suspend
                            }
                            (
                                InstructionKind::Call { .. },
                                SemanticOriginalFunctionBody::Defined(_),
                            ) => {
                                signature.call_kind == CallKind::Direct
                                    && callee.call_kind == CallKind::Direct
                                    && callee.foreign.is_none()
                            }
                            _ => false,
                        };
                        if !valid {
                            return Err(fail(
                                "actual static call changes its original foreign or Suspend ABI role",
                            ));
                        }
                        b.spend(callee.parameters.len().saturating_mul(2).saturating_add(1), WORK)?;
                        let mut parameters = Vec::with_capacity(callee.parameters.len());
                        for parameter in &callee.parameters {
                            let ty = &callee
                                .locals
                                .get(parameter.0 as usize)
                                .filter(|local| local.id == *parameter)
                                .ok_or_else(|| fail("actual original callee parameter is absent"))?
                                .ty;
                            parameters.push(ty);
                        }
                        (*function, arguments, parameters, &callee.return_type)
                    }
                    InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                        if closure =>
                    {
                        let (operand, arguments) = match &instruction.kind {
                            InstructionKind::CallClosure { callee, arguments }
                            | InstructionKind::CallClosureOnce { callee, arguments }
                            | InstructionKind::CallClosureBorrowed { callee, arguments, .. } => {
                                (SemanticOperand::Value(*callee), arguments)
                            }
                            InstructionKind::CallClosureShared { callee, arguments }
                            | InstructionKind::CallClosureMut { callee, arguments } => {
                                (SemanticOperand::Local(*callee), arguments)
                            }
                            _ => {
                                return Err(fail(
                                    "actual closure call changed during original resolution",
                                ));
                            }
                        };
                        let receiver = self.operand(instance, &operand, point, b)?;
                        let id = if let InstructionKind::CallClosureBorrowed { callee, kind, .. } = instruction.kind {
                            let Type::Address { mutable, inner } = &receiver else {
                                return Err(fail("borrowed known closure has no actual environment address"));
                            };
                            let Type::Closure(id) = inner.as_ref() else {
                                return Err(fail("borrowed known closure changes its complete pointee type"));
                            };
                            if !matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                                || kind == CallableKind::FnMut && !*mutable {
                                return Err(fail("borrowed known closure changes its original invocation mode"));
                            }
                            let original = self.original.borrowed_closure_receiver(instance, site.recipe,
                                self.program, self.inventory, b)?;
                            self.borrowed_invocation_place(instance, point.function, instruction, &original, b)?;
                            let whole = self.ssa_value(instance, point.function, callee, b, 0)?;
                            if !self.pointee(&whole, b)?.equal_paid(&original.inner, b)? {
                                return Err(fail("borrowed known closure changes its complete original environment value"));
                            }
                            *id
                        } else {
                            let Type::Closure(id) = receiver else {
                                return Err(fail("original closure callee has another complete actual type"));
                            };
                            *id
                        };
                        let callee = self
                            .module
                            .closures
                            .get(id.0 as usize)
                            .filter(|callee| callee.id == id)
                            .ok_or_else(|| fail("original closure call has no actual entry"))?;
                        if let InstructionKind::CallClosureBorrowed { kind, .. } = instruction.kind
                            && !matches!((callee.kind, kind),
                                (CallableKind::Fn, CallableKind::Fn)
                                | (CallableKind::Fn | CallableKind::FnMut, CallableKind::FnMut))
                        {
                            return Err(fail("borrowed known closure entry has another complete callable role"));
                        }
                        let original_receiver = self.original.borrowed_closure_receiver(
                            instance, site.recipe, self.program, self.inventory, b)?;
                        self.known_call_place(instance, point.function,
                            (point.block.id, point.instruction), instruction, &original_receiver, b)?;
                        b.spend(callee.parameters.len(), WORK)?;
                        (
                            callee.function,
                            arguments,
                            callee.parameters.iter().collect(),
                            &callee.result,
                        )
                    }
                    _ => {
                        return Err(unknown(
                            "original body call needs its full independent dynamic, foreign or suspend boundary",
                        ));
                    }
                };
                // A machine body may have several independently checked
                // invocation worlds. Check this boundary against every one;
                // no global representative identifies their local classes.
                let formals = &self.inventory.definition(original, b)?.parameters;
                if actuals.len() != expected_actuals.len() || formals.len() != actuals.len() {
                    return Err(fail("actual BodyCall changes its original mixed arity"));
                }
                // Proposed arguments belong to this one caller occurrence.
                // Each callee below still proves its own rigid substitution.
                for (proposed, expected) in actuals.iter().zip(&expected_actuals) {
                    let proposed = self.actual_value(instance, proposed, b, 0)?;
                    if !self.capture_arguments_equal(&proposed, expected, b)? {
                        return Err(fail("actual BodyCall differs from original caller-domain mixed inference"));
                    }
                }
                if closure || !self.closed_call_targets(callee, original, formals, &expected_actuals, b)? {
                    let worlds = self.function_worlds(callee, b)?;
                    if worlds.is_empty() { return Err(fail("actual BodyCall has no complete original target world")); }
                    for target in worlds {
                        if target.definition != original || target.actuals.len() != expected_actuals.len() {
                            return Err(fail("actual BodyCall changes its independently inferred original target or mixed arity"));
                        }
                        for (ordinal, ((callee, expected), formal)) in target.actuals.iter()
                            .zip(&expected_actuals).zip(formals).enumerate()
                        {
                            let callee = self.actual_value(target, callee, b, 0)?;
                            let valid =
                                self.body_call_actual(target, ordinal, formal, expected, &callee, b)?;
                            if !valid {
                                return Err(fail(
                                    "actual BodyCall target changes its independent original rigid mixed context",
                                ));
                            }
                        }
                    }
                }
                self.arguments(point.function, arguments, parameters.iter().copied(), b)?;
                self.inputs(arguments, point, b)?;
                if let Some(anchor) = self.original.prepared_shared_call_input(
                    instance.definition, site.recipe, self.inventory, b,
                )? {
                    let [argument] = arguments.as_slice() else {
                        return Err(fail("prepared shared Address lost its sole actual call input"));
                    };
                    let (_, _, operation) = self.defining(point.function, *argument, b)?;
                    let InstructionKind::ReborrowAddress { address, mutable: false } = operation.kind
                    else { return Err(fail("prepared shared Address has no actual call reborrow")) };
                    for source in self.call_operand_origins(point.function, address, b)? {
                        if !self.anchored_value(instance, point.function, anchor, source, b, 0)? {
                            return Err(fail("prepared shared Address selects another original input place"));
                        }
                    }
                }
                if matches!(instruction.kind, InstructionKind::Call { .. } | InstructionKind::SuspendCall { .. }) {
                    self.scalar_call_inputs(instance, point.function, site.recipe, arguments, &parameters, b)?;
                }
                if matches!(instruction.kind, InstructionKind::ForeignCall { .. }) {
                    self.foreign_projected_arguments(instance, point.function, site.recipe, arguments, &parameters, b)?;
                }
                let (input_target, expected_inputs) = self.original.call_inputs(
                    instance,
                    site.recipe,
                    self.program,
                    self.inventory,
                    b,
                )?;
                if input_target != original || expected_inputs.len() != arguments.len() {
                    return Err(fail(
                        "actual call loses its independently inferred original input roles",
                    ));
                }
                for (ordinal, (actual, expected)) in arguments.iter().zip(&expected_inputs).enumerate() {
                    if self.uses_full_value(&expected.ty, b)? {
                        let operand = *actual;
                        let actual = self.ssa_value(instance, point.function, operand, b, 0)?;
                        let valid = if matches!(expected.ty, Type::Address { .. } | Type::Desc(_)) {
                            self.outer_call_role(instance, point.function, (point.block.id, point.instruction), ordinal, operand, &actual, expected, b, 0)?
                        } else {
                            self.capture_values_equal(&actual, expected, b)?
                        };
                        if !valid {
                            return Err(fail(
                                "actual call input differs from its complete original role",
                            ));
                        }
                    }
                }
                self.result(instruction, point.function, result)?;
                if closure && let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind {
                    let Some(Type::Closure(id)) = point.function.values.get(callee.0 as usize) else {
                        return Err(fail("known consuming operation loses its complete receiver"));
                    };
                    self.known_consuming_receiver(instance, point.function,
                        (point.block.id, point.instruction), callee, site.recipe, b)?;
                    let upper = operation.ok_or_else(|| fail("known consuming operation loses its original execution upper"))?;
                    if upper == CallKind::Suspend && point.function.call_kind != CallKind::Suspend {
                        return Err(fail("known Suspend consuming operation has no original Suspend caller"));
                    }
                    record_consuming_operation(point.function.id, point.block.id,
                        point.instruction, *id, upper, b)?;
                }
            }
            (
                SemanticSiteRecipeKind::Call {
                    target,
                    actuals: recipes,
                },
                SemanticSiteAction::Call {
                    target: actual_target,
                    actuals,
                },
            ) => {
                let callee = self.inventory.function(actual_target, b)?;
                if callee.environment.is_some()
                    || !self.actual_vectors(instance, recipes, actuals, b, 0)?
                {
                    return Err(fail(
                        "actual call changes its original definition or full mixed substitution",
                    ));
                }
                let mut caller_actuals = Vec::new();
                for actual in actuals {
                    let actual = self.actual_value(instance, actual, b, 0)?;
                    self.push_value(&mut caller_actuals, actual, b)?;
                }
                let worlds = self.function_worlds(callee.id, b)?;
                if worlds.is_empty() {
                    return Err(fail("actual call has no complete original target world"));
                }
                for world in worlds {
                    if world.definition != *target {
                        return Err(fail("actual call changes its original definition"));
                    }
                    self.operation_mixed_arguments(world, &caller_actuals, b)?;
                }
                let instruction =
                    instruction.ok_or_else(|| fail("actual call instruction is absent"))?;
                let InstructionKind::Call {
                    function,
                    arguments,
                } = &instruction.kind
                else {
                    return Err(unknown(
                        "suspend and foreign semantic calls require their full independent boundary contracts",
                    ));
                };
                if *function != callee.id {
                    return Err(fail("original call does not select the actual callee"));
                }
                b.spend(callee.parameters.len(), WORK)?;
                let mut expected = Vec::with_capacity(callee.parameters.len());
                for local in &callee.parameters {
                    expected.push(
                        &callee
                            .locals
                            .get(local.0 as usize)
                            .filter(|entry| entry.id == *local)
                            .ok_or_else(|| fail("actual callee parameter is absent"))?
                            .ty,
                    );
                }
                self.arguments(point.function, arguments, expected.into_iter(), b)?;
                self.inputs(arguments, point, b)?;
                self.result(instruction, point.function, &callee.return_type)?;
            }
            (
                SemanticSiteRecipeKind::CallableCall { callee: recipe },
                SemanticSiteAction::CallableCall { callee },
            ) => {
                let expected = self.ty(instance, recipe, b, 0)?;
                if !geometry::same_type(&expected, self.operand(instance, callee, point, b)?, b)? {
                    return Err(fail(
                        "actual callable operand changes its original whole type",
                    ));
                }
                let Type::Closure(id) = expected else {
                    return Err(unknown(
                        "semantic code-pointer calls need the complete scoped signature proof",
                    ));
                };
                let definition = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|definition| definition.id == id)
                    .ok_or_else(|| fail("actual callable has no exact environment definition"))?;
                let instruction =
                    instruction.ok_or_else(|| fail("actual callable instruction is absent"))?;
                let (operand, arguments) = match &instruction.kind {
                    InstructionKind::CallClosure { callee, arguments }
                    | InstructionKind::CallClosureOnce { callee, arguments } => {
                        (SemanticOperand::Value(*callee), arguments)
                    }
                    InstructionKind::CallClosureShared { callee, arguments }
                    | InstructionKind::CallClosureMut { callee, arguments } => {
                        (SemanticOperand::Local(*callee), arguments)
                    }
                    _ => {
                        return Err(unknown(
                            "semantic borrowed callable needs its complete actual head relation",
                        ));
                    }
                };
                if *callee != operand {
                    return Err(fail("callable recipe selects another actual operand"));
                }
                self.arguments(point.function, arguments, definition.parameters.iter(), b)?;
                self.inputs(arguments, point, b)?;
                self.result(instruction, point.function, &definition.result)?;
            }
            (
                SemanticSiteRecipeKind::Closure { definition, environment },
                SemanticSiteAction::TypeUse {
                    subject: subject @ SemanticOperand::Value(result),
                    expected,
                },
            ) => {
                let (node, target) = self.original.function_reference(
                    instance, site.recipe, self.inventory, b,
                )?.ok_or_else(|| fail("thin closure has no original fixed code proof"))?;
                if target != *definition || !environment.is_empty() || point.edge != SemanticSiteEdge::Before {
                    return Err(fail("thin closure changes its original creator, domain or result boundary"));
                }
                let original = self.recipe_value(instance, &SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Expression(node),
                }, b, 0)?;
                if !matches!(original.ty, Type::FunctionPointer { .. } | Type::BorrowedFunctionPointer { .. })
                    || !empty(&original.life) || !original.classes.is_empty()
                {
                    return Err(fail("thin closure changes its complete original code type"));
                }
                self.original_claim(instance, expected, &original, b)?;
                let producer = point.instruction.checked_sub(1)
                    .and_then(|position| point.block.instructions.get(position))
                    .ok_or_else(|| fail("thin closure has no actual code reference"))?;
                if producer.result != Some(*result)
                    || !matches!(producer.kind, InstructionKind::FunctionRef { .. })
                {
                    return Err(fail("thin closure changes its exact actual code result"));
                }
                self.function_reference(instance, site.recipe, point.function, subject, &original.ty, b)?;
                self.result(producer, point.function, &original.ty)?;
            }
            (
                SemanticSiteRecipeKind::Closure {
                    definition,
                    environment: recipes,
                },
                SemanticSiteAction::Closure {
                    result: SemanticOperand::Value(result),
                    closure: SemanticClosureTarget::Typed(closure),
                    environment,
                },
            ) => {
                let actual = self
                    .module
                    .closures
                    .get(closure.0 as usize)
                    .filter(|actual| actual.id == *closure)
                    .ok_or_else(|| fail("actual closure construction has no definition"))?;
                let child = self.closure_creator_world(actual.function, instance.id, *closure, b)?;
                let closed = child
                    .environment
                    .as_ref()
                    .ok_or_else(|| fail("actual closure has no creator-sealed environment"))?;
                if child.definition != *definition
                    || !child.actuals.is_empty()
                    || !self.actual_vectors(instance, recipes, environment, b, 0)?
                    || !self.actual_vectors(instance, recipes, &closed.actuals, b, 0)?
                {
                    return Err(fail(
                        "closure construction changes its original definition, creator or environment",
                    ));
                }
                let instruction =
                    instruction.ok_or_else(|| fail("actual closure constructor is absent"))?;
                let InstructionKind::MakeClosure {
                    closure: id,
                    captures,
                } = &instruction.kind
                else {
                    return Err(fail("closure ghost site is not an actual MakeClosure"));
                };
                if id != closure || instruction.result != Some(*result) {
                    return Err(fail(
                        "closure ghost site changes its actual constructed environment",
                    ));
                }
                self.arguments(point.function, captures, actual.captures.iter(), b)?;
                self.inputs(captures, point, b)?;
                self.creator_heads(instance, *closure, captures, b)?;
                self.result(instruction, point.function, &Type::Closure(*closure))?;
            }
            _ => {
                return Err(fail(
                    "actual semantic action changes its original operation recipe",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn sites(&self, b: &mut Budget) -> CompileResult<()> {
        // These directories contain only references to the complete immutable
        // event table. They answer presence/location queries; every original
        // recipe, actual event, duplicate and state obligation is still checked.
        let mut world_sites = BTreeMap::new();
        let mut created_closures = BTreeSet::new();
        self.inventory.profile_step("event directory", b);
        for site in &self.program.sites {
            self.inventory.profile_site(site, b);
            site_index::append(&mut world_sites, site.instance, site, b)?;
        }
        for instance in &self.program.instances {
            self.inventory.profile_step("world entry", b);
            self.inventory.profile_instance(instance, b);
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                continue;
            };
            let function = self.inventory.function(target, b)?;
            let definition = self.inventory.definition(instance.definition, b)?;
            b.lookup(world_sites.len(), WORK)?;
            let sites = world_sites.get(&instance.id).map_or(&[][..], Vec::as_slice);
            let SemanticDefinitionBody::FunctionWithBody {
                sites: recipes,
                body,
                foreign,
                ..
            } = &definition.body
            else {
                return Err(fail("original function body is absent"));
            };
            if let Some(actual) = &function.foreign {
                b.spend(recipes.len().saturating_add(1), WORK)?;
                if !matches!(body, SemanticOriginalFunctionBody::External)
                    || foreign.as_ref() != Some(actual)
                    || function.call_kind != CallKind::Direct
                    || function.environment.is_some()
                    || !function.blocks.is_empty()
                    || !recipes.is_empty()
                    || !sites.is_empty()
                {
                    return Err(fail(
                        "foreign declaration changes its original external body or emits runtime sites",
                    ));
                }
                // instances() has already checked the exact original ABI,
                // promises, full P/R and class header. External declarations
                // have no executable CFG to initialize here.
                continue;
            }
            let initialization = crate::shared_initialization(function, b)?;
            let mut declared = BTreeMap::new();
            let mut declared_values = BTreeMap::new();
            self.inventory.profile_step("local declarations", b);
            for &site in sites {
                self.inventory.profile_site(site, b);
                b.spend(1, WORK)?;
                let SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                } = &site.action
                else {
                    continue;
                };
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("declaration snapshot retains a Core emission site"));
                };
                let image_at = if let Some(slot) = self.binding_slot(instance, function, *local, b)? {
                    (slot.initial_store().0, slot.initial_store().1, SemanticSiteEdge::After)
                } else { (block, instruction as usize, edge) };
                let images = self.image_heads(instance, expected, image_at, b)?;
                if let SemanticTypeValue::Resolved { ty, .. } = expected {
                    if self.uses_full_value(ty, b)? {
                        let value = self.resolved(instance, expected, b, 0)?;
                        b.lookup(declared_values.len(), WORK)?;
                        if declared_values.insert(local, value).is_some() {
                            return Err(unknown(
                                "repeated semantic local activation needs its full class partition",
                            ));
                        }
                    }
                }
                b.spend(declared.len().saturating_add(1), WORK)?;
                if declared.insert(*local, images).is_some() {
                    return Err(unknown(
                        "repeated local declarations need their full original activation partition",
                    ));
                }
            }
            self.inventory.profile_step("original operations", b);
            let mut code_references = BTreeSet::new();
            let mut ct_trees = BTreeMap::new();
            let original_nodes = self.inventory.original_index(definition.id, b)?;
            b.spend(recipes.len(), "original recipe coverage")?;
            let mut emitted_recipes = vec![false; recipes.len()];
            let mut operation_sites = BTreeMap::new();
            // Original registration -> actual constructed value -> actual affine slot.
            // Scope activation, each dynamic take and LIFO ordering are checked by defers.
            let mut defer_actions = BTreeMap::new();
            let mut registered_actions = BTreeMap::new();
            let mut cleanup_takes = Vec::new();
            for &site in sites {
                self.inventory.profile_site(site, b);
                b.spend(1, WORK)?;
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("actual semantic site retains a Core address"));
                };
                let block = function
                    .blocks
                    .get(block.0 as usize)
                    .filter(|actual| actual.id == block)
                    .ok_or_else(|| fail("semantic operation has no actual block"))?;
                if !initialization.reachable[block.id.0 as usize] {
                    return Err(fail("semantic operation has no reachable actual site"));
                }
                let (ordinal, recipe) = original_nodes.recipe_row(site.recipe, b)?;
                if !recipes.get(ordinal).is_some_and(|row| std::ptr::eq(recipe, row)) {
                    return Err(fail("actual emission belongs to another original recipe table"));
                }
                if std::mem::replace(&mut emitted_recipes[ordinal], true) {
                    return Err(fail("one original semantic event is claimed by multiple actual emissions"));
                }
                let requires_binding = match &site.action {
                    SemanticSiteAction::Closure { closure: SemanticClosureTarget::Typed(id), result, .. } => {
                        if original_nodes.is_defer_operand(site.recipe, b)? {
                            let SemanticOperand::Value(mint) = result else {
                                return Err(fail("defer registration lacks its closure construction SSA"));
                            };
                            let entry = self.module.closures.get(id.0 as usize)
                                .filter(|entry| entry.id == *id && entry.kind == CallableKind::FnOnce
                                    && entry.parameters.is_empty() && entry.result == Type::Unit)
                                .ok_or_else(|| fail("defer requires a zero-parameter FnOnce Unit action"))?;
                            if self.inventory.function(&SemanticFunctionTarget::Typed(entry.function), b)?.call_kind != CallKind::Direct {
                                return Err(fail("defer action must be synchronous"));
                            }
                            b.lookup(defer_actions.len(), WORK)?;
                            if defer_actions.insert(*mint, (*id, block.id, instruction as usize)).is_some() {
                                return Err(fail("two original defer actions claim one closure construction"));
                            }
                        }
                        b.lookup(created_closures.len(), WORK)?;
                        created_closures.insert(*id);
                        true
                    }
                    SemanticSiteAction::Closure { .. } | SemanticSiteAction::Call { .. }
                    | SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::CallableCall { .. } => true,
                    SemanticSiteAction::DeclareLocal { .. } | SemanticSiteAction::Construct { .. }
                    | SemanticSiteAction::TypeUse { .. } | SemanticSiteAction::CoreBodyCall => false,
                };
                if requires_binding {
                    site_index::append(&mut operation_sites, (block.id, instruction as usize), site, b)?;
                }
                let checked = self.operation(
                    instance,
                    site,
                    recipe,
                    &Point {
                        function,
                        block,
                        instruction: instruction as usize,
                        edge,
                        initialization: &initialization,
                    },
                    &mut ct_trees,
                    b,
                );
                if let Err(mut errors) = checked {
                    if errors.iter().any(|error| error.category == zil_base::ResultCategory::ResourceLimit) {
                        // Optional contextual formatting must not replace the
                        // actual exhausted proof's category, caller or ledger.
                        return Err(errors);
                    }
                    b.spend(192, WORK)?;
                    for error in &mut errors {
                        error.notes.push(format!("actual semantic instance {}:{} original definition {}:{} function {} recipe {} at block {} instruction {} {:?}",
                            instance.id.module.0,instance.id.index,definition.id.module.0,definition.id.index,function.id.0,site.recipe.0,block.id.0,instruction,edge));
                    }
                    return Err(errors);
                }
                if let SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Value(value),
                    expected: SemanticTypeValue::Resolved { ty: Type::FunctionPointer { .. } | Type::BorrowedFunctionPointer { .. }, .. },
                } = &site.action
                    && self.original.function_reference(instance, site.recipe, self.inventory, b)?.is_some()
                {
                    b.lookup(code_references.len(), WORK)?;
                    if !code_references.insert(*value) {
                        return Err(fail("actual code reference repeats its exact original TypeUse"));
                    }
                }
            }
            self.inventory.profile_step("recipe coverage", b);
            for (recipe, emitted) in recipes.iter().zip(emitted_recipes) {
                self.inventory.profile_recipe(recipe.id, b);
                b.spend(1, WORK)?;
                if !emitted {
                    let node = original_nodes.site_node(recipe.id, b)?
                        .ok_or_else(|| fail("emission recipe has no original operation node"))?;
                    let (SemanticOriginalNode::Expression(id) | SemanticOriginalNode::Statement(id)) = node;
                    // A raw source stage is proved universally as well. Its
                    // selected CT execution is checked in the frozen context;
                    // it is not an extra executable operation in this runtime.
                    if original_nodes.erased(id, b)? {
                        continue;
                    }
                    if let SemanticOriginalNode::Statement(statement) = node
                        && let SemanticSiteRecipeKind::DeclareLocal { expected } = &recipe.kind
                        && let CoreStatementKind::TupleBinding { pattern, .. } = &original_nodes.get_stmt(statement, b)?.kind
                    {
                        let reference = original_pattern_reference_mode(pattern, 0, &mut |depth| {
                            if depth > b.partial_depth {
                                return Err(vec![zil_base::Diagnostic::limit(
                                    zil_base::Phase::Analyze, "reference binding syntax depth", Some(pattern.span),
                                )]);
                            }
                            b.spend(1, WORK)
                        })?.is_some();
                        if reference || matches!(pattern.kind, CorePatternKind::Record { .. } | CorePatternKind::RecordConstructor { .. })
                            && matches!(self.recipe_value(instance, expected, b, 0)?.ty, Type::Package { .. })
                        {
                            continue;
                        }
                    }
                    if let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind
                        && original_nodes.navigation(id, b)?
                    {
                        let value = self.recipe_value(instance, expected, b, 0)?;
                        self.actual_type(&value.ty, b)?;
                        continue;
                    }
                    let mut errors = fail("original emission recipe has no actual operation");
                    errors[0].notes.push(format!(
                        "original module {} definition {} recipe {} node {}",
                        definition.id.module.0, definition.id.index, recipe.id.0, id.0
                    ));
                    return Err(errors);
                }
            }
            // Primitive BorrowLocal/ReadAddress and call-adapter SSA may have
            // no TypeUse of their own. Reuse the actual original whole-value
            // reconstruction; inferring geometry from equal erased Types or
            // through a Reborrow edge would lose outer/child alias structure.
            let nested = |ty: &Type| matches!(ty, Type::Address { inner, .. }
                if matches!(inner.as_ref(), Type::Address { .. } | Type::Desc(_)));
            for (ordinal, ty) in function.values.iter().enumerate() {
                b.spend(1, WORK)?;
                if !nested(ty) { continue; }
                let id = ValueId(u32::try_from(ordinal).map_err(|_| fail("raw geometry SSA ordinal overflow"))?);
                let value = self.ssa_value(instance, function, id, b, 0).map_err(|mut errors| {
                    for error in &mut errors {
                        error.notes.push(format!("complete raw geometry producer function {} SSA {} original instance {:?}, Type {:?}", function.id.0, id.0, instance.id, ty));
                    }
                    errors
                })?;
                if !geometry::same_type(&value.ty, ty, b)? {
                    return Err(fail("raw geometry reconstruction changes its complete actual Type"));
                }
                crate::dyn_metadata::shape(&value.life, b)?;
                b.lookup(b.semantic_raw_value_shapes.len(), WORK)?;
                if let Some(old) = b.semantic_raw_value_shapes.get(&(function.id, id)).cloned() {
                    crate::dyn_metadata::shape(&old, b)?;
                    if old != value.life { return Err(fail("raw SSA has different complete original geometries in its actual worlds")); }
                } else { b.semantic_raw_value_shapes.insert((function.id, id), value.life.clone()); }
            }
            for (local, value) in &declared_values {
                b.spend(1, WORK)?;
                if !nested(&value.ty) { continue; }
                crate::dyn_metadata::shape(&value.life, b)?;
                b.lookup(b.semantic_raw_local_shapes.len(), WORK)?;
                if let Some(old) = b.semantic_raw_local_shapes.get(&(function.id, **local)).cloned() {
                    crate::dyn_metadata::shape(&old, b)?;
                    if old != value.life { return Err(fail("raw local has different complete declared geometries in its actual worlds")); }
                } else { b.semantic_raw_local_shapes.insert((function.id, **local), value.life.clone()); }
            }
            self.inventory.profile_step("actual CFG coverage", b);
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    self.inventory.profile_operation(block.id, index, b);
                    b.spend(1, WORK)?;
                    if let InstructionKind::RegisterDefer { local, .. } = instruction.kind {
                        let producer = index.checked_sub(1).and_then(|pos| block.instructions.get(pos));
                        let Some(Instruction { kind: InstructionKind::Store { local: slot, value }, .. }) = producer else {
                            return Err(fail("defer registration must immediately follow its closure Store"));
                        };
                        b.lookup(defer_actions.len(), WORK)?;
                        let Some((closure, origin, formation)) = defer_actions.remove(value) else {
                            return Err(fail("defer Store operand differs from the closure formed at its original registration"));
                        };
                        let intervening = formation.checked_add(1).zip(index.checked_sub(1))
                            .and_then(|(start, end)| block.instructions.get(start..end))
                            .ok_or_else(|| fail("defer registration must follow its own creation point"))?;
                        b.spend(intervening.len(), WORK)?;
                        if origin != block.id || intervening.iter().any(|instruction|
                            !matches!(instruction.kind, InstructionKind::StorageLive(slot) if slot == local)) {
                            return Err(fail("defer registration is separated from its original action by another effect or CFG edge"));
                        }
                        b.lookup(registered_actions.len(), WORK)?;
                        if *slot != local || function.locals.get(local.0 as usize).map(|v| &v.ty) != Some(&Type::Closure(closure))
                            || registered_actions.insert(local, closure).is_some() {
                            return Err(fail("defer registration changes or reuses the actual closure slot"));
                        }
                    }
                    if let InstructionKind::WriteCapture { index: field, value } = instruction.kind
                        && function.values.get(value.0 as usize)
                            .is_some_and(|ty| matches!(ty, Type::Address { mutable: false, .. }))
                    {
                        let definition = function.environment
                            .and_then(|id| self.module.closures.get(id.0 as usize))
                            .filter(|definition| definition.function == function.id)
                            .ok_or_else(|| fail("shared reference capture write has no authentic closure entry"))?;
                        if !matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
                            || definition.storage_life.is_none() || instruction.result.is_some()
                        {
                            return Err(fail("shared reference capture write needs exclusive actual entry access"));
                        }
                        available_before(function, (block.id, index, SemanticSiteEdge::Before), value, b)?;
                        let expected = self.capture_whole_value(instance, function, field, b, 0)?;
                        let incoming = self.ssa_value(instance, function, value, b, 0)?;
                        if !matches!(expected.ty, Type::Address { mutable: false, .. })
                            || !self.outer_role(instance, function, &incoming, &expected, b, 0)?
                        {
                            return Err(fail("capture write changes its complete original shared reference slot"));
                        }
                        b.lookup(b.semantic_capture_writes.len(), WORK)?;
                        b.semantic_capture_writes.insert((function.id, field, value));
                    }
                    if let InstructionKind::WriteAddress { address, value } = instruction.kind
                        && function.values.get(value.0 as usize).is_some_and(|ty|
                            matches!(ty, Type::Address { .. }) || crate::stored_flow::option_address::candidate(ty)) {
                        let destination = self.ssa_value(instance, function, address, b, 0)?;
                        if !matches!(destination.ty, Type::Address { mutable: true, .. }) {
                            return Err(fail("actual raw pointer replacement has no exclusive storage head"));
                        }
                        let expected = self.pointee(&destination, b)?;
                        let incoming = self.ssa_value(instance, function, value, b, 0)?;
                        if !incoming.equal_paid(&expected, b)?
                            && !self.outer_role(instance, function, &incoming, &expected, b, 0)? {
                            return Err(fail("raw pointer replacement changes its complete original child or directed receiving role"));
                        }
                        b.lookup(b.semantic_raw_writes.len(), WORK)?;
                        b.semantic_raw_writes.insert((function.id, block.id, index));
                    }
                    if let InstructionKind::Store { local, value } = instruction.kind {
                        if !function
                            .locals
                            .get(local.0 as usize)
                            .is_some_and(|declaration| {
                                declaration.id == local
                                    && function.values.get(value.0 as usize)
                                        == Some(&declaration.ty)
                            })
                        {
                            return Err(fail(
                                "actual stored occupant changes its complete declared type",
                            ));
                        }
                        b.lookup(declared_values.len(), WORK)?;
                        if let Some(expected) = declared_values.get(&local) {
                            let actual =
                                match self.reference_store_value(instance, function, local, b)? {
                                    Some(role) => role,
                                    None => self.ssa_value(instance, function, value, b, 0)?,
                                };
                            let shell = self.binding_slot(instance, function, local, b)?;
                            let physical_expected = if let Some(slot) = &shell {
                                if slot.initial_store() != (block.id, index) || slot.array() != value {
                                    return Err(fail("persistent Store replaces another original shell activation"));
                                }
                                self.persistent_slot_array_value(instance, function, slot, expected, b)?
                            } else { expected.copy(b)? };
                            if !actual.equal_paid(&physical_expected, b)?
                                && !self.outer_role(instance, function, &actual, &physical_expected, b, 0)? {
                                return Err(fail(
                                    "actual Store changes its complete original local class constraint",
                                ));
                            }
                        }
                        b.spend(declared.len().saturating_add(1), WORK)?;
                        if let Some(images) = declared.get(&local) {
                            let subject = if let Some(slot) = self.binding_slot(instance, function, local, b)? {
                                if slot.initial_store() != (block.id, index) || slot.array() != value {
                                    return Err(fail("persistent head Store changes its exact initializer"));
                                }
                                slot.initial()
                            } else { value };
                            self.match_heads(
                                instance,
                                images,
                                &SemanticOperand::Value(subject),
                                (block.id, index, SemanticSiteEdge::Before),
                                b,
                            )?;
                        }
                    }
                    if let (InstructionKind::StoreOwnedArrayElement { local, index: element, value }
                        | InstructionKind::StorePackageField { local, index: element, value }) = instruction.kind
                        && let Some(slot) = self.binding_slot(instance, function, local, b)? {
                        if element != 0 { return Err(fail("persistent refill changes its original element selector")); }
                        available_before(function, (block.id, index, SemanticSiteEdge::Before), value, b)?;
                        let incoming = self.ssa_value(instance, function, value, b, 0)?;
                        if !incoming.equal_paid(slot.logical(), b)? {
                            return Err(fail("persistent refill changes its complete original logical value"));
                        }
                        if let Some(images) = declared.get(&local) {
                            self.match_heads(instance, images, &SemanticOperand::Value(value),
                                (block.id, index, SemanticSiteEdge::Before), b)?;
                        }
                    }
                    if matches!(instruction.kind, InstructionKind::FunctionRef { .. }) {
                        let output = instruction.result
                            .ok_or_else(|| fail("actual function reference has no code result SSA"))?;
                        b.lookup(code_references.len(), WORK)?;
                        if !code_references.contains(&output) {
                            return Err(fail("actual function reference has no exact original TypeUse"));
                        }
                    }
                    let requires = match instruction.kind {
                        InstructionKind::OwnedStoreNew { .. }
                        | InstructionKind::OwnedStoreReserve { .. }
                        | InstructionKind::OwnedStorePush { .. }
                        | InstructionKind::OwnedStorePop { .. }
                        | InstructionKind::OwnedStoreLen { .. }
                        | InstructionKind::OwnedStoreCapacity { .. }
                        | InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. } => Some(3),
                        InstructionKind::Call { .. }
                        | InstructionKind::SuspendCall { .. }
                        | InstructionKind::ForeignCall { .. } => Some(0),
                        InstructionKind::TaskYield
                        | InstructionKind::TaskCreate { .. }
                        | InstructionKind::TaskCreateOwned { .. }
                        | InstructionKind::TaskPoll { .. }
                        | InstructionKind::TaskRequestCancel { .. }
                        | InstructionKind::TaskPollClose { .. }
                        | InstructionKind::TaskFinishDestroy { .. }
                        | InstructionKind::MutexCreate { .. }
                        | InstructionKind::MutexLock { .. }
                        | InstructionKind::MutexValue { .. }
                        | InstructionKind::WithSyncCell { .. }
                        | InstructionKind::SyncI32 { .. }
                        | InstructionKind::SyncI32CompareExchange { .. }
                        | InstructionKind::WithScopedThread { .. }
                        | InstructionKind::WithForeignCallback { .. }
                        | InstructionKind::AsyncRegistrationRequestClose { .. }
                        | InstructionKind::AsyncRegistrationPollClose { .. }
                        | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                        | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. } => Some(3),
                        InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureOnce { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. } => Some(1),
                        InstructionKind::MakeClosure { .. } => Some(2),
                        InstructionKind::CallIndirect { .. } => Some(3),
                        _ => None,
                    };
                    if let Some(kind) = requires {
                        b.lookup(operation_sites.len(), WORK)?;
                        let rows = operation_sites.get(&(block.id, index)).map(Vec::as_slice).unwrap_or(&[]);
                        b.spend(rows.len(), WORK)?;
                        let count = rows.iter().copied().filter(|site| site.instance == instance.id
                            && matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, .. } if actual == block.id && instruction as usize == index)
                            && matches!((&site.action, kind), (SemanticSiteAction::Call { .. }, 0) | (SemanticSiteAction::CallableCall { .. }, 1) | (SemanticSiteAction::Closure { .. }, 2) | (SemanticSiteAction::BodyCall { .. }, 0 | 1 | 3))).count();
                        if count == 0 && let InstructionKind::CallClosureOnce { callee, arguments } = &instruction.kind
                            && instruction.result.is_none() && arguments.is_empty()
                            && let Some(previous) = index.checked_sub(1).and_then(|pos| block.instructions.get(pos))
                            && previous.result == Some(*callee)
                            && let InstructionKind::TakeLocal { local } = previous.kind {
                            b.spend(1, WORK)?;
                            cleanup_takes.push((local, *callee));
                            continue;
                        }
                        if count != 1 {
                            return Err(fail(
                                "actual call or closure creation omits or duplicates its complete original recipe",
                            ));
                        }
                    }
                }
                if let Terminator::Return(value) = block.terminator {
                    self.inventory.profile_operation(block.id, block.instructions.len(), b);
                    b.spend(1, WORK)?;
                    let projected = match value {
                        Some(v) if function.values.get(v.0 as usize) != Some(&function.return_type) =>
                            crate::tasks::projected_creation_return(self.module, function, block, v, b)?,
                        _ => false,
                    };
                    if self.uses_full_value(&function.return_type, b)? {
                        let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                            &definition.body
                        else {
                            return Err(fail(
                                "actual result has no complete original function signature",
                            ));
                        };
                        let expected = self.recipe_value(instance, &signature.result, b, 0)?;
                        let Some(value) = value else {
                            return Err(fail("class-bearing actual Return omits its result SSA"));
                        };
                        let mut actual = self.ssa_value(instance, function, value, b, 0)?;
                        if projected {
                            if Some(&actual.ty) != function.values.get(value.0 as usize) {
                                return Err(fail("Task creation differs from the original expression"));
                            }
                            crate::dyn_metadata::ty(&function.return_type, b)?;
                            actual.geometry_mut(b)?.ty = function.return_type.clone();
                        }
                        if definition.elided_result_ordinal().is_some()
                            && self.outer_role(instance, function, &actual, &expected, b, 0)? {
                            let binder = function.storage_life.as_ref()
                                .and_then(|header| crate::signature_life::outer(header, StorageBase::Result))
                                .ok_or_else(|| fail("inferred return has no concrete outer binder"))?;
                            b.lookup(b.semantic_receiving_returns.len(), WORK)?;
                            b.semantic_receiving_returns.insert((function.id, block.id, value, binder));
                        }
                        if !self.capture_values_equal(&actual, &expected, b)?
                            && !self.outer_role(instance, function, &actual, &expected, b, 0)?
                            && !self.receiver_return_role(instance, function, block.id, value, &actual, &expected, b)?
                            && !self.receiving_field_return(instance, function, block.id, value, &actual, &expected, b)?
                            && !self.forwarded_receiver_return(instance, function, block.id, value, &actual, &expected, b)? {
                            return Err(fail(
                                "actual Return changes its complete original result class/source role",
                            ));
                        }
                    }
                    match value {
                        Some(value)
                            if function.values.get(value.0 as usize)
                                == Some(&function.return_type) || projected =>
                        {
                            Point {
                                function,
                                block,
                                instruction: block.instructions.len(),
                                edge: SemanticSiteEdge::Before,
                                initialization: &initialization,
                            }
                            .value(value, b)?;
                        }
                        None if function.return_type == Type::Unit => {}
                        _ => {
                            return Err(fail(
                                "actual Return does not preserve the complete original result type",
                            ));
                        }
                    }
                }
            }
            if !defer_actions.is_empty() {
                return Err(fail("original defer action is never registered in its actual closure slot"));
            }
            for (local, callee) in cleanup_takes {
                b.lookup(registered_actions.len(), WORK)?;
                let Some(closure) = registered_actions.get(&local) else {
                    return Err(fail("implicit cleanup calls a slot without an original defer registration"));
                };
                if function.values.get(callee.0 as usize) != Some(&Type::Closure(*closure)) {
                    return Err(fail("implicit cleanup take loses its registered closure identity"));
                }
            }
        }
        // A hidden environment cannot authenticate itself. Every concrete
        // entry is sealed at its actual creator's MakeClosure, even if unused.
        self.inventory.profile_stage("closure creation coverage", b);
        for closure in &self.module.closures {
            b.lookup(created_closures.len(), WORK)?;
            if !created_closures.contains(&closure.id) {
                return Err(fail("actual closure environment has no checked original creation site"));
            }
        }
        Ok(())
    }
}
