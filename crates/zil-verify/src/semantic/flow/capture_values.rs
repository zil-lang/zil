//! A physical capture selects one exact original free binder at its real
//! creator. This applies equally to owning values, phantom domains and retained
//! borrows; field reads later use the independently checked capture directory.
use super::*;
impl CheckedContext {
    pub(in crate::semantic) fn capture_creator_source(
        &self, module: &TypedModule, creator: &SemanticInstance, f: &TypedFunction,
        value: ValueId, source: &original::CapturedSource, b: &mut Budget,
    ) -> Check<bool> {
        let mut pending = vec![(creator, f, value, 0usize)];
        let mut seen = BTreeSet::new();
        while let Some((creator, f, value, depth)) = pending.pop() {
            b.spend(seen.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
            if !seen.insert((creator.id, value, depth)) { continue; }
            let Some(&definition) = source.parents.get(depth) else {
                if !self.direct_creator_source(module, creator, f, value, source.root, b)? { return Ok(false); }
                continue;
            };
            if creator.definition != definition { return Ok(false); }
            let Some(slot) = self.forwarded_capture_slot(module, f, value, b)? else { return Ok(false); };
            let environment = f.environment.ok_or_else(|| bad("forwarded capture has no actual environment"))?;
            let parent = self.instance(module, creator.parent.ok_or_else(|| bad("forwarded capture lacks its lexical creator"))?, b)?;
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = parent.target else {
                return Err(bad("forwarded capture creator is not executable"));
            };
            let function = module.functions.get(id.0 as usize).filter(|body| body.id == id)
                .ok_or_else(|| bad("forwarded capture creator function is absent"))?;
            let boundaries = self.closure_boundaries.borrow();
            b.spend(boundaries.len())?;
            let mut bound = false;
            for boundary in boundaries.iter().filter(|row| row.child == creator.id) {
                if boundary.creator != ScopeKey::Instance(parent.id) || boundary.at.function != id {
                    return Err(bad("forwarded capture belongs to another actual lexical creator"));
                }
                let instruction = function.blocks.get(boundary.at.block.0 as usize)
                    .and_then(|block| block.instructions.get(boundary.at.instruction as usize))
                    .ok_or_else(|| bad("forwarded environment occurrence has no constructor"))?;
                let InstructionKind::MakeClosure { closure, captures } = &instruction.kind else {
                    return Err(bad("forwarded environment is not its actual constructor"));
                };
                if *closure != environment || boundary.value.ty != Type::Closure(environment) {
                    return Err(bad("forwarded capture changes its actual environment identity"));
                }
                let operand = *captures.get(slot as usize).ok_or_else(|| bad("parent constructor omits the actual captured slot"))?;
                b.spend(1)?;
                pending.push((parent, function, operand, depth + 1));
                bound = true;
            }
            if !bound { return Err(unsupported("forwarded capture lacks a sealed parent occurrence", None)); }
        }
        Ok(true)
    }

    fn forwarded_capture_slot(&self, module: &TypedModule, f: &TypedFunction, mut value: ValueId, b: &mut Budget) -> Check<Option<u32>> {
        let mut seen = BTreeSet::new();
        loop {
            b.spend(seen.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
            if !seen.insert(value) { return Err(bad("forwarded capture cycles through its own SSA definition")); }
            let (_, _, producer) = self.site_index.producers.get(f, value, b)?;
            match producer.kind {
                InstructionKind::ReadCapture { index } | InstructionKind::BorrowCapture { index, .. }
                | InstructionKind::DescribeCapture { index } | InstructionKind::TakeCapture { index } => return Ok(Some(index)),
                InstructionKind::ReborrowAddress { address, .. } | InstructionKind::DescribeAddress { address } => value = address,
                InstructionKind::TakeLocal { local } => return self.bound_package_capture(module, f, local, b),
                InstructionKind::Load(local) | InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } =>
                {
                    let mut spills = self.capture_spills.borrow_mut();
                    b.spend(spills.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
                    let slot = match spills.entry((f.id, local)) {
                        std::collections::btree_map::Entry::Occupied(known) => *known.get(),
                        std::collections::btree_map::Entry::Vacant(new) => {
                            *new.insert(capture_mirrors::activation(&self.site_index.producers, f, local, b)?)
                        }
                    };
                    return Ok(slot);
                }
                _ => return Ok(None),
            }
        }
    }

    fn direct_creator_source(&self,module:&TypedModule,creator:&SemanticInstance,f:&TypedFunction,mut value:ValueId,source:original::intrinsics::BufferSource,b:&mut Budget)->Check<bool>{
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
