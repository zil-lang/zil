use super::*;
impl Builder<'_, '_> {
    pub(super) fn closed_blocks(&mut self) -> R<Vec<CtBlock>> {
        let span = self.span;
        self.compiler.budget.work(
            self.blocks
                .len()
                .saturating_mul(3)
                .saturating_add(self.values.len().saturating_mul(2)),
            span,
        )?;
        let mut reachable = vec![false; self.blocks.len()];
        let mut pending = VecDeque::new();
        self.compiler.budget.node(span)?;
        reachable[0] = true;
        pending.push_back(0usize);
        while let Some(block) = pending.pop_front() {
            self.compiler.budget.work(1, span)?;
            let targets = match self.blocks[block]
                .terminator
                .as_ref()
                .unwrap_or(&CtTerminator::Trap)
            {
                CtTerminator::Jump(target) => [Some(*target), None],
                CtTerminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => [Some(*then_block), Some(*else_block)],
                _ => [None, None],
            };
            for target in targets.into_iter().flatten() {
                self.compiler.budget.work(1, span)?;
                let index = target.0 as usize;
                if !reachable[index] {
                    self.compiler.budget.node(span)?;
                    reachable[index] = true;
                    pending.push_back(index);
                }
            }
        }
        let mut blocks = vec![None; self.blocks.len()];
        let mut count = 0u32;
        let mut used = vec![false; self.values.len()];
        for (index, block) in self.blocks.iter().enumerate() {
            self.compiler.budget.work(1, span)?;
            if !reachable[index] {
                continue;
            }
            blocks[index] = Some(CtBlockId(count));
            count += 1;
            for instruction in &block.instructions {
                self.compiler.budget.work(1, instruction.span)?;
                if let Some(value) = instruction.result {
                    used[value.0 as usize] = true;
                }
            }
        }
        let mut values = vec![None; self.values.len()];
        let mut types = Vec::new();
        for (index, ty) in std::mem::take(&mut self.values).into_iter().enumerate() {
            self.compiler.budget.work(1, span)?;
            if used[index] {
                self.compiler.budget.node(span)?;
                values[index] = Some(CtValueId(types.len() as u32));
                types.push(ty);
            }
        }
        self.values = types;
        let mut result = Vec::new();
        for (index, block) in std::mem::take(&mut self.blocks).into_iter().enumerate() {
            self.compiler.budget.work(1, span)?;
            let Some(id) = blocks[index] else {
                continue;
            };
            let mut instructions = block.instructions;
            for instruction in &mut instructions {
                self.compiler.budget.work(1, instruction.span)?;
                let remap = |v: &mut CtValueId| -> R<()> {
                    *v = values[v.0 as usize].ok_or_else(|| {
                        error(
                            instruction.span,
                            "CT value refers to an unreachable source definition",
                        )
                    })?;
                    Ok(())
                };
                if let Some(value) = &mut instruction.result {
                    remap(value)?;
                }
                match &mut instruction.kind {
                    CtInstructionKind::Store { value, .. }
                    | CtInstructionKind::ConvertInteger { value, .. }
                    | CtInstructionKind::ConvertNumber { value, .. }
                    | CtInstructionKind::Length(value)
                    | CtInstructionKind::VariantTag(value)
                    | CtInstructionKind::VariantField { value, .. }
                    | CtInstructionKind::MetaProject { value, .. } => {
                        self.compiler.budget.work(1, instruction.span)?;
                        remap(value)?;
                    }
                    CtInstructionKind::Unary { operand, .. } => {
                        self.compiler.budget.work(1, instruction.span)?;
                        remap(operand)?;
                    }
                    CtInstructionKind::Binary { left, right, .. } => {
                        self.compiler.budget.work(2, instruction.span)?;
                        remap(left)?;
                        remap(right)?;
                    }
                    CtInstructionKind::MakeTuple(args)
                    | CtInstructionKind::MakeArray(args)
                    | CtInstructionKind::MakeList(args)
                    | CtInstructionKind::MakeRecord { fields: args, .. }
                    | CtInstructionKind::MakeVariant { fields: args, .. }
                    | CtInstructionKind::Call {
                        arguments: args, ..
                    }
                    | CtInstructionKind::TypeApply {
                        arguments: args, ..
                    }
                    | CtInstructionKind::TypeConstruct {
                        arguments: args, ..
                    }
                    | CtInstructionKind::Quote { holes: args, .. }
                    | CtInstructionKind::Text {
                        arguments: args, ..
                    }
                    | CtInstructionKind::SyntaxBuild {
                        arguments: args, ..
                    }
                    | CtInstructionKind::Query {
                        arguments: args, ..
                    } => {
                        self.compiler.budget.work(args.len(), instruction.span)?;
                        for arg in args {
                            remap(arg)?;
                        }
                    }
                    CtInstructionKind::Extract { aggregate, index } => {
                        self.compiler.budget.work(2, instruction.span)?;
                        remap(aggregate)?;
                        remap(index)?;
                    }
                    CtInstructionKind::ExtractField { aggregate, .. } => {
                        self.compiler.budget.work(1, instruction.span)?;
                        remap(aggregate)?;
                    }
                    CtInstructionKind::Replace {
                        aggregate,
                        index,
                        value,
                    } => {
                        self.compiler.budget.work(3, instruction.span)?;
                        remap(aggregate)?;
                        remap(index)?;
                        remap(value)?;
                    }
                    CtInstructionKind::ReplaceField {
                        aggregate, value, ..
                    } => {
                        self.compiler.budget.work(2, instruction.span)?;
                        remap(aggregate)?;
                        remap(value)?;
                    }
                    CtInstructionKind::Parse { input, .. }
                    | CtInstructionKind::ParseIdent { input } => {
                        self.compiler.budget.work(1, instruction.span)?;
                        remap(input)?;
                    }
                    CtInstructionKind::TokenInspect { source, index, .. } => {
                        self.compiler.budget.work(2, instruction.span)?;
                        remap(source)?;
                        if let Some(index) = index {
                            remap(index)?;
                        }
                    }
                    CtInstructionKind::NameBuild { text, .. } => {
                        self.compiler.budget.work(1, instruction.span)?;
                        remap(text)?;
                    }
                    CtInstructionKind::Constant(_)
                    | CtInstructionKind::InvocationInput { .. }
                    | CtInstructionKind::StorageLive(_)
                    | CtInstructionKind::StorageDead(_)
                    | CtInstructionKind::Load(_)
                    | CtInstructionKind::TypePrimitive(_)
                    | CtInstructionKind::TypeBuiltin(_)
                    | CtInstructionKind::TypeName(_) => {}
                }
            }
            let value = |v: CtValueId| -> R<CtValueId> {
                values[v.0 as usize]
                    .ok_or_else(|| error(span, "CT terminator uses an unreachable source value"))
            };
            let terminator = match block.terminator.unwrap_or(CtTerminator::Trap) {
                CtTerminator::Return(v) => CtTerminator::Return(v.map(value).transpose()?),
                CtTerminator::Jump(to) => CtTerminator::Jump(blocks[to.0 as usize].unwrap()),
                CtTerminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => CtTerminator::Branch {
                    condition: value(condition)?,
                    then_block: blocks[then_block.0 as usize].unwrap(),
                    else_block: blocks[else_block.0 as usize].unwrap(),
                },
                CtTerminator::Trap => CtTerminator::Trap,
            };
            self.compiler.budget.node(span)?;
            result.push(CtBlock {
                id,
                instructions,
                terminator,
            });
        }
        Ok(result)
    }
}
