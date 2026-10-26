//! Focused measurement executable, not a compiler mode or product gate.
//! Typed input is prepared once; clone/setup costs are outside Verify timing.
use std::{hint::black_box, time::Instant};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).expect("source path required");
    let rounds: usize = args.get(2).map(|n| n.parse().unwrap()).unwrap_or(7);
    assert!((1..=100).contains(&rounds));
    let text = std::fs::read_to_string(path).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "frontier-bench.zil", text);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &parsed).unwrap();
    let core = zil_types::infer_source_closures(&core, &limits).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let mut samples = Vec::new();
    // Warm both solver paths once. All subsequent results still pass through a
    // fresh independent Verify; no verified snapshot is reused.
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for _ in 0..rounds {
        let start = Instant::now();
        let candidate = zil_analyze::analyze(black_box(&typed), &limits).unwrap();
        let analyze_ns = start.elapsed().as_nanos();
        assert_eq!(candidate, plan);
        let module = typed.clone();
        let candidate = candidate.clone();
        let start = Instant::now();
        let checked = zil_verify::verify_module(module, candidate, &target, &limits).unwrap();
        let verify_ns = start.elapsed().as_nanos();
        black_box(checked);
        let start = Instant::now();
        let report = compile_source(black_box(&source), &options);
        let full_ns = start.elapsed().as_nanos();
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{report:?}"
        );
        samples.push(vec![analyze_ns, verify_ns, full_ns]);
    }
    println!(
        "{{\"source_sha256\":\"{}\",\"candidate_sha256\":\"{}\",\"functions\":{},\"samples_ns\":{:?}}}",
        source.fingerprint().to_hex(),
        Fingerprint::of(&wire).to_hex(),
        typed.functions.len(),
        samples
    );
}
