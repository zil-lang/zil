from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/emit.rs');s=p.read_text();raw=s.encode()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='ce868d3a70edef216f6c149e3340efb4e9d559a0'
def change(old,new):
 global s
 assert s.count(old)==1,(old,s.count(old));s=s.replace(old,new)
change('mod byte_buffer;','mod byte_buffer;\nmod cleanup_tails;')
change('''        .emit()?
        .into_items()''','''        .emit(cleanup_tails::plan(function, verified.limits(),
            task_worker || verified.task_ownership(function.id).is_some()))?
        .into_items()''')
change('fn emit(mut self) -> CompileResult<m::Code> {','fn emit(mut self, cleanup: Option<cleanup_tails::Plan<\'_>>) -> CompileResult<m::Code> {')
change('''        self.copy_allocations();''','''        if cleanup.is_some() { self.cleanup_return_slot()?; }
        self.copy_allocations();''')
change('''        for block in &self.function.blocks {
            self.event(m::Event::Label(format!("bb.{}", block.id.0)));
            for (index, instruction) in block.instructions.iter().enumerate() {''','''        for (block_index, block) in self.function.blocks.iter().enumerate() {
            self.event(m::Event::Label(format!("bb.{}", block.id.0)));
            let shared = cleanup.as_ref().and_then(|p| p.starts[block_index]);
            let end = shared.map_or(block.instructions.len(), |start| start.at);
            for (index, instruction) in block.instructions[..end].iter().enumerate() {''')
change('''            self.terminator(&block.terminator)?;
        }
        // Even a statically unreachable''','''            if let Some(start) = shared {
                let Terminator::Return(value) = block.terminator else { unreachable!() };
                self.cleanup_exit(value, start.node)?;
            } else {
                self.terminator(&block.terminator)?;
            }
        }
        if let Some(plan) = &cleanup { self.cleanup_nodes(plan)?; }
        // Even a statically unreachable''')
p.write_text(s)
print('Implemented optional bounded per-function cleanup suffix interning; no verifier or inliner changes')
p=Path('crates/zil-backend-llvm/src/emit/cleanup_tails.rs');s=p.read_text()
s=s.replace('-> Option<(u8, LocalId)> {','-> Option<Option<(u8, LocalId)>> {',1)
s=s.replace('''        InstructionKind::DropManaged { local } => (1, local),
        _ => return None,''','''        InstructionKind::DropManaged { local } => (1, local),
        InstructionKind::StorageDead(local) => (2, local),
        InstructionKind::ScopeExit { .. } => return Some(None),
        _ => return None,''',1)
s=s.replace('''        (Type::Closure(_), 0) => Some((tag, id)),
        (Type::Managed(payload), 1) if **payload == Type::ByteBuffer => Some((tag, id)),''','''        (Type::Closure(_), 0 | 2) => Some(Some((tag, id))),
        (Type::Managed(payload), 1 | 2) if **payload == Type::ByteBuffer => Some(Some((tag, id))),
        (ty, 2) if scalar(ty) => Some(None),''',1)
s=s.replace('''            let Some((tag, local)) = action(function, instruction) else { break; };
            // Account''','''            let Some(action) = action(function, instruction) else { break; };
            at -= 1;
            let Some((tag, local)) = action else { continue; };
            // Account''',1)
s=s.replace('''            at -= 1;
            occurrences += 1;''','''            occurrences += 1;''',1)
s=s.replace('''        let mut f=input(&vec![vec![0,1,2,3]; 4]);''','''        let sequences=vec![vec![0,1,2,3];4];
        let mut f=input(&sequences);''',1)
s=s.replace('''        let f=input(&vec![vec![0,1,2,3]; 4]);''','''        let sequences=vec![vec![0,1,2,3];4];
        let f=input(&sequences);''',1)
s=s.replace('''kind:InstructionKind::StorageDead(LocalId(0)),span:Span::default()});}''','''kind:InstructionKind::StorageLive(LocalId(0)),span:Span::default()});}''',1)
p.write_text(s)
