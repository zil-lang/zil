//! A physical capture selects one exact original free binder at its real
//! creator. This applies equally to owning values, phantom domains and retained
//! borrows; field reads later use the independently checked capture directory.
use super::*;
impl CheckedContext {
    pub(in crate::semantic) fn capture_creator_source(&self,module:&TypedModule,creator:&SemanticInstance,f:&TypedFunction,mut value:ValueId,source:original::intrinsics::BufferSource,b:&mut Budget)->Check<bool>{
        let local=self.buffer_source_local(module,creator,f,source,b)?;
        let mut seen=BTreeSet::new();loop{
            b.spend(1)?;if !seen.insert(value){return Err(bad("capture creator has a cyclic actual producer chain"));}
            if let Some(source)=self.prepared_closure_source(module,creator,f,value,b)?{value=source;continue;}
            let (_, _, producer) = self.site_index.producers.get(f, value, b)?;
            match &producer.kind {
                InstructionKind::Load(actual)|InstructionKind::TakeLocal{local:actual}|InstructionKind::DescribeLocal{local:actual}|InstructionKind::BorrowLocal{local:actual}|InstructionKind::BorrowLocalMut{local:actual}=>return Ok(*actual==local),
                InstructionKind::TakeOwnedArrayElement{local:actual,index:0}|InstructionKind::BorrowOwnedArrayElement{local:actual,index:0,..}=>{
                    self.persistent_initialization(module,creator,f,*actual,b)?;
                    return Ok(*actual==local);
                },
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::DescribeAddress{address}=>value=*address,
                _=>return Ok(false),
            }
        }
    }
}
