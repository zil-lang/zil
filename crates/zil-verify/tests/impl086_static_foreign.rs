//! Independent raw/wire controls. This crate has no Types or Analyze dependency;
//! these are trust-boundary tests, not source/native or full-language evidence.
use std::collections::BTreeSet;
use zil_base::{
    Endian, FileId, Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;
use zil_verify::{
    verify_ct, verify_ct_bytes, verify_ct_program, verify_ct_program_bytes, verify_module,
    verify_module_bytes,
};

fn span() -> Span {
    Span::new(FileId(0), 10, 11)
}
fn array(ty: Type, length: u32) -> Type {
    Type::Array {
        element: Box::new(ty),
        length,
    }
}
fn address(ty: Type, mutable: bool) -> Type {
    Type::Address {
        mutable,
        inner: Box::new(ty),
    }
}

#[derive(Clone)]
struct Case {
    module: TypedModule,
    plan: CandidatePlan,
}
impl Case {
    fn bind(&mut self) {
        self.plan.module_fingerprint = self.module.fingerprint();
        self.plan.target_fingerprint = self.module.target.fingerprint();
    }
    fn pass(&mut self) {
        self.bind();
        let limits = Limits::default();
        let target = &self.module.target;
        verify_module(self.module.clone(), self.plan.clone(), target, &limits).unwrap();
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        assert!(verify_module_bytes(&bytes, target, &limits).is_ok());
    }
    fn reject(&mut self, expected: &str) {
        self.bind();
        let limits = Limits::default();
        let target = &self.module.target;
        let diagnostics =
            verify_module(self.module.clone(), self.plan.clone(), target, &limits).unwrap_err();
        assert_eq!(diagnostics[0].code, expected, "raw {diagnostics:?}");
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        let diagnostics = verify_module_bytes(&bytes, target, &limits).unwrap_err();
        assert_eq!(diagnostics[0].code, expected, "wire {diagnostics:?}");
    }
}

fn record(name: &str, fields: Vec<Type>) -> Type {
    Type::Record {
        name: name.into(),
        fields: fields
            .into_iter()
            .enumerate()
            .map(|(i, ty)| (format!("f{i}"), ty))
            .collect(),
    }
}

fn callback_type() -> Type {
    Type::ForeignCallback {
        parameters: vec![Type::I32],
        result: Box::new(Type::I32),
    }
}

impl Body {
    fn parameter(&mut self, ty: Type) -> LocalId {
        let local = self.local(ty, false);
        self.function.locals[local.0 as usize].mutable = false;
        self.function.parameters.push(local);
        self.initialized.insert(local);
        local
    }
}

impl Case {
    fn reject_any(&mut self) {
        self.bind();
        let limits = Limits::default();
        assert!(
            verify_module(
                self.module.clone(),
                self.plan.clone(),
                &self.module.target,
                &limits
            )
            .is_err()
        );
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        assert!(verify_module_bytes(&bytes, &self.module.target, &limits).is_err());
    }
}

fn literal(constant: Constant) -> Case {
    let mut body = Body::new(0, "literal", constant.ty());
    let value = body.constant(constant);
    case(vec![body.finish(Some(value))])
}

fn buffered(mutable: bool, ty: Type) -> Case {
    let mut body = Body::new(1, "caller", Type::Unit);
    let local = body.local(ty.clone(), true);
    let pointer = body.borrow(local, mutable);
    body.enter();
    body.call(vec![pointer]);
    body.exit();
    case(vec![
        external(vec![address(ty, mutable)]),
        body.finish(None),
    ])
}

fn callback_case() -> Case {
    let cb = callback_type();
    let mut ext = external(vec![cb.clone()]);
    ext.0.foreign.as_mut().unwrap().static_callbacks = true;
    let mut body = Body::new(1, "register", Type::Unit);
    let reference = body.value(
        cb,
        InstructionKind::ForeignFunctionRef {
            function: FunctionId(2),
        },
        vec![],
    );
    body.enter();
    body.call(vec![reference]);
    body.exit();
    let mut callback = Body::new(2, "callback", Type::I32);
    let parameter = callback.parameter(Type::I32);
    let returned = callback.value(Type::I32, InstructionKind::Load(parameter), vec![]);
    case(vec![
        ext,
        body.finish(None),
        callback.finish(Some(returned)),
    ])
}

#[test]
fn u16_remains_an_ordinary_integer_in_runtime_and_both_ct_entries() {
    for constant in [
        Constant::U8(255),
        Constant::U16(65535),
        Constant::I32(-1),
        Constant::I64(i64::MAX),
    ] {
        for target in [Type::U8, Type::U16, Type::I32, Type::I64] {
            let mut body = Body::new(0, "convert", target.clone());
            let input = body.constant(constant.clone());
            let output = body.value(
                target.clone(),
                InstructionKind::ConvertInteger {
                    value: input,
                    target,
                },
                vec![],
            );
            case(vec![body.finish(Some(output))]).pass();
        }
    }
    for op in [
        BinaryOp::Add,
        BinaryOp::AddWrapping,
        BinaryOp::ShiftLeft,
        BinaryOp::ShiftRight,
        BinaryOp::BitXor,
    ] {
        let mut body = Body::new(0, "u16", Type::U16);
        let left = body.constant(Constant::U16(65535));
        let right = body.constant(Constant::U16(1));
        let out = body.value(
            Type::U16,
            InstructionKind::Binary { op, left, right },
            vec![],
        );
        case(vec![body.finish(Some(out))]).pass();
    }
    let mut source = literal(Constant::U16(65535));
    source.bind();
    let raw = CtProgram {
        module: source.module.clone(),
        plan: source.plan.clone(),
        entry: FunctionId(0),
        max_steps: 100,
        max_call_depth: 8,
        max_live_slots: 32,
    };
    verify_ct_program(raw.clone(), &source.module.target, &Limits::default()).unwrap();
    let bytes = wire::encode_ct_program(&raw, &Limits::default()).unwrap();
    verify_ct_program_bytes(&bytes, &source.module.target, &Limits::default()).unwrap();
    let flat = CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: Fingerprint::ZERO,
        target: source.module.target.clone(),
        dependencies: vec![],
        instructions: vec![CtInstruction {
            result: ValueId(0),
            ty: Type::U16,
            kind: CtInstructionKind::Constant(Constant::U16(65535)),
        }],
        result: ValueId(0),
        max_steps: 8,
    };
    verify_ct(flat.clone(), &flat.target, &Limits::default()).unwrap();
    let bytes = wire::encode_ct(&flat, &Limits::default()).unwrap();
    verify_ct_bytes(&bytes, &flat.target, &Limits::default()).unwrap();
}

#[test]
fn static_utf16_and_nullable_values_roundtrip_on_both_supported_targets() {
    for constant in [
        Constant::NullHandle,
        Constant::NullWideString,
        Constant::WideString(vec![0]),
        Constant::WideString(vec![0x4e2d, 0x6587, 0]),
        Constant::WideString(vec![0xd83d, 0xde42, 0]),
    ] {
        literal(constant.clone()).pass();
        let mut windows = literal(constant);
        windows.module.target = TargetProfile::windows_x64_llvm();
        windows.pass();
    }
}

#[test]
fn raw_utf16_cannot_omit_or_embed_nul_or_forge_surrogate_pairs() {
    for units in [
        vec![],
        vec![65],
        vec![0, 0],
        vec![65, 0, 66, 0],
        vec![0xd800, 0],
        vec![0xdc00, 0],
        vec![0xd800, 65, 0],
        vec![0xd800, 0xd800, 0],
    ] {
        literal(Constant::WideString(units)).reject("E-VERIFY-FOREIGN-STRING");
    }
}

#[test]
fn opaque_handles_only_gain_copy_storage_and_equality_not_pointer_authority() {
    for op in [
        BinaryOp::Equal,
        BinaryOp::NotEqual,
        BinaryOp::Less,
        BinaryOp::Add,
        BinaryOp::BitAnd,
    ] {
        let mut body = Body::new(0, "handle_op", Type::Bool);
        let left = body.constant(Constant::NullHandle);
        let right = body.constant(Constant::NullHandle);
        let result = body.value(
            Type::Bool,
            InstructionKind::Binary { op, left, right },
            vec![],
        );
        let mut test = case(vec![body.finish(Some(result))]);
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) {
            test.pass();
        } else {
            test.reject("E-VERIFY-TYPE");
        }
    }
    let mut body = Body::new(0, "handle_cast", Type::I64);
    let value = body.constant(Constant::NullHandle);
    let converted = body.value(
        Type::I64,
        InstructionKind::ConvertInteger {
            value,
            target: Type::I64,
        },
        vec![],
    );
    case(vec![body.finish(Some(converted))]).reject("E-VERIFY-TYPE");
    let mut forged = literal(Constant::NullHandle);
    forged.module.functions[0].return_type = address(Type::I32, false);
    forged.module.functions[0].values[0] = address(Type::I32, false);
    forged.reject_any();
    let mut dereference = Body::new(0, "dereference", Type::I32);
    let handle = dereference.constant(Constant::NullHandle);
    let read = dereference.value(
        Type::I32,
        InstructionKind::ReadAddress { address: handle },
        vec![],
    );
    case(vec![dereference.finish(Some(read))]).reject_any();
    buffered(true, Type::ForeignHandle).pass();
}

#[test]
fn c_records_distinguish_shared_static_values_from_mutable_all_bit_valid_data() {
    let pod = record(
        "Mutable",
        vec![
            Type::I32,
            Type::ForeignHandle,
            array(Type::U16, 4),
            record("Point", vec![Type::I32, Type::I32]),
        ],
    );
    buffered(true, pod.clone()).pass();
    buffered(false, pod).pass();
    let static_data = record("Static", vec![Type::CWideString, Type::ForeignHandle]);
    buffered(false, static_data.clone()).pass();
    buffered(true, static_data).reject("E-VERIFY-FOREIGN");
    for leaf in [Type::CWideString, callback_type()] {
        let nested = record("Outer", vec![array(record("Inner", vec![leaf]), 2)]);
        let mut shared = case(vec![external(vec![address(nested.clone(), false)])]);
        shared.module.functions[0]
            .foreign
            .as_mut()
            .unwrap()
            .static_callbacks = true;
        shared.pass();
        let mut mutable = case(vec![external(vec![address(nested, true)])]);
        mutable.module.functions[0]
            .foreign
            .as_mut()
            .unwrap()
            .static_callbacks = true;
        mutable.reject("E-VERIFY-FOREIGN");
    }
    for leaf in [
        Type::Bool,
        Type::Unit,
        address(Type::I32, false),
        Type::Tuple(vec![Type::I32]),
        Type::Enum {
            name: "E".into(),
            variants: vec![("A".into(), vec![])],
        },
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::Unit),
        },
        array(Type::I32, 0),
        record("Empty", vec![]),
    ] {
        for mutable in [false, true] {
            let nested = record("Outer", vec![array(record("Inner", vec![leaf.clone()]), 2)]);
            case(vec![external(vec![address(nested, mutable)])]).reject("E-VERIFY-FOREIGN");
        }
    }
}

#[test]
fn direct_native_parameters_and_results_have_directional_validity() {
    for ty in [
        Type::U8,
        Type::U16,
        Type::I32,
        Type::I64,
        Type::ForeignHandle,
        Type::CWideString,
    ] {
        case(vec![external(vec![ty])]).pass();
    }
    for result in [
        Type::Unit,
        Type::U8,
        Type::U16,
        Type::I32,
        Type::I64,
        Type::ForeignHandle,
    ] {
        let mut declaration = case(vec![external(vec![])]);
        declaration.module.functions[0].return_type = result;
        declaration.pass();
    }
    for result in [
        Type::CWideString,
        callback_type(),
        address(Type::I32, false),
        record("R", vec![Type::I32]),
    ] {
        let mut declaration = case(vec![external(vec![])]);
        declaration.module.functions[0].return_type = result;
        declaration.reject("E-VERIFY-FOREIGN");
    }
    case(vec![external(vec![record("ByValue", vec![Type::I32])])]).reject("E-VERIFY-FOREIGN");
}

#[test]
fn callback_reentry_is_explicit_for_direct_and_transitively_nested_parameters() {
    let mut valid = callback_case();
    valid.pass();
    valid.module.functions[0]
        .foreign
        .as_mut()
        .unwrap()
        .static_callbacks = false;
    valid.reject("E-VERIFY-FOREIGN");
    let nested = record(
        "Callbacks",
        vec![array(record("Nested", vec![callback_type()]), 2)],
    );
    let mut declaration = case(vec![external(vec![address(nested, false)])]);
    declaration.reject("E-VERIFY-FOREIGN");
    declaration.module.functions[0]
        .foreign
        .as_mut()
        .unwrap()
        .static_callbacks = true;
    declaration.pass();
    // A call may reenter an earlier registration without a callback parameter.
    let mut reentry = case(vec![external(vec![address(Type::I32, true)])]);
    reentry.module.functions[0]
        .foreign
        .as_mut()
        .unwrap()
        .static_callbacks = true;
    reentry.pass();
}

#[test]
fn static_callback_and_wide_values_can_share_one_initialized_outbound_record() {
    let ty = record("Registration", vec![Type::CWideString, callback_type()]);
    let mut ext = external(vec![address(ty.clone(), false)]);
    ext.0.foreign.as_mut().unwrap().static_callbacks = true;
    let mut body = Body::new(1, "register_record", Type::Unit);
    let name = body.constant(Constant::WideString(vec![0x754c, 0x9762, 0]));
    let cb = body.value(
        callback_type(),
        InstructionKind::ForeignFunctionRef {
            function: FunctionId(2),
        },
        vec![],
    );
    let value = body.value(
        ty.clone(),
        InstructionKind::MakeAggregate {
            fields: vec![name, cb],
        },
        vec![],
    );
    let local = body.local(ty, false);
    body.instruction(InstructionKind::Store { local, value });
    let address = body.borrow(local, false);
    body.enter();
    body.call(vec![address]);
    body.exit();
    let callback = callback_case();
    case(vec![
        ext,
        body.finish(None),
        (
            callback.module.functions[2].clone(),
            callback.plan.functions[2].clone(),
        ),
    ])
    .pass();
}

#[test]
fn managed_native_tokens_keep_cleanup_debt_and_teardown_cannot_be_a_callback() {
    let payload = record("NativeOwner", vec![Type::ForeignHandle]);
    let managed = Type::Managed(Box::new(payload.clone()));
    let mut ext = external(vec![]);
    ext.0.return_type = Type::ForeignHandle;
    let mut body = Body::new(1, "managed", Type::Unit);
    let local = body.local(managed.clone(), false);
    body.enter();
    let token = body.value(
        Type::ForeignHandle,
        InstructionKind::ForeignCall {
            function: FunctionId(0),
            arguments: vec![],
            region: 0,
        },
        vec![],
    );
    body.exit();
    let owner = body.value(
        managed,
        InstructionKind::MakeManaged {
            fields: vec![token],
        },
        vec![],
    );
    body.instruction(InstructionKind::Store {
        local,
        value: owner,
    });
    let desc = body.value(
        Type::Desc(Box::new(payload.clone())),
        InstructionKind::DescribeLocal { local },
        vec![],
    );
    body.value(
        Type::ForeignHandle,
        InstructionKind::ReadDesc { desc, index: 0 },
        vec![],
    );
    body.instruction(InstructionKind::DropManaged { local });
    body.instruction(InstructionKind::StorageDead(local));
    let mut test = case(vec![ext, body.finish(None)]);
    test.module.resources.push(ResourceDefinition {
        name: "NativeOwner".into(),
        payload: payload.clone(),
        teardown: None,
    });
    test.pass();
    let mut missing = test;
    missing.module.functions[1].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { .. }));
    missing.reject_any();

    let mut teardown = callback_case();
    let target = &mut teardown.module.functions[2];
    target.locals[0].ty = Type::Desc(Box::new(payload.clone()));
    target.return_type = Type::Unit;
    target.values.clear();
    target.blocks[0].instructions.clear();
    target.blocks[0].terminator = Terminator::Return(None);
    teardown.plan.functions[2].holds.clear();
    teardown.module.resources.push(ResourceDefinition {
        name: "NativeOwner".into(),
        payload,
        teardown: Some(FunctionId(2)),
    });
    teardown.reject("E-VERIFY-FOREIGN-CALLBACK");
}

#[test]
fn callback_sources_are_real_static_definitions_and_keep_their_actual_signature() {
    for id in [FunctionId(0), FunctionId(99)] {
        let mut wrong = callback_case();
        wrong.module.functions[1].blocks[0].instructions[0].kind =
            InstructionKind::ForeignFunctionRef { function: id };
        wrong.reject("E-VERIFY-FOREIGN-CALLBACK");
    }
    for parameter in [
        Type::U8,
        Type::U16,
        Type::Bool,
        Type::CWideString,
        address(Type::I32, false),
        callback_type(),
    ] {
        let mut wrong = callback_case();
        wrong.module.functions[2].locals[0].ty = parameter;
        wrong.reject("E-VERIFY-FOREIGN-CALLBACK");
    }
    for result in [Type::U16, Type::Bool, Type::CWideString, callback_type()] {
        let mut wrong = callback_case();
        wrong.module.functions[2].return_type = result;
        wrong.reject("E-VERIFY-FOREIGN-CALLBACK");
    }
    let mut mismatch = callback_case();
    mismatch.module.functions[1].values[0] = Type::ForeignCallback {
        parameters: vec![Type::I64],
        result: Box::new(Type::I32),
    };
    mismatch.reject("E-VERIFY-TYPE");
    let mut bodyless = callback_case();
    bodyless.module.functions[2].blocks.clear();
    bodyless.plan.functions[2].blocks.clear();
    bodyless.reject("E-VERIFY-FOREIGN-CALLBACK");
    let mut environment = callback_case();
    environment.module.functions[2].environment = Some(ClosureId(0));
    environment.module.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(2),
        captures: vec![],
        parameters: vec![Type::I32],
        result: Type::I32,
        kind: CallableKind::Fn,
    });
    environment.reject("E-VERIFY-FOREIGN-CALLBACK");
}

#[test]
fn foreign_callbacks_cannot_be_forged_or_used_as_ordinary_function_pointers() {
    let mut ordinary = callback_case();
    ordinary.module.functions[1].blocks[0].instructions[0].kind = InstructionKind::FunctionRef {
        function: FunctionId(2),
    };
    ordinary.reject("E-VERIFY-TYPE");
    let mut integer = callback_case();
    integer.module.functions[1].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I64(42));
    integer.reject("E-VERIFY-TYPE");
    let mut indirect = callback_case();
    let function = &mut indirect.module.functions[1];
    function.values.extend([Type::I32, Type::I32]);
    function.blocks[0].instructions.extend([
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::Constant(Constant::I32(5)),
            span: span(),
        },
        Instruction {
            result: Some(ValueId(2)),
            kind: InstructionKind::CallIndirect {
                callee: ValueId(0),
                arguments: vec![ValueId(1)],
            },
            span: span(),
        },
    ]);
    indirect.plan.functions[1].holds.extend([
        ValueHold {
            value: ValueId(1),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(2),
            roots: vec![],
        },
    ]);
    indirect.reject("E-VERIFY-INDIRECT-CALL");
    for parameter in [
        Type::U16,
        Type::Bool,
        Type::CWideString,
        address(Type::I32, false),
    ] {
        let invalid = Type::ForeignCallback {
            parameters: vec![parameter],
            result: Box::new(Type::I32),
        };
        let mut declaration = case(vec![external(vec![invalid])]);
        declaration.module.functions[0]
            .foreign
            .as_mut()
            .unwrap()
            .static_callbacks = true;
        declaration.reject("E-VERIFY-FOREIGN-CALLBACK");
    }
}

#[test]
fn c_records_keep_exact_nominal_fields_and_existing_loan_initialization_rules() {
    let payload = record("Payload", vec![Type::I32, Type::ForeignHandle]);
    let valid = buffered(true, payload.clone());
    for change in 0..3 {
        let mut wrong = valid.clone();
        let Type::Address { inner, .. } = &mut wrong.module.functions[0].locals[0].ty else {
            unreachable!()
        };
        let Type::Record { name, fields } = inner.as_mut() else {
            unreachable!()
        };
        match change {
            0 => *name = "Different".into(),
            1 => fields[0].0 = "renamed".into(),
            _ => fields.swap(0, 1),
        }
        wrong.reject("E-VERIFY-FOREIGN");
    }
    let mut noescape = valid.clone();
    noescape.module.functions[0]
        .foreign
        .as_mut()
        .unwrap()
        .noescape_parameters
        .clear();
    noescape.reject("E-VERIFY-FOREIGN");
    let mut outside = callback_case();
    outside.module.functions[1].blocks[0]
        .instructions
        .retain(|i| {
            !matches!(
                i.kind,
                InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. }
            )
        });
    outside.reject("E-VERIFY-UNSAFE");
    for initialized in [false, true] {
        let mut body = Body::new(1, "storage", Type::Unit);
        let local = body.local(payload.clone(), initialized);
        let pointer = body.borrow(local, false);
        if initialized {
            body.instruction(InstructionKind::StorageDead(local));
        }
        body.enter();
        body.call(vec![pointer]);
        body.exit();
        case(vec![
            external(vec![address(payload.clone(), false)]),
            body.finish(None),
        ])
        .reject(if initialized {
            "E-BORROW-CONFLICT"
        } else {
            "E-VERIFY-INITIALIZATION"
        });
    }
    let mut body = Body::new(1, "alias", Type::Unit);
    let local = body.local(payload.clone(), true);
    let parent = body.borrow(local, true);
    let child = body.reborrow(parent, false);
    body.enter();
    body.call(vec![parent, child]);
    body.exit();
    case(vec![
        external(vec![
            address(payload.clone(), true),
            address(payload, false),
        ]),
        body.finish(None),
    ])
    .reject_any();
}

#[test]
fn new_external_profile_rejects_target_confusion_without_removing_old_scalar_profiles() {
    for change in 0..4 {
        let mut native = literal(Constant::NullHandle);
        match change {
            0 => native.module.target.triple = "aarch64-unknown-linux-gnu".into(),
            1 => native.module.target.triple = "x86_64-pc-windows-gnu".into(),
            2 => native.module.target.pointer_width = 32,
            _ => native.module.target.endian = Endian::Big,
        }
        native.reject_any();
        let mut record_only = case(vec![external(vec![address(
            record("R", vec![Type::I32]),
            false,
        )])]);
        record_only.module.target = native.module.target;
        record_only.reject_any();
    }
    let mut old = literal(Constant::U16(65535));
    old.module.target.triple = "i686-unknown-linux-gnu".into();
    old.module.target.pointer_width = 32;
    old.pass();
    let mut hidden = case(vec![external(vec![])]);
    hidden.module.resources.push(ResourceDefinition {
        name: "Hidden".into(),
        payload: record("Hidden", vec![Type::ForeignHandle]),
        teardown: None,
    });
    hidden.module.target.triple = "aarch64-unknown-linux-gnu".into();
    hidden.reject_any();
}

#[test]
fn nested_c_layout_multiplication_cannot_overflow_into_an_accepted_record() {
    let mut huge = Type::I64;
    for _ in 0..5 {
        huge = array(huge, 10000);
    }
    case(vec![external(vec![address(
        record("Huge", vec![huge]),
        true,
    )])])
    .reject("E-VERIFY-FOREIGN");
}

#[test]
fn foreign_values_are_closed_to_both_ct_entrypoints_even_with_forged_type_claims() {
    for constant in [
        Constant::NullHandle,
        Constant::NullWideString,
        Constant::WideString(vec![65, 0]),
    ] {
        let mut source = literal(constant.clone());
        source.bind();
        let raw = CtProgram {
            module: source.module.clone(),
            plan: source.plan.clone(),
            entry: FunctionId(0),
            max_steps: 100,
            max_call_depth: 8,
            max_live_slots: 32,
        };
        assert!(verify_ct_program(raw.clone(), &source.module.target, &Limits::default()).is_err());
        let bytes = wire::encode_ct_program(&raw, &Limits::default()).unwrap();
        assert!(
            verify_ct_program_bytes(&bytes, &source.module.target, &Limits::default()).is_err()
        );
        for claim in [constant.ty(), Type::I32, array(Type::ForeignHandle, 1)] {
            let flat = CtModule {
                schema_version: SCHEMA_VERSION,
                input_fingerprint: Fingerprint::ZERO,
                target: source.module.target.clone(),
                dependencies: vec![],
                instructions: vec![CtInstruction {
                    result: ValueId(0),
                    ty: claim,
                    kind: CtInstructionKind::Constant(constant.clone()),
                }],
                result: ValueId(0),
                max_steps: 8,
            };
            assert_eq!(
                verify_ct(flat.clone(), &flat.target, &Limits::default()).unwrap_err()[0].code,
                "E-CT-FORBIDDEN"
            );
            let bytes = wire::encode_ct(&flat, &Limits::default()).unwrap();
            assert_eq!(
                verify_ct_bytes(&bytes, &flat.target, &Limits::default()).unwrap_err()[0].code,
                "E-CT-FORBIDDEN"
            );
        }
    }
}

#[test]
fn pure_static_callback_construction_does_not_grant_ct_execution() {
    let mut body = Body::new(0, "static_ref", callback_type());
    let reference = body.value(
        callback_type(),
        InstructionKind::ForeignFunctionRef {
            function: FunctionId(1),
        },
        vec![],
    );
    let mut callback = Body::new(1, "callback", Type::I32);
    let parameter = callback.parameter(Type::I32);
    let returned = callback.value(Type::I32, InstructionKind::Load(parameter), vec![]);
    let mut test = case(vec![
        body.finish(Some(reference)),
        callback.finish(Some(returned)),
    ]);
    test.pass();
    let raw = CtProgram {
        module: test.module.clone(),
        plan: test.plan.clone(),
        entry: FunctionId(0),
        max_steps: 100,
        max_call_depth: 8,
        max_live_slots: 32,
    };
    assert_eq!(
        verify_ct_program(raw.clone(), &test.module.target, &Limits::default()).unwrap_err()[0]
            .code,
        "E-CT-FORBIDDEN"
    );
    let bytes = wire::encode_ct_program(&raw, &Limits::default()).unwrap();
    assert_eq!(
        verify_ct_program_bytes(&bytes, &test.module.target, &Limits::default()).unwrap_err()[0]
            .code,
        "E-CT-FORBIDDEN"
    );
}

#[test]
fn raw_and_wire_static_units_obey_the_same_collection_bound() {
    let mut test = literal(Constant::WideString(vec![65, 66, 67, 68, 0]));
    test.bind();
    let limits = Limits {
        max_nodes: 4,
        ..Limits::default()
    };
    assert_eq!(
        verify_module(
            test.module.clone(),
            test.plan.clone(),
            &test.module.target,
            &limits
        )
        .unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
    let bytes =
        wire::encode_module_candidate(&test.module, &test.plan, &Limits::default()).unwrap();
    assert_eq!(
        verify_module_bytes(&bytes, &test.module.target, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}

fn minimum_budget(test: &mut Case, encoded: bool) -> usize {
    test.bind();
    let bytes =
        wire::encode_module_candidate(&test.module, &test.plan, &Limits::default()).unwrap();
    let check = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        };
        if encoded {
            verify_module_bytes(&bytes, &test.module.target, &limits)
        } else {
            verify_module(
                test.module.clone(),
                test.plan.clone(),
                &test.module.target,
                &limits,
            )
        }
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    check(high).unwrap();
    while low < high {
        let middle = low + (high - low) / 2;
        match check(middle) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                low = middle + 1;
            }
        }
    }
    check(low).unwrap();
    assert_eq!(
        check(low - 1).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    low
}

#[test]
fn wide_units_and_callback_records_have_exact_raw_and_wire_work_boundaries() {
    let mut short = literal(Constant::WideString(vec![65, 0]));
    let mut units = vec![65; 2048];
    units.push(0);
    let mut long = literal(Constant::WideString(units));
    let raw_short = minimum_budget(&mut short, false);
    let raw_long = minimum_budget(&mut long, false);
    assert!(raw_long >= raw_short + 2047);
    let wire_long = minimum_budget(&mut long, true);
    let callback_raw = minimum_budget(&mut callback_case(), false);
    let callback_wire = minimum_budget(&mut callback_case(), true);
    println!(
        "086 budget raw_short={raw_short} raw_wide={raw_long} wire_wide={wire_long} raw_callback={callback_raw} wire_callback={callback_wire}"
    );
}

#[test]
fn new_wire_payloads_reject_all_truncation_prefixes_old_versions_and_stale_identity() {
    let mut test = callback_case();
    test.bind();
    let bytes =
        wire::encode_module_candidate(&test.module, &test.plan, &Limits::default()).unwrap();
    for end in 0..bytes.len() {
        assert!(
            verify_module_bytes(&bytes[..end], &test.module.target, &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    for version in (0u32..SCHEMA_VERSION).chain([SCHEMA_VERSION + 1, u32::MAX]) {
        let mut changed = bytes.clone();
        changed[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            verify_module_bytes(&changed, &test.module.target, &Limits::default()).unwrap_err()[0]
                .code,
            "E-WIRE-VERSION"
        );
    }
    let mut wide = literal(Constant::WideString(vec![65, 0]));
    wide.bind();
    wide.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::WideString(vec![66, 0]));
    let errors = verify_module(
        wide.module.clone(),
        wide.plan.clone(),
        &wide.module.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "E-VERIFY-PLAN-IDENTITY");
}

struct Body {
    function: TypedFunction,
    holds: Vec<ValueHold>,
    initialized: BTreeSet<LocalId>,
}
impl Body {
    fn new(id: u32, name: &str, result: Type) -> Self {
        Self {
            function: TypedFunction {
                id: FunctionId(id),
                name: name.into(),
                environment: None,
                callable_contract: None,
                callable_return_contract: None,
                result_mode: PassingMode::Infer,
                foreign: None,
                parameters: vec![],
                return_type: result,
                locals: vec![],
                values: vec![],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Return(None),
                }],
                entry: BlockId(0),
                span: span(),
            },
            holds: vec![],
            initialized: BTreeSet::new(),
        }
    }
    fn instruction(&mut self, kind: InstructionKind) {
        match kind {
            InstructionKind::Store { local, .. } => {
                self.initialized.insert(local);
            }
            InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local)
            | InstructionKind::TakeLocal { local } => {
                self.initialized.remove(&local);
            }
            _ => {}
        }
        self.function.blocks[0].instructions.push(Instruction {
            result: None,
            kind,
            span: span(),
        });
    }
    fn value(&mut self, ty: Type, kind: InstructionKind, roots: Vec<HoldRoot>) -> ValueId {
        let id = ValueId(self.function.values.len() as u32);
        self.function.values.push(ty);
        self.holds.push(ValueHold { value: id, roots });
        self.function.blocks[0].instructions.push(Instruction {
            result: Some(id),
            kind,
            span: span(),
        });
        id
    }
    fn constant(&mut self, c: Constant) -> ValueId {
        self.value(c.ty(), InstructionKind::Constant(c), vec![])
    }
    fn zero(&mut self, ty: &Type) -> ValueId {
        match ty {
            Type::U8 => self.constant(Constant::U8(0)),
            Type::U16 => self.constant(Constant::U16(0)),
            Type::ForeignHandle => self.constant(Constant::NullHandle),
            Type::CWideString => self.constant(Constant::NullWideString),
            Type::Record { fields, .. } => {
                let fields = fields.iter().map(|(_, ty)| self.zero(ty)).collect();
                self.value(
                    ty.clone(),
                    InstructionKind::MakeAggregate { fields },
                    vec![],
                )
            }
            Type::I32 => self.constant(Constant::I32(0)),
            Type::I64 => self.constant(Constant::I64(0)),
            Type::Bool => self.constant(Constant::Bool(false)),
            Type::Array { element, length } => {
                let values = (0..*length).map(|_| self.zero(element)).collect();
                self.value(
                    ty.clone(),
                    InstructionKind::MakeAggregate { fields: values },
                    vec![],
                )
            }
            _ => panic!("unsupported test initializer"),
        }
    }
    fn local(&mut self, ty: Type, initialize: bool) -> LocalId {
        let id = LocalId(self.function.locals.len() as u32);
        self.function.locals.push(LocalDecl {
            id,
            name: format!("local{}", id.0),
            ty: ty.clone(),
            mutable: true,
            span: span(),
        });
        if initialize {
            let value = self.zero(&ty);
            self.instruction(InstructionKind::Store { local: id, value });
        }
        id
    }
    fn borrow(&mut self, local: LocalId, mutable: bool) -> ValueId {
        let ty = address(self.function.locals[local.0 as usize].ty.clone(), mutable);
        let kind = if mutable {
            InstructionKind::BorrowLocalMut { local }
        } else {
            InstructionKind::BorrowLocal { local }
        };
        self.value(ty, kind, vec![HoldRoot::Local(local)])
    }
    fn reborrow(&mut self, value: ValueId, mutable: bool) -> ValueId {
        let Type::Address { inner, .. } = self.function.values[value.0 as usize].clone() else {
            panic!()
        };
        self.value(
            Type::Address { mutable, inner },
            InstructionKind::ReborrowAddress {
                address: value,
                mutable,
            },
            self.holds[value.0 as usize].roots.clone(),
        )
    }
    fn enter(&mut self) {
        self.instruction(InstructionKind::UnsafeEnter { region: 0 });
        self.function.blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .span = Span::new(FileId(0), 0, 100);
    }
    fn call(&mut self, arguments: Vec<ValueId>) {
        self.instruction(InstructionKind::ForeignCall {
            function: FunctionId(0),
            arguments,
            region: 0,
        });
    }
    fn exit(&mut self) {
        self.instruction(InstructionKind::UnsafeExit { region: 0 });
    }
    fn finish(mut self, value: Option<ValueId>) -> (TypedFunction, FunctionPlan) {
        self.function.blocks[0].terminator = Terminator::Return(value);
        let plan = FunctionPlan {
            function: self.function.id,
            holds: self.holds,
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: self.function.parameters.clone(),
                initialized_out: self.initialized.into_iter().collect(),
            }],
        };
        (self.function, plan)
    }
}

fn external(parameters: Vec<Type>) -> (TypedFunction, FunctionPlan) {
    let mut f = Body::new(0, "borrowed_buffer", Type::Unit).function;
    f.blocks.clear();
    f.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "borrowed_buffer".into(),
        static_callbacks: false,
        noescape_parameters: parameters
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, Type::Address { .. }))
            .map(|(i, _)| i as u32)
            .collect(),
    });
    for (i, ty) in parameters.into_iter().enumerate() {
        let id = LocalId(i as u32);
        f.parameters.push(id);
        f.locals.push(LocalDecl {
            id,
            name: format!("p{i}"),
            ty,
            mutable: false,
            span: span(),
        });
    }
    (
        f,
        FunctionPlan {
            function: FunctionId(0),
            blocks: vec![],
            holds: vec![],
        },
    )
}
fn case(parts: Vec<(TypedFunction, FunctionPlan)>) -> Case {
    let (functions, plans) = parts.into_iter().unzip();
    let module = TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"086 independent raw fixture"),
        target: TargetProfile::host_llvm(),
        functions,
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = plans;
    Case { module, plan }
}
