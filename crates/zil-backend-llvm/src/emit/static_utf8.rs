//! Exact immutable program data, using the existing fat-slice ABI in every
//! machine consumer. UTF8 validity and static origin come only from Verify.
use super::*;

fn spend(count: usize, remaining: &mut usize) -> CompileResult<()> {
    *remaining = remaining.checked_sub(count).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "static UTF8 machine budget exceeded",
            None,
        )]
    })?;
    Ok(())
}

pub(crate) fn reserve_emission(count: usize, remaining: &mut usize) -> CompileResult<()> {
    spend(count.saturating_mul(16).saturating_add(64), remaining)
}

#[cfg(test)]
pub(crate) fn planned_remaining(
    lir: &LirView<'_>,
    limits: &zil_base::Limits,
    remaining: usize,
) -> CompileResult<usize> {
    copy_aggregates::plan(lir, limits, remaining).map(|plan| plan.remaining_work)
}

fn symbol(function: FunctionId, value: ValueId) -> String {
    format!("zil.utf8.{}.{}", function.0, value.0)
}

pub(crate) fn global(function: FunctionId, value: ValueId, bytes: &[u8]) -> m::Global {
    m::Global {
        name: symbol(function, value),
        linkage: m::Linkage::Private,
        ty: m::Ty::array(bytes.len() as u64, m::Ty::Int(8)),
        initializer: Some(m::Val::Bytes(bytes.to_vec())),
        constant: true,
        unnamed_addr: true,
        alignment: 1,
    }
}

impl FunctionEmitter<'_> {
    pub(super) fn static_utf8(
        &mut self,
        instruction: &Instruction,
        count: usize,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let pair = self.temp();
        let ty = m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]);
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(pair.clone())),
            m::Op::Insert {
                arg: m::Arg::new(ty.clone(), m::Val::Zero),
                index: 0,
                field: m::Arg::new(m::Ty::Ptr, m::global(symbol(self.function.id, result))),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result.0)),
            m::Op::Insert {
                arg: m::Arg::new(ty, m::val(pair)),
                index: 1,
                field: m::Arg::new(m::Ty::Int(64), m::val(count)),
            },
        )));
        Ok(())
    }
}

/// Preserve exact raw-LIR binding without another inventory scan. The original
/// structural equality traversal now prepays byte comparisons where they occur.
/// Aliased immutable views require neither comparison nor byte-copy payment.
pub(crate) fn same_functions(
    left: &[TypedFunction],
    right: &[TypedFunction],
    remaining: &mut usize,
    limits: &zil_base::Limits,
) -> CompileResult<bool> {
    if std::ptr::eq(left, right) {
        return Ok(true);
    }
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.iter().zip(right) {
        let TypedFunction {
            dyn_instance,
            retained_return_sources,
            storage_life,
            call_kind,
            reference_return_bound,
            callable_return_contract,
            callable_contract,
            protocol_contract,
            id,
            name,
            foreign,
            environment,
            parameters,
            return_type,
            result_mode,
            locals,
            values,
            blocks,
            entry,
            span,
        } = left;
        if let (Some(a), Some(b)) = (dyn_instance, &right.dyn_instance) {
            reserve_dyn_instance(a, limits, remaining)?;
            reserve_dyn_instance(b, limits, remaining)?;
        }
        if dyn_instance != &right.dyn_instance
            || retained_return_sources != &right.retained_return_sources
            || storage_life != &right.storage_life
            || call_kind != &right.call_kind
            || reference_return_bound != &right.reference_return_bound
            || callable_return_contract != &right.callable_return_contract
            || callable_contract != &right.callable_contract
            || protocol_contract != &right.protocol_contract
            || id != &right.id
            || name != &right.name
            || foreign != &right.foreign
            || environment != &right.environment
            || parameters != &right.parameters
            || return_type != &right.return_type
            || result_mode != &right.result_mode
            || locals != &right.locals
            || values != &right.values
            || entry != &right.entry
            || span != &right.span
            || blocks.len() != right.blocks.len()
        {
            return Ok(false);
        }
        for (left, right) in blocks.iter().zip(&right.blocks) {
            if left.id != right.id
                || left.terminator != right.terminator
                || left.instructions.len() != right.instructions.len()
            {
                return Ok(false);
            }
            for (left, right) in left.instructions.iter().zip(&right.instructions) {
                if left.result != right.result || left.span != right.span {
                    return Ok(false);
                }
                if let (
                    InstructionKind::StaticUtf8 { bytes: a },
                    InstructionKind::StaticUtf8 { bytes: b },
                ) = (&left.kind, &right.kind)
                {
                    if a.len() != b.len() {
                        return Ok(false);
                    }
                    spend(a.len().saturating_add(1), remaining)?;
                }
                if left.kind != right.kind {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

struct RecipeWork<'a> {
    remaining: &'a mut usize,
    limits: &'a zil_base::Limits,
}
impl zil_ir::DynRecipeVisitor for RecipeWork<'_> {
    type Error = Vec<Diagnostic>;
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth > self.limits.max_call_depth.min(128) {
            return Err(vec![Diagnostic::limit(
                Phase::Codegen,
                "dynamic recipe machine depth exceeded",
                None,
            )]);
        }
        spend(1, self.remaining)
    }
    fn rows(&mut self, n: usize, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        spend(n, self.remaining)
    }
    fn ty(&mut self, value: &Type, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        super::stored_borrows::type_work(value, self.remaining, self.limits, depth)
    }
    fn life(&mut self, value: &zil_ir::StorageLifeShape, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        super::stored_borrows::shape(value, self.remaining)
    }
    fn identity(
        &mut self,
        value: &zil_ir::LifeDeclarationIdentity,
        depth: usize,
    ) -> CompileResult<()> {
        self.node(depth)?;
        self.rows(value.package.len(), depth)?;
        for part in value.module_path.iter().chain(&value.declaration_path) {
            self.rows(part.len(), depth)?;
        }
        self.rows(value.generation_path.len(), depth)
    }
    fn position(&mut self, value: &zil_ir::StoragePosition, depth: usize) -> CompileResult<()> {
        self.rows(value.path.len(), depth.saturating_add(value.path.len()))
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let Constant::WideString(units) = value {
            self.rows(units.len(), depth)?;
        }
        Ok(())
    }
    fn text(&mut self, value: &str, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        spend(value.len(), self.remaining)
    }
}
pub(super) fn reserve_dyn_instance(
    value: &zil_ir::DynFunctionInstance,
    limits: &zil_base::Limits,
    remaining: &mut usize,
) -> CompileResult<()> {
    use zil_ir::DynRecipeMetadata;
    value.visit_dyn_metadata(&mut RecipeWork { remaining, limits }, 0)
}
