from pathlib import Path
p=Path('crates/zil-backend-llvm/src/emit/cleanup_tails.rs');s=p.read_text();old='Span::new(zil_base::FileId(id as u32), local, local + 1)';assert s.count(old)==1;s=s.replace(old,'Span::new(zil_base::FileId(id as u32), local as usize, local as usize + 1)');p.write_text(s)
p=Path('crates/zil-backend-llvm/src/emit.rs');s=p.read_text()
old='.emit(cleanup_tails::plan(';assert s.count(old)==1;s=s.replace(old,'.emit_with_cleanup(cleanup_tails::plan(',1)
old="    fn emit(mut self, cleanup: Option<cleanup_tails::Plan<'_>>) -> CompileResult<m::Code> {";assert s.count(old)==1
new="""    #[cfg(test)]
    fn emit(self) -> CompileResult<m::Code> {
        self.emit_with_cleanup(None)
    }
    fn emit_with_cleanup(mut self, cleanup: Option<cleanup_tails::Plan<'_>>) -> CompileResult<m::Code> {"""
s=s.replace(old,new,1);p.write_text(s)
print('Preserved complete existing emitter controls; corrected test fixture offset types')
