//! Independent hostile raw/wire candidates for owned byte composition.
//! Every CFG and expected root below is constructed without Types or Analyze.
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static CASE: AtomicUsize = AtomicUsize::new(0);
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;
use zil_verify::{verify_module, verify_module_bytes};

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
    fn record(&self, bytes: &[u8], code: &str) {
        if let Some(base) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            let directory = PathBuf::from(base).join(format!(
                "raw099-{}-{}",
                std::process::id(),
                CASE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("candidate.wire"), bytes).unwrap();
            fs::write(
                directory.join("candidate.txt"),
                format!("{:#?}\n{:#?}\n", self.module, self.plan),
            )
            .unwrap();
            fs::write(
                directory.join("result.json"),
                format!(
                    r#"{{"status":"Pass","raw":"{code}","wire":"{code}","wire_bytes":{}}}"#,
                    bytes.len()
                ),
            )
            .unwrap();
        }
    }
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
        self.record(&bytes, "Success");
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
        assert_eq!(
            raw[0].category,
            if code == "E-UNSUPPORTED" {
                ResultCategory::Unsupported
            } else {
                ResultCategory::SafeReject
            }
        );
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        let decoded = verify_module_bytes(&bytes, &self.module.target, &limits).unwrap_err();
        assert_eq!(decoded[0].code, code, "wire {decoded:?}");
        assert_eq!(decoded[0].category, raw[0].category);
        self.record(&bytes, code);
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
            | InstructionKind::DropPackage { local }
            | InstructionKind::EndPackageCapture { local, .. }
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
            if self.f.environment.is_some()
                && matches!(self.f.locals[index].ty, Type::Package { .. })
            {
                self.emit(InstructionKind::EndPackageCapture { local, index: 0 });
                continue;
            }
            if self.f.locals[index].ty == owned() {
                self.emit(InstructionKind::DropManaged { local });
            } else if matches!(self.f.locals[index].ty, Type::Package { .. }) {
                self.emit(InstructionKind::DropPackage { local });
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
        source_fingerprint: Fingerprint::of(
            b"099 independently constructed owning byte composition",
        ),
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

fn package() -> Type {
    Type::Package {
        name: "Bytes".into(),
        fields: vec![("left".into(), owned()), ("right".into(), owned())],
    }
}
impl Body {
    fn package(&mut self) -> LocalId {
        let local = self.local(package());
        let a = self.value(owned(), InstructionKind::ByteBufferNew, vec![]);
        let b = self.value(owned(), InstructionKind::ByteBufferNew, vec![]);
        let whole = self.value(
            package(),
            InstructionKind::MakePackage { fields: vec![a, b] },
            vec![],
        );
        self.emit(InstructionKind::Store {
            local,
            value: whole,
        });
        local
    }
    fn field(&mut self, local: LocalId, index: u32) -> ValueId {
        self.value(
            desc(),
            InstructionKind::DescribePackageField { local, index },
            vec![HoldRoot::Local(local)],
        )
    }
    fn capture(&mut self) -> ValueId {
        self.value(
            desc(),
            InstructionKind::DescribeCapture { index: 0 },
            vec![HoldRoot::Capture(0)],
        )
    }
    fn bind_package(&mut self) -> LocalId {
        let local = LocalId(self.f.locals.len() as u32);
        self.f.locals.push(LocalDecl {
            id: local,
            name: "environment_view".into(),
            ty: package(),
            mutable: true,
            span: span(),
        });
        self.emit(InstructionKind::BindPackageCapture { local, index: 0 });
        self.initialized.insert(local);
        local
    }
}
fn captured(body: Body, result: ValueId, kind: CallableKind, package_capture: bool) -> Case {
    let result_type = body.f.return_type.clone();
    let mut t = case(vec![body.finish(Some(result))]);
    t.module.closures.push(ClosureDefinition {
        lending_captures: Vec::new(),
        id: ClosureId(0),
        function: FunctionId(0),
        kind,
        captures: vec![if package_capture { package() } else { owned() }],
        parameters: vec![],
        result: result_type,
    });
    t
}
fn package_read() -> Case {
    let mut f = Body::new(0, Type::I64);
    let p = f.package();
    let d = f.field(p, 0);
    let view = f.view(d, false);
    let size = f.length(view);
    case(vec![f.finish(Some(size))])
}

#[test]
fn package_field_backing_is_reconstructed_from_raw_and_wire() {
    package_read().pass();
    let mut t = package_read();
    let descriptor = t.module.functions[0].blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(i.kind, InstructionKind::DescribePackageField { .. }))
        .unwrap()
        .result
        .unwrap();
    t.plan.functions[0].holds[descriptor.0 as usize]
        .roots
        .clear();
    t.reject("E-VERIFY-HOLD");
    let mut t = package_read();
    t.module.resources.clear();
    t.reject("E-VERIFY-BYTE-BUFFER");
}

#[test]
fn live_field_window_rejects_take_replace_and_whole_transfer() {
    for action in 0..4 {
        let mut f = Body::new(0, Type::I64);
        let p = f.package();
        let d = f.field(p, 0);
        let view = f.view(d, false);
        match action {
            0 => {
                let value = f.value(
                    owned(),
                    InstructionKind::TakePackageField { local: p, index: 0 },
                    vec![],
                );
                let moved = f.local(owned());
                f.emit(InstructionKind::Store {
                    local: moved,
                    value,
                });
            }
            1 => {
                let value = f.value(owned(), InstructionKind::ByteBufferNew, vec![]);
                f.emit(InstructionKind::DropPackageField { local: p, index: 0 });
                f.emit(InstructionKind::StorePackageField {
                    local: p,
                    index: 0,
                    value,
                });
            }
            2 => {
                let value = f.value(package(), InstructionKind::TakeLocal { local: p }, vec![]);
                f.initialized.remove(&p);
                let target = f.local(package());
                f.emit(InstructionKind::Store {
                    local: target,
                    value,
                });
            }
            _ => f.emit(InstructionKind::DropPackage { local: p }),
        }
        let size = f.length(view);
        case(vec![f.finish(Some(size))]).reject("E-BORROW-CONFLICT");
    }
}

#[test]
fn checked_owned_fields_separate_but_same_field_requires_last_use() {
    for same_field in [true, false] {
        for live in [true, false] {
            let mut f = Body::new(0, Type::I64);
            let p = f.package();
            let left = f.field(p, 0);
            let changed = f.field(p, if same_field { 0 } else { 1 });
            let view = f.view(left, false);
            let size = if !live { Some(f.length(view)) } else { None };
            f.reserve(changed);
            let size = size.unwrap_or_else(|| f.length(view));
            let mut t = case(vec![f.finish(Some(size))]);
            if live && same_field {
                t.reject("E-BORROW-CONFLICT");
            } else {
                t.pass();
            }
        }
    }
}

#[test]
fn duplicate_managed_ssa_cannot_be_hidden_in_distinct_package_fields() {
    let mut t = package_read();
    let fields = t.module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find_map(|i| match &mut i.kind {
            InstructionKind::MakePackage { fields } => Some(fields),
            _ => None,
        })
        .unwrap();
    fields[1] = fields[0];
    t.reject("E-VERIFY-RESOURCE");
}

#[test]
fn direct_and_package_capture_roots_are_independently_reconstructed() {
    for is_package in [false, true] {
        let mut f = Body::new(0, Type::I64);
        f.f.environment = Some(ClosureId(0));
        let d = if is_package {
            let p = f.bind_package();
            f.field(p, 0)
        } else {
            f.capture()
        };
        let view = f.view(d, false);
        let size = f.length(view);
        let mut t = captured(f, size, CallableKind::Fn, is_package);
        t.pass();
        t.plan.functions[0].holds[d.0 as usize].roots.clear();
        t.reject("E-VERIFY-HOLD");
    }
}

#[test]
fn owning_capture_mode_cannot_hide_a_backing_write() {
    for is_package in [false, true] {
        for kind in [CallableKind::Fn, CallableKind::FnMut] {
            for via_helper in [false, true] {
                let mut f = Body::new(0, Type::I64);
                f.f.environment = Some(ClosureId(0));
                let d = if is_package {
                    let p = f.bind_package();
                    f.field(p, 0)
                } else {
                    f.capture()
                };
                if via_helper {
                    f.call(1, vec![d], Type::Unit, vec![]);
                } else {
                    f.reserve(d);
                }
                let view = f.view(d, false);
                let size = f.length(view);
                let mut t = captured(f, size, kind, is_package);
                if via_helper {
                    let mut helper = Body::new(1, Type::Unit);
                    let desc = helper.parameter_value(desc());
                    helper.reserve(desc);
                    let (function, plan) = helper.finish(None);
                    t.module.functions.push(function);
                    t.plan.functions.push(plan);
                }
                if kind == CallableKind::Fn {
                    t.reject("E-VERIFY-RESOURCE");
                } else {
                    t.pass();
                }
            }
        }
    }
}

#[test]
fn capture_views_stay_live_across_owner_consumption() {
    for is_package in [false, true] {
        let mut f = Body::new(0, Type::I64);
        f.f.environment = Some(ClosureId(0));
        let p = is_package.then(|| f.bind_package());
        let d = if let Some(p) = p {
            f.field(p, 0)
        } else {
            f.capture()
        };
        let view = f.view(d, false);
        let moved = f.value(
            owned(),
            if let Some(p) = p {
                InstructionKind::TakePackageField { local: p, index: 0 }
            } else {
                InstructionKind::TakeCapture { index: 0 }
            },
            vec![],
        );
        let slot = f.local(owned());
        f.emit(InstructionKind::Store {
            local: slot,
            value: moved,
        });
        let size = f.length(view);
        captured(f, size, CallableKind::FnOnce, is_package).reject("E-BORROW-CONFLICT");
    }
}

#[test]
fn helper_actual_aliases_are_checked_against_package_and_capture_roots() {
    for is_package in [false, true] {
        let mut f = Body::new(0, Type::I64);
        f.f.environment = Some(ClosureId(0));
        let d = if is_package {
            let p = f.bind_package();
            f.field(p, 0)
        } else {
            f.capture()
        };
        let size = f.call(1, vec![d, d], Type::I64, vec![]).unwrap();
        let mut t = captured(f, size, CallableKind::FnMut, is_package);
        let mut helper = Body::new(1, Type::I64);
        let a = helper.parameter_value(desc());
        let b = helper.parameter_value(desc());
        let view = helper.view(a, false);
        helper.reserve(b);
        let n = helper.length(view);
        let (function, plan) = helper.finish(Some(n));
        t.module.functions.push(function);
        t.plan.functions.push(plan);
        t.reject("E-BORROW-CONFLICT");
    }
}

#[test]
fn owned_package_slice_escape_is_rejected_and_lending_captures_remain_closed() {
    let mut f = Body::new(0, slice(false));
    let p = f.package();
    let d = f.field(p, 0);
    let view = f.view(d, false);
    case(vec![f.finish(Some(view))]).reject("E-BORROW-ESCAPE");
    for is_package in [false, true] {
        let mut f = Body::new(0, slice(false));
        f.f.environment = Some(ClosureId(0));
        let d = if is_package {
            let p = f.bind_package();
            f.field(p, 0)
        } else {
            f.capture()
        };
        let view = f.view(d, false);
        // Closure result shapes intentionally remain the finite Copy profile.
        captured(f, view, CallableKind::Fn, is_package).reject("E-UNSUPPORTED");
    }
}
