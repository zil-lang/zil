//! Research-only deterministic per-stage accounting, never a production budget.
//! A unit is an element/word/byte visit or copy, with conservative bulk charges.
//! BTree bookkeeping and allocator internals are not CPU-instruction estimates.
pub const LIMIT: usize = 100_000_000;

/// Length-only conservative storage/format bound, not an operand visitor.
pub fn instruction_items(kind: &zil_ir::InstructionKind) -> usize {
    use zil_ir::InstructionKind::*;
    let terms = match kind {
        MakeManaged { fields } | MakeAggregate { fields } | MakeVariant { fields, .. } => {
            fields.len()
        }
        MakeClosure { captures, .. } => captures.len(),
        Call { arguments, .. }
        | ForeignCall { arguments, .. }
        | CallIndirect { arguments, .. }
        | CallClosure { arguments, .. }
        | CallClosureShared { arguments, .. }
        | CallClosureMut { arguments, .. }
        | CallClosureOnce { arguments, .. } => arguments.len(),
        _ => 4,
    };
    terms.saturating_add(4)
}
pub fn instruction_bound(kind: &zil_ir::InstructionKind) -> usize {
    if let zil_ir::InstructionKind::Unsupported(text) = kind {
        return text.len().saturating_mul(12).saturating_add(128);
    }
    instruction_items(kind).saturating_mul(128)
}

pub struct Work {
    limit: usize,
    used: usize,
    phase: &'static str,
}
impl Work {
    pub fn new(limit: usize, phase: &'static str) -> Self {
        Self {
            limit,
            used: 0,
            phase,
        }
    }
    pub fn charge(&mut self, amount: usize) -> Result<(), String> {
        let next = self
            .used
            .checked_add(amount)
            .ok_or_else(|| format!("ResourceLimit: {} overflow", self.phase))?;
        if next > self.limit {
            return Err(format!(
                "ResourceLimit: {} work {next} > {}",
                self.phase, self.limit
            ));
        }
        self.used = next;
        Ok(())
    }
    pub fn used(&self) -> usize {
        self.used
    }
}
