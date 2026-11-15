use super::*;
use zil_base::{Fingerprint, Limits, ModuleId, SCHEMA_VERSION};
use zil_ir::*;

fn function(id: u32, ty: Type, kinds: Vec<InstructionKind>) -> TypedFunction {
    let count = kinds.len();
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(id),
        name: format!("function{id}"),
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: ty.clone(),
        result_mode: PassingMode::Infer,
        locals: vec![].into(),
        values: vec![ty; count].into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: kinds
                .into_iter()
                .enumerate()
                .map(|(index, kind)| Instruction {
                    result: Some(ValueId(index as u32)),
                    kind,
                    span: Span::default(),
                })
                .collect(),
            terminator: Terminator::Return(Some(ValueId(count as u32 - 1))),
        }]
        .into(),
        entry: BlockId(0),
        span: Span::default(),
    }
}

fn call(id: u32) -> InstructionKind {
    InstructionKind::Call {
        function: FunctionId(id),
        arguments: vec![],
    }
}

fn functions() -> Vec<TypedFunction> {
    vec![
        function(
            0,
            Type::I32,
            vec![InstructionKind::Constant(Constant::I32(7))],
        ),
        function(1, Type::I32, vec![call(0)]),
        function(
            2,
            Type::I32,
            vec![InstructionKind::Constant(Constant::I32(11))],
        ),
        function(
            3,
            Type::I32,
            vec![InstructionKind::Constant(Constant::I32(13))],
        ),
        function(
            4,
            Type::I32,
            vec![
                call(2),
                call(3),
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(0),
                    right: ValueId(1),
                },
            ],
        ),
    ]
}

fn checked(functions: Vec<TypedFunction>) -> VerifiedModule {
    let module = TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"scalar-header-dependencies"),
        target: TargetProfile::host_llvm(),
        functions: functions.into(),
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = module
        .functions
        .iter()
        .map(|function| FunctionPlan {
            function: function.id,
            holds: (0..function.values.len())
                .map(|index| ValueHold {
                    value: ValueId(index as u32),
                    roots: vec![],
                })
                .collect(),
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![],
                initialized_out: vec![],
            }]
            .into(),
        })
        .collect();
    let target = module.target.clone();
    zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap()
}

fn lower(verified: &VerifiedModule, session: &mut Session) -> m::Module {
    let module = verified.module();
    let raw = LirModule {
        schema_version: module.schema_version,
        module_id: module.module_id,
        verified_fingerprint: verified.fingerprint(),
        target: module.target.clone(),
        functions: module.functions.clone(),
        closures: module.closures.clone(),
        resources: module.resources.clone(),
        layouts: [
            (Type::Unit, 0, 1),
            (Type::Bool, 1, 1),
            (Type::I32, 4, 4),
            (Type::I64, 8, 8),
            (Type::F32, 4, 4),
            (Type::F64, 8, 8),
            (Type::U8, 1, 1),
            (Type::U16, 2, 2),
            (Type::U32, 4, 4),
            (Type::I8, 1, 1),
            (Type::I16, 2, 2),
            (Type::Isize, 8, 8),
            (Type::ForeignHandle, 8, 8),
            (Type::CWideString, 8, 8),
        ]
        .into_iter()
        .map(|(ty, size, alignment)| ScalarLayout {
            ty,
            size,
            alignment,
        })
        .collect(),
    };
    super::super::lower_view_with_session(
        verified,
        &raw.as_view(),
        &EmitOptions {
            entry: EntryPoint::None,
        },
        false,
        false,
        1,
        Some(session),
    )
    .unwrap()
    .0
}

#[test]
fn header_name_edit_preserves_unrelated_two_callee_caller_and_output_identities() {
    let mut session = Session::default();
    let original = lower(&checked(functions()), &mut session);
    let before = session.identity().unwrap();
    let body = session.output_bodies[4].clone();
    let mut changed = functions();
    changed[0].name = "renamed".into();
    let verified = checked(changed);
    let machine = lower(&verified, &mut session);
    assert_eq!((session.hits, session.misses), (3, 2));
    let after = session.identity().unwrap();
    assert!(!Arc::ptr_eq(&before.0, &after.0));
    assert_eq!(original.items, machine.items);
    assert!(before.1.ptr_eq(&after.1));
    for index in 2..5 {
        assert_eq!(before.1[index], after.1[index]);
    }
    assert!(body.ptr_eq(&session.output_bodies[4]));
    assert_eq!(machine, lower(&verified, &mut Session::default()));
}

#[test]
fn scalar_callee_abi_edit_relowers_consumers_and_preserves_other_headers() {
    let mut session = Session::default();
    lower(&checked(functions()), &mut session);
    let before = session.identity().unwrap();
    let mut changed = functions();
    changed[0] = function(
        0,
        Type::I64,
        vec![InstructionKind::Constant(Constant::I64(7))],
    );
    changed[1] = function(1, Type::I64, vec![call(0)]);
    let verified = checked(changed);
    let machine = lower(&verified, &mut session);
    assert_eq!((session.hits, session.misses), (3, 2));
    let after = session.identity().unwrap();
    assert_ne!(before.1[0], after.1[0]);
    assert_ne!(before.1[1], after.1[1]);
    assert_eq!(before.1[4], after.1[4]);
    assert_eq!(machine, lower(&verified, &mut Session::default()));
}

#[test]
fn own_file_header_is_bound_even_when_source_body_is_identical() {
    let original = functions();
    let mut changed = original.clone();
    changed[2].span.file = zil_base::FileId(9);
    assert!(same_input(&original[2], &changed[2]));
    let mut session = Session::default();
    lower(&checked(original), &mut session);
    let verified = checked(changed);
    let machine = lower(&verified, &mut session);
    assert_eq!((session.hits, session.misses), (3, 2));
    assert_eq!(machine, lower(&verified, &mut Session::default()));
}

#[test]
fn changed_ids_bind_current_headers_and_inventory_edits_fall_back() {
    let mut reidentified = functions();
    reidentified.swap(0, 2);
    reidentified[0].id = FunctionId(0);
    reidentified[2].id = FunctionId(2);
    reidentified[1].blocks[0].instructions[0].kind = call(2);
    reidentified[4].blocks[0].instructions[0].kind = call(0);
    let mut removed = functions();
    removed.truncate(3);
    let mut added = functions();
    added.push(function(
        5,
        Type::I32,
        vec![InstructionKind::Constant(Constant::I32(17))],
    ));
    for (changed, expected) in [(reidentified, (1, 4)), (removed, (0, 3)), (added, (0, 6))] {
        let mut session = Session::default();
        lower(&checked(functions()), &mut session);
        let verified = checked(changed);
        let machine = lower(&verified, &mut session);
        assert_eq!((session.hits, session.misses), expected);
        assert_eq!(machine, lower(&verified, &mut Session::default()));
    }
}
