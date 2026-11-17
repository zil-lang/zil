use super::*;
use zil_base::Span;
use zil_ir::{BasicBlock, BlockId, CallKind, PassingMode, PersistentVec};

fn function(value: i32) -> TypedFunction {
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(0),
        name: "template".into(),
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![].into(),
        values: vec![Type::I32].into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Constant(Constant::I32(value)),
                span: Span::default(),
            }],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into(),
        entry: BlockId(0),
        span: Span::default(),
    }
}

#[test]
fn current_wrappers_share_three_cost_scans_and_keep_complete_header_binding() {
    let templates = [function(1), function(2), function(3)];
    let functions: PersistentVec<_> = (0..5105)
        .map(|index| {
            let mut f = templates[index % 3].clone();
            f.id = FunctionId(index as u32);
            f.name = format!("current_{index}");
            f
        })
        .collect();
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Inputs>();
    let inputs = Inputs::new(&functions);
    assert_eq!(inputs.observations.len(), 3);
    assert_eq!(inputs.owner_reuses, 5102);
    // A different wrapper allocation is the actual LIR transport case.
    let wrappers: PersistentVec<_> = functions.iter().cloned().collect();
    for (original, wrapper) in functions.iter().zip(&wrappers) {
        assert_eq!(key(Some(&inputs), wrapper), key(Some(&inputs), original));
        assert!(same(Some(&inputs), original, wrapper));
        assert!(cacheable_with_inputs(Some(&inputs), wrapper));
    }
    let mut changed = wrappers[0].clone();
    changed.return_type = Type::Tuple(vec![Type::I32]);
    assert!(!same(Some(&inputs), &functions[0], &changed));
    assert!(!cacheable_with_inputs(Some(&inputs), &changed));
    changed = wrappers[0].clone();
    changed.parameters.push(LocalId(0));
    assert!(!same(Some(&inputs), &functions[0], &changed));
    changed = wrappers[0].clone();
    changed.entry = BlockId(1);
    assert!(!same(Some(&inputs), &functions[0], &changed));
    changed = wrappers[0].clone();
    changed.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(99),
        arguments: vec![],
    };
    assert!(!same(Some(&inputs), &functions[0], &changed));
    assert_eq!(inputs.observed_cost(&changed), None);
}

#[test]
fn owner_cost_reuse_preserves_exact_optional_fuel_exhaustion_for_lir_wrappers() {
    let mut template = function(1);
    template.values = vec![Type::I32; 9_997].into();
    assert_eq!(cost(&template), Some(10_000));
    let functions: PersistentVec<_> = (0..100)
        .map(|index| {
            let mut f = template.clone();
            f.id = FunctionId(index);
            f
        })
        .collect();
    let inputs = Inputs::new(&functions);
    assert_eq!(inputs.observations.len(), 1);
    // 10,000 for the first row + 49 * 20,000 for scan/equality leaves
    // exactly one scan, but no equality fuel. That row keeps its own identity.
    assert_eq!(inputs.identities.len(), 51);
    for (index, f) in functions.iter().enumerate() {
        match index {
            0..50 => assert_eq!(key(Some(&inputs), f), Key::Semantic(0)),
            50 => assert_eq!(key(Some(&inputs), f), Key::Semantic(50)),
            _ => {
                assert!(matches!(key(Some(&inputs), f), Key::Owner(_)));
                assert!(matches!(key(Some(&inputs), &f.clone()), Key::Owner(_)));
            }
        }
    }
}

#[test]
fn independent_equal_bodies_keep_semantic_fallback_and_unsupported_cost_is_exact() {
    let mut second = function(1);
    second.id = FunctionId(1);
    let functions: PersistentVec<_> = vec![function(1), second].into();
    let inputs = Inputs::new(&functions);
    assert_eq!(inputs.observations.len(), 2);
    assert!(same(Some(&inputs), &functions[0], &functions[1]));
    let mut changed = functions[1].clone();
    changed.values[0] = Type::Tuple(vec![Type::I32]);
    assert!(!cacheable_with_inputs(Some(&inputs), &changed));
    assert!(!same(Some(&inputs), &functions[1], &changed));
    let mut markers = function(1);
    markers.environment = Some(ClosureId(0));
    let rejected: PersistentVec<_> = vec![markers.clone(), markers].into();
    let inputs = Inputs::new(&rejected);
    assert_eq!(inputs.observations.len(), 1);
    assert_eq!(inputs.observations[0].cost, None);
    assert!(!cacheable_with_inputs(Some(&inputs), &rejected[0]));
}

fn lir(functions: PersistentVec<TypedFunction>) -> LirModule {
    LirModule {
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: zil_base::ModuleId(0),
        verified_fingerprint: zil_base::Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions,
        closures: vec![],
        resources: vec![],
        layouts: vec![],
    }
}

#[test]
fn scoped_runtime_binds_value_owners_and_keeps_target_and_current_abi_errors() {
    use zil_ir::{AtomicOrdering, SyncCellKind, SyncI32Op};
    let mut atomic = function(1);
    atomic.values[0] = Type::Address {
        mutable: false,
        inner: Box::new(Type::SyncCell(SyncCellKind::AtomicI32)),
    };
    atomic.blocks[0].instructions[0].kind = InstructionKind::SyncI32 {
        op: SyncI32Op::Load,
        address: ValueId(0),
        value: None,
        ordering: AtomicOrdering::SeqCst,
    };
    let mut mutex = atomic.clone();
    mutex.id = FunctionId(1);
    mutex.values[0] = Type::Address {
        mutable: false,
        inner: Box::new(Type::SyncCell(SyncCellKind::MutexI32)),
    };
    assert!(atomic.blocks.ptr_eq(&mutex.blocks));
    let mut model = lir(vec![atomic.clone(), mutex].into());
    model.target.triple = "x86_64-unknown-linux-gnu".into();
    let names = scoped_thread::runtime(&model.as_view())
        .unwrap()
        .into_items()
        .unwrap()
        .into_iter()
        .filter_map(|item| match item {
            m::Item::Function(f) => Some(f.name),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name == "pthread_mutex_lock"));
    assert!(names.iter().any(|name| name == "abort"));
    assert!(!names.iter().any(|name| name == "pthread_create"));
    let mut conflict = function(0);
    conflict.foreign = Some(zil_ir::ForeignDeclaration {
        abi: "C".into(),
        symbol: "pthread_mutex_lock".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
        scoped_callback: None,
    });
    model.functions.push(conflict);
    let errors = scoped_thread::runtime(&model.as_view()).unwrap_err();
    assert_eq!(errors[0].code, "E-CODEGEN-FOREIGN");
    model.target.triple = "unsupported".into();
    assert!(scoped_thread::runtime(&model.as_view()).is_err());
    model.functions = vec![function(7)].into();
    assert!(!scoped_thread::present(&model.as_view()));
    assert!(
        scoped_thread::runtime(&model.as_view())
            .unwrap()
            .into_items()
            .unwrap()
            .is_empty()
    );
    model.functions = vec![atomic].into();
    model.target.triple = "x86_64-unknown-linux-gnu".into();
    assert!(scoped_thread::present(&model.as_view()));
    assert!(
        scoped_thread::runtime(&model.as_view())
            .unwrap()
            .into_items()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn scoped_linker_fold_checks_one_shared_owner_and_updates_each_current_slot() {
    let template = function(1);
    let mut functions: PersistentVec<_> = (0..128)
        .map(|id| {
            let mut f = template.clone();
            f.id = FunctionId(id);
            f
        })
        .collect();
    let mut session = crate::incremental::BackendSession::default();
    assert!(!session.scoped_threads(&functions));
    assert_eq!(session.summary().scoped_thread_functions_checked, 1);
    assert_eq!(session.summary().scoped_thread_functions_reused, 127);
    let mut checkpoint = session.clone();
    let original = functions.clone();
    functions[127].blocks[0].instructions[0].kind = InstructionKind::WithScopedThread {
        worker: FunctionId(0),
        argument: ValueId(0),
        body: ValueId(0),
    };
    assert!(session.scoped_threads(&functions));
    assert_eq!(session.summary().scoped_thread_functions_checked, 1);
    assert!(!checkpoint.scoped_threads(&original));
    let reordered: PersistentVec<_> = functions.iter().rev().cloned().collect();
    assert!(session.scoped_threads(&reordered));
    let removed: PersistentVec<_> = reordered
        .iter()
        .filter(|f| f.id != FunctionId(127))
        .cloned()
        .collect();
    assert!(!session.scoped_threads(&removed));
    assert!(!session.scoped_threads(&PersistentVec::new()));
}

#[test]
fn runtime_projection_keeps_current_runtime_bodies_foreign_errors_and_cow_changes() {
    use zil_ir::{AtomicOrdering, SyncCellKind, SyncI32Op};
    let mut runtime = function(1);
    runtime.id = FunctionId(1000);
    runtime.values[0] = Type::Address {
        mutable: false,
        inner: Box::new(Type::SyncCell(SyncCellKind::MutexI32)),
    };
    runtime.blocks[0].instructions[0].kind = InstructionKind::SyncI32 {
        op: SyncI32Op::Load,
        address: ValueId(0),
        value: None,
        ordering: AtomicOrdering::SeqCst,
    };
    let mut functions: PersistentVec<_> = (0..256)
        .map(|id| {
            let mut f = function(7);
            f.id = FunctionId(id);
            f
        })
        .collect();
    functions.push(runtime.clone());
    let mut full = lir(functions);
    let inputs = Inputs::new(&full.functions);
    let selected = runtime_candidates(Some(&inputs), &full.functions).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0], runtime);
    let mut projected = full.clone();
    projected.functions = selected;
    let emit = |model: &LirModule| {
        scoped_thread::runtime(&model.as_view()).map(|code| code.into_items().unwrap())
    };
    assert_eq!(emit(&full), emit(&projected));
    let mut conflict = function(0);
    conflict.id = FunctionId(2000);
    conflict.foreign = Some(zil_ir::ForeignDeclaration {
        abi: "C".into(),
        symbol: "pthread_mutex_lock".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
        scoped_callback: None,
    });
    full.functions.push(conflict);
    let inputs = Inputs::new(&full.functions);
    projected.functions = runtime_candidates(Some(&inputs), &full.functions).unwrap();
    assert_eq!(projected.functions.len(), 2);
    assert_eq!(emit(&full), emit(&projected));
    assert!(emit(&projected).is_err());
    let mut current = full.functions.clone();
    current[0] = runtime;
    current[0].id = FunctionId(0);
    // A predicate from the old source inventory cannot hide a newly inserted
    // runtime instruction, including when its outer ID and signature match.
    let changed = runtime_candidates(Some(&inputs), &current).unwrap();
    assert!(changed.iter().any(|f| f.id == FunctionId(0)));
}
