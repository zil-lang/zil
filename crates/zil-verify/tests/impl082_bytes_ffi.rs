//! Independent raw/wire controls. This crate has no Types or Analyze dependency;
//! these are trust-boundary tests, not source/native or full-language evidence.
use std::collections::BTreeSet;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;
use zil_verify::{
    verify_ct, verify_ct_bytes, verify_ct_program, verify_module, verify_module_bytes,
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
    }
    fn pass(&mut self) {
        self.bind();
        let limits = Limits::default();
        let target = &self.module.target;
        assert!(verify_module(self.module.clone(), self.plan.clone(), target, &limits).is_ok());
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
                initialized_in: vec![],
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
        source_fingerprint: Fingerprint::of(b"082 independent raw fixture"),
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
fn buffered(mutable: bool, target: Type) -> Case {
    let mut b = Body::new(1, "caller", Type::Unit);
    let local = b.local(target.clone(), true);
    let pointer = b.borrow(local, mutable);
    b.enter();
    b.call(vec![pointer]);
    b.exit();
    case(vec![
        external(vec![address(target, mutable)]),
        b.finish(None),
    ])
}
fn integers(c: Constant, target: Type) -> Case {
    let mut b = Body::new(0, "integer", target.clone());
    let value = b.constant(c);
    let result = b.value(
        target.clone(),
        InstructionKind::ConvertInteger { value, target },
        vec![],
    );
    case(vec![b.finish(Some(result))])
}

#[test]
fn every_integer_conversion_and_u8_boundary_is_typed_independently() {
    for c in [
        Constant::U8(0),
        Constant::U8(255),
        Constant::I32(-1),
        Constant::I32(256),
        Constant::I64(i64::MIN),
        Constant::I64(i64::MAX),
    ] {
        for target in [Type::U8, Type::I32, Type::I64] {
            integers(c.clone(), target).pass();
        }
    }
    // The verifier admits checked operations, including those which trap; it
    // does not counterfeit a proof that the conversion's runtime guard passes.
    integers(Constant::Bool(false), Type::U8).reject("E-VERIFY-TYPE");
    integers(Constant::U8(255), Type::Bool).reject("E-VERIFY-TYPE");
    let mut wrong = integers(Constant::U8(255), Type::I32);
    wrong.module.functions[0].values[1] = Type::I64;
    wrong.reject("E-VERIFY-TYPE");
    let mut future = integers(Constant::U8(255), Type::I32);
    future.module.functions[0].blocks[0].instructions[1].kind = InstructionKind::ConvertInteger {
        value: ValueId(1),
        target: Type::I32,
    };
    future.reject("E-VERIFY-DOMINANCE");
}

#[test]
fn u8_integer_operators_and_matching_integer_shift_scope() {
    for op in [
        BinaryOp::Add,
        BinaryOp::Subtract,
        BinaryOp::Multiply,
        BinaryOp::Divide,
        BinaryOp::Remainder,
        BinaryOp::AddWrapping,
        BinaryOp::SubtractWrapping,
        BinaryOp::MultiplyWrapping,
        BinaryOp::BitAnd,
        BinaryOp::BitOr,
        BinaryOp::BitXor,
        BinaryOp::ShiftLeft,
        BinaryOp::ShiftRight,
        BinaryOp::Equal,
        BinaryOp::NotEqual,
        BinaryOp::Less,
        BinaryOp::LessEqual,
        BinaryOp::Greater,
        BinaryOp::GreaterEqual,
    ] {
        let compare = matches!(
            op,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        );
        let result_ty = if compare { Type::Bool } else { Type::U8 };
        let mut b = Body::new(0, "operator", result_ty.clone());
        let left = b.constant(Constant::U8(255));
        let right = b.constant(Constant::U8(8));
        let result = b.value(
            result_ty,
            InstructionKind::Binary { op, left, right },
            vec![],
        );
        case(vec![b.finish(Some(result))]).pass();
    }
    for op in [UnaryOp::Negate, UnaryOp::BitNot] {
        let mut b = Body::new(0, "unary", Type::U8);
        let operand = b.constant(Constant::U8(255));
        let result = b.value(Type::U8, InstructionKind::Unary { op, operand }, vec![]);
        case(vec![b.finish(Some(result))]).pass();
    }
    for ty in [Type::I32, Type::I64] {
        let mut b = Body::new(0, "signed_shift", ty.clone());
        let left = b.zero(&ty);
        let right = b.zero(&ty);
        let result = b.value(
            ty,
            InstructionKind::Binary {
                op: BinaryOp::ShiftLeft,
                left,
                right,
            },
            vec![],
        );
        // 124 opens the existing signed integer operation. Dynamic count bounds
        // are enforced at CT/runtime; they are not a raw admission premise.
        case(vec![b.finish(Some(result))]).pass();
    }
}

#[test]
fn borrowed_c_scalar_and_recursive_array_targets_work_without_new_trust() {
    for target in [
        Type::U8,
        Type::I32,
        Type::I64,
        array(Type::U8, 4),
        array(array(Type::I64, 2), 2),
    ] {
        for mutable in [false, true] {
            buffered(mutable, target.clone()).pass();
        }
    }
}

#[test]
fn noescape_table_must_be_exact_ordered_and_unique() {
    let mut valid = case(vec![external(vec![
        Type::I32,
        address(Type::U8, false),
        address(array(Type::I64, 2), true),
    ])]);
    valid.pass();
    for indices in [
        vec![],
        vec![1],
        vec![2],
        vec![2, 1],
        vec![1, 1],
        vec![0, 1],
        vec![1, 3],
        vec![1, 2, 2],
    ] {
        let mut invalid = valid.clone();
        invalid.module.functions[0]
            .foreign
            .as_mut()
            .unwrap()
            .noescape_parameters = indices;
        invalid.reject("E-VERIFY-FOREIGN");
    }
}

#[test]
fn ffi_nearby_copy_targets_are_not_automatically_c_safe() {
    for target in [
        Type::Unit,
        Type::Bool,
        array(Type::U8, 0),
        array(array(Type::U8, 0), 3),
        array(Type::Bool, 2),
        address(Type::U8, false),
        Type::Tuple(vec![Type::I32]),
        Type::Record {
            name: "R".into(),
            fields: vec![("x".into(), Type::Bool)],
        },
        Type::Enum {
            name: "E".into(),
            variants: vec![("A".into(), vec![])],
        },
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::I32),
        },
    ] {
        case(vec![external(vec![address(target, true)])]).reject("E-VERIFY-FOREIGN");
    }
    for ty in [Type::Unit, Type::Bool, array(Type::U8, 4)] {
        case(vec![external(vec![ty])]).reject("E-VERIFY-FOREIGN");
    }
    for ty in [Type::Bool, address(Type::U8, false)] {
        let mut invalid = case(vec![external(vec![])]);
        invalid.module.functions[0].return_type = ty;
        invalid.reject("E-VERIFY-FOREIGN");
    }
}

#[test]
fn ffi_requires_matching_type_and_explicit_active_unsafe_region() {
    let valid = buffered(true, array(Type::U8, 4));
    let mut mismatch = valid.clone();
    mismatch.module.functions[0].locals[0].ty = address(array(Type::U8, 3), true);
    mismatch.reject("E-VERIFY-FOREIGN");
    let mut shared = valid.clone();
    shared.module.functions[0].locals[0].ty = address(array(Type::U8, 4), false);
    shared.reject("E-VERIFY-FOREIGN");
    let mut missing = valid.clone();
    missing.module.functions[1].blocks[0]
        .instructions
        .retain(|i| {
            !matches!(
                i.kind,
                InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. }
            )
        });
    missing.reject("E-VERIFY-UNSAFE");
    let mut wrong_region = valid.clone();
    for instruction in &mut wrong_region.module.functions[1].blocks[0].instructions {
        if let InstructionKind::ForeignCall { region, .. } = &mut instruction.kind {
            *region = 1;
        }
    }
    wrong_region.reject("E-VERIFY-UNSAFE");
}

#[test]
fn two_shared_views_can_overlap_but_mutable_arguments_cannot() {
    for mutable in [false, true] {
        let mut b = Body::new(1, "pair", Type::Unit);
        let local = b.local(Type::U8, true);
        let first = b.borrow(local, mutable);
        b.enter();
        b.call(vec![first, first]);
        b.exit();
        let mut test = case(vec![
            external(vec![address(Type::U8, mutable), address(Type::U8, mutable)]),
            b.finish(None),
        ]);
        if mutable {
            test.reject("E-VERIFY-MOVE");
        } else {
            test.pass();
        }
    }
    let mut b = Body::new(1, "pair_distinct_mutable", Type::Unit);
    let first = b.local(Type::U8, true);
    let second = b.local(Type::U8, true);
    let a = b.borrow(first, true);
    let b_value = b.borrow(second, true);
    b.enter();
    b.call(vec![a, b_value]);
    b.exit();
    case(vec![
        external(vec![address(Type::U8, true), address(Type::U8, true)]),
        b.finish(None),
    ])
    .pass();
}

#[test]
fn ffi_is_a_real_loan_consumer_not_an_address_liveness_escape() {
    let mut b = Body::new(1, "twice", Type::Unit);
    let local = b.local(Type::U8, true);
    let pointer = b.borrow(local, true);
    b.enter();
    b.call(vec![pointer]);
    b.call(vec![pointer]);
    b.exit();
    case(vec![
        external(vec![address(Type::U8, true)]),
        b.finish(None),
    ])
    .reject("E-VERIFY-MOVE");

    let mut b = Body::new(1, "uninitialized", Type::Unit);
    let local = b.local(array(Type::U8, 4), false);
    let pointer = b.borrow(local, true);
    b.enter();
    b.call(vec![pointer]);
    b.exit();
    case(vec![
        external(vec![address(array(Type::U8, 4), true)]),
        b.finish(None),
    ])
    .reject("E-VERIFY-INITIALIZATION");

    let mut b = Body::new(1, "dead", Type::Unit);
    let local = b.local(Type::U8, true);
    let pointer = b.borrow(local, false);
    b.instruction(InstructionKind::StorageDead(local));
    b.enter();
    b.call(vec![pointer]);
    b.exit();
    case(vec![
        external(vec![address(Type::U8, false)]),
        b.finish(None),
    ])
    .reject("E-BORROW-CONFLICT");
}

#[test]
fn ffi_checks_active_child_and_parent_authority() {
    let mut b = Body::new(1, "reborrow", Type::Unit);
    let local = b.local(Type::U8, true);
    let parent = b.borrow(local, true);
    let child = b.reborrow(parent, true);
    b.enter();
    b.call(vec![child]);
    b.exit();
    b.value(
        Type::U8,
        InstructionKind::ReadAddress { address: parent },
        vec![],
    );
    case(vec![
        external(vec![address(Type::U8, true)]),
        b.finish(None),
    ])
    .pass();

    let mut b = Body::new(1, "shared_child", Type::Unit);
    let local = b.local(Type::U8, true);
    let parent = b.borrow(local, true);
    let child = b.reborrow(parent, false);
    b.enter();
    b.call(vec![parent]);
    b.exit();
    b.value(
        Type::U8,
        InstructionKind::ReadAddress { address: child },
        vec![],
    );
    case(vec![
        external(vec![address(Type::U8, true)]),
        b.finish(None),
    ])
    .reject("E-BORROW-CONFLICT");
}

#[test]
fn parent_and_child_cannot_be_simultaneous_foreign_arguments() {
    for child_mutable in [false, true] {
        let mut b = Body::new(1, "parent_child_pair", Type::Unit);
        let local = b.local(Type::U8, true);
        let parent = b.borrow(local, true);
        let child = b.reborrow(parent, child_mutable);
        b.enter();
        b.call(vec![parent, child]);
        b.exit();
        case(vec![
            external(vec![
                address(Type::U8, true),
                address(Type::U8, child_mutable),
            ]),
            b.finish(None),
        ])
        .reject("E-BORROW-CONFLICT");
    }
}

#[test]
fn foreign_transfer_cannot_consume_a_mutable_load_view() {
    let mut b = Body::new(1, "loaded_view", Type::Unit);
    let local = b.local(Type::U8, true);
    let pointer = b.borrow(local, true);
    let carrier = b.local(address(Type::U8, true), false);
    b.instruction(InstructionKind::Store {
        local: carrier,
        value: pointer,
    });
    let view = b.value(
        address(Type::U8, true),
        InstructionKind::Load(carrier),
        vec![HoldRoot::Local(local)],
    );
    b.enter();
    b.call(vec![view]);
    b.exit();
    case(vec![
        external(vec![address(Type::U8, true)]),
        b.finish(None),
    ])
    .reject("E-VERIFY-MOVE");
}

#[test]
fn shared_foreign_access_respects_another_exclusive_window() {
    let mut b = Body::new(1, "shared_versus_mutable", Type::Unit);
    let local = b.local(Type::U8, true);
    let parent = b.borrow(local, true);
    let shared = b.reborrow(parent, false);
    let writer = b.reborrow(parent, true);
    b.enter();
    b.call(vec![shared]);
    b.exit();
    b.value(
        Type::U8,
        InstructionKind::ReadAddress { address: writer },
        vec![],
    );
    case(vec![
        external(vec![address(Type::U8, false)]),
        b.finish(None),
    ])
    .reject("E-BORROW-CONFLICT");
}

#[test]
fn embedded_conversion_target_is_bounded_before_fingerprint() {
    let mut invalid = integers(Constant::U8(0), Type::I32);
    let mut deep = Type::U8;
    for _ in 0..20 {
        deep = array(deep, 1);
    }
    invalid.module.functions[0].blocks[0].instructions[1].kind = InstructionKind::ConvertInteger {
        value: ValueId(0),
        target: deep,
    };
    // Deliberately retain a stale plan. Shape rejection must precede hashing or
    // candidate identity handling, even though the function values are shallow.
    let limits = Limits {
        max_call_depth: 8,
        ..Limits::default()
    };
    let errors = verify_module(
        invalid.module.clone(),
        invalid.plan.clone(),
        &invalid.module.target,
        &limits,
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    let mut invalid = integers(Constant::U8(0), Type::I32);
    invalid.module.functions[0].blocks[0].instructions[1].kind = InstructionKind::ConvertInteger {
        value: ValueId(0),
        target: array(Type::U8, 5000),
    };
    let limits = Limits {
        max_analysis_steps: 500,
        ..Limits::default()
    };
    let errors = verify_module(
        invalid.module.clone(),
        invalid.plan.clone(),
        &invalid.module.target,
        &limits,
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}

fn ct_conversion() -> CtModule {
    CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: Fingerprint::of(b"082 CT"),
        target: TargetProfile::host_llvm(),
        instructions: vec![
            CtInstruction {
                result: ValueId(0),
                ty: Type::U8,
                kind: CtInstructionKind::Constant(Constant::U8(255)),
            },
            CtInstruction {
                result: ValueId(1),
                ty: Type::I32,
                kind: CtInstructionKind::ConvertInteger {
                    value: ValueId(0),
                    target: Type::I32,
                },
            },
        ],
        result: ValueId(1),
        max_steps: 2,
        dependencies: vec![],
    }
}
fn check_ct_pair(module: &CtModule, expected: Option<&str>) {
    let limits = Limits::default();
    let bytes = wire::encode_ct(module, &limits).unwrap();
    for result in [
        verify_ct(module.clone(), &module.target, &limits),
        verify_ct_bytes(&bytes, &module.target, &limits),
    ] {
        if let Some(code) = expected {
            assert_eq!(result.unwrap_err()[0].code, code);
        } else {
            assert_eq!(result.unwrap().module(), module);
        }
    }
}

#[test]
fn pure_ct_admits_bytes_and_conversions_but_not_type_forgery() {
    let valid = ct_conversion();
    check_ct_pair(&valid, None);
    let mut bad = valid.clone();
    bad.instructions[1].ty = Type::U8;
    check_ct_pair(&bad, Some("E-CT-TYPE"));
    let mut bad = valid.clone();
    bad.instructions[1].kind = CtInstructionKind::ConvertInteger {
        value: ValueId(0),
        target: Type::Bool,
    };
    check_ct_pair(&bad, Some("E-VERIFY-TYPE"));
    let mut bad = valid.clone();
    bad.instructions[1].kind = CtInstructionKind::ConvertInteger {
        value: ValueId(1),
        target: Type::I32,
    };
    check_ct_pair(&bad, Some("E-CT-SSA"));
    let mut bad = valid;
    bad.instructions[1].kind = CtInstructionKind::ConvertInteger {
        value: ValueId(0),
        target: array(Type::U8, 5000),
    };
    let limits = Limits {
        max_analysis_steps: 500,
        ..Limits::default()
    };
    assert_eq!(
        verify_ct(bad, &TargetProfile::host_llvm(), &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}

#[test]
fn pure_cfg_ct_accepts_conversion_and_rejects_all_foreign_programs() {
    let mut pure = integers(Constant::U8(255), Type::I32);
    pure.bind();
    let program = CtProgram {
        module: pure.module,
        plan: pure.plan,
        entry: FunctionId(0),
        max_steps: 32,
        max_call_depth: 8,
        max_live_slots: 32,
    };
    assert!(verify_ct_program(program, &TargetProfile::host_llvm(), &Limits::default()).is_ok());
    let mut foreign = buffered(false, array(Type::U8, 4));
    foreign.bind();
    let program = CtProgram {
        module: foreign.module,
        plan: foreign.plan,
        entry: FunctionId(1),
        max_steps: 32,
        max_call_depth: 8,
        max_live_slots: 32,
    };
    assert_eq!(
        verify_ct_program(program, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err()[0]
            .code,
        "E-CT-FORBIDDEN"
    );
}

#[test]
fn byte_buffer_and_conversion_complete_budget_boundaries() {
    for (name, mut test) in [
        ("buffer", buffered(true, array(Type::U8, 4))),
        ("conversion", integers(Constant::U8(255), Type::I32)),
    ] {
        test.bind();
        let bytes =
            wire::encode_module_candidate(&test.module, &test.plan, &Limits::default()).unwrap();
        let verify = |n, wire_path| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if wire_path {
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
        for wire_path in [false, true] {
            let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
            assert!(verify(high, wire_path).is_ok());
            while low + 1 < high {
                let middle = low + (high - low) / 2;
                if verify(middle, wire_path).is_ok() {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            assert!(verify(high, wire_path).is_ok());
            assert_eq!(
                verify(high - 1, wire_path).unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
            println!(
                "082 exact {name} {} complete budget n={high}",
                if wire_path { "wire" } else { "raw" }
            );
        }
    }
}
