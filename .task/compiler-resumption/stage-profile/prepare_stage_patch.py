#!/usr/bin/env python3
"""Create a diagnostic-only patch in memory. Never edits the source worktree."""
import argparse, difflib, hashlib, json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
repo=a.repo.resolve();out=a.out.resolve();old={};new={}
def edit(path):
    if path not in old:old[path]=(repo/path).read_text();new[path]=old[path]
    return new[path]
def sub(path,before,after,count=1):
    text=edit(path);actual=text.count(before)
    if actual!=count:raise SystemExit(f'{path}: expected {count} matches, got {actual}: {before[:100]!r}')
    new[path]=text.replace(before,after)
def timed(path,expr,label,count=1):sub(path,expr,f'zil_base::__profile_timed!({json.dumps(label)}, {expr})',count)
base='crates/zil-base/src/lib.rs'
sub(base,'pub mod numeric;','''// Temporary diagnostics only; remove this patch after profiling.
#[doc(hidden)]
pub mod __stage_profile {
    use std::{io::Write, sync::OnceLock, time::Instant};
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    fn enabled() -> bool { *ENABLED.get_or_init(|| std::env::var_os("ZIL_PROFILE_STAGE_TIMING").is_some()) }
    fn now() -> u128 { EPOCH.get_or_init(Instant::now).elapsed().as_nanos() }
    pub struct Span(Option<(String, u128)>);
    pub fn span(name: &str) -> Span {
        Span(enabled().then(|| (name.to_owned(), now())))
    }
    pub fn event(phase: &str, disposition: &str) {
        if enabled() {
            let at = now();
            let _ = writeln!(std::io::stderr().lock(), "{{\\"zil_profile\\":1,\\"kind\\":\\"event\\",\\"pid\\":{},\\"thread\\":\\"{:?}\\",\\"phase\\":{},\\"disposition\\":{},\\"at_ns\\":{at}}}", std::process::id(), std::thread::current().id(), crate::json_string(phase), crate::json_string(disposition));
        }
    }
    impl Drop for Span {
        fn drop(&mut self) {
            if let Some((name, start)) = &self.0 {
                let end = now();
                let _ = writeln!(std::io::stderr().lock(), "{{\\"zil_profile\\":1,\\"kind\\":\\"span\\",\\"pid\\":{},\\"thread\\":\\"{:?}\\",\\"name\\":{},\\"start_ns\\":{start},\\"end_ns\\":{end},\\"wall_ns\\":{}}}", std::process::id(), std::thread::current().id(), crate::json_string(name), end-start);
            }
        }
    }
}
#[doc(hidden)]
#[macro_export]
macro_rules! __profile_timed { ($name:expr, $expression:expr) => {{ let _profile_span = $crate::__stage_profile::span($name); $expression }}; }

pub mod numeric;''')
project='crates/zil-driver/src/project.rs'
sub(project,') -> CompileReport {\n        let start = Instant::now();\n        let mut effective_options = options.clone();',') -> CompileReport {\n        let _profile_total = zil_base::__stage_profile::span("driver.project_request.total");\n        let start = Instant::now();\n        let mut effective_options = options.clone();',2)
# There are two such entry points. Relabel the path adapter's outer scope.
text=edit(project);needle='pub(super) fn compile_project_path('
head,tail=text.split(needle,1);tail=tail.replace('"driver.project_request.total"','"driver.path.total"',1);new[project]=head+needle+tail
timed(project,'load_sources(&mut project, path, options, self)','driver.filesystem_discovery')
sub(project,'self.compile_project_request(&project, options, &mut |_| {}, Some(parsed))','self.compile_project_request(&project, options, &mut |event| zil_base::__stage_profile::event(event.phase.as_str(), event.disposition.as_str()), Some(parsed))')
sub(project,') -> CompileResult<()> {\n        options\n            .cancellation\n            .check(Phase::Driver)',') -> CompileResult<()> {\n        let _profile_pipeline = zil_base::__stage_profile::span("driver.project_pipeline.total");\n        options\n            .cancellation\n            .check(Phase::Driver)')
timed(project,'zil_syntax::parse_with_limits(\n                &project.modules[index],\n                &parse_limits,\n            )','driver.discovery.parse')
semantic='crates/zil-driver/src/semantic_cache.rs'
sub(semantic,'pub(super) fn load(&self, options: &CompileOptions) -> Option<(TypedProgram, CandidatePlan)> {','pub(super) fn load(&self, options: &CompileOptions) -> Option<(TypedProgram, CandidatePlan)> {\n        let _profile_restore = zil_base::__stage_profile::span("driver.semantic_cache.restore");')
session='crates/zil-driver/src/session.rs'
sub(session,') -> CompileResult<(RawSnapshot, Arc<QuerySnapshot>)> {\n        options',') -> CompileResult<(RawSnapshot, Arc<QuerySnapshot>)> {\n        let _profile_query = zil_base::__stage_profile::span(&format!("driver.query.{}", key.phase.as_str()));\n        options')
timed(session,'typed.verify(candidate, options)','driver.verify')
timed(session,'zil_lower::lower_view(&verified)','driver.lower')
timed(session,'fs::read_to_string(&artifact.llvm_ir)','driver.llvm_sidecar.readback')
timed(session,'publication.commit(artifact, &options.cancellation)','driver.artifact.publish')
timed(session,'artifact.commit_object_cache()','driver.object_cache.commit')
backend='crates/zil-backend-llvm/src/build.rs'
sub(backend,') -> CompileResult<BuildArtifact> {\n    check_cancelled(cancelled)?;',') -> CompileResult<BuildArtifact> {\n    let _profile_build = zil_base::__stage_profile::span("backend.total");\n    check_cancelled(cancelled)?;')
expr='''crate::emit::lower_view_with_closed_scalar_policy_and_jobs(
            verified,
            lir,
            &options.emit,
            options.optimization == Optimization::O2 && options.incremental_o2,
            options.closed_executable
                && options.backend == NativeBackend::LlvmApi
                && options.codegen.units >= 2,
            options.codegen.jobs,
        )'''
timed(backend,expr,'backend.machine_lower')
timed(backend,'crate::task_abi::derive(verified, &machine, &mut task_work)','backend.task_abi')
timed(backend,'crate::machine_checked_add_chain::fold_checked_add_chains(&mut machine)','backend.checked_add_chain')
timed(backend,'crate::machine_scalar::fold_known_leaf_calls(&mut machine, lir)','backend.scalar_fold')
timed(backend,'machine.to_text()','backend.diagnostic_render')
timed(backend,'fs::write(&ir_path, llvm.as_bytes())','backend.diagnostic_write')
sub(backend,'fn compile_units(\n','fn compile_units(\n') # assert anchor exists
sub(backend,') -> CompileResult<Vec<UnitResult>> {\n    let mut schedule',') -> CompileResult<Vec<UnitResult>> {\n    let _profile_units = zil_base::__stage_profile::span("backend.object_workers.wall");\n    let mut schedule')
sub(backend,'    use zil_llvm_api::{Config, Optimization as ApiOpt};','    let _profile_unit = zil_base::__stage_profile::span("backend.object_unit.total");\n    use zil_llvm_api::{Config, Optimization as ApiOpt};')
sub(backend,'fn run_command(command: &mut Command, code: &str) -> CompileResult<Vec<String>> {','fn run_command(command: &mut Command, code: &str) -> CompileResult<Vec<String>> {\n    let _profile_external = zil_base::__stage_profile::span("backend.external_tool.wall");')
timed(backend,'zil_llvm_api::validate_module(&unit.module)','backend.current_object_preflight',2)
cgu='crates/zil-backend-llvm/src/machine_cgu.rs'
sub(cgu,') -> (Plan, Option<layout::Pending>) {',') -> (Plan, Option<layout::Pending>) {\n    let _profile_partition = zil_base::__stage_profile::span("backend.partition.total");')
layout='crates/zil-backend-llvm/src/machine_cgu/layout.rs'
sub(layout,'pub(super) fn replay(&self, module: Module) -> Vec<Module> {','pub(super) fn replay(&self, module: Module) -> Vec<Module> {\n        let _profile_replay = zil_base::__stage_profile::span("backend.layout.replay");')
sub(layout,'pub(super) fn load(&self, module: &Module, lir: &LirView<\'_>, count: usize) -> Option<Layout> {','pub(super) fn load(&self, module: &Module, lir: &LirView<\'_>, count: usize) -> Option<Layout> {\n        let _profile_load = zil_base::__stage_profile::span("backend.layout.load_validate");')
sub(layout,') -> Option<Fingerprint> {\n    if !(2..=MAX_UNITS)',') -> Option<Fingerprint> {\n    let _profile_key = zil_base::__stage_profile::span("backend.layout.whole_input_key");\n    if !(2..=MAX_UNITS)')
objects='crates/zil-backend-llvm/src/object_cache.rs'
sub(objects,'fn machine_key(namespace: Fingerprint, module: &Module) -> Fingerprint {','fn machine_key(namespace: Fingerprint, module: &Module) -> Fingerprint {\n    let _profile_key = zil_base::__stage_profile::span("backend.object.action_key");')
for path in ['crates/zil-backend-llvm/src/machine_scalar.rs','crates/zil-backend-llvm/src/machine_checked_add_chain.rs','crates/zil-backend-llvm/src/machine_stable_identity.rs']:
    text=edit(path);n=text.count('zil_llvm_api::validate_module(module)')
    if n:timed(path,'zil_llvm_api::validate_module(module)','backend.transform.full_preflight',n)
patch=''.join(''.join(difflib.unified_diff(old[path].splitlines(True),new[path].splitlines(True),fromfile='a/'+path,tofile='b/'+path,n=3)) for path in old if new[path]!=old[path])
out.parent.mkdir(parents=True,exist_ok=True);out.write_text(patch)
out.with_suffix('.manifest.json').write_text(json.dumps({'repo':str(repo),'patch_sha256':hashlib.sha256(patch.encode()).hexdigest(),'files':{path:{'before_sha256':hashlib.sha256(old[path].encode()).hexdigest(),'after_sha256':hashlib.sha256(new[path].encode()).hexdigest()} for path in old if new[path]!=old[path]},'executed_builds':0,'executed_benchmarks':0},indent=2)+'\n')
print(f'Prepared {out}: {len(old)} inspected files, {len(patch.splitlines())} patch lines; source files unchanged.')
