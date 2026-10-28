//! Hostile raw/wire boundary evidence only. No Types/Analyze implementation or
//! source/native success is used as the oracle for these independent cases.
use std::collections::BTreeSet;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;
use zil_verify::{verify_ct_program, verify_module, verify_module_bytes};

fn span() -> Span {
    Span::new(FileId(0), 1, 2)
}
fn owned() -> Type {
    Type::Managed(Box::new(Type::ByteBuffer))
}
fn desc() -> Type {
    Type::Desc(Box::new(Type::ByteBuffer))
}
fn slice(mutable: bool) -> Type {
    Type::Address {
        mutable,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U8),
        }),
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
        let raw = verify_module(
            self.module.clone(),
            self.plan.clone(),
            &self.module.target,
            &limits,
        )
        .unwrap();
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        let decoded = verify_module_bytes(&bytes, &self.module.target, &limits).unwrap();
        assert_eq!(raw.module(), decoded.module());
        assert_eq!(raw.plan(), decoded.plan());
    }
    fn reject(&mut self, code: &str) {
        self.bind();
        let limits = Limits::default();
        let raw = verify_module(
            self.module.clone(),
            self.plan.clone(),
            &self.module.target,
            &limits,
        )
        .unwrap_err();
        assert_eq!(raw[0].code, code, "raw {raw:?}");
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        let decoded = verify_module_bytes(&bytes, &self.module.target, &limits).unwrap_err();
        assert_eq!(decoded[0].code, code, "wire {decoded:?}");
    }
}

struct Body {
    f: TypedFunction,
    holds: Vec<ValueHold>,
    initialized: BTreeSet<LocalId>,
}
impl Body {
    fn new(id: u32, result: Type) -> Self {
        Self {
            f: TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                id: FunctionId(id),
                name: format!("f{id}"),
                foreign: None,
                environment: None,
                parameters: vec![],
                return_type: result,
                result_mode: PassingMode::Infer,
                locals: vec![],
                values: vec![],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction {
                        result: None,
                        kind: InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        span: span(),
                    }],
                    terminator: Terminator::Return(None),
                }],
                entry: BlockId(0),
                span: span(),
            },
            holds: vec![],
            initialized: BTreeSet::new(),
        }
    }
    fn parameter(&mut self, ty: Type) -> LocalId {
        let local = LocalId(self.f.locals.len() as u32);
        self.f.locals.push(LocalDecl {
            id: local,
            name: format!("p{}", local.0),
            ty,
            mutable: true,
            span: span(),
        });
        self.f.parameters.push(local);
        self.initialized.insert(local);
        local
    }
    fn parameter_value(&mut self, ty: Type) -> ValueId {
        let index = self.f.parameters.len() as u32;
        let local = self.parameter(ty.clone());
        let roots = if ty == desc() || matches!(ty, Type::Address { .. }) {
            vec![HoldRoot::Parameter(index)]
        } else {
            vec![]
        };
        self.value(ty, InstructionKind::Load(local), roots)
    }
    fn local(&mut self, ty: Type) -> LocalId {
        let local = LocalId(self.f.locals.len() as u32);
        self.f.locals.push(LocalDecl {
            id: local,
            name: format!("l{}", local.0),
            ty,
            mutable: true,
            span: span(),
        });
        self.emit(InstructionKind::StorageLive(local));
        local
    }
    fn value(&mut self, ty: Type, kind: InstructionKind, roots: Vec<HoldRoot>) -> ValueId {
        let result = ValueId(self.f.values.len() as u32);
        self.f.values.push(ty);
        self.holds.push(ValueHold {
            value: result,
            roots,
        });
        self.f.blocks[0].instructions.push(Instruction {
            result: Some(result),
            kind,
            span: span(),
        });
        result
    }
    fn emit(&mut self, kind: InstructionKind) {
        match kind {
            InstructionKind::Store { local, .. } => {
                self.initialized.insert(local);
            }
            InstructionKind::DropManaged { local }
            | InstructionKind::TakeLocal { local }
            | InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local) => {
                self.initialized.remove(&local);
            }
            _ => {}
        }
        self.f.blocks[0].instructions.push(Instruction {
            result: None,
            kind,
            span: span(),
        });
    }
    fn constant(&mut self, constant: Constant) -> ValueId {
        self.value(constant.ty(), InstructionKind::Constant(constant), vec![])
    }
    fn new_buffer(&mut self) -> LocalId {
        let local = self.local(owned());
        let value = self.value(owned(), InstructionKind::ByteBufferNew, vec![]);
        self.emit(InstructionKind::Store { local, value });
        local
    }
    fn describe(&mut self, local: LocalId) -> ValueId {
        self.value(
            desc(),
            InstructionKind::DescribeLocal { local },
            vec![HoldRoot::Local(local)],
        )
    }
    fn view(&mut self, buffer: ValueId, mutable: bool) -> ValueId {
        self.value(
            slice(mutable),
            InstructionKind::ByteBufferBorrow { buffer, mutable },
            self.holds[buffer.0 as usize].roots.clone(),
        )
    }
    fn length(&mut self, address: ValueId) -> ValueId {
        self.value(Type::I64, InstructionKind::SliceLength { address }, vec![])
    }
    fn reserve(&mut self, buffer: ValueId) -> ValueId {
        let capacity = self.constant(Constant::I64(32));
        self.value(
            Type::Bool,
            InstructionKind::ByteBufferReserve { buffer, capacity },
            vec![],
        )
    }
    fn take(&mut self, local: LocalId) -> ValueId {
        self.initialized.remove(&local);
        self.value(owned(), InstructionKind::TakeLocal { local }, vec![])
    }
    fn call(
        &mut self,
        function: u32,
        arguments: Vec<ValueId>,
        ty: Type,
        roots: Vec<HoldRoot>,
    ) -> Option<ValueId> {
        let kind = InstructionKind::Call {
            function: FunctionId(function),
            arguments,
        };
        if ty == Type::Unit {
            self.emit(kind);
            None
        } else {
            Some(self.value(ty, kind, roots))
        }
    }
    fn finish(mut self, returned: Option<ValueId>) -> (TypedFunction, FunctionPlan) {
        for index in (0..self.f.locals.len()).rev() {
            let local = LocalId(index as u32);
            if self.f.locals[index].ty == owned() {
                self.emit(InstructionKind::DropManaged { local });
            }
            self.emit(InstructionKind::StorageDead(local));
        }
        self.emit(InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        });
        self.f.blocks[0].terminator = Terminator::Return(returned);
        let plan = FunctionPlan {
            function: self.f.id,
            holds: self.holds,
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: self.f.parameters.clone(),
                initialized_out: self.initialized.into_iter().collect(),
            }],
        };
        (self.f, plan)
    }
}

fn case(parts: Vec<(TypedFunction, FunctionPlan)>) -> Case {
    let (functions, plans) = parts.into_iter().unzip();
    let module = TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"095 hostile raw byte storage fixtures"),
        target: TargetProfile::host_llvm(),
        functions,
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "@builtin:ByteBuffer".into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = plans;
    Case { module, plan }
}
fn simple() -> Case {
    let mut f = Body::new(0, Type::I64);
    let local = f.new_buffer();
    let b = f.describe(local);
    f.reserve(b);
    let byte = f.constant(Constant::U8(65));
    f.value(
        Type::Bool,
        InstructionKind::ByteBufferPush {
            buffer: b,
            value: byte,
        },
        vec![],
    );
    f.value(
        Type::I64,
        InstructionKind::ByteBufferCapacity { buffer: b },
        vec![],
    );
    f.value(
        Type::I64,
        InstructionKind::ByteBufferLen { buffer: b },
        vec![],
    );
    let s = f.view(b, false);
    let n = f.length(s);
    case(vec![f.finish(Some(n))])
}

#[test]
fn all_byte_storage_operations_round_trip_raw_and_wire() {
    simple().pass();
}

#[test]
fn canonical_definition_is_required_and_cannot_replace_its_cleanup() {
    let mut t = simple();
    t.module.resources.clear();
    t.reject("E-VERIFY-BYTE-BUFFER");
    let mut t = simple();
    t.module.resources[0].name = "user:ByteBuffer".into();
    t.reject("E-VERIFY-BYTE-BUFFER");
    let mut t = simple();
    t.module.resources[0].teardown = Some(FunctionId(0));
    t.reject("E-VERIFY-BYTE-BUFFER");
    let mut t = simple();
    t.module.resources.push(t.module.resources[0].clone());
    t.reject("E-VERIFY-BYTE-BUFFER");
    let mut t = simple();
    t.module.resources[0].payload = Type::Record {
        name: "@builtin:ByteBuffer".into(),
        fields: vec![],
    };
    t.reject("E-VERIFY-BYTE-BUFFER");
}

#[test]
fn ordinary_managed_constructor_and_field_access_cannot_forge_the_head() {
    let mut t = simple();
    t.module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::ByteBufferNew))
        .unwrap()
        .kind = InstructionKind::MakeManaged { fields: vec![] };
    t.reject("E-VERIFY-BYTE-BUFFER");
    for write in [false, true] {
        let mut f = Body::new(0, Type::Unit);
        let local = f.new_buffer();
        let b = f.describe(local);
        let byte = f.constant(Constant::U8(0));
        if write {
            f.emit(InstructionKind::WriteDesc {
                desc: b,
                index: 0,
                value: byte,
            });
        } else {
            f.value(
                Type::U8,
                InstructionKind::ReadDesc { desc: b, index: 0 },
                vec![],
            );
        }
        case(vec![f.finish(None)]).reject("E-VERIFY-BYTE-BUFFER");
    }
}

#[test]
fn exact_operand_types_are_checked_for_every_new_operation() {
    for wrong in [Type::I32, Type::U8, Type::Bool] {
        let mut t = simple();
        let operand = t.module.functions[0].blocks[0]
            .instructions
            .iter()
            .find_map(|i| match i.kind {
                InstructionKind::ByteBufferReserve { capacity, .. } => Some(capacity),
                _ => None,
            })
            .unwrap();
        t.module.functions[0].values[operand.0 as usize] = wrong;
        t.reject("E-VERIFY-TYPE");
    }
    let mut f = Body::new(0, Type::Unit);
    let local = f.new_buffer();
    let b = f.describe(local);
    let value = f.constant(Constant::I64(2));
    f.value(
        Type::Bool,
        InstructionKind::ByteBufferPush { buffer: b, value },
        vec![],
    );
    case(vec![f.finish(None)]).reject("E-VERIFY-BYTE-BUFFER");
    let mut f = Body::new(0, Type::Unit);
    let token = f.constant(Constant::NullHandle);
    f.value(
        Type::I64,
        InstructionKind::ByteBufferLen { buffer: token },
        vec![],
    );
    case(vec![f.finish(None)]).reject("E-VERIFY-BYTE-BUFFER");
}

#[test]
fn create_and_give_factories_preserve_unique_responsibility() {
    let mut make = Body::new(0, owned());
    make.f.result_mode = PassingMode::Create;
    let fresh = make.value(owned(), InstructionKind::ByteBufferNew, vec![]);
    let mut relay = Body::new(1, owned());
    relay.f.result_mode = PassingMode::Give;
    let input = relay.parameter(owned());
    let transferred = relay.take(input);
    let mut caller = Body::new(2, Type::I64);
    let local = caller.local(owned());
    let a = caller.call(0, vec![], owned(), vec![]).unwrap();
    caller.emit(InstructionKind::Store { local, value: a });
    let a = caller.take(local);
    let a = caller.call(1, vec![a], owned(), vec![]).unwrap();
    caller.emit(InstructionKind::Store { local, value: a });
    let b = caller.describe(local);
    let n = caller.value(
        Type::I64,
        InstructionKind::ByteBufferLen { buffer: b },
        vec![],
    );
    case(vec![
        make.finish(Some(fresh)),
        relay.finish(Some(transferred)),
        caller.finish(Some(n)),
    ])
    .pass();
}

#[test]
fn description_parameter_can_return_shared_or_exclusive_slice() {
    for mutable in [false, true] {
        let mut helper = Body::new(0, slice(mutable));
        let b = helper.parameter_value(desc());
        let returned = helper.view(b, mutable);
        let mut caller = Body::new(1, Type::I64);
        let local = caller.new_buffer();
        let b = caller.describe(local);
        let s = caller
            .call(0, vec![b], slice(mutable), vec![HoldRoot::Local(local)])
            .unwrap();
        let n = caller.length(s);
        case(vec![helper.finish(Some(returned)), caller.finish(Some(n))]).pass();
    }
}

#[test]
fn owned_parameter_or_local_storage_cannot_escape_as_a_bare_slice() {
    for parameter in [false, true] {
        let mut f = Body::new(0, slice(false));
        let local = if parameter {
            f.parameter(owned())
        } else {
            f.new_buffer()
        };
        let b = f.describe(local);
        let s = f.view(b, false);
        case(vec![f.finish(Some(s))]).reject("E-BORROW-ESCAPE");
    }
}

#[test]
fn shared_window_allows_metadata_reads_but_blocks_reserve_and_push() {
    for write in [0, 1, 2] {
        let mut f = Body::new(0, Type::I64);
        let local = f.new_buffer();
        let b = f.describe(local);
        let s = f.view(b, false);
        match write {
            0 => {
                f.value(
                    Type::I64,
                    InstructionKind::ByteBufferLen { buffer: b },
                    vec![],
                );
            }
            1 => {
                f.reserve(b);
            }
            _ => {
                let value = f.constant(Constant::U8(7));
                f.value(
                    Type::Bool,
                    InstructionKind::ByteBufferPush { buffer: b, value },
                    vec![],
                );
            }
        }
        let n = f.length(s);
        let mut t = case(vec![f.finish(Some(n))]);
        if write == 0 {
            t.pass();
        } else {
            t.reject("E-BORROW-CONFLICT");
        }
    }
}

#[test]
fn exclusive_window_blocks_metadata_and_other_windows_until_last_use() {
    for nested_window in [false, true] {
        let mut f = Body::new(0, Type::I64);
        let local = f.new_buffer();
        let b = f.describe(local);
        let s = f.view(b, true);
        if nested_window {
            f.view(b, false);
        } else {
            f.value(
                Type::I64,
                InstructionKind::ByteBufferCapacity { buffer: b },
                vec![],
            );
        }
        let n = f.length(s);
        case(vec![f.finish(Some(n))]).reject("E-BORROW-CONFLICT");
    }
    let mut f = Body::new(0, Type::I64);
    let local = f.new_buffer();
    let b = f.describe(local);
    let s = f.view(b, true);
    f.length(s);
    f.reserve(b);
    let s = f.view(b, false);
    let n = f.length(s);
    case(vec![f.finish(Some(n))]).pass();
}

#[test]
fn live_window_cannot_cross_give_or_discharge() {
    for transfer in [false, true] {
        let mut f = Body::new(0, Type::I64);
        let local = f.new_buffer();
        let b = f.describe(local);
        let s = f.view(b, false);
        if transfer {
            let moved = f.take(local);
            let destination = f.local(owned());
            f.emit(InstructionKind::Store {
                local: destination,
                value: moved,
            });
        } else {
            f.emit(InstructionKind::DropManaged { local });
        }
        let n = f.length(s);
        case(vec![f.finish(Some(n))]).reject("E-BORROW-CONFLICT");
    }
}

fn alias_helper(id: u32, dangerous_order: bool) -> (TypedFunction, FunctionPlan) {
    let mut f = Body::new(id, Type::I64);
    let a = f.parameter_value(desc());
    let b = f.parameter_value(desc());
    let s = f.view(a, false);
    if dangerous_order {
        f.reserve(b);
    }
    let n = f.length(s);
    if !dangerous_order {
        f.reserve(b);
    }
    f.finish(Some(n))
}
fn alias_caller(id: u32, callee: u32, same: bool) -> (TypedFunction, FunctionPlan) {
    let mut f = Body::new(id, Type::I64);
    let a = f.new_buffer();
    let b = if same { a } else { f.new_buffer() };
    let a = f.describe(a);
    let b = f.describe(b);
    let n = f.call(callee, vec![a, b], Type::I64, vec![]).unwrap();
    f.finish(Some(n))
}

#[test]
fn necessary_apart_comes_from_live_windows_not_different_parameter_names() {
    case(vec![alias_helper(0, true), alias_caller(1, 0, true)]).reject("E-BORROW-CONFLICT");
    case(vec![alias_helper(0, true), alias_caller(1, 0, false)]).pass();
    case(vec![alias_helper(0, false), alias_caller(1, 0, true)]).pass();
}

#[test]
fn separation_obligations_propagate_through_forwarding_helpers() {
    let mut relay = Body::new(1, Type::I64);
    let a = relay.parameter_value(desc());
    let b = relay.parameter_value(desc());
    let n = relay.call(0, vec![a, b], Type::I64, vec![]).unwrap();
    case(vec![
        alias_helper(0, true),
        relay.finish(Some(n)),
        alias_caller(2, 1, true),
    ])
    .reject("E-BORROW-CONFLICT");
}

#[test]
fn ordinary_helper_effects_conflict_with_live_caller_windows() {
    for write in [false, true] {
        for mutable in [false, true] {
            let mut helper = Body::new(0, Type::Unit);
            let b = helper.parameter_value(desc());
            if write {
                helper.reserve(b);
            } else {
                helper.value(
                    Type::I64,
                    InstructionKind::ByteBufferLen { buffer: b },
                    vec![],
                );
            }
            let mut caller = Body::new(1, Type::I64);
            let local = caller.new_buffer();
            let b = caller.describe(local);
            let s = caller.view(b, mutable);
            caller.call(0, vec![b], Type::Unit, vec![]);
            let n = caller.length(s);
            let mut t = case(vec![helper.finish(None), caller.finish(Some(n))]);
            if write || mutable {
                t.reject("E-BORROW-CONFLICT");
            } else {
                t.pass();
            }
        }
    }
}

#[test]
fn same_source_description_read_write_without_live_windows_remains_legal() {
    let mut helper = Body::new(0, Type::I64);
    let a = helper.parameter_value(desc());
    let b = helper.parameter_value(desc());
    helper.reserve(b);
    let n = helper.value(
        Type::I64,
        InstructionKind::ByteBufferLen { buffer: a },
        vec![],
    );
    case(vec![helper.finish(Some(n)), alias_caller(1, 0, true)]).pass();
}

#[test]
fn description_and_slice_parameters_cannot_hide_aliasing_at_call_entry() {
    let mut helper = Body::new(0, Type::I64);
    let b = helper.parameter_value(desc());
    let s = helper.parameter_value(slice(false));
    helper.reserve(b);
    let n = helper.length(s);
    let mut caller = Body::new(1, Type::I64);
    let local = caller.new_buffer();
    let b = caller.describe(local);
    let s = caller.view(b, false);
    let n2 = caller.call(0, vec![b, s], Type::I64, vec![]).unwrap();
    case(vec![helper.finish(Some(n)), caller.finish(Some(n2))]).reject("E-BORROW-CONFLICT");
}

#[test]
fn owned_load_and_duplicate_ssa_store_cannot_copy_responsibility() {
    let mut f = Body::new(0, Type::Unit);
    let local = f.new_buffer();
    f.value(owned(), InstructionKind::Load(local), vec![]);
    case(vec![f.finish(None)]).reject("E-VERIFY-RESOURCE");
    let mut f = Body::new(0, Type::Unit);
    let a = f.local(owned());
    let b = f.local(owned());
    let value = f.value(owned(), InstructionKind::ByteBufferNew, vec![]);
    f.emit(InstructionKind::Store { local: a, value });
    f.emit(InstructionKind::Store { local: b, value });
    case(vec![f.finish(None)]).reject("E-VERIFY-RESOURCE");
}

#[test]
fn description_and_borrow_origin_claims_are_independently_rebuilt() {
    let mut t = simple();
    let index = t.module.functions[0].blocks[0]
        .instructions
        .iter()
        .find_map(|i| {
            matches!(i.kind, InstructionKind::DescribeLocal { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    t.plan.functions[0].holds[index.0 as usize].roots.clear();
    t.reject("E-VERIFY-HOLD");
    let mut t = simple();
    let index = t.module.functions[0].blocks[0]
        .instructions
        .iter()
        .find_map(|i| {
            matches!(i.kind, InstructionKind::ByteBufferBorrow { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    t.plan.functions[0].holds[index.0 as usize].roots = vec![HoldRoot::Parameter(0)];
    t.reject("E-VERIFY-HOLD");
}

#[test]
fn unknown_operands_and_wrong_byte_storage_result_do_not_reach_lowering() {
    let mut t = simple();
    for i in &mut t.module.functions[0].blocks[0].instructions {
        if let InstructionKind::ByteBufferReserve { capacity, .. } = &mut i.kind {
            *capacity = ValueId(u32::MAX);
        }
    }
    t.reject("E-VERIFY-ID");
    let mut t = simple();
    let id = t.module.functions[0].blocks[0]
        .instructions
        .iter()
        .find_map(|i| {
            matches!(i.kind, InstructionKind::ByteBufferBorrow { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    t.module.functions[0].values[id.0 as usize] = slice(true);
    t.reject("E-VERIFY-TYPE");
}

#[test]
fn byte_storage_remains_outside_compile_time_and_aggregate_profiles() {
    let mut t = simple();
    t.bind();
    let raw = CtProgram {
        module: t.module,
        plan: t.plan,
        entry: FunctionId(0),
        max_steps: 100,
        max_call_depth: 10,
        max_live_slots: 100,
    };
    let errors =
        verify_ct_program(raw, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
    for ty in [
        Type::ByteBuffer,
        Type::Array {
            element: Box::new(owned()),
            length: 1,
        },
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(owned()),
        },
        Type::Address {
            mutable: false,
            inner: Box::new(Type::ByteBuffer),
        },
    ] {
        let mut f = Body::new(0, Type::Unit);
        f.parameter(ty);
        let mut t = case(vec![f.finish(None)]);
        t.bind();
        let errors = verify_module(
            t.module,
            t.plan,
            &TargetProfile::host_llvm(),
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::Unsupported);
    }
}

#[test]
fn package_and_capture_cannot_hide_borrowed_byte_storage() {
    let mut f = Body::new(0, Type::Unit);
    f.parameter(Type::Package {
        name: "P".into(),
        fields: vec![("b".into(), desc())],
    });
    let mut t = case(vec![f.finish(None)]);
    t.bind();
    let errors = verify_module(
        t.module,
        t.plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::Unsupported);
    let f = Body::new(0, Type::Unit);
    let mut t = case(vec![f.finish(None)]);
    t.module.closures.push(ClosureDefinition {
        lending_captures: Vec::new(),
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![desc()],
        parameters: vec![],
        result: Type::Unit,
        kind: CallableKind::FnMut,
    });
    t.module.functions[0].environment = Some(ClosureId(0));
    t.bind();
    let errors = verify_module(
        t.module,
        t.plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::Unsupported);
}

#[test]
fn verification_budget_boundary_is_deterministic_for_raw_and_wire() {
    let mut t = simple();
    t.bind();
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        };
        if verify_module(t.module.clone(), t.plan.clone(), &t.module.target, &limits).is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    let limits = Limits {
        max_analysis_steps: low,
        ..Limits::default()
    };
    verify_module(t.module.clone(), t.plan.clone(), &t.module.target, &limits).unwrap();
    let fail = Limits {
        max_analysis_steps: low - 1,
        ..Limits::default()
    };
    let errors =
        verify_module(t.module.clone(), t.plan.clone(), &t.module.target, &fail).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    let bytes = wire::encode_module_candidate(&t.module, &t.plan, &Limits::default()).unwrap();
    verify_module_bytes(&bytes, &t.module.target, &limits).unwrap();
    let errors = verify_module_bytes(&bytes, &t.module.target, &fail).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    eprintln!("095 byte storage exact verification budget: {low}");
}

#[test]
fn wire_truncation_and_preceding_version_cannot_admit_byte_storage() {
    let mut t = simple();
    t.bind();
    let bytes = wire::encode_module_candidate(&t.module, &t.plan, &Limits::default()).unwrap();
    for end in 0..bytes.len() {
        assert!(
            verify_module_bytes(&bytes[..end], &t.module.target, &Limits::default()).is_err(),
            "truncation {end}"
        );
    }
    let mut previous = bytes.clone();
    previous[8..12].copy_from_slice(&16u32.to_le_bytes());
    assert!(verify_module_bytes(&previous, &t.module.target, &Limits::default()).is_err());
}

#[test]
fn recursive_no_return_prefix_keeps_required_parameter_separation() {
    let mut first = Body::new(0, Type::Unit);
    let a = first.parameter_value(desc());
    let b = first.parameter_value(desc());
    first.call(1, vec![b, a], Type::Unit, vec![]);
    let mut second = Body::new(1, Type::Unit);
    let a = second.parameter_value(desc());
    let b = second.parameter_value(desc());
    let s = second.view(a, false);
    second.reserve(b);
    second.length(s);
    second.call(0, vec![a, b], Type::Unit, vec![]);
    let mut second = second.finish(None);
    assert!(matches!(
        second.0.blocks[0].instructions.pop().unwrap().kind,
        InstructionKind::ScopeExit {
            kind: CleanupExitKind::Return,
            ..
        }
    ));
    second.0.blocks[0].terminator = Terminator::Trap;
    let mut caller = Body::new(2, Type::Unit);
    let local = caller.new_buffer();
    let b = caller.describe(local);
    caller.call(0, vec![b, b], Type::Unit, vec![]);
    case(vec![first.finish(None), second, caller.finish(None)]).reject("E-BORROW-CONFLICT");
}

#[test]
fn recursive_write_prefix_is_visible_to_the_callers_live_window() {
    let mut first = Body::new(0, Type::Unit);
    let b = first.parameter_value(desc());
    first.call(1, vec![b], Type::Unit, vec![]);
    let mut second = Body::new(1, Type::Unit);
    let b = second.parameter_value(desc());
    second.reserve(b);
    second.call(0, vec![b], Type::Unit, vec![]);
    let mut second = second.finish(None);
    assert!(matches!(
        second.0.blocks[0].instructions.pop().unwrap().kind,
        InstructionKind::ScopeExit {
            kind: CleanupExitKind::Return,
            ..
        }
    ));
    second.0.blocks[0].terminator = Terminator::Trap;
    let mut caller = Body::new(2, Type::I64);
    let local = caller.new_buffer();
    let b = caller.describe(local);
    let s = caller.view(b, false);
    caller.call(0, vec![b], Type::Unit, vec![]);
    let n = caller.length(s);
    case(vec![first.finish(None), second, caller.finish(Some(n))]).reject("E-BORROW-CONFLICT");
}

#[test]
fn reserve_capacity_must_dominate_a_real_control_flow_join() {
    let mut t = simple();
    let f = &mut t.module.functions[0];
    let cap_pos = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Constant(Constant::I64(32))))
        .unwrap();
    let instructions = std::mem::take(&mut f.blocks[0].instructions);
    let terminator = f.blocks[0].terminator.clone();
    let condition = ValueId(f.values.len() as u32);
    f.values.push(Type::Bool);
    t.plan.functions[0].holds.push(ValueHold {
        value: condition,
        roots: vec![],
    });
    let mut entry = instructions[..=cap_pos].to_vec();
    entry.push(Instruction {
        result: Some(condition),
        kind: InstructionKind::Constant(Constant::Bool(true)),
        span: span(),
    });
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: entry,
            terminator: Terminator::Branch {
                condition,
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: instructions[cap_pos + 1..].to_vec(),
            terminator,
        },
    ];
    t.plan.functions[0].blocks = (0..4)
        .map(|id| BlockPlan {
            block: BlockId(id),
            initialized_in: if id == 0 { vec![] } else { vec![LocalId(0)] },
            initialized_out: if id == 3 { vec![] } else { vec![LocalId(0)] },
        })
        .collect();
    t.pass();
    let capacity = t.module.functions[0].blocks[0].instructions.remove(cap_pos);
    t.module.functions[0].blocks[1].instructions.push(capacity);
    t.reject("E-VERIFY-DOMINANCE");
}

#[test]
fn exclusive_child_reborrow_suspends_its_byte_window_parent() {
    for use_parent in [false, true] {
        let mut f = Body::new(0, Type::I64);
        let local = f.new_buffer();
        let b = f.describe(local);
        let parent = f.view(b, true);
        let child = f.value(
            slice(true),
            InstructionKind::ReborrowAddress {
                address: parent,
                mutable: true,
            },
            vec![HoldRoot::Local(local)],
        );
        if use_parent {
            f.length(parent);
        }
        let n = f.length(child);
        let mut t = case(vec![f.finish(Some(n))]);
        if use_parent {
            t.reject("E-BORROW-CONFLICT");
        } else {
            t.pass();
        }
    }
}

#[test]
fn shared_and_exclusive_views_from_unknown_parameters_require_real_separation() {
    let mut helper = Body::new(0, Type::I64);
    let a = helper.parameter_value(desc());
    let b = helper.parameter_value(desc());
    let shared = helper.view(a, false);
    let exclusive = helper.view(b, true);
    helper.length(shared);
    let n = helper.length(exclusive);
    case(vec![helper.finish(Some(n)), alias_caller(1, 0, true)]).reject("E-BORROW-CONFLICT");
}

#[test]
fn byte_storage_profile_is_checked_independently_of_the_raw_type_tag() {
    let mut t = simple();
    t.module.target = TargetProfile::windows_x64_llvm();
    t.pass();
    let mut t = simple();
    t.module.target.pointer_width = 32;
    t.bind();
    let errors = verify_module(
        t.module,
        t.plan,
        &TargetProfile {
            pointer_width: 32,
            ..TargetProfile::host_llvm()
        },
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::Unsupported);
}

#[test]
fn byte_borrow_wire_boolean_and_fingerprint_are_not_trusted() {
    let mut t = simple();
    t.bind();
    let (result, buffer) = t.module.functions[0].blocks[0]
        .instructions
        .iter()
        .find_map(|i| match i.kind {
            InstructionKind::ByteBufferBorrow {
                buffer,
                mutable: false,
            } => Some((i.result.unwrap(), buffer)),
            _ => None,
        })
        .unwrap();
    let mut pattern = vec![1];
    pattern.extend(result.0.to_le_bytes());
    pattern.push(68);
    pattern.extend(buffer.0.to_le_bytes());
    pattern.push(0);
    pattern.extend(0u32.to_le_bytes());
    pattern.extend(1u64.to_le_bytes());
    pattern.extend(2u64.to_le_bytes());
    let mut bytes = wire::encode_module_candidate(&t.module, &t.plan, &Limits::default()).unwrap();
    let offsets: Vec<_> = bytes
        .windows(pattern.len())
        .enumerate()
        .filter_map(|(i, bytes)| (bytes == pattern).then_some(i))
        .collect();
    assert_eq!(offsets.len(), 1);
    bytes[offsets[0] + 10] = 2;
    let errors = verify_module_bytes(&bytes, &t.module.target, &Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, "E-WIRE-BOOL");
    for instruction in &mut t.module.functions[0].blocks[0].instructions {
        if matches!(
            instruction.kind,
            InstructionKind::Constant(Constant::I64(32))
        ) {
            instruction.kind = InstructionKind::Constant(Constant::I64(64));
        }
    }
    let errors = verify_module(
        t.module,
        t.plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "E-VERIFY-PLAN-IDENTITY");
}
