//! Select a real inline Copy WriteAddress using the complete original assignment
//! occurrence. Its reservation and memory authority are checked separately.
use super::*;

impl Check<'_, '_> {
    pub(super) fn copy_assignment_sites(&self, b: &mut Budget) -> CompileResult<()> {
        for function in &self.module.functions {
            let mut candidates = Vec::new();
            for block in &function.blocks {
                for instruction in &block.instructions {
                    b.spend(1, WORK)?;
                    if let InstructionKind::WriteAddress { value, .. } = instruction.kind
                        && let Some(ty) = function.values.get(value.0 as usize)
                        && ty.is_inline_scalar_product(&mut || b.spend(1, WORK))?
                    {
                        b.spend(1, WORK)?;
                        candidates.push(instruction);
                    }
                }
            }
            if candidates.is_empty() {
                continue;
            }
            let worlds = self.function_worlds(function.id, b)?;
            if worlds.is_empty() {
                continue;
            }
            for world in worlds {
                let original = self.inventory.original_index(world.definition, b)?;
                let mut assignments = BTreeMap::new();
                for statement in original.active_writes(b)? {
                    let CoreStatementKind::Assign { target, value } = &statement.kind else {
                        continue;
                    };
                    b.lookup(assignments.len(), WORK)?;
                    assignments
                        .entry(target.span)
                        .and_modify(|entry| *entry = None)
                        .or_insert(Some((statement.id, target, value)));
                }
                let mut matched = Vec::new();
                let mut used = BTreeSet::new();
                for operation in candidates {
                    b.lookup(assignments.len(), WORK)?;
                    let Some(Some((node, target, incoming))) = assignments.get(&operation.span)
                    else {
                        continue;
                    };
                    let InstructionKind::WriteAddress { address, value } = operation.kind else {
                        unreachable!();
                    };
                    if self.assignment_place(world, function, target, address, b, 0)?
                        && self.copy_expression_source(world, function, incoming, value, b, 0)?
                    {
                        b.lookup(used.len(), WORK)?;
                        if !used.insert(*node) {
                            return Err(fail(
                                "one original Copy assignment selects several actual writes",
                            ));
                        }
                        b.spend(1, WORK)?;
                        matched.push(operation);
                    }
                }
                candidates = matched;
                if candidates.is_empty() {
                    break;
                }
            }
            for operation in candidates {
                let InstructionKind::WriteAddress { address, .. } = operation.kind else {
                    unreachable!();
                };
                b.lookup(b.semantic_copy_writes.len(), WORK)?;
                b.semantic_copy_writes.insert((function.id, address));
            }
        }
        Ok(())
    }

    fn assignment_place(
        &self,
        world: &SemanticInstance,
        function: &TypedFunction,
        source: &SemanticOriginalExpr,
        actual: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(original::resource(
                "original Copy assignment target exceeds its complete path depth",
            ));
        }
        let (_, _, producer) = self.defining(function, actual, b)?;
        Ok(match (&source.kind, &producer.kind) {
            (CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. }, _) => {
                self.assignment_place(world, function, inner, actual, b, depth + 1)?
            }
            (_, InstructionKind::ReborrowAddress { address, .. }) => {
                self.assignment_place(world, function, source, *address, b, depth + 1)?
            }
            (
                CoreExprKind::Index { object, index },
                InstructionKind::AddressIndex {
                    address,
                    index: actual_index,
                },
            ) => {
                self.assignment_place(world, function, object, *address, b, depth + 1)?
                    && self.copy_expression_source(
                        world,
                        function,
                        index,
                        *actual_index,
                        b,
                        depth + 1,
                    )?
            }
            (
                CoreExprKind::Member { object, member, .. },
                InstructionKind::AddressField { address, index },
            ) => {
                let Some(Type::Address { inner, .. }) = function.values.get(address.0 as usize)
                else {
                    return Ok(false);
                };
                b.spend(member.len(), WORK)?;
                let selected = match inner.as_ref() {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => fields
                        .get(*index as usize)
                        .is_some_and(|(name, _)| name == member),
                    Type::Tuple(_) => member.parse::<u32>() == Ok(*index),
                    _ => false,
                };
                selected
                    && self.assignment_place(world, function, object, *address, b, depth + 1)?
            }
            (
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    operand,
                },
                _,
            ) => self.copy_expression_source(world, function, operand, actual, b, depth + 1)?,
            (CoreExprKind::Name(_), _) => {
                self.copy_expression_source(world, function, source, actual, b, depth + 1)?
            }
            _ => false,
        })
    }

    pub(super) fn copy_expression_source(
        &self,
        world: &SemanticInstance,
        function: &TypedFunction,
        source: &SemanticOriginalExpr,
        actual: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(original::resource(
                "original Copy expression exceeds its complete operand depth",
            ));
        }
        let (block, at, producer) = self.defining(function, actual, b)?;
        Ok(match (&source.kind, &producer.kind) {
            (CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. }, _) => {
                self.copy_expression_source(world, function, inner, actual, b, depth + 1)?
            }
            (CoreExprKind::Name(_), InstructionKind::Load(local) | InstructionKind::DescribeLocal { local }) => {
                let Some(anchor) =
                    self.original
                        .local_read_anchor(world.definition, source.id, b)?
                else {
                    return Ok(false);
                };
                self.anchored_local(world, function, anchor, b)? == *local
            }
            (
                CoreExprKind::Member { object, member, .. },
                InstructionKind::ReadDesc { desc: parent, index }
                | InstructionKind::ExtractField { aggregate: parent, index },
            ) if matches!(producer.kind, InstructionKind::ReadDesc { .. })
                || matches!(function.values.get(parent.0 as usize), Some(Type::Record { .. })) => {
                operations::available_before(function, (block, at, SemanticSiteEdge::Before), *parent, b)?;
                let parent_type = function.values.get(parent.0 as usize)
                    .ok_or_else(|| fail("actual Copy field has no complete parent type"))?;
                let record = match (&producer.kind, parent_type) {
                    (InstructionKind::ReadDesc { .. }, Type::Desc(record)) => record.as_ref(),
                    (InstructionKind::ExtractField { .. }, record) => record,
                    _ => return Ok(false),
                };
                let Type::Record { fields, .. } = record else { return Ok(false); };
                let Some((name, field)) = fields.get(*index as usize) else { return Ok(false); };
                let result = function.values.get(actual.0 as usize)
                    .ok_or_else(|| fail("actual Copy field has no complete result type"))?;
                b.spend(name.len().saturating_add(member.len()), WORK)?;
                name == member && geometry::same_type(field, result, b)?
                    && self.copy_expression_source(world, function, object, *parent, b, depth + 1)?
            }
            (CoreExprKind::Integer(text), InstructionKind::Constant(value))
                if value.ty().is_integer() =>
            {
                original::mixed::integer_constant(text, &value.ty(), b)? == *value
            }
            (CoreExprKind::Bool(expected), InstructionKind::Constant(Constant::Bool(actual))) => {
                expected == actual
            }
            (CoreExprKind::Unit, InstructionKind::Constant(Constant::Unit)) => true,
            (CoreExprKind::Literal(expected), InstructionKind::Constant(actual)) => {
                expected == actual
            }
            (
                CoreExprKind::Binary { op, left, right },
                InstructionKind::Binary {
                    op: actual_op,
                    left: a,
                    right: z,
                },
            ) => {
                op == actual_op
                    && self.copy_expression_source(world, function, left, *a, b, depth + 1)?
                    && self.copy_expression_source(world, function, right, *z, b, depth + 1)?
            }
            (
                CoreExprKind::Unary { op, operand },
                InstructionKind::Unary {
                    op: actual_op,
                    operand: input,
                },
            ) => {
                op == actual_op
                    && self.copy_expression_source(world, function, operand, *input, b, depth + 1)?
            }
            (
                CoreExprKind::Index { .. }
                | CoreExprKind::Member { .. }
                | CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    ..
                },
                InstructionKind::ReadAddress { address },
            ) => self.assignment_place(world, function, source, *address, b, depth + 1)?,
            _ => match self.original_subject_value(world, function, source.id, b)? {
                Some(value) => self.operation_operand_matches(function, actual, value, b)?,
                None => false,
            },
        })
    }
}
