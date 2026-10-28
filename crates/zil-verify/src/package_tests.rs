//! Public raw/wire candidates only. No source solver or ownership analyzer is
//! called by these fixtures; the shared plan helper supplies initialization and
//! empty address-Hold claims, not package/resource facts.
use super::*;
use zil_base::{FileId, ModuleId};

mod captures;
mod fields;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn payload() -> Type {
    Type::Record {
        name: "PackageToken".into(),
        fields: vec![("code".into(), Type::I32)],
    }
}
fn owned() -> Type {
    Type::Managed(Box::new(payload()))
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn package() -> Type {
    Type::Package {
        name: "Pair".into(),
        fields: vec![
            ("first".into(), owned()),
            ("second".into(), owned()),
            ("number".into(), Type::I32),
        ],
    }
}
fn ins(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}
struct Body(TypedFunction);
impl Body {
    fn new(result: Type) -> Self {
        Self(TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(0),
            name: "main".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: result,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) })],
                terminator: Terminator::Trap,
            }],
            entry: BlockId(0),
            span: span(),
        })
    }
    fn op(&mut self, kind: InstructionKind) {
        self.0.blocks[0].instructions.push(ins(None, kind));
    }
    fn value(&mut self, ty: Type, kind: InstructionKind) -> ValueId {
        let id = ValueId(self.0.values.len() as u32);
        self.0.values.push(ty);
        self.0.blocks[0].instructions.push(ins(Some(id), kind));
        id
    }
    fn storage(&mut self, ty: Type) -> LocalId {
        let id = LocalId(self.0.locals.len() as u32);
        self.0.locals.push(LocalDecl {
            id,
            name: format!("local{}", id.0),
            ty,
            mutable: true,
            span: span(),
        });
        self.op(InstructionKind::StorageLive(id));
        id
    }
    fn parameter(&mut self, ty: Type) -> LocalId {
        let id = self.storage(ty);
        self.0.blocks[0].instructions.pop();
        self.0.parameters.push(id);
        id
    }
    fn token(&mut self, code: i32) -> ValueId {
        let code = self.value(Type::I32, InstructionKind::Constant(Constant::I32(code)));
        self.value(owned(), InstructionKind::MakeManaged { fields: vec![code] })
    }
    fn package_value(&mut self) -> ValueId {
        let first = self.token(65);
        let second = self.token(66);
        let number = self.value(Type::I32, InstructionKind::Constant(Constant::I32(7)));
        self.value(
            package(),
            InstructionKind::MakePackage {
                fields: vec![first, second, number],
            },
        )
    }
    fn package_local(&mut self) -> LocalId {
        let local = self.storage(package());
        let value = self.package_value();
        self.op(InstructionKind::Store { local, value });
        local
    }
    fn finish(mut self, value: Option<ValueId>, kind: CleanupExitKind) -> TypedFunction {
        for local in (0..self.0.locals.len()).rev() {
            self.op(InstructionKind::StorageDead(LocalId(local as u32)));
        }
        self.op(InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind,
        });
        self.0.blocks[0].terminator = Terminator::Return(value);
        self.0
    }
}
fn module(function: TypedFunction) -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"independent package candidates"),
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "PackageToken".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn plain() -> TypedModule {
    let mut b = Body::new(Type::I32);
    let local = b.package_local();
    let value = b.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    b.op(InstructionKind::DropPackage { local });
    module(b.finish(Some(value), CleanupExitKind::Return))
}
fn check_both(
    module: TypedModule,
) -> (CompileResult<VerifiedModule>, CompileResult<VerifiedModule>) {
    let plan = resource_tests::plan(&module);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    (
        verify_module(module, plan, &target, &limits),
        verify_module_bytes(&bytes, &target, &limits),
    )
}
fn accepted(module: TypedModule) {
    let (raw, wire) = check_both(module);
    raw.unwrap();
    wire.unwrap();
}
fn rejected(module: TypedModule, needle: &str) {
    let (raw, wire) = check_both(module);
    for result in [raw, wire] {
        let errors = result.unwrap_err();
        assert!(
            format!("{errors:?}").contains(needle),
            "expected {needle}: {errors:?}"
        );
    }
}

#[test]
fn complete_package_copy_field_and_cleanup_pass_raw_and_wire() {
    accepted(plain());
}

#[test]
fn whole_package_return_uses_ordinary_value_mode() {
    let mut b = Body::new(package());
    let local = b.package_local();
    let value = b.value(package(), InstructionKind::TakeLocal { local });
    accepted(module(b.finish(Some(value), CleanupExitKind::Return)));
}

#[test]
fn package_parameter_field_description_and_cleanup_are_checked() {
    let mut b = Body::new(Type::I32);
    let local = b.parameter(package());
    let d = b.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 1 },
    );
    let value = b.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    b.op(InstructionKind::DropPackage { local });
    accepted(module(b.finish(Some(value), CleanupExitKind::Return)));
}

#[test]
fn package_construction_consumes_each_managed_input_once() {
    let mut m = plain();
    for instruction in &mut m.functions[0].blocks[0].instructions {
        if let InstructionKind::MakePackage { fields } = &mut instruction.kind {
            fields[1] = fields[0];
        }
    }
    rejected(m, "SSA value was not available exactly once");
}

#[test]
fn package_load_cannot_copy_the_owner() {
    let mut m = plain();
    let f = &mut m.functions[0];
    let value = ValueId(f.values.len() as u32);
    f.values.push(package());
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
        .unwrap();
    f.blocks[0]
        .instructions
        .insert(at, ins(Some(value), InstructionKind::Load(LocalId(0))));
    rejected(m, "never Load");
}

#[test]
fn package_cleanup_cannot_be_omitted_or_duplicated() {
    let mut missing = plain();
    missing.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    rejected(missing, "outstanding resource responsibility");
    let mut duplicate = plain();
    let block = &mut duplicate.functions[0].blocks[0];
    let at = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
        .unwrap();
    block.instructions.insert(
        at,
        ins(None, InstructionKind::DropPackage { local: LocalId(0) }),
    );
    rejected(duplicate, "duplicate automatic cleanup");
}

#[test]
fn package_field_description_blocks_whole_move_until_last_use() {
    let mut b = Body::new(Type::I32);
    let local = b.package_local();
    let d = b.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 0 },
    );
    let moved = b.value(package(), InstructionKind::TakeLocal { local });
    let value = b.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    let destination = b.storage(package());
    b.op(InstructionKind::Store {
        local: destination,
        value: moved,
    });
    b.op(InstructionKind::DropPackage { local: destination });
    rejected(
        module(b.finish(Some(value), CleanupExitKind::Return)),
        "invalidate a live description",
    );
}

#[test]
fn package_names_field_names_and_manages_admission_are_checked() {
    for ty in [
        Type::Package {
            name: String::new(),
            fields: vec![("r".into(), owned())],
        },
        Type::Package {
            name: "P".into(),
            fields: vec![(String::new(), owned())],
        },
        Type::Package {
            name: "P".into(),
            fields: vec![("r".into(), owned()), ("r".into(), Type::I32)],
        },
    ] {
        let mut m = plain();
        m.functions[0].locals[0].ty = ty;
        rejected(m, "E-VERIFY-PACKAGE");
    }
    for fields in [vec![], vec![("value".into(), Type::I32)]] {
        let mut m = plain();
        m.functions[0].locals[0].ty = Type::Package {
            name: "EmptyOwner".into(),
            fields,
        };
        rejected(m, "at least one manages field");
    }
}

#[test]
fn package_manages_fields_require_matching_nominal_resources() {
    for unknown in [true, false] {
        let mut m = plain();
        let Type::Package { fields, .. } = &mut m.functions[0].locals[0].ty else {
            unreachable!()
        };
        let mut p = payload();
        let Type::Record {
            name,
            fields: inner,
        } = &mut p
        else {
            unreachable!()
        };
        if unknown {
            *name = "Missing".into();
        } else {
            inner[0].1 = Type::I64;
        }
        fields[0].1 = Type::Managed(Box::new(p));
        rejected(m, "absent from its nominal definition table");
    }
}

#[test]
fn package_fields_reject_noncopy_and_nested_owners() {
    for extra in [
        package(),
        desc(),
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Tuple(vec![owned()]),
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(owned()),
        },
    ] {
        let mut m = plain();
        let Type::Package { fields, .. } = &mut m.functions[0].locals[0].ty else {
            unreachable!()
        };
        fields[2].1 = extra;
        rejected(m, "non-Copy type");
    }
}

#[test]
fn package_cannot_hide_inside_copy_containers_or_indirect_signatures() {
    for ty in [
        Type::Tuple(vec![package()]),
        Type::Record {
            name: "Hidden".into(),
            fields: vec![("p".into(), package())],
        },
        Type::Enum {
            name: "Hidden".into(),
            variants: vec![("P".into(), vec![package()])],
        },
        Type::FunctionPointer {
            parameters: vec![package()],
            result: Box::new(Type::Unit),
        },
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(package()),
        },
    ] {
        let mut m = plain();
        m.functions[0].locals[0].ty = ty;
        rejected(m, "non-Copy type");
    }
}

#[test]
fn callable_package_field_relabel_requires_a_real_closure_definition() {
    let mut m = plain();
    let Type::Package { fields, .. } = &mut m.functions[0].locals[0].ty else {
        unreachable!()
    };
    fields[2].1 = Type::Closure(ClosureId(0));
    // Schema21 admits a concrete callable field shape, never an absent ID.
    let (raw, wire) = check_both(m);
    for result in [raw, wire] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == zil_base::ResultCategory::SafeReject
                && d.code == "E-VERIFY-CLOSURE"
                && d.message == "closure identity is outside the definition table"),
            "{errors:?}"
        );
    }
}

#[test]
fn shared_package_window_relabel_cannot_turn_a_stored_owner_into_an_address() {
    let mut m = plain();
    m.functions[0].locals[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(package()),
    };
    // Schema21 admits a shared window shape; this unchanged forged Store
    // still attempts to publish an owning Package into address-typed storage.
    let (raw, wire) = check_both(m);
    for result in [raw, wire] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == zil_base::ResultCategory::SafeReject
                && d.code == "E-VERIFY-TYPE"
                && d.message == "stored value does not match local type"),
            "{errors:?}"
        );
    }
}

#[test]
fn owning_error_union_local_relabeling_still_requires_actual_injection() {
    for ty in [
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(package()),
        },
        Type::ErrorUnion {
            error: Box::new(package()),
            result: Box::new(Type::I32),
        },
    ] {
        let mut m = plain();
        // Both finite types are admitted by 166. Merely relabeling storage
        // cannot stand in for MakeErrorUnion or its consuming payload transfer.
        m.functions[0].locals[0].ty = ty;
        let (raw, wire) = check_both(m);
        for result in [raw, wire] {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| d.phase == Phase::Verify
                    && d.category == zil_base::ResultCategory::SafeReject
                    && d.code == "E-VERIFY-TYPE"
                    && d.message == "stored value does not match local type"),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn package_array_relabel_cannot_replace_required_owned_array_operations() {
    let mut module = plain();
    module.functions[0].locals[0].ty = Type::Array {
        element: Box::new(package()),
        length: 1,
    };
    let (raw, wire) = check_both(module);
    for result in [raw, wire] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-TYPE"),
            "{errors:?}"
        );
    }
}

#[test]
fn construction_checks_arity_field_order_operands_and_result_shape() {
    for change in 0..5 {
        let mut m = plain();
        let f = &mut m.functions[0];
        let make = f.blocks[0]
            .instructions
            .iter_mut()
            .find(|i| matches!(i.kind, InstructionKind::MakePackage { .. }))
            .unwrap();
        let InstructionKind::MakePackage { fields } = &mut make.kind else {
            unreachable!()
        };
        let needle = match change {
            0 => {
                fields.pop();
                "every declared field"
            }
            1 => {
                fields.swap(0, 2);
                "declared field type"
            }
            2 => {
                fields[0] = ValueId(u32::MAX);
                "absent SSA value"
            }
            3 => {
                make.result = None;
                "requires a typed result"
            }
            _ => {
                f.values[make.result.unwrap().0 as usize] = Type::I32;
                "actual nominal package"
            }
        };
        rejected(m, needle);
    }
    let mut m = plain();
    let block = &mut m.functions[0].blocks[0];
    let make = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakePackage { .. }))
        .unwrap();
    block.instructions.swap(make, 2);
    rejected(m, "does not precede and dominate");
}

#[test]
fn projections_reject_owning_copy_wrong_kind_index_and_local() {
    for (kind, ty, needle) in [
        (
            InstructionKind::ReadPackageField {
                local: LocalId(0),
                index: 0,
            },
            owned(),
            "cannot copy an owning field",
        ),
        (
            InstructionKind::DescribePackageField {
                local: LocalId(0),
                index: 2,
            },
            desc(),
            "requires a manages field",
        ),
        (
            InstructionKind::ReadPackageField {
                local: LocalId(0),
                index: 3,
            },
            Type::I32,
            "outside its declaration",
        ),
        (
            InstructionKind::DescribePackageField {
                local: LocalId(0),
                index: u32::MAX,
            },
            desc(),
            "outside its declaration",
        ),
        (
            InstructionKind::ReadPackageField {
                local: LocalId(99),
                index: 2,
            },
            Type::I32,
            "absent local",
        ),
    ] {
        let mut m = plain();
        let f = &mut m.functions[0];
        let i = f.blocks[0]
            .instructions
            .iter_mut()
            .find(|i| matches!(i.kind, InstructionKind::ReadPackageField { .. }))
            .unwrap();
        let result = i.result.unwrap();
        f.values[result.0 as usize] = ty;
        i.kind = kind;
        rejected(m, needle);
    }
}

#[test]
fn projection_and_cleanup_require_real_package_storage() {
    for description in [false, true] {
        let mut b = Body::new(Type::Unit);
        let local = b.storage(Type::I32);
        let v = b.value(Type::I32, InstructionKind::Constant(Constant::I32(1)));
        b.op(InstructionKind::Store { local, value: v });
        b.value(
            if description { desc() } else { Type::I32 },
            if description {
                InstructionKind::DescribePackageField { local, index: 0 }
            } else {
                InstructionKind::ReadPackageField { local, index: 0 }
            },
        );
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "requires a package local",
        );
    }
    let mut b = Body::new(Type::Unit);
    let local = b.storage(owned());
    let value = b.token(1);
    b.op(InstructionKind::Store { local, value });
    b.op(InstructionKind::DropPackage { local });
    rejected(
        module(b.finish(None, CleanupExitKind::Return)),
        "requires a package local",
    );
}

#[test]
fn package_cannot_use_resource_result_modes_or_resource_operations() {
    for mode in [PassingMode::Give, PassingMode::Create] {
        let mut b = Body::new(package());
        b.0.result_mode = mode;
        let value = b.package_value();
        rejected(
            module(b.finish(Some(value), CleanupExitKind::Return)),
            "non-resource result must not claim",
        );
    }
    for kind in [
        InstructionKind::DescribeLocal { local: LocalId(0) },
        InstructionKind::DropManaged { local: LocalId(0) },
    ] {
        let mut b = Body::new(Type::Unit);
        b.package_local();
        if matches!(kind, InstructionKind::DescribeLocal { .. }) {
            b.value(desc(), kind);
        } else {
            b.op(kind);
        }
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "actual managed position",
        );
    }
}

#[test]
fn ordinary_call_consumes_and_returns_the_complete_package_once() {
    let mut callee = Body::new(package());
    callee.0.id = FunctionId(1);
    callee.0.name = "identity".into();
    let parameter = callee.parameter(package());
    let value = callee.value(package(), InstructionKind::TakeLocal { local: parameter });
    let callee = callee.finish(Some(value), CleanupExitKind::Return);
    let mut caller = Body::new(Type::Unit);
    let arg = caller.package_value();
    let result = caller.value(
        package(),
        InstructionKind::Call {
            function: FunctionId(1),
            arguments: vec![arg],
        },
    );
    let local = caller.storage(package());
    caller.op(InstructionKind::Store {
        local,
        value: result,
    });
    caller.op(InstructionKind::DropPackage { local });
    let mut m = module(caller.finish(None, CleanupExitKind::Return));
    m.functions.push(callee);
    accepted(m.clone());
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Store { .. }))
        .unwrap();
    f.blocks[0].instructions[at].kind = InstructionKind::Store { local, value: arg };
    rejected(m, "SSA value was not available exactly once");
}

#[test]
fn whole_move_may_follow_the_last_field_description_use() {
    let mut b = Body::new(package());
    let local = b.package_local();
    for index in 0..2 {
        let d = b.value(
            desc(),
            InstructionKind::DescribePackageField { local, index },
        );
        let v = b.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
        b.op(InstructionKind::WriteDesc {
            desc: d,
            index: 0,
            value: v,
        });
    }
    let value = b.value(package(), InstructionKind::TakeLocal { local });
    accepted(module(b.finish(Some(value), CleanupExitKind::Return)));
}

#[test]
fn description_alias_in_local_storage_still_blocks_package_move() {
    let mut b = Body::new(package());
    let local = b.package_local();
    let d = b.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 1 },
    );
    let alias = b.storage(desc());
    b.op(InstructionKind::Store {
        local: alias,
        value: d,
    });
    let value = b.value(package(), InstructionKind::TakeLocal { local });
    let d2 = b.value(desc(), InstructionKind::Load(alias));
    b.value(Type::I32, InstructionKind::ReadDesc { desc: d2, index: 0 });
    rejected(
        module(b.finish(Some(value), CleanupExitKind::Return)),
        "invalidate a live description",
    );
}

#[test]
fn package_cleanup_must_preserve_other_local_construction_order() {
    for correct in [true, false] {
        let mut b = Body::new(Type::Unit);
        let older = b.package_local();
        let newer = b.storage(owned());
        let value = b.token(3);
        b.op(InstructionKind::Store {
            local: newer,
            value,
        });
        let ops = [
            InstructionKind::DropManaged { local: newer },
            InstructionKind::DropPackage { local: older },
        ];
        for i in if correct { [0, 1] } else { [1, 0] } {
            b.op(ops[i].clone());
        }
        let m = module(b.finish(None, CleanupExitKind::Return));
        if correct {
            accepted(m);
        } else {
            rejected(m, "not in reverse successful initialization order");
        }
    }
}

#[test]
fn package_field_descriptions_never_follow_rebuilt_storage() {
    let mut b = Body::new(Type::I32);
    let local = b.package_local();
    let d = b.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 0 },
    );
    b.op(InstructionKind::DropPackage { local });
    b.op(InstructionKind::StorageDead(local));
    b.op(InstructionKind::StorageLive(local));
    let value = b.package_value();
    b.op(InstructionKind::Store { local, value });
    let result = b.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    b.op(InstructionKind::DropPackage { local });
    rejected(
        module(b.finish(Some(result), CleanupExitKind::Return)),
        "invalidate a live description",
    );
}

#[test]
fn complete_package_requires_strict_scope_even_without_error_operations() {
    let mut m = plain();
    m.functions[0].blocks[0].instructions.retain(|i| {
        !matches!(
            i.kind,
            InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
        )
    });
    rejected(m, "E-VERIFY-CLEANUP");
    let mut m = plain();
    m.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::StorageDead(_)));
    rejected(m, "E-VERIFY-CLEANUP");
}

#[test]
fn package_ssa_cannot_be_lost_after_the_root_storage_scope_closes() {
    let mut b = Body::new(Type::Unit);
    b.package_value();
    rejected(
        module(b.finish(None, CleanupExitKind::Return)),
        "normal return leaves",
    );
    let mut b = Body::new(package());
    let v = b.package_value();
    accepted(module(b.finish(Some(v), CleanupExitKind::Return)));
}

fn error_type() -> Type {
    Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::Unit),
    }
}
fn error_value(b: &mut Body) -> ValueId {
    let value = b.value(Type::I32, InstructionKind::Constant(Constant::I32(9)));
    b.value(
        error_type(),
        InstructionKind::MakeErrorUnion {
            is_error: true,
            value,
        },
    )
}

#[test]
fn later_argument_error_cleans_staged_package_and_keeps_unstored_debt_visible() {
    let mut b = Body::new(error_type());
    let argument = b.package_local();
    let error = error_value(&mut b);
    b.op(InstructionKind::DropPackage { local: argument });
    let m = module(b.finish(Some(error), CleanupExitKind::Error));
    accepted(m.clone());
    let mut missing = m;
    missing.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    rejected(missing, "outstanding resource responsibility");
    let mut b = Body::new(error_type());
    b.package_value();
    let error = error_value(&mut b);
    rejected(
        module(b.finish(Some(error), CleanupExitKind::Error)),
        "normal return leaves",
    );
}

#[test]
fn failed_preparation_cleans_only_independent_completed_temporaries_in_reverse_order() {
    for completed in 0..=2 {
        let mut b = Body::new(error_type());
        let mut locals = vec![];
        for code in 0..completed {
            let local = b.storage(owned());
            let value = b.token(code);
            b.op(InstructionKind::Store { local, value });
            locals.push(local);
        }
        let error = error_value(&mut b);
        for &local in locals.iter().rev() {
            b.op(InstructionKind::DropManaged { local });
        }
        let m = module(b.finish(Some(error), CleanupExitKind::Error));
        accepted(m.clone());
        if completed == 2 {
            let mut wrong = m;
            let ops = &mut wrong.functions[0].blocks[0].instructions;
            let at = ops
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                .unwrap();
            ops.swap(at, at + 1);
            rejected(wrong, "not in reverse successful initialization order");
        }
    }
}

fn description_closure(move_before_call: bool) -> TypedModule {
    let mut main = Body::new(Type::I32);
    let owner = main.package_local();
    let d = main.value(
        desc(),
        InstructionKind::DescribePackageField {
            local: owner,
            index: 1,
        },
    );
    let environment = main.storage(Type::Closure(ClosureId(0)));
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![d],
        },
    );
    main.op(InstructionKind::Store {
        local: environment,
        value: closure,
    });
    let destination = main.storage(package());
    if move_before_call {
        let moved = main.value(package(), InstructionKind::TakeLocal { local: owner });
        main.op(InstructionKind::Store {
            local: destination,
            value: moved,
        });
    }
    let value = main.value(
        Type::I32,
        InstructionKind::CallClosureShared {
            callee: environment,
            arguments: vec![],
        },
    );
    if !move_before_call {
        let moved = main.value(package(), InstructionKind::TakeLocal { local: owner });
        main.op(InstructionKind::Store {
            local: destination,
            value: moved,
        });
    }
    main.op(InstructionKind::DropPackage { local: destination });
    let mut entry = Body::new(Type::I32);
    entry.0.id = FunctionId(1);
    entry.0.name = "closure_entry".into();
    entry.0.environment = Some(ClosureId(0));
    let d = entry.value(desc(), InstructionKind::ReadCapture { index: 0 });
    let value_entry = entry.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    // A borrowed target may be written by a shared callable; this is distinct
    // from writing an owned capture through an Fn entry.
    entry.op(InstructionKind::WriteDesc {
        desc: d,
        index: 0,
        value: value_entry,
    });
    let mut m = module(main.finish(Some(value), CleanupExitKind::Return));
    m.functions
        .push(entry.finish(Some(value_entry), CleanupExitKind::Return));
    m.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(1),
        kind: CallableKind::Fn,
        captures: vec![desc()],
        parameters: vec![],
        result: Type::I32,
    });
    m
}

#[test]
fn package_field_description_capture_retains_owner_and_allows_move_after_last_call() {
    accepted(description_closure(false));
    rejected(description_closure(true), "invalidate a live description");
}

#[test]
fn package_capture_requires_views_and_closure_package_signatures_remain_closed() {
    let mut m = description_closure(false);
    m.closures[0].captures = vec![package()];
    rejected(m, "capture operand type differs");
    for result in [false, true] {
        let mut m = description_closure(false);
        if result {
            m.closures[0].result = package();
        } else {
            m.closures[0].parameters.push(package());
        }
        rejected(m, "non-Copy type");
    }
}

#[test]
fn package_function_reference_cannot_erase_ordinary_move_contract() {
    let mut entry = Body::new(package());
    entry.0.id = FunctionId(1);
    entry.0.name = "owned_entry".into();
    let v = entry.package_value();
    let mut b = Body::new(Type::Unit);
    b.value(
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(package()),
        },
        InstructionKind::FunctionRef {
            function: FunctionId(1),
        },
    );
    let mut m = module(b.finish(None, CleanupExitKind::Return));
    m.functions
        .push(entry.finish(Some(v), CleanupExitKind::Return));
    rejected(m, "non-Copy type");
}

#[test]
fn descriptions_of_distinct_packages_do_not_block_unrelated_moves() {
    let mut b = Body::new(Type::I32);
    let first = b.package_local();
    let second = b.package_local();
    let d = b.value(
        desc(),
        InstructionKind::DescribePackageField {
            local: first,
            index: 0,
        },
    );
    let moved = b.value(package(), InstructionKind::TakeLocal { local: second });
    let destination = b.storage(package());
    b.op(InstructionKind::Store {
        local: destination,
        value: moved,
    });
    let value = b.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    b.op(InstructionKind::DropPackage { local: destination });
    b.op(InstructionKind::DropPackage { local: first });
    accepted(module(b.finish(Some(value), CleanupExitKind::Return)));
}

fn branch_description(move_before_use: bool) -> TypedModule {
    let mut b = Body::new(Type::I32);
    let condition = b.parameter(Type::Bool);
    let first = b.package_local();
    let second = b.package_local();
    let alias = b.storage(desc());
    let destination = b.storage(package());
    let condition = b.value(Type::Bool, InstructionKind::Load(condition));
    let root = b.0.blocks[0].instructions.len();
    let da = b.value(
        desc(),
        InstructionKind::DescribePackageField {
            local: first,
            index: 0,
        },
    );
    b.op(InstructionKind::Store {
        local: alias,
        value: da,
    });
    let left = b.0.blocks[0].instructions.split_off(root);
    let db = b.value(
        desc(),
        InstructionKind::DescribePackageField {
            local: second,
            index: 1,
        },
    );
    b.op(InstructionKind::Store {
        local: alias,
        value: db,
    });
    let right = b.0.blocks[0].instructions.split_off(root);
    if move_before_use {
        let moved = b.value(package(), InstructionKind::TakeLocal { local: second });
        b.op(InstructionKind::Store {
            local: destination,
            value: moved,
        });
    }
    let d = b.value(desc(), InstructionKind::Load(alias));
    let result = b.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    if !move_before_use {
        let moved = b.value(package(), InstructionKind::TakeLocal { local: second });
        b.op(InstructionKind::Store {
            local: destination,
            value: moved,
        });
    }
    b.op(InstructionKind::DropPackage { local: destination });
    b.op(InstructionKind::DropPackage { local: first });
    let mut f = b.finish(Some(result), CleanupExitKind::Return);
    let merge = f.blocks[0].instructions.split_off(root);
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: left,
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: right,
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: merge,
            terminator: Terminator::Return(Some(result)),
        },
    ]);
    module(f)
}

#[test]
fn branch_join_retains_all_package_field_description_origins() {
    accepted(branch_description(false));
    rejected(branch_description(true), "invalidate a live description");
}

fn conditional_move() -> TypedModule {
    let mut b = Body::new(Type::Unit);
    let parameter = b.parameter(package());
    let flag = b.parameter(Type::Bool);
    let condition = b.value(Type::Bool, InstructionKind::Load(flag));
    let prefix = b.0.blocks[0].instructions.len();
    let moved = b.value(package(), InstructionKind::TakeLocal { local: parameter });
    b.op(InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![moved],
    });
    let then = b.0.blocks[0].instructions.split_off(prefix);
    b.op(InstructionKind::DropPackage { local: parameter });
    let mut f = b.finish(None, CleanupExitKind::Return);
    let merge = f.blocks[0].instructions.split_off(prefix);
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: then,
            terminator: Terminator::Jump(BlockId(2)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: merge,
            terminator: Terminator::Return(None),
        },
    ]);
    let mut sink = Body::new(Type::Unit);
    sink.0.id = FunctionId(1);
    sink.0.name = "sink".into();
    let local = sink.parameter(package());
    sink.op(InstructionKind::DropPackage { local });
    let mut m = module(f);
    m.functions.push(sink.finish(None, CleanupExitKind::Return));
    m
}

#[test]
fn conditional_whole_move_preserves_other_edges_cleanup_responsibility() {
    accepted(conditional_move());
    let mut m = conditional_move();
    m.functions[0].blocks[2]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    rejected(m, "outstanding resource responsibility");
}

#[test]
fn projection_or_second_move_after_conditional_transfer_is_rejected() {
    for projection in [false, true] {
        let mut m = conditional_move();
        let f = &mut m.functions[0];
        let id = ValueId(f.values.len() as u32);
        let (ty, kind) = if projection {
            (
                Type::I32,
                InstructionKind::ReadPackageField {
                    local: LocalId(0),
                    index: 2,
                },
            )
        } else {
            (package(), InstructionKind::TakeLocal { local: LocalId(0) })
        };
        f.values.push(ty);
        f.blocks[2].instructions.insert(0, ins(Some(id), kind));
        rejected(m, "before initialization");
    }
}

fn package_loop() -> TypedModule {
    let mut b = Body::new(Type::Unit);
    let flag = b.parameter(Type::Bool);
    let root = b.0.blocks[0].instructions.len();
    let condition = b.value(Type::Bool, InstructionKind::Load(flag));
    let header = b.0.blocks[0].instructions.split_off(root);
    b.op(InstructionKind::ScopeEnter { scope: ScopeId(1) });
    let local = b.package_local();
    b.op(InstructionKind::DropPackage { local });
    b.op(InstructionKind::StorageDead(local));
    b.op(InstructionKind::ScopeExit {
        scope: ScopeId(1),
        kind: CleanupExitKind::Continue,
    });
    let body = b.0.blocks[0].instructions.split_off(root);
    let mut f = b.0;
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: header,
            terminator: Terminator::Branch {
                condition,
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: body,
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![
                ins(None, InstructionKind::StorageDead(flag)),
                ins(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                ),
            ],
            terminator: Terminator::Return(None),
        },
    ]);
    module(f)
}

#[test]
fn package_loop_reactivation_must_discharge_each_previous_iteration() {
    accepted(package_loop());
    let mut m = package_loop();
    m.functions[0].blocks[2]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    rejected(m, "outstanding resource responsibility");
    let mut m = package_loop();
    m.functions[0].blocks[2]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::Store { .. }));
    // The local's conditional drop is harmless, but the never-submitted SSA
    // owner survives the backedge and must not be silently overwritten.
    rejected(m, "loop redefines a managed SSA result");
}

#[test]
fn package_candidate_identity_and_work_limits_remain_independent() {
    let m = plain();
    let p = resource_tests::plan(&m);
    let mut changed = m.clone();
    let Type::Package { name, .. } = &mut changed.functions[0].locals[0].ty else {
        unreachable!()
    };
    *name = "OtherPair".into();
    let errors = verify_module(changed, p, &m.target, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-VERIFY-PLAN-IDENTITY"),
        "{errors:?}"
    );
    for limits in [
        Limits {
            max_analysis_steps: 64,
            ..Limits::default()
        },
        Limits {
            max_call_depth: 1,
            ..Limits::default()
        },
    ] {
        let errors =
            verify_module(m.clone(), resource_tests::plan(&m), &m.target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.category == zil_base::ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }
}

#[test]
fn package_field_metadata_and_nominal_lookup_pay_for_repeated_admission() {
    let small = plain();
    let mut large = small.clone();
    for index in 0..128 {
        let name = format!("Unused{index}");
        large.resources.push(ResourceDefinition {
            payload: Type::Record {
                name: name.clone(),
                fields: vec![],
            },
            name,
            teardown: None,
        });
    }
    for (m, exhausts) in [(&small, false), (&large, true)] {
        let mut budget = Budget { remaining: 512 };
        let result = packages::check_type_for(m, &package(), None, &mut budget);
        if exhausts {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        } else {
            result.unwrap();
        }
    }
}
