//! A physical capture selects one exact original free binder at its real
//! creator. This applies equally to owning values, phantom domains and retained
//! borrows; field reads later use the independently checked capture directory.
use super::*;
impl CheckedContext {
    pub(super) fn capture_creator_source(&self,module:&TypedModule,creator:&SemanticInstance,f:&TypedFunction,mut value:ValueId,source:original::intrinsics::BufferSource,b:&mut Budget)->Check<bool>{
        let local=self.buffer_source_local(module,creator,f,source,b)?;
        let mut seen=BTreeSet::new();loop{
            b.spend(1)?;if !seen.insert(value){return Err(bad("capture creator has a cyclic actual producer chain"));}
            let mut producer=None;
            for block in &f.blocks{b.spend(block.instructions.len())?;for instruction in &block.instructions{if instruction.result==Some(value)&&producer.replace(instruction).is_some(){return Err(bad("capture creator operand has several actual SSA definitions"));}}}
            match &producer.ok_or_else(||bad("capture creator actual operand is undefined"))?.kind{
                InstructionKind::Load(actual)|InstructionKind::TakeLocal{local:actual}|InstructionKind::DescribeLocal{local:actual}|InstructionKind::BorrowLocal{local:actual}|InstructionKind::BorrowLocalMut{local:actual}=>return Ok(*actual==local),
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::DescribeAddress{address}=>value=*address,
                _=>return Ok(false),
            }
        }
    }
}
