//! CT admission is a closed capability set on independently checked ordinary CFG.
use super::*;

/// Immutable execution authority; a raw CtProgram or ordinary VerifiedModule
/// cannot manufacture this wrapper or enable new CT capabilities.
#[derive(Clone, Debug)]
pub struct VerifiedCTProgram {
    verified: VerifiedModule,
    entry: FunctionId,
    max_steps: u64,
    max_call_depth: usize,
    max_live_slots: usize,
}
impl VerifiedCTProgram {
    pub fn module(&self) -> &TypedModule {
        self.verified.module()
    }
    pub fn entry(&self) -> FunctionId {
        self.entry
    }
    pub fn max_steps(&self) -> u64 {
        self.max_steps
    }
    pub fn max_call_depth(&self) -> usize {
        self.max_call_depth
    }
    pub fn max_live_slots(&self) -> usize {
        self.max_live_slots
    }
}

pub fn verify_ct_program(
    raw: CtProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedCTProgram> {
    if raw.frozen_context.is_some() {
        return Err(vec![Diagnostic::error(Phase::Ct,"E-CT-FROZEN-CONTEXT",None,"CT frozen-context request requires the complete independently supplied source view")]);
    }
    quotas(&raw,limits)?;
    let verified = verify_module(raw.module, raw.plan, target, limits)?;
    admit(verified,raw.entry,raw.max_steps,raw.max_call_depth,raw.max_live_slots)
}

pub(super) fn quotas(raw:&CtProgram,limits:&Limits)->CompileResult<()> {
    let max_slots = limits.max_values.saturating_add(limits.max_locals);
    if raw.max_steps == 0
        || raw.max_steps > limits.max_ct_steps
        || raw.max_call_depth == 0
        || raw.max_call_depth > limits.max_call_depth
        || raw.max_live_slots == 0
        || raw.max_live_slots > max_slots
    {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "CT program declared execution budget exceeds selected quota",
            None,
        )]);
    }
    Ok(())
}

pub(super) fn admit(verified:VerifiedModule,entry_id:FunctionId,max_steps:u64,max_call_depth:usize,max_live_slots:usize)->CompileResult<VerifiedCTProgram> {
    let module=verified.module();
    pure(module)?;
    let bad=|span,message|vec![Diagnostic::error(Phase::Ct,"E-CT-FORBIDDEN",span,message)];
    let Some(entry) = module.functions.get(entry_id.0 as usize) else {
        return Err(bad(None, "CT entry function does not exist"));
    };
    if !entry.parameters.is_empty() || entry.environment.is_some() {
        return Err(bad(
            Some(entry.span),
            "CT query entry must be closed and have no parameters",
        ));
    }
    Ok(VerifiedCTProgram {
        verified,
        entry: entry_id,
        max_steps,
        max_call_depth,
        max_live_slots,
    })
}

pub(super) fn pure(module:&TypedModule)->CompileResult<()> {
    let bad = |span, message| {
        vec![Diagnostic::error(
            Phase::Ct,
            "E-CT-FORBIDDEN",
            span,
            message,
        )]
    };
    if !module.closures.is_empty() {
        return Err(bad(
            None,
            "CT program cannot carry runtime closure environments",
        ));
    }
    for f in &module.functions {
        if f.foreign.is_some()
            || f.environment.is_some()
            || !serializable(&f.return_type)
            || f.locals.iter().any(|l| !serializable(&l.ty))
            || f.values.iter().any(|t| !serializable(t))
        {
            return Err(bad(
                Some(f.span),
                "CT program supports serializable pure Copy values only",
            ));
        }
        for block in &f.blocks {
            // Terminators are an explicit pure-control capability set as well.
            match block.terminator {
                Terminator::Return(_)
                | Terminator::Jump(_)
                | Terminator::Branch { .. }
                | Terminator::Unreachable
                | Terminator::Trap => {}
            }
            for instruction in &block.instructions {
                match instruction.kind {
                    InstructionKind::BorrowCapture { .. } => {
                        return Err(bad(
                            Some(instruction.span),
                            "runtime environment borrowing is outside the pure CT capability set",
                        ));
                    }
                    InstructionKind::Constant(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::Call { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::MakeVariant { .. }
                    | InstructionKind::VariantTag { .. }
                    | InstructionKind::ReadVariant { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::UpdateField { .. }
                    | InstructionKind::ExtractIndex { .. }
                    | InstructionKind::UpdateIndex { .. } => {}
                    _ => {
                        return Err(bad(
                            Some(instruction.span),
                            "instruction is not in the pure CT capability set",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn verify_ct_program_bytes(
    bytes: &[u8],
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedCTProgram> {
    verify_ct_program(
        zil_ir::wire::decode_ct_program(bytes, limits)?,
        target,
        limits,
    )
}

pub(super) fn serializable(ty: &Type) -> bool {
    match ty {
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I8
        | Type::I16
        | Type::Isize
        | Type::I64
        | Type::F32
        | Type::U64
        | Type::Usize
        | Type::F64
        | Type::U8
        | Type::U16
        | Type::U32 => true,
        Type::Tuple(fields) => fields.iter().all(serializable),
        Type::Record { fields, .. } => fields.iter().all(|(_, t)| serializable(t)),
        Type::Array { element, .. } => serializable(element),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fields)| fields.iter().all(serializable)),
        _ => false,
    }
}
