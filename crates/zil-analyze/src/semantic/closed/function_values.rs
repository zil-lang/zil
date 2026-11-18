//! Original code identities and indirect operand boundaries. A matching
//! pointer ABI alone never selects an original referenced definition.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    pub(super) fn invocation_code_recipe(
        &self,
        instance: &SemanticInstance,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        if signature.bound_lifetimes != 0 || signature.call_kind != CallKind::Direct
            || !matches!(signature.result_mode, PassingMode::Infer | PassingMode::Copy)
        {
            return Err(unknown("original borrowed Code requires its complete invocation-only ABI"));
        }
        let part = |recipe: &SemanticTypeRecipe, b: &mut Budget| -> CompileResult<(Type, Option<u32>)> {
            if let SemanticTypeRecipe::Address { mutable, lifetime, inner } = recipe {
                let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(ordinal)) = lifetime else {
                    return Err(unknown("borrowed Code requires a local original invocation lifetime"));
                };
                let inner = self.recipe_value(instance, inner, b, depth + 1)?;
                if !empty(&inner.life) || !inner.classes.is_empty() {
                    return Err(unknown("borrowed Code needs its full nested pointee class relation"));
                }
                Ok((Type::Address { mutable: *mutable, inner: Box::new(inner.into_type(b)?) }, Some(*ordinal)))
            } else {
                let data = self.recipe_value(instance, recipe, b, depth + 1)?;
                if !empty(&data.life) || !data.classes.is_empty() {
                    return Err(unknown("borrowed Code needs its full nested value class relation"));
                }
                Ok((data.into_type(b)?, None))
            }
        };
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            let ty = part(&parameter.ty, b)?;
            if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy)
                && !(ty.1.is_some() && parameter.mode == PassingMode::Borrow)
            {
                return Err(unknown("borrowed Code requires its complete original parameter mode"));
            }
            self.push_value(&mut parameters, ty, b)?;
        }
        let actual = value::invocation_code(parameters, part(&signature.result, b)?, b)?;
        if !matches!(&actual.ty, Type::BorrowedFunctionPointer { life, .. }
            if life.invocation_binder_count == signature.invocation_lifetimes)
        {
            return Err(fail("borrowed Code changes the original invocation domain"));
        }
        Ok(actual)
    }

    pub(super) fn function_reference(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        subject: &SemanticOperand,
        expected: &Type,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let Some((_, original)) = self
            .original
            .function_reference(instance, site, self.inventory, b)?
        else {
            return Ok(());
        };
        let SemanticOperand::Value(value) = subject else {
            return Err(fail(
                "original function reference has no actual emitted code value",
            ));
        };
        let (_, _, instruction) = self.defining(function, *value, b)?;
        let target = match (expected, &instruction.kind) {
            (Type::FunctionPointer { .. } | Type::BorrowedFunctionPointer { .. },
                InstructionKind::FunctionRef { function })
            | (Type::ForeignCallback { .. }, InstructionKind::ForeignFunctionRef { function }) => {
                function
            }
            _ => {
                return Err(fail(
                    "original code reference changes its complete pointer family or actual opcode",
                ));
            }
        };
        let worlds = self.function_worlds(*target, b)?;
        if worlds.is_empty() {
            return Err(fail(
                "actual function reference has no complete original world",
            ));
        }
        let declaration = self.inventory.definition(original, b)?;
        let thin = declaration.kind == SemanticDefinitionKind::Closure;
        let borrowed = match expected {
            Type::BorrowedFunctionPointer { life, .. } => Some(life),
            _ => None,
        };
        let mut creators = 0usize;
        for world in worlds {
            if world.parent == Some(instance.id) {
                creators += 1;
            }
            if world.definition != original
                || !world.actuals.is_empty()
                || thin && world.parent.is_none()
                || !thin && world.parent.is_some()
                || world.environment.is_some()
                || world.self_type.is_some()
                || borrowed.is_none() && !world.header_embedding.is_empty()
            {
                return Err(fail(
                    "actual function reference selects another original specialization or environment",
                ));
            }
        }
        if thin && (creators != 1 || declaration.parent != Some(instance.definition)
            || !declaration.environment.is_empty() || !matches!(expected, Type::FunctionPointer { .. } | Type::BorrowedFunctionPointer { .. }))
        {
            return Err(fail("thin function reference changes its exact original creator"));
        }
        let actual = self
            .inventory
            .functions
            .get(target)
            .ok_or_else(|| fail("actual function reference selects no real function"))?;
        let (parameters, result) = match expected {
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer { parameters, result, .. }
            | Type::ForeignCallback { parameters, result } => (parameters, result),
            _ => {
                return Err(fail(
                    "original thin code value changes its complete code family",
                ));
            }
        };
        if let Some(life) = borrowed {
            crate::signature_life::charge(life, b)?;
            crate::borrowed_fp::relation(parameters, result, life)?;
            if actual.storage_life.as_ref() != Some(life) {
                return Err(fail("borrowed code target changes its exact actual invocation ABI"));
            }
        } else if thin && let Some(header) = &actual.storage_life {
            crate::signature_life::charge(header, b)?;
            if header.fixed_binder_count != 0 || header.invocation_binder_count != 0
                || !header.bindings.is_empty() || !header.static_bindings.is_empty()
                || !header.classes.is_empty()
            {
                return Err(fail("thin function reference changes its class-free fixed code ABI"));
            }
        }
        if actual.call_kind != CallKind::Direct
            || actual.foreign.is_some()
            || actual.environment.is_some()
            || actual.return_type != **result
            || actual.parameters.len() != parameters.len()
            || !matches!(actual.result_mode, PassingMode::Infer | PassingMode::Copy)
        {
            return Err(fail(
                "actual function reference changes its original fixed code ABI",
            ));
        }
        for (local, expected) in actual.parameters.iter().zip(parameters) {
            let actual = actual
                .locals
                .get(local.0 as usize)
                .filter(|decl| decl.id == *local)
                .ok_or_else(|| fail("actual code parameter has no real local"))?;
            crate::dyn_metadata::ty(&actual.ty, b)?;
            crate::dyn_metadata::ty(expected, b)?;
            if actual.ty != *expected {
                return Err(fail(
                    "actual function reference changes its complete original parameter or mode",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn thin_child_creator(
        &self,
        child: &SemanticInstance,
        entry: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let creator = self.inventory.instance(
            child.parent.ok_or_else(|| fail("thin child has no actual lexical creator"))?, b,
        )?;
        let SemanticInstanceTarget::Function(target) = &creator.target else {
            return Err(fail("thin child has no actual function creator"));
        };
        let function = self.inventory.function(target, b)?;
        let mut found = false;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != creator.id {
                continue;
            }
            let SemanticSiteAction::TypeUse {
                subject: subject @ SemanticOperand::Value(value),
                expected: SemanticTypeValue::Resolved { ty: expected @ (Type::FunctionPointer { .. } | Type::BorrowedFunctionPointer { .. }), .. },
            } = &site.action else { continue; };
            if !self.original.function_reference(creator, site.recipe, self.inventory, b)?
                .is_some_and(|(_, target)| target == child.definition)
            {
                continue;
            }
            if !matches!(self.defining(function, *value, b)?.2.kind,
                InstructionKind::FunctionRef { function } if function == entry)
                || found
            {
                return Err(fail("thin child changes or repeats its actual creator reference"));
            }
            self.function_reference(creator, site.recipe, function, subject, expected, b)?;
            found = true;
        }
        if !found {
            return Err(fail("thin child has no original checked code creation site"));
        }
        Ok(())
    }

    fn original_index_operand_matches(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        node: zil_base::NodeId,
        actual: ValueId,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        if let Some(original) = self.original_subject_value(instance, function, node, b)? {
            return self.operation_operand_matches(function, actual, original, b);
        }
        let original = self.inventory.original_index(instance.definition, b)?.get_expr(node, b)?;
        let (_, _, operation) = self.defining(function, actual, b)?;
        let (CoreExprKind::Integer(text), InstructionKind::Constant(value)) =
            (&original.kind, &operation.kind)
        else {
            return Ok(false);
        };
        let expected = self.recipe_value(
            instance,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(node),
            },
            b,
            0,
        )?;
        Ok(expected.ty.is_integer()
            && expected.ty == value.ty()
            && original::mixed::integer_constant(text, &expected.ty, b)? == *value)
    }

    fn projected_code_operand(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: original::LocalAnchor,
        projections: &[original::CodeProjection],
        mut value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        for projection in projections.iter().rev() {
            b.spend(1, WORK)?;
            let (_, _, actual) = self.defining(function, value, b)?;
            match (projection, &actual.kind) {
                (
                    original::CodeProjection::Field(expected),
                    InstructionKind::ExtractField { aggregate, index },
                ) if expected == index => value = *aggregate,
                (
                    original::CodeProjection::Index(node),
                    InstructionKind::ExtractIndex { aggregate, index },
                ) if self.original_index_operand_matches(instance, function, *node, *index, b)? =>
                    value = *aggregate,
                _ => return Ok(false),
            }
        }
        let local = self.anchored_local(instance, function, local, b)?;
        Ok(self.receiver_local(function, value, b, 0)? == local)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn code_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let Some(original::CodeCallBoundary { anchor, code, invocation }) =
            self.original
                .code_boundary(instance, site, self.program, self.inventory, b)?
        else {
            return Ok(None);
        };
        if !actuals.is_empty() {
            return Err(fail("original indirect code call invents generic actuals"));
        }
        let (parameters, result) = match &code.ty {
            Type::FunctionPointer { parameters, result } if invocation.is_none() => (parameters, result),
            Type::BorrowedFunctionPointer { parameters, result, life } if invocation.is_some() => {
                crate::signature_life::charge(life, b)?;
                crate::borrowed_fp::relation(parameters, result, life)?;
                (parameters, result)
            }
            _ => return Err(fail("original indirect call loses its complete code type")),
        };
        if !empty(&code.life) || !code.classes.is_empty() {
            return Err(unknown(
                "indirect code call needs its full persistent class boundary",
            ));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original indirect call has no actual instruction"))?;
        let InstructionKind::CallIndirect { callee, arguments } = &instruction.kind else {
            return Err(fail(
                "original indirect call is replaced by another actual opcode",
            ));
        };
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            *callee,
            b,
        )?;
        let anchored = match anchor {
            original::CodeCallAnchor::Local(anchor) => {
                let local = self.anchored_local(instance, function, *anchor, b)?;
                self.receiver_local(function, *callee, b, 0)? == local
            }
            original::CodeCallAnchor::Projected { local, projections } => {
                self.projected_code_operand(instance, function, *local, projections, *callee, b)?
            }
            original::CodeCallAnchor::CallResult { node, indirect } => {
                let recipe = self.inventory.call_anchor(instance.definition, *node, b)?;
                let source = self.inventory.emitted_site(self.program, instance.id, recipe, b)?;
                let SemanticSiteAddress::Typed {
                    block: producer_block,
                    instruction: producer_index,
                    edge: SemanticSiteEdge::After,
                } = source.at else {
                    return Err(fail("original code result has no actual call completion"));
                };
                if !matches!(source.action, SemanticSiteAction::BodyCall { .. }
                    | SemanticSiteAction::Call { .. })
                {
                    return Err(fail("original code result changes its call recipe"));
                }
                let producer = function.blocks.get(producer_block.0 as usize)
                    .filter(|actual| actual.id == producer_block)
                    .and_then(|block| block.instructions.get(producer_index as usize))
                    .ok_or_else(|| fail("original code result has no actual call instruction"))?;
                if !matches!((&producer.kind, *indirect),
                    (InstructionKind::Call { .. }, false)
                    | (InstructionKind::CallIndirect { .. }, true))
                {
                    return Err(fail("original code result changes its actual call family"));
                }
                let result = producer.result
                    .ok_or_else(|| fail("original code producer omits its actual result"))?;
                operations::available_before(
                    function, (block, index, SemanticSiteEdge::Before), result, b,
                )?;
                function.values.get(result.0 as usize) == Some(&code.ty)
                    && self.operation_operand_matches(function, *callee, result, b)?
            }
        };
        if !anchored
            || function.values.get(callee.0 as usize) != Some(&code.ty)
            || parameters.len() != arguments.len()
        {
            return Err(fail(
                "actual indirect call selects another original code operand or arity",
            ));
        }
        for (ordinal, (argument, expected)) in arguments.iter().zip(parameters).enumerate() {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *argument,
                b,
            )?;
            crate::dyn_metadata::ty(expected, b)?;
            if function.values.get(argument.0 as usize) != Some(expected) {
                return Err(fail(
                    "actual indirect argument changes its complete original type",
                ));
            }
            if let (Type::Address { .. }, Some((inputs, _))) = (expected, &invocation) {
                let (node, original) = inputs.get(ordinal)
                    .ok_or_else(|| fail("borrowed indirect call omits an original input"))?;
                let actual = self.ssa_value(instance, function, *argument, b, 0)?;
                if !self.outer_role(instance, function, &actual, original, b, 0)? {
                    return Err(fail("borrowed indirect call changes the complete original argument relation"));
                }
                if let Some(source) = self.original_subject_value(instance, function, *node, b)?
                    && !self.operation_operand_matches(function, *argument, source, b)?
                    && !matches!(self.defining(function, *argument, b)?.2.kind,
                        InstructionKind::ReborrowAddress { address, .. } if address == source)
                {
                    return Err(fail("borrowed indirect argument selects another original operand"));
                }
            } else {
                self.zero_data(expected, b, 0)?;
            }
        }
        if let Some((_, output)) = invocation {
            if output.ty != **result {
                return Err(fail("borrowed indirect result changes its original invocation type"));
            }
            self.result(instruction, function, result)?;
            return Ok(Some(output));
        }
        self.zero_data(result, b, 0)?;
        self.result(instruction, function, result)?;
        crate::dyn_metadata::ty(result, b)?;
        Ok(Some(value::scalar(result.as_ref().clone())))
    }
}
