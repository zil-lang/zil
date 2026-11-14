//! Bind synchronization schemes to the actual worker, head and parent body.
//! Mutex lock debt, access exclusivity and thread join stay in their real kernels.
use super::*;
use crate::semantic::value::Value;
impl Check<'_, '_> {
    pub(super) fn mutex_payload(&self, payload: &Type, b: &mut Budget) -> CompileResult<()> {
        crate::dyn_metadata::ty(payload, b)?;
        if *payload != Type::I32 {
            return Err(unknown(
                "original mutex scheme requires its complete protected payload theorem",
            ));
        }
        // This language type has its own sealed operations and affine cleanup,
        // not a fabricated user ResourceDefinition. Its full child domain is
        // exactly i32 here; real allocation/lock/drop remain actual obligations.
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn mutex_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        intrinsic: original::sealed::Intrinsic,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index));
        // Use the exact block ID, never a same-position block with another ID.
        let instruction = instruction
            .ok_or_else(|| fail("original synchronization call has no actual instruction"))?;
        b.spend(3, WORK)?;
        let values: Vec<ValueId> = match (intrinsic, &instruction.kind) {
            (original::sealed::Intrinsic::MutexCreate, InstructionKind::MutexCreate { value }) => {
                vec![*value]
            }
            (original::sealed::Intrinsic::MutexLock, InstructionKind::MutexLock { mutex })
            | (
                original::sealed::Intrinsic::MutexValue,
                InstructionKind::MutexValue { guard: mutex },
            ) => {
                let anchor = self
                    .original
                    .intrinsic_anchor(instance, site, self.inventory, b)?;
                self.task_head(instance, function, *mutex, anchor, b, 0)?;
                vec![*mutex]
            }
            (
                original::sealed::Intrinsic::ThreadScope,
                InstructionKind::WithScopedThread {
                    worker,
                    argument,
                    body,
                },
            ) => {
                let targets = self
                    .original
                    .native_targets(instance, site, self.inventory, b)?;
                if targets.as_slice() != [self.function_instance(*worker, b)?.definition] {
                    return Err(fail(
                        "structured thread executes another original worker target",
                    ));
                }
                let target = self
                    .module
                    .functions
                    .get(worker.0 as usize)
                    .filter(|target| target.id == *worker)
                    .ok_or_else(|| fail("thread worker actual header is absent"))?;
                if target.call_kind != CallKind::Direct {
                    return Err(fail("structured thread changes the actual worker ABI"));
                }
                let anchor = self
                    .original
                    .intrinsic_anchor(instance, site, self.inventory, b)?;
                self.task_head(instance, function, *argument, anchor, b, 0)?;
                self.thread_parent_body(instance, site, function, *body, b)?;
                vec![*argument, *body]
            }
            _ => {
                return Err(fail(
                    "actual synchronization opcode changes its original sealed scheme",
                ));
            }
        };
        b.spend(values.len(), WORK)?;
        let expected =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        if expected.len() != values.len() {
            return Err(fail(
                "synchronization call changes its ordered complete inputs",
            ));
        }
        if !actuals.is_empty() {
            if intrinsic == original::sealed::Intrinsic::ThreadScope {
                if actuals.len() != expected.len() {
                    return Err(fail(
                        "actual scoped-thread call changes its complete Type actual arity",
                    ));
                }
                for (actual, expected) in actuals.iter().zip(&expected) {
                    if !matches!(self.actual_value(instance,actual,b,0)?,crate::semantic::value::Argument::Type(value) if value==*expected)
                    {
                        return Err(fail(
                            "actual scoped-thread Type actual changes its original input or parent environment",
                        ));
                    }
                }
            } else {
                let [actual] = actuals else {
                    return Err(fail(
                        "actual synchronization call changes its payload Type arity",
                    ));
                };
                if !matches!(self.actual_value(instance,actual,b,0)?,crate::semantic::value::Argument::Type(value) if value.ty==Type::I32&&empty(&value.life)&&value.classes.is_empty())
                {
                    return Err(fail(
                        "actual synchronization Type actual changes its independently resolved sealed payload",
                    ));
                }
            }
        }
        for (id, wanted) in values.iter().zip(&expected) {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *id,
                b,
            )?;
            if function.values.get(id.0 as usize) != Some(&wanted.ty) {
                return Err(fail(
                    "synchronization call changes its original full input carrier",
                ));
            }
            if matches!(
                wanted.ty,
                Type::Address { .. } | Type::Desc(_) | Type::MutexGuard { .. }
            ) {
                let value = self.ssa_value(instance, function, *id, b, 0)?;
                let valid = if matches!(wanted.ty, Type::Address { .. }) {
                    self.outer_role(instance, function, &value, wanted, b, 0)?
                } else {
                    value == *wanted
                };
                if !valid {
                    return Err(fail(
                        "synchronization call changes the same original head/class relation",
                    ));
                }
            }
            self.actual_type(&wanted.ty, b)?;
        }
        let output =
            self.original
                .intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        Ok(output)
    }

    fn thread_parent_body(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let declaration = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &declaration.body else {
            return Err(fail("thread source has no complete original body"));
        };
        let mut original = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("thread call selects an original statement"));
                };
                if original.replace(node).is_some() {
                    return Err(fail("thread operation has competing original source calls"));
                }
            }
        }
        let index = super::super::original_index::index(declaration, b)?;
        let expression = index.get_expr(
            original.ok_or_else(|| fail("thread operation has no original source call"))?,
            b,
        )?;
        let CoreExprKind::Call { arguments, .. } = &expression.kind else {
            return Err(unknown(
                "structured thread source needs its complete original call form",
            ));
        };
        let body = arguments
            .get(2)
            .ok_or_else(|| fail("thread operation loses its original parent body"))?;
        if !matches!(body.kind, CoreExprKind::Closure { .. }) {
            return Err(unknown(
                "named structured-thread parent bodies need their exact original value flow",
            ));
        }
        let mut linked = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.node != SemanticOriginalNode::Expression(body.id) {
                continue;
            }
            for emitted in &self.program.sites {
                b.spend(1, WORK)?;
                if emitted.instance != instance.id
                    || emitted.recipe != link.site
                    || !matches!(emitted.action, SemanticSiteAction::Closure { .. })
                {
                    continue;
                }
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge: SemanticSiteEdge::After,
                } = emitted.at
                else {
                    return Err(fail(
                        "thread body creator loses its actual emission boundary",
                    ));
                };
                let actual = function
                    .blocks
                    .get(block.0 as usize)
                    .filter(|actual| actual.id == block)
                    .and_then(|block| block.instructions.get(instruction as usize))
                    .ok_or_else(|| fail("thread parent body has no actual creator"))?;
                if !matches!(actual.kind, InstructionKind::MakeClosure { .. })
                    || linked.replace(actual.result).is_some()
                {
                    return Err(fail("thread body has another or duplicate actual creator"));
                }
            }
        }
        if linked.flatten() != Some(value) {
            return Err(fail(
                "structured thread consumes another actual parent environment",
            ));
        }
        Ok(())
    }

    pub(super) fn synchronization_result(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at,SemanticSiteAddress::Typed {block:actual,instruction,edge:SemanticSiteEdge::After} if actual==block && instruction as usize==index)
                || !matches!(site.action, SemanticSiteAction::BodyCall { .. })
            {
                continue;
            }
            let intrinsic = self
                .original
                .intrinsic(instance, site.recipe, self.inventory, b)?
                .ok_or_else(|| fail("synchronization result has no original scheme"))?;
            if !matches!(
                intrinsic,
                original::sealed::Intrinsic::MutexCreate
                    | original::sealed::Intrinsic::MutexLock
                    | original::sealed::Intrinsic::MutexValue
                    | original::sealed::Intrinsic::ThreadScope
            ) {
                return Err(fail(
                    "another original operation claims this synchronization result",
                ));
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                return Err(fail("synchronization result changes its actual action"));
            };
            let value = self.mutex_operation(
                instance,
                site.recipe,
                intrinsic,
                function,
                block,
                index,
                actuals,
                b,
            )?;
            if selected.replace(value).is_some() {
                return Err(fail(
                    "synchronization result has duplicate original operations",
                ));
            }
        }
        selected.ok_or_else(|| fail("synchronization result has no original operation bridge"))
    }
}
