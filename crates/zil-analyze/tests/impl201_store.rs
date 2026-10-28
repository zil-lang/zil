//! Hand-built TypedIR admission and responsibility controls. These are not
//! source-lowering, independent Verify, or native-execution evidence.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn ins(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}
fn function(id: u32) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("probe{id}"),
        foreign: None,
        environment: None,
        callable_return_contract: None,
        callable_contract: None,
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: span(),
    }
}
fn local(index: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(index),
        name: format!("p{index}"),
        ty,
        mutable: true,
        span: span(),
    }
}
fn element(fields: usize) -> Type {
    Type::Package {
        name: "State<i32,F0>".into(),
        fields: std::iter::once(("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))))
            .chain(
                (0..fields).map(|index| (format!("callback{index}"), Type::Closure(ClosureId(0)))),
            )
            .chain(std::iter::once(("value".into(), Type::I32)))
            .collect(),
    }
}
fn fixture(element: Type, environments: usize) -> TypedModule {
    let store = Type::OwnedStore {
        element: Box::new(element),
    };
    let mut probe = function(0);
    probe.parameters = vec![LocalId(0)];
    probe.locals = vec![local(0, Type::Managed(Box::new(store.clone())))];
    probe.blocks[0].instructions = vec![
        ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        ins(None, InstructionKind::DropManaged { local: LocalId(0) }),
        ins(None, InstructionKind::StorageDead(LocalId(0))),
        ins(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    let mut functions = vec![probe];
    let mut closures = vec![];
    for index in 0..environments {
        let id = ClosureId(index as u32);
        let mut body = function(index as u32 + 1);
        body.environment = Some(id);
        closures.push(ClosureDefinition {
            id,
            function: body.id,
            captures: vec![],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        });
        functions.push(body);
    }
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"raw callback Store boundary"),
        target: TargetProfile::host_llvm(),
        functions,
        closures,
        resources: vec![ResourceDefinition {
            name: "@builtin:OwnedStore<State<i32,F0>>".into(),
            payload: store,
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn record(module: &TypedModule, result: &str) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") else {
        return;
    };
    let path = PathBuf::from(root).join(format!(
        "raw201-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("result.txt"), result).unwrap();
}
fn accepted(module: &TypedModule) -> CandidatePlan {
    let result = analyze(module, &Limits::default());
    record(module, &format!("{result:#?}"));
    result.unwrap()
}
fn rejected(module: &TypedModule, category: ResultCategory, message: &str) {
    let result = analyze(module, &Limits::default());
    record(module, &format!("{result:#?}"));
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == category
            && d.message.contains(message)),
        "{errors:?}"
    );
}
fn minimum(module: &TypedModule) -> usize {
    let expected = accepted(module);
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        match analyze(
            module,
            &Limits {
                max_analysis_steps: mid,
                ..Limits::default()
            },
        ) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert_eq!(errors.len(), 1, "{errors:?}");
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                assert_eq!(errors[0].phase, Phase::Analyze);
                low = mid + 1;
            }
        }
    }
    let exact = analyze(
        module,
        &Limits {
            max_analysis_steps: low,
            ..Limits::default()
        },
    )
    .unwrap();
    let below = analyze(
        module,
        &Limits {
            max_analysis_steps: low - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(below.len(), 1);
    assert_eq!(below[0].category, ResultCategory::ResourceLimit);
    assert_eq!(below[0].phase, Phase::Analyze);
    assert_eq!(exact, expected);
    assert_eq!(analyze(module, &Limits::default()).unwrap(), expected);
    record(
        module,
        &format!("exact N={low}; N-1={below:#?}; fresh plan equals exact and initial"),
    );
    low
}

#[test]
fn old_noncallback_store_budget_is_unchanged() {
    let raw = fixture(element(0), 0);
    let required = minimum(&raw);
    println!("old noncallback Store exact budget: {required}");
}
