use super::*;
use zil_base::{FileId, Fingerprint, SourceUnit};
use zil_syntax::{ParsedSource, attributes::AttributeContext};

fn program() -> TypedProgram {
    let text = (0..64)
        .map(|i| {
            format!(
                "fn f{i}(x:i32)->i32 {{let y=x+1; if y<10 {{return y*2;}}else{{return y+3;}}}} "
            )
        })
        .collect::<String>();
    let parsed = ParsedSource::parse(
        Arc::new(SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)),
        &Limits::default(),
        &AttributeContext::default(),
    );
    assert!(parsed.parsed().is_ok());
    let core = zil_expand::expand(parsed.source(), &parsed.parsed().module).unwrap();
    check_program(
        &CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            modules: vec![ProgramModule {
                exports: vec![],
                core: Arc::new(core),
                imports: vec![],
            }],
        },
        &TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn original_post_consumers_record_the_same_per_function_and_independent_ledger_fees() {
    let program = program();
    let functions = program.module.functions.iter().cloned().collect::<Vec<_>>();
    let limits = Limits::default();
    let mut remaining = limits.max_analysis_steps;
    let mut resources = vec![];
    let mut observations = PostObservations::new(&functions);
    tasks::add_definition_recording(
        &functions,
        &mut resources,
        &mut remaining,
        &limits,
        Some(&mut observations),
    )
    .unwrap();
    registration::add_definitions_recording(
        &functions,
        &mut resources,
        &mut remaining,
        Some(&mut observations),
    )
    .unwrap();
    byte_buffer::add_definition_recording(
        &functions,
        &mut resources,
        &limits,
        Some(&mut observations),
    )
    .unwrap();
    dynamic_store::add_definitions_recording(
        &functions,
        &mut resources,
        &[],
        &[],
        &limits,
        &mut remaining,
        Some(&mut observations),
    )
    .unwrap();
    assert!(resources.is_empty());
    assert_eq!(observations.observed_functions(), 64);
    let receipts = observations.receipts(&functions).unwrap();
    let expected = functions
        .iter()
        .map(|function| PostReceipt::check(function, &limits).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(receipts, expected);
    assert_eq!(
        receipts.iter().map(|receipt| receipt.shared).sum::<usize>(),
        limits.max_analysis_steps - remaining
    );
}

#[test]
fn incomplete_duplicate_and_unattributed_post_work_cannot_create_a_current_context() {
    let program = program();
    let functions = program
        .module
        .functions
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>();
    for failure in 0..3 {
        let mut observations = PostObservations::new(&functions);
        for pass in [
            PostPass::Tasks,
            PostPass::Registration,
            PostPass::Bytes,
            PostPass::Stores,
        ] {
            observations.record(pass, FunctionId(0), 3, 4);
            if failure != 0 {
                observations.record(pass, FunctionId(1), 3, 4);
            }
            if failure == 1 {
                observations.record(pass, FunctionId(0), 3, 4);
            }
            observations.finish(pass, if failure == 2 { 7 } else { 6 }, 8);
        }
        assert_eq!(observations.observed_functions(), 0);
        assert!(observations.receipts(&functions).is_none());
    }
}
