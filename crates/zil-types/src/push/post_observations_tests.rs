use super::*;
use zil_base::{FileId, Fingerprint, SourceUnit};
use zil_syntax::{ParsedSource, attributes::AttributeContext};

fn program() -> TypedProgram {
    let text = (0..64)
        .map(|i| {
            format!(
                "fn post_fixture_{i}(x:i32)->i32 {{let y=x+1; if y<10 {{return y*2;}}else{{return y+3;}}}} "
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
    let mut program = program();
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
    // Complete-equal output adoption may supply new immutable array owners.
    // Retained scopes must compare their actual contents in that case.
    for function in program.module.functions.iter_mut() {
        function.values = function.values.iter().cloned().collect::<Vec<_>>().into();
        function.locals = function.locals.iter().cloned().collect::<Vec<_>>().into();
    }
    assert!(
        !functions[0]
            .values
            .ptr_eq(&program.module.functions[0].values)
    );
    assert!(
        !functions[0]
            .locals
            .ptr_eq(&program.module.functions[0].locals)
    );
    let receipts = observations
        .receipts(program.module.functions.iter())
        .unwrap();
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
        assert!(observations.receipts(functions.iter()).is_none());
    }
}

#[test]
fn reused_resource_scan_stacks_preserve_nested_roots_and_independent_fee_boundaries() {
    let program = program();
    let mut functions = program
        .module
        .functions
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>();
    for function in &mut functions {
        function.locals = vec![function.locals[0].clone()].into();
        function.return_type = Type::I32;
    }
    functions[0].values = vec![
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(Type::Array {
                element: Box::new(Type::I32),
                length: 2,
            }),
        },
        Type::I32,
    ]
    .into();
    functions[1].values = vec![Type::Managed(Box::new(Type::ByteBuffer))].into();
    // Registration: 7 roots plus 15 charged visits/edges, for 22 total.
    // Byte scan consumes 12 and reserves its
    // resource-name tail; Store's independent scan consumes 10 and no shared
    // work. These obligations must not depend on allocator reuse.
    let byte_bound = 12 + BYTE_BUFFER_RESOURCE_NAME.len() + 2;
    for allowance in 0..=byte_bound + 1 {
        let limits = Limits {
            max_analysis_steps: allowance,
            ..Limits::default()
        };
        let mut definitions = Vec::new();
        let mut remaining = allowance;
        let registration =
            registration::add_definitions(&functions, &mut definitions, &mut remaining);
        assert_eq!(
            registration.is_ok(),
            allowance >= 22,
            "registration {allowance}"
        );
        assert!(definitions.is_empty());
        if let Err(error) = registration {
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        } else {
            assert_eq!(remaining, allowance - 22);
        }
        let mut definitions = Vec::new();
        let bytes = byte_buffer::add_definition(&functions, &mut definitions, &limits);
        assert_eq!(bytes.is_ok(), allowance >= byte_bound, "bytes {allowance}");
        if let Err(error) = bytes {
            assert!(definitions.is_empty());
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        } else {
            assert_eq!(definitions.len(), 1);
            assert_eq!(definitions[0].payload, Type::ByteBuffer);
        }
        let mut definitions = Vec::new();
        let mut shared = 123;
        let stores = dynamic_store::add_definitions(
            &functions,
            &mut definitions,
            &[],
            &[],
            &limits,
            &mut shared,
        );
        assert_eq!(stores.is_ok(), allowance >= 10, "stores {allowance}");
        assert_eq!(shared, 123);
        assert!(definitions.is_empty());
        if let Err(error) = stores {
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        }
    }
}
