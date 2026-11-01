from pathlib import Path
import argparse
p=argparse.ArgumentParser(description='Apply timing only to a private 66f15461 source snapshot')
p.add_argument('--source',type=Path,required=True)
root=p.parse_args().source.resolve()
assert 'struct ResearchLap' not in (root/'crates/zil-driver/src/lib.rs').read_text()
def edit(file,pairs):
 p=root/file;s=p.read_text()
 for old,new in pairs:
  assert s.count(old)==1,(file,old[:100],s.count(old))
  s=s.replace(old,new)
 p.write_text(s)
helper='''
struct ResearchLap(std::time::Instant);
impl ResearchLap {
    fn new() -> Self { Self(std::time::Instant::now()) }
    fn lap(&mut self, name: &str) {
        let elapsed = self.0.elapsed().as_nanos();
        eprintln!("RESEARCH_TIME {} {}", name, elapsed);
        self.0 = std::time::Instant::now();
    }
}
'''
p=root/'crates/zil-driver/src/lib.rs';p.write_text(p.read_text()+helper)
p=root/'crates/zil-backend-llvm/src/build.rs';p.write_text(p.read_text()+helper)
edit('crates/zil-driver/src/session.rs',[
 ('        let result = execute();','        let mut timer = super::ResearchLap::new();\n        let result = execute();\n        timer.lap(key.phase.as_str());'),
 ('        let mut analysis_key = typed_snapshot.key.clone();','        let mut timer = super::ResearchLap::new();\n        let mut analysis_key = typed_snapshot.key.clone();'),
 ('        let (candidate, _) = queried?;','        let (candidate, _) = queried?;\n        timer.lap("analysis_total");'),
 ('        let verified = typed.verify(candidate, options);','        timer.lap("verify_prepare");\n        let verified = typed.verify(candidate, options);\n        timer.lap("verify");'),
 ('        let lir = zil_lower::lower_view(&verified)?;','        timer.lap("lower_prepare");\n        let lir = zil_lower::lower_view(&verified)?;\n        timer.lap("lower");'),
 ('        let artifact = zil_backend_llvm::build_view_with_cancellation_and_stable_names(','        timer.lap("backend_prepare");\n        let artifact = zil_backend_llvm::build_view_with_cancellation_and_stable_names('),
 ('        event(Phase::Link, QueryDisposition::Executed, report, observer);','        timer.lap("backend_total");\n        event(Phase::Link, QueryDisposition::Executed, report, observer);'),
 ('        report.commands.extend(artifact.commands);','        report.commands.extend(artifact.commands);\n        timer.lap("publication");')])
edit('crates/zil-driver/src/project.rs',[
 ('        let loaded = if options.stop_after == Stage::Parse {','        let mut timer = super::ResearchLap::new();\n        let loaded = if options.stop_after == Stage::Parse {'),
 ('        let mut report = match loaded {','        timer.lap("filesystem_parse");\n        let mut report = match loaded {')])
edit('crates/zil-backend-llvm/src/build.rs',[
 ('    let mut machine = crate::emit::lower_view_with_divisor_policy(','    let mut timer = ResearchLap::new();\n    let mut machine = crate::emit::lower_view_with_divisor_policy('),
 ('    // The opt-in O2 strategy contracts','    timer.lap("machine_lower");\n    // The opt-in O2 strategy contracts'),
 ('        crate::machine_scalar::fold_known_leaf_calls(&mut machine, lir);','        timer.lap("machine_chain");\n        crate::machine_scalar::fold_known_leaf_calls(&mut machine, lir);\n        timer.lap("machine_scalar");'),
 ('    let llvm = machine.to_text();','    timer.lap("machine_stable_prepare");\n    let llvm = machine.to_text();\n    timer.lap("machine_text");'),
 ('    let mut cache_summary = ObjectCacheSummary {','    timer.lap("machine_partition");\n    let mut cache_summary = ObjectCacheSummary {'),
 ('    let mut pending_cache = PendingObjectCache {','    timer.lap("cache_prepare");\n    let mut pending_cache = PendingObjectCache {'),
 ('    let objects = if !split && options.backend == NativeBackend::Clang {','    timer.lap("codegen_prepare");\n    let objects = if !split && options.backend == NativeBackend::Clang {'),
 ('    if let Some(config) = &options.windows {','    timer.lap("codegen_units");\n    if let Some(config) = &options.windows {'),
 ('    let recorded_command = commands[0].clone();','    timer.lap("link");\n    let recorded_command = commands[0].clone();')])
