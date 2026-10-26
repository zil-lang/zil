//! Independent untrusted-IR controls. These fixtures do not depend on Types or
//! Analyze and are not reported as source/native execution evidence.
use std::collections::BTreeSet;
use zil_base::{
    Endian, FileId, Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;
use zil_verify::{verify_ct_program, verify_ct_program_bytes, verify_module, verify_module_bytes};

fn span() -> Span {
    Span::new(FileId(0), 1, 2)
}
fn array(element: Type, length: u32) -> Type {
    Type::Array {
        element: Box::new(element),
        length,
    }
}
fn address(inner: Type, mutable: bool) -> Type {
    Type::Address {
        inner: Box::new(inner),
        mutable,
    }
}
fn slice(element: Type, mutable: bool) -> Type {
    address(
        Type::Slice {
            element: Box::new(element),
        },
        mutable,
    )
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
        let checked = verify_module(
            self.module.clone(),
            self.plan.clone(),
            &self.module.target,
            &limits,
        )
        .unwrap();
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        let decoded = verify_module_bytes(&bytes, &self.module.target, &limits).unwrap();
        assert_eq!(checked.module(), decoded.module());
        assert_eq!(checked.plan(), decoded.plan());
    }
    fn reject(&mut self, expected: &str) {
        self.bind();
        let limits = Limits::default();
        let errors = verify_module(
            self.module.clone(),
            self.plan.clone(),
            &self.module.target,
            &limits,
        )
        .unwrap_err();
        assert_eq!(errors[0].code, expected, "raw {errors:?}");
        let bytes = wire::encode_module_candidate(&self.module, &self.plan, &limits).unwrap();
        let errors = verify_module_bytes(&bytes, &self.module.target, &limits).unwrap_err();
        assert_eq!(errors[0].code, expected, "wire {errors:?}");
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
                id: FunctionId(id),
                name: format!("f{id}"),
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
    fn local(&mut self, ty: Type) -> LocalId {
        let id = LocalId(self.f.locals.len() as u32);
        self.f.locals.push(LocalDecl {
            id,
            name: format!("p{}", id.0),
            ty,
            mutable: true,
            span: span(),
        });
        id
    }
    fn parameter(&mut self, ty: Type) -> LocalId {
        let local = self.local(ty);
        self.f.locals[local.0 as usize].mutable = false;
        self.f.parameters.push(local);
        self.initialized.insert(local);
        local
    }
    fn value(&mut self, ty: Type, kind: InstructionKind, roots: Vec<HoldRoot>) -> ValueId {
        let value = ValueId(self.f.values.len() as u32);
        self.f.values.push(ty);
        self.holds.push(ValueHold { value, roots });
        self.f.blocks[0].instructions.push(Instruction {
            result: Some(value),
            kind,
            span: span(),
        });
        value
    }
    fn emit(&mut self, kind: InstructionKind) {
        match &kind {
            InstructionKind::Store { local, .. } => {
                self.initialized.insert(*local);
            }
            InstructionKind::TakeLocal { local }
            | InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local) => {
                self.initialized.remove(local);
            }
            _ => {}
        }
        self.f.blocks[0].instructions.push(Instruction {
            result: None,
            kind,
            span: span(),
        });
    }
    fn constant(&mut self, c: Constant) -> ValueId {
        self.value(c.ty(), InstructionKind::Constant(c), vec![])
    }
    fn parameter_value(&mut self, ty: Type) -> ValueId {
        let index = self.f.parameters.len() as u32;
        let local = self.parameter(ty.clone());
        self.value(
            ty,
            InstructionKind::Load(local),
            vec![HoldRoot::Parameter(index)],
        )
    }
    fn range(&mut self, source: ValueId, start: Option<ValueId>, end: Option<ValueId>) -> ValueId {
        let Type::Address { mutable, inner } = &self.f.values[source.0 as usize] else {
            panic!()
        };
        let (Type::Array { element, .. } | Type::Slice { element }) = inner.as_ref() else {
            panic!()
        };
        self.value(
            slice((**element).clone(), *mutable),
            InstructionKind::AddressSlice {
                address: source,
                start,
                end,
            },
            self.holds[source.0 as usize].roots.clone(),
        )
    }
    fn length(&mut self, source: ValueId) -> ValueId {
        self.value(
            Type::I64,
            InstructionKind::SliceLength { address: source },
            vec![],
        )
    }
    fn reborrow(&mut self, source: ValueId, mutable: bool) -> ValueId {
        let Type::Address { inner, .. } = &self.f.values[source.0 as usize] else {
            panic!()
        };
        self.value(
            address((**inner).clone(), mutable),
            InstructionKind::ReborrowAddress {
                address: source,
                mutable,
            },
            self.holds[source.0 as usize].roots.clone(),
        )
    }
    fn index(&mut self, source: ValueId, index: ValueId) -> ValueId {
        let Type::Address { mutable, inner } = &self.f.values[source.0 as usize] else {
            panic!()
        };
        let (Type::Array { element, .. } | Type::Slice { element }) = inner.as_ref() else {
            panic!()
        };
        self.value(
            address((**element).clone(), *mutable),
            InstructionKind::AddressIndex {
                address: source,
                index,
            },
            self.holds[source.0 as usize].roots.clone(),
        )
    }
    fn finish(mut self, returned: Option<ValueId>) -> (TypedFunction, FunctionPlan) {
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
        source_fingerprint: Fingerprint::of(b"091 independent raw fixtures"),
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
fn len_case(element: Type, mutable: bool) -> Case {
    let mut body = Body::new(0, Type::I64);
    let p = body.parameter_value(slice(element, mutable));
    let n = body.length(p);
    case(vec![body.finish(Some(n))])
}
fn range_case(mutable: bool) -> Case {
    let mut body = Body::new(0, Type::I64);
    let p = body.parameter_value(address(array(Type::I32, 4), mutable));
    let lo = body.constant(Constant::I32(1));
    let hi = body.constant(Constant::U16(3));
    let s = body.range(p, Some(lo), Some(hi));
    let n = body.length(s);
    case(vec![body.finish(Some(n))])
}

#[test]
fn review_third_range_operand_must_dominate_across_a_real_cfg_join() {
    for mutable in [false, true] {
        let mut body = Body::new(0, Type::I64);
        let p = body.parameter_value(address(array(Type::I32, 4), mutable));
        let low = body.constant(Constant::I32(1));
        let high = body.constant(Constant::I32(3));
        let view = body.range(p, Some(low), Some(high));
        let length = body.length(view);
        let choose = body.constant(Constant::Bool(true));
        let (mut function, mut plan) = body.finish(Some(length));
        let instructions = std::mem::take(&mut function.blocks[0].instructions);
        function.blocks = vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    instructions[0].clone(),
                    instructions[1].clone(),
                    instructions[2].clone(),
                    instructions[5].clone(),
                ],
                terminator: Terminator::Branch {
                    condition: choose,
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
                instructions: instructions[3..5].to_vec(),
                terminator: Terminator::Return(Some(length)),
            },
        ];
        plan.blocks = (0..4)
            .map(|id| BlockPlan {
                block: BlockId(id),
                initialized_in: vec![LocalId(0)],
                initialized_out: vec![LocalId(0)],
            })
            .collect();
        let mut test = case(vec![(function, plan)]);
        test.pass();
        // The ID, integer type, definitions and candidate fingerprint remain
        // well formed. Only the third operand's dominance is now false.
        let high_instruction = test.module.functions[0].blocks[0].instructions.remove(2);
        test.module.functions[0].blocks[1]
            .instructions
            .push(high_instruction);
        test.reject("E-VERIFY-DOMINANCE");
    }
}

#[test]
fn ordinary_slice_windows_admit_copy_elements_and_both_targets() {
    let elements = [
        Type::Unit,
        Type::Bool,
        Type::U8,
        Type::U16,
        Type::I32,
        Type::I64,
        Type::Tuple(vec![]),
        Type::Tuple(vec![Type::I32, Type::Bool]),
        array(Type::U8, 3),
        Type::Record {
            name: "Pair".into(),
            fields: vec![("x".into(), Type::I32)],
        },
        Type::Enum {
            name: "E".into(),
            variants: vec![("A".into(), vec![]), ("B".into(), vec![Type::I64])],
        },
    ];
    for element in elements {
        for mutable in [false, true] {
            let mut test = len_case(element.clone(), mutable);
            test.pass();
            test.module.target = TargetProfile::windows_x64_llvm();
            test.pass();
        }
    }
}

#[test]
fn range_optional_endpoints_integer_widths_and_subranges_are_not_new_loans() {
    for start in [None, Some(Constant::U8(0)), Some(Constant::I32(-1))] {
        for end in [None, Some(Constant::U16(4)), Some(Constant::I64(5))] {
            for mutable in [false, true] {
                let mut body = Body::new(0, Type::I64);
                let p = body.parameter_value(address(array(Type::I32, 4), mutable));
                let lo = start.clone().map(|v| body.constant(v));
                let hi = end.clone().map(|v| body.constant(v));
                let first = body.range(p, lo, hi);
                let second = body.range(first, None, None);
                let n = body.length(second);
                // Out-of-range integer values are a checked runtime trap, not
                // a reason to trust/reject the raw integer type differently.
                case(vec![body.finish(Some(n))]).pass();
            }
        }
    }
}

#[test]
fn mutable_slice_index_uses_existing_exclusive_qualification() {
    let mut body = Body::new(0, Type::I32);
    let p = body.parameter_value(slice(Type::I32, true));
    let s = body.range(p, None, None);
    let zero = body.constant(Constant::I32(0));
    let ptr = body.index(s, zero);
    let next = body.constant(Constant::I32(9));
    body.emit(InstructionKind::WriteAddress {
        address: ptr,
        value: next,
    });
    let got = body.value(
        Type::I32,
        InstructionKind::ReadAddress { address: ptr },
        vec![],
    );
    case(vec![body.finish(Some(got))]).pass();
}

#[test]
fn ordinary_returned_slice_origins_cross_two_call_boundaries() {
    for mutable in [false, true] {
        let ty = slice(Type::I32, mutable);
        let mut leaf = Body::new(0, ty.clone());
        let p = leaf.parameter_value(ty.clone());
        let view = leaf.range(p, None, None);
        let returned = if mutable {
            leaf.reborrow(view, true)
        } else {
            view
        };
        let mut middle = Body::new(1, ty.clone());
        let p = middle.parameter_value(ty.clone());
        let argument = middle.reborrow(p, mutable);
        let ret = middle.value(
            ty.clone(),
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![argument],
            },
            vec![HoldRoot::Parameter(0)],
        );
        let mut caller = Body::new(2, Type::I64);
        let p = caller.parameter_value(ty.clone());
        let argument = caller.reborrow(p, mutable);
        let ret2 = caller.value(
            ty,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![argument],
            },
            vec![HoldRoot::Parameter(0)],
        );
        let n = caller.length(ret2);
        let mut test = case(vec![
            leaf.finish(Some(returned)),
            middle.finish(Some(ret)),
            caller.finish(Some(n)),
        ]);
        test.pass();
        let verified = verify_module(
            test.module.clone(),
            test.plan.clone(),
            &test.module.target,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(verified.result_origins(FunctionId(0)), Some(vec![0]));
        assert_eq!(verified.result_origins(FunctionId(1)), Some(vec![0]));
        assert_eq!(verified.result_origins(FunctionId(2)), Some(vec![]));
    }
}

#[test]
fn slice_projection_result_elements_permissions_and_len_type_are_rebuilt() {
    for replacement in [
        slice(Type::U8, false),
        slice(Type::I32, true),
        address(array(Type::I32, 4), false),
    ] {
        let mut test = range_case(false);
        test.module.functions[0].values[3] = replacement;
        test.reject("E-VERIFY-TYPE");
    }
    let mut test = range_case(false);
    test.module.functions[0].values[4] = Type::I32;
    test.module.functions[0].return_type = Type::I32;
    test.reject("E-VERIFY-TYPE");
}

#[test]
fn both_range_endpoints_are_typed_and_the_third_operand_is_checked() {
    for which in [true, false] {
        let mut test = range_case(false);
        let idx = if which { 1 } else { 2 };
        test.module.functions[0].values[idx] = Type::Bool;
        test.module.functions[0].blocks[0].instructions[idx].kind =
            InstructionKind::Constant(Constant::Bool(true));
        test.reject("E-VERIFY-ADDRESS");
    }
    for which in [true, false] {
        let mut test = range_case(false);
        let InstructionKind::AddressSlice { start, end, .. } =
            &mut test.module.functions[0].blocks[0].instructions[3].kind
        else {
            panic!()
        };
        *if which { start } else { end } = Some(ValueId(999));
        test.reject("E-VERIFY-ID");
    }
    let mut test = range_case(false);
    let instructions = &mut test.module.functions[0].blocks[0].instructions;
    instructions.swap(2, 3);
    test.reject("E-VERIFY-DOMINANCE");
}

#[test]
fn forged_hold_claims_never_manufacture_a_slice_origin() {
    let mut test = range_case(false);
    test.plan.functions[0].holds[3].roots.clear();
    test.reject("E-VERIFY-HOLD");
    let mut test = range_case(false);
    test.plan.functions[0].holds[3].roots = vec![HoldRoot::Local(LocalId(0))];
    test.reject("E-VERIFY-HOLD");
}

#[test]
fn unsupported_slice_storage_and_nested_elements_are_rejected_independently() {
    let unsized_type = Type::Slice {
        element: Box::new(Type::I32),
    };
    let candidates = [
        unsized_type.clone(),
        array(unsized_type.clone(), 1),
        Type::Tuple(vec![slice(Type::I32, false)]),
        Type::Record {
            name: "Bad".into(),
            fields: vec![("view".into(), slice(Type::I32, false))],
        },
        address(address(unsized_type.clone(), false), false),
        slice(address(Type::I32, false), false),
        slice(unsized_type, false),
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(slice(Type::I32, false)),
        },
        Type::FunctionPointer {
            parameters: vec![slice(Type::I32, false)],
            result: Box::new(Type::I32),
        },
    ];
    for ty in candidates {
        let mut body = Body::new(0, Type::Unit);
        body.parameter(ty);
        case(vec![body.finish(None)]).reject("E-UNSUPPORTED");
    }
}

#[test]
fn whole_unsized_reads_writes_and_non_slice_len_have_no_admission() {
    let mut test = len_case(Type::I32, false);
    test.module.functions[0].blocks[0].instructions[1].kind = InstructionKind::ReadAddress {
        address: ValueId(0),
    };
    test.reject("E-VERIFY-ADDRESS");
    let mut body = Body::new(0, Type::Unit);
    let p = body.parameter_value(slice(Type::I32, true));
    let one = body.constant(Constant::I32(1));
    body.emit(InstructionKind::WriteAddress {
        address: p,
        value: one,
    });
    case(vec![body.finish(None)]).reject("E-UNSUPPORTED");
    let mut body = Body::new(0, Type::I64);
    let p = body.parameter_value(address(array(Type::I32, 4), false));
    let n = body.length(p);
    case(vec![body.finish(Some(n))]).reject("E-VERIFY-ADDRESS");
}

#[test]
fn shared_window_cannot_write_or_become_exclusive() {
    let mut body = Body::new(0, Type::Unit);
    let p = body.parameter_value(slice(Type::I32, false));
    let zero = body.constant(Constant::I32(0));
    let ptr = body.index(p, zero);
    body.emit(InstructionKind::WriteAddress {
        address: ptr,
        value: zero,
    });
    case(vec![body.finish(None)]).reject("E-VERIFY-ADDRESS");
    let mut body = Body::new(0, Type::Unit);
    let p = body.parameter_value(slice(Type::I32, false));
    body.reborrow(p, true);
    case(vec![body.finish(None)]).reject("E-VERIFY-ADDRESS");
}

#[test]
fn projected_mutable_fat_pointer_is_a_view_not_a_second_owned_qualification() {
    let mut body = Body::new(0, slice(Type::I32, true));
    let p = body.parameter_value(slice(Type::I32, true));
    let view = body.range(p, None, None);
    case(vec![body.finish(Some(view))]).reject("E-VERIFY-MOVE");
    let mut body = Body::new(0, Type::Unit);
    let p = body.parameter_value(slice(Type::I32, true));
    let view = body.range(p, None, None);
    let slot = body.local(slice(Type::I32, true));
    body.emit(InstructionKind::Store {
        local: slot,
        value: view,
    });
    case(vec![body.finish(None)]).reject("E-VERIFY-MOVE");
}

#[test]
fn length_cannot_bypass_a_live_exclusive_child_but_parent_resumes_after_use() {
    for parent_first in [true, false] {
        let mut body = Body::new(0, Type::I64);
        let p = body.parameter_value(slice(Type::I32, true));
        let projected = body.range(p, None, None);
        let child = body.reborrow(projected, true);
        let (a, b) = if parent_first {
            let a = body.length(p);
            let b = body.length(child);
            (a, b)
        } else {
            let a = body.length(child);
            let b = body.length(p);
            (a, b)
        };
        let sum = body.value(
            Type::I64,
            InstructionKind::Binary {
                op: BinaryOp::Add,
                left: a,
                right: b,
            },
            vec![],
        );
        let mut test = case(vec![body.finish(Some(sum))]);
        if parent_first {
            test.reject("E-BORROW-CONFLICT");
        } else {
            test.pass();
        }
    }
}

#[test]
fn discarded_range_and_index_still_check_the_implicit_dynamic_length_read() {
    for index_projection in [false, true] {
        for parent_first in [false, true] {
            let mut body = Body::new(0, Type::I64);
            let p = body.parameter_value(slice(Type::I32, true));
            let child = body.reborrow(p, true);
            let zero = body.constant(Constant::I32(0));
            let first = if parent_first {
                None
            } else {
                Some(body.length(child))
            };
            if index_projection {
                body.index(p, zero);
            } else {
                body.range(p, None, None);
            }
            let result = first.unwrap_or_else(|| body.length(child));
            let mut test = case(vec![body.finish(Some(result))]);
            if parent_first {
                test.reject("E-BORROW-CONFLICT");
            } else {
                test.pass();
            }
        }
    }
}

#[test]
fn mutable_slice_transfer_consumes_the_actual_carrier_once() {
    for duplicate_store in [false, true] {
        let mut body = Body::new(0, Type::Unit);
        let p = body.parameter_value(slice(Type::I32, true));
        let child = body.reborrow(p, true);
        let first = body.local(slice(Type::I32, true));
        body.emit(InstructionKind::Store {
            local: first,
            value: child,
        });
        if duplicate_store {
            let second = body.local(slice(Type::I32, true));
            body.emit(InstructionKind::Store {
                local: second,
                value: child,
            });
        } else {
            body.length(child);
        }
        case(vec![body.finish(None)]).reject("E-VERIFY-MOVE");
    }
}

#[test]
fn branch_joins_preserve_all_slice_return_origins_and_mutable_carrier_state() {
    for mutable in [false, true] {
        let ty = slice(Type::I32, mutable);
        let mut body = Body::new(0, ty.clone());
        let left = body.parameter_value(ty.clone());
        let right = body.parameter_value(ty.clone());
        let choose = body.constant(Constant::Bool(true));
        let selected = body.local(ty.clone());
        let a = body.reborrow(left, mutable);
        body.emit(InstructionKind::Store {
            local: selected,
            value: a,
        });
        let b = body.reborrow(right, mutable);
        body.emit(InstructionKind::Store {
            local: selected,
            value: b,
        });
        let loaded = body.value(
            ty.clone(),
            if mutable {
                InstructionKind::TakeLocal { local: selected }
            } else {
                InstructionKind::Load(selected)
            },
            vec![HoldRoot::Parameter(0), HoldRoot::Parameter(1)],
        );
        let view = body.range(loaded, None, None);
        let returned = if mutable {
            body.reborrow(view, true)
        } else {
            view
        };
        let (mut f, mut plan) = body.finish(Some(returned));
        let ops = std::mem::take(&mut f.blocks[0].instructions);
        f.blocks = vec![
            BasicBlock {
                id: BlockId(0),
                instructions: ops[..3].to_vec(),
                terminator: Terminator::Branch {
                    condition: choose,
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: ops[3..5].to_vec(),
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: ops[5..7].to_vec(),
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: ops[7..].to_vec(),
                terminator: Terminator::Return(Some(returned)),
            },
        ];
        let params = vec![LocalId(0), LocalId(1)];
        let all = vec![LocalId(0), LocalId(1), selected];
        plan.blocks = vec![
            BlockPlan {
                block: BlockId(0),
                initialized_in: params.clone(),
                initialized_out: params.clone(),
            },
            BlockPlan {
                block: BlockId(1),
                initialized_in: params.clone(),
                initialized_out: all.clone(),
            },
            BlockPlan {
                block: BlockId(2),
                initialized_in: params.clone(),
                initialized_out: all.clone(),
            },
            BlockPlan {
                block: BlockId(3),
                initialized_in: all.clone(),
                initialized_out: if mutable { params } else { all },
            },
        ];
        let mut test = case(vec![(f, plan)]);
        test.pass();
        let verified = verify_module(
            test.module.clone(),
            test.plan.clone(),
            &test.module.target,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(verified.result_origins(FunctionId(0)), Some(vec![0, 1]));
        test.plan.functions[0].holds[loaded.0 as usize].roots.pop();
        test.reject("E-VERIFY-HOLD");
    }
}

#[test]
fn apparent_disjoint_ranges_still_use_the_whole_backing_origin() {
    let mut body = Body::new(0, Type::I64);
    let p = body.parameter_value(slice(Type::I32, true));
    let zero = body.constant(Constant::I32(0));
    let one = body.constant(Constant::I32(1));
    let two = body.constant(Constant::I32(2));
    let first = body.range(p, Some(zero), Some(one));
    let left = body.reborrow(first, true);
    let second = body.range(p, Some(one), Some(two));
    let right = body.reborrow(second, true);
    let a = body.length(left);
    let b = body.length(right);
    let sum = body.value(
        Type::I64,
        InstructionKind::Binary {
            op: BinaryOp::Add,
            left: a,
            right: b,
        },
        vec![],
    );
    case(vec![body.finish(Some(sum))]).reject("E-BORROW-CONFLICT");
}

#[test]
fn slice_from_local_array_cannot_escape_and_late_len_keeps_storage_alive() {
    let make = |destroy: bool| {
        let mut body = Body::new(
            0,
            if destroy {
                Type::I64
            } else {
                slice(Type::I32, false)
            },
        );
        let ty = array(Type::I32, 1);
        let local = body.local(ty.clone());
        body.emit(InstructionKind::StorageLive(local));
        let zero = body.constant(Constant::I32(0));
        let arr = body.value(
            ty.clone(),
            InstructionKind::MakeAggregate { fields: vec![zero] },
            vec![],
        );
        body.emit(InstructionKind::Store { local, value: arr });
        let addr = body.value(
            address(ty, false),
            InstructionKind::BorrowLocal { local },
            vec![HoldRoot::Local(local)],
        );
        let s = body.range(addr, None, None);
        let returned = if destroy {
            body.emit(InstructionKind::StorageDead(local));
            body.length(s)
        } else {
            s
        };
        case(vec![body.finish(Some(returned))])
    };
    make(false).reject("E-BORROW-ESCAPE");
    make(true).reject("E-BORROW-CONFLICT");
}

#[test]
fn slice_profiles_reject_foreign_abi_and_ct() {
    let mut foreign = Body::new(0, Type::Unit);
    foreign.parameter(slice(Type::U8, false));
    let (mut f, mut plan) = foreign.finish(None);
    f.blocks.clear();
    f.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "consume".into(),
        noescape_parameters: vec![0],
        static_callbacks: false,
    });
    plan.blocks.clear();
    case(vec![(f, plan)]).reject("E-VERIFY-FOREIGN");
    let mut test = len_case(Type::U8, false);
    test.pass();
    let ct = CtProgram {
        module: test.module.clone(),
        plan: test.plan.clone(),
        entry: FunctionId(0),
        max_steps: 100,
        max_call_depth: 8,
        max_live_slots: 32,
    };
    assert_eq!(
        verify_ct_program(ct.clone(), &test.module.target, &Limits::default()).unwrap_err()[0].code,
        "E-CT-FORBIDDEN"
    );
    let bytes = wire::encode_ct_program(&ct, &Limits::default()).unwrap();
    assert_eq!(
        verify_ct_program_bytes(&bytes, &test.module.target, &Limits::default()).unwrap_err()[0]
            .code,
        "E-CT-FORBIDDEN"
    );
}

#[test]
fn slice_captures_and_explicit_callable_signatures_remain_closed() {
    for position in 0..3 {
        let mut body = Body::new(0, Type::Unit);
        body.f.environment = Some(ClosureId(0));
        let mut closure = ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::Fn,
        };
        match position {
            0 => closure.captures.push(slice(Type::I32, false)),
            1 => {
                closure.parameters.push(slice(Type::I32, false));
                body.parameter(slice(Type::I32, false));
            }
            _ => {
                closure.result = slice(Type::I32, false);
                body.f.return_type = closure.result.clone();
            }
        }
        let mut test = case(vec![body.finish(None)]);
        test.module.closures.push(closure);
        test.reject("E-UNSUPPORTED");
    }
}

#[test]
fn closure_body_local_slice_does_not_cross_its_callable_boundary() {
    let mut body = Body::new(0, Type::I64);
    body.f.environment = Some(ClosureId(0));
    let ty = array(Type::I32, 1);
    let local = body.local(ty.clone());
    body.emit(InstructionKind::StorageLive(local));
    let zero = body.constant(Constant::I32(0));
    let initialized = body.value(
        ty.clone(),
        InstructionKind::MakeAggregate { fields: vec![zero] },
        vec![],
    );
    body.emit(InstructionKind::Store {
        local,
        value: initialized,
    });
    let borrowed = body.value(
        address(ty, false),
        InstructionKind::BorrowLocal { local },
        vec![HoldRoot::Local(local)],
    );
    let window = body.range(borrowed, None, None);
    let n = body.length(window);
    body.emit(InstructionKind::StorageDead(local));
    let mut test = case(vec![body.finish(Some(n))]);
    test.module.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![],
        parameters: vec![],
        result: Type::I64,
        kind: CallableKind::Fn,
    });
    test.pass();
}

#[test]
fn slice_targets_require_the_exact_x64_profile_not_just_a_pointer_width() {
    for change in 0..4 {
        let mut test = len_case(Type::I32, false);
        match change {
            0 => test.module.target.pointer_width = 32,
            1 => test.module.target.endian = Endian::Big,
            2 => test.module.target.triple = "aarch64-unknown-linux-gnu".into(),
            _ => test.module.target.backend = "cranelift".into(),
        }
        test.reject("E-UNSUPPORTED");
    }
    let mut test = len_case(Type::I32, false);
    test.module.target.id = "selected-slice-profile".into();
    test.pass();
    assert_eq!(
        verify_module(
            test.module.clone(),
            test.plan.clone(),
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-TARGET"
    );
    assert_eq!(
        verify_module(
            test.module.clone(),
            test.plan.clone(),
            &TargetProfile::windows_x64_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-TARGET"
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
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    check(hi).unwrap();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match check(mid) {
            Ok(_) => hi = mid,
            Err(e) => {
                assert_eq!(e[0].category, ResultCategory::ResourceLimit);
                lo = mid + 1;
            }
        }
    }
    check(lo).unwrap();
    assert_eq!(
        check(lo - 1).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    lo
}

#[test]
fn slice_shapes_and_operations_obey_exact_work_and_depth_budgets() {
    let raw = minimum_budget(&mut range_case(true), false);
    let wire = minimum_budget(&mut range_case(true), true);
    let wide = minimum_budget(&mut len_case(array(Type::I32, 128), false), false);
    let small = minimum_budget(&mut len_case(array(Type::I32, 1), false), false);
    assert!(wide >= small + 127);
    let mut deep = Type::I32;
    for _ in 0..16 {
        deep = array(deep, 1);
    }
    let mut test = len_case(deep, false);
    test.bind();
    let limits = Limits {
        max_call_depth: 4,
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
    println!("091 work boundaries raw={raw} wire={wire} array1={small} array128={wide}");
}

#[test]
fn new_wire_endpoints_reject_truncation_versions_and_stale_identity() {
    let mut test = range_case(false);
    test.bind();
    let bytes =
        wire::encode_module_candidate(&test.module, &test.plan, &Limits::default()).unwrap();
    for end in 0..bytes.len() {
        assert!(
            verify_module_bytes(&bytes[..end], &test.module.target, &Limits::default()).is_err(),
            "prefix={end}"
        );
    }
    for version in (0..SCHEMA_VERSION).chain([SCHEMA_VERSION + 1, u32::MAX]) {
        let mut bytes = bytes.clone();
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            verify_module_bytes(&bytes, &test.module.target, &Limits::default()).unwrap_err()[0]
                .code,
            "E-WIRE-VERSION"
        );
    }
    let InstructionKind::AddressSlice { start, .. } =
        &mut test.module.functions[0].blocks[0].instructions[3].kind
    else {
        panic!()
    };
    *start = None;
    assert_eq!(
        verify_module(
            test.module.clone(),
            test.plan.clone(),
            &test.module.target,
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-PLAN-IDENTITY"
    );
}
