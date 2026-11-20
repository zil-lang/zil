//! Control flow, aggregates, calls, references and budgets. Every program
//! is admitted by the solver and the checker before it is evaluated.

mod common;

use common::*;
use zil_interp::{EvalBudget, EvalError, Resource, Value, evaluate};
use zil_ir::body::*;

fn i32c(v: i128) -> Operand {
    ic(IntTy::I32, v)
}

/// `enum Shape { Circle(i32), Rect(i32, i32) }`
fn shape(m: &mut ModuleBuilder) -> (AdtId, TyId) {
    let i32t = m.int(IntTy::I32);
    m.adt(
        "m.Shape",
        AdtKind::Enum,
        &[("Circle", &[i32t]), ("Rect", &[i32t, i32t])],
    )
}

#[test]
fn enum_construction_switch_and_field_reads() {
    // fn area(sel: bool) -> i32 {
    //   let s = if sel { Rect(3, 4) } else { Circle(5) };
    //   match s { Circle(r) => r, Rect(w, h) => w * h }
    // }
    let mut m = ModuleBuilder::new();
    let (adt, shape_ty) = shape(&mut m);
    let bool_t = m.ty(TyKind::Bool);
    let i32t = m.int(IntTy::I32);
    let empty = m.empty;
    let mut f = FnBuilder::new("area", &[bool_t], i32t);
    let s = f.local(shape_ty);
    let d = f.local(i32t);
    let out = f.local(i32t);
    let w = f.local(i32t);
    let h = f.local(i32t);
    let (b0, rect, circle, matched, circ_arm, rect_arm) = (
        f.block(),
        f.block(),
        f.block(),
        f.block(),
        f.block(),
        f.block(),
    );
    f.terminate(
        b0,
        Terminator::Switch {
            discr: cp(Local(0)),
            arms: vec![(1, rect)],
            otherwise: circle,
        },
    );
    let variant = |v| AggregateKind::Adt {
        adt,
        variant: VariantIdx(v),
        args: empty,
    };
    f.assign(
        rect,
        l(s),
        Rvalue::Aggregate {
            kind: variant(1),
            operands: vec![i32c(3), i32c(4)],
        },
    );
    f.terminate(rect, goto(matched));
    f.assign(
        circle,
        l(s),
        Rvalue::Aggregate {
            kind: variant(0),
            operands: vec![i32c(5)],
        },
    );
    f.terminate(circle, goto(matched));
    f.assign(matched, l(d), Rvalue::Discriminant(l(s)));
    f.terminate(
        matched,
        Terminator::Switch {
            discr: cp(d),
            arms: vec![(0, circ_arm)],
            otherwise: rect_arm,
        },
    );
    f.assign(
        circ_arm,
        l(out),
        Rvalue::Use(Operand::Copy(l(s).field(m.field(adt, 0, 0)))),
    );
    f.terminate(circ_arm, ret(out));
    f.assign(
        rect_arm,
        l(w),
        Rvalue::Use(Operand::Copy(l(s).field(m.field(adt, 1, 0)))),
    );
    f.assign(
        rect_arm,
        l(h),
        Rvalue::Use(Operand::Copy(l(s).field(m.field(adt, 1, 1)))),
    );
    f.assign(
        rect_arm,
        l(out),
        Rvalue::Binary {
            op: BinOp::Mul,
            lhs: cp(w),
            rhs: cp(h),
        },
    );
    f.terminate(rect_arm, ret(out));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![Value::Bool(true)]),
        Ok(iv(IntTy::I32, 12))
    );
    assert_eq!(
        eval(&vm, id, vec![Value::Bool(false)]),
        Ok(iv(IntTy::I32, 5))
    );
}

#[test]
fn an_enum_value_is_a_structured_result() {
    let mut m = ModuleBuilder::new();
    let (adt, shape_ty) = shape(&mut m);
    let empty = m.empty;
    let mut f = FnBuilder::new("mk", &[], shape_ty);
    let s = f.local(shape_ty);
    let b = f.block();
    f.assign(
        b,
        l(s),
        Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt,
                variant: VariantIdx(1),
                args: empty,
            },
            operands: vec![i32c(3), i32c(-4)],
        },
    );
    f.terminate(b, Terminator::Return { value: mv(s) });
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![]),
        Ok(Value::Aggregate {
            ty: shape_ty,
            variant: Some(1),
            fields: vec![iv(IntTy::I32, 3), iv(IntTy::I32, -4)],
        })
    );
    // An enum argument is accepted and read back.
    let arg = Value::Aggregate {
        ty: shape_ty,
        variant: Some(0),
        fields: vec![iv(IntTy::I32, 9)],
    };
    let mut m = ModuleBuilder::new();
    let (adt, shape_ty2) = shape(&mut m);
    assert_eq!(shape_ty, shape_ty2);
    let i32t = m.int(IntTy::I32);
    let mut f = FnBuilder::new("radius_or_zero", &[shape_ty], i32t);
    let d = f.local(i32t);
    let out = f.local(i32t);
    let (b0, circ, other) = (f.block(), f.block(), f.block());
    f.assign(b0, l(d), Rvalue::Discriminant(l(Local(0))));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: cp(d),
            arms: vec![(0, circ)],
            otherwise: other,
        },
    );
    f.assign(
        circ,
        l(out),
        Rvalue::Use(Operand::Copy(l(Local(0)).field(m.field(adt, 0, 0)))),
    );
    f.terminate(circ, ret(out));
    f.assign(other, l(out), Rvalue::Use(i32c(0)));
    f.terminate(other, ret(out));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(eval(&vm, id, vec![arg]), Ok(iv(IntTy::I32, 9)));
}

/// `fn sum(n: u32) -> u32 { let i = 0; let acc = 0; while i < n { i += 1; acc += i } acc }`
fn sum_module() -> (zil_check::VerifiedModule, FnId) {
    let mut m = ModuleBuilder::new();
    let u32t = m.int(IntTy::U32);
    let bool_t = m.ty(TyKind::Bool);
    let mut f = FnBuilder::new("sum", &[u32t], u32t);
    let i = f.local(u32t);
    let acc = f.local(u32t);
    let c = f.local(bool_t);
    let (b0, head, body, exit) = (f.block(), f.block(), f.block(), f.block());
    f.assign(b0, l(i), Rvalue::Use(ic(IntTy::U32, 0)));
    f.assign(b0, l(acc), Rvalue::Use(ic(IntTy::U32, 0)));
    f.terminate(b0, goto(head));
    f.assign(
        head,
        l(c),
        Rvalue::Binary {
            op: BinOp::Lt,
            lhs: cp(i),
            rhs: cp(Local(0)),
        },
    );
    f.terminate(
        head,
        Terminator::Switch {
            discr: cp(c),
            arms: vec![(1, body)],
            otherwise: exit,
        },
    );
    f.assign(
        body,
        l(i),
        Rvalue::Binary {
            op: BinOp::Add,
            lhs: cp(i),
            rhs: ic(IntTy::U32, 1),
        },
    );
    f.assign(
        body,
        l(acc),
        Rvalue::Binary {
            op: BinOp::Add,
            lhs: cp(acc),
            rhs: cp(i),
        },
    );
    f.terminate(body, goto(head));
    f.terminate(exit, ret(acc));
    let id = m.function(f);
    (m.verify(), id)
}

#[test]
fn a_loop_runs_to_completion() {
    let (vm, id) = sum_module();
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::U32, 100)]),
        Ok(iv(IntTy::U32, 5050))
    );
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::U32, 0)]),
        Ok(iv(IntTy::U32, 0))
    );
    // 1 + ... + 92681 = 92681 * 92682 / 2 = 4294930221 fits in u32;
    // adding 92682 gives 4295022903 > 4294967295.
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::U32, 92681)]),
        Ok(iv(IntTy::U32, 4_294_930_221))
    );
    assert!(is_trap(
        &eval(&vm, id, vec![iv(IntTy::U32, 92682)]),
        TrapKind::Overflow
    ));
}

#[test]
fn the_step_budget_is_exact_and_exhaustion_is_a_resource_limit() {
    let (vm, id) = sum_module();
    // Entry: 2 assignments + goto = 3. Each iteration: head (1 + switch)
    // + body (2 + goto) = 5. Exit: head (1 + switch) + return = 3.
    let steps = 3 + 100 * 5 + 3;
    let budget = |steps| EvalBudget {
        steps,
        call_depth: 1,
    };
    assert_eq!(
        evaluate(&vm, id, vec![iv(IntTy::U32, 100)], budget(steps)),
        Ok(iv(IntTy::U32, 5050))
    );
    assert_eq!(
        evaluate(&vm, id, vec![iv(IntTy::U32, 100)], budget(steps - 1)),
        Err(EvalError::ResourceLimit {
            resource: Resource::Steps
        })
    );
}

#[test]
fn an_infinite_loop_exhausts_the_budget() {
    let mut m = ModuleBuilder::new();
    let unit = m.ty(TyKind::Unit);
    let mut f = FnBuilder::new("spin", &[], unit);
    let b = f.block();
    f.terminate(b, goto(b));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        evaluate(
            &vm,
            id,
            vec![],
            EvalBudget {
                steps: 10_000,
                call_depth: 4
            }
        ),
        Err(EvalError::ResourceLimit {
            resource: Resource::Steps
        })
    );
}

/// `fn fact(n: u64) -> u64 { if n == 0 { 1 } else { n * fact(n - 1) } }`
fn fact_module() -> (zil_check::VerifiedModule, FnId) {
    let mut m = ModuleBuilder::new();
    let u64t = m.int(IntTy::U64);
    let bool_t = m.ty(TyKind::Bool);
    let mut f = FnBuilder::new("fact", &[u64t], u64t);
    let c = f.local(bool_t);
    let k = f.local(u64t);
    let r = f.local(u64t);
    let out = f.local(u64t);
    let (b0, base, rec, after) = (f.block(), f.block(), f.block(), f.block());
    f.assign(
        b0,
        l(c),
        Rvalue::Binary {
            op: BinOp::Eq,
            lhs: cp(Local(0)),
            rhs: ic(IntTy::U64, 0),
        },
    );
    f.terminate(
        b0,
        Terminator::Switch {
            discr: cp(c),
            arms: vec![(1, base)],
            otherwise: rec,
        },
    );
    f.assign(base, l(out), Rvalue::Use(ic(IntTy::U64, 1)));
    f.terminate(base, ret(out));
    f.assign(
        rec,
        l(k),
        Rvalue::Binary {
            op: BinOp::Sub,
            lhs: cp(Local(0)),
            rhs: ic(IntTy::U64, 1),
        },
    );
    f.terminate(
        rec,
        Terminator::Call {
            callee: Callee::Direct(FnId(0)),
            args: vec![cp(k)],
            dest: l(r),
            target: after,
            cancel: None,
        },
    );
    f.assign(
        after,
        l(out),
        Rvalue::Binary {
            op: BinOp::Mul,
            lhs: cp(Local(0)),
            rhs: cp(r),
        },
    );
    f.terminate(after, ret(out));
    let id = m.function(f);
    (m.verify(), id)
}

#[test]
fn recursion_and_the_call_depth_limit() {
    let (vm, id) = fact_module();
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::U64, 20)]),
        Ok(iv(IntTy::U64, 2_432_902_008_176_640_000))
    );
    // 21! = 51090942171709440000 > u64::MAX.
    assert!(is_trap(
        &eval(&vm, id, vec![iv(IntTy::U64, 21)]),
        TrapKind::Overflow
    ));
    // fact(20) needs 21 simultaneous frames.
    let depth = |call_depth| EvalBudget {
        steps: 1_000_000,
        call_depth,
    };
    assert_eq!(
        evaluate(&vm, id, vec![iv(IntTy::U64, 20)], depth(21)),
        Ok(iv(IntTy::U64, 2_432_902_008_176_640_000))
    );
    assert_eq!(
        evaluate(&vm, id, vec![iv(IntTy::U64, 20)], depth(20)),
        Err(EvalError::ResourceLimit {
            resource: Resource::CallDepth
        })
    );
    // Deep recursion is bounded by the budget, not by the host stack.
    assert_eq!(
        evaluate(&vm, id, vec![iv(IntTy::U64, 1_000_000)], depth(100_000)),
        Err(EvalError::ResourceLimit {
            resource: Resource::CallDepth
        })
    );
}

#[test]
fn an_indirect_call_through_a_function_value() {
    // fn double(x: i32) -> i32 { x + x }
    // fn f(x: i32) -> i32 { let g = double; g(x) }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let fn_t = m.ty(TyKind::FnPtr {
        params: vec![i32t],
        result: i32t,
    });
    let mut d = FnBuilder::new("double", &[i32t], i32t);
    let o = d.local(i32t);
    let b = d.block();
    d.assign(
        b,
        l(o),
        Rvalue::Binary {
            op: BinOp::Add,
            lhs: cp(Local(0)),
            rhs: cp(Local(0)),
        },
    );
    d.terminate(b, ret(o));
    let double = m.function(d);
    let mut f = FnBuilder::new("f", &[i32t], i32t);
    let g = f.local(fn_t);
    let out = f.local(i32t);
    let (b0, b1) = (f.block(), f.block());
    f.assign(b0, l(g), Rvalue::FnRef(double));
    f.terminate(
        b0,
        Terminator::Call {
            callee: Callee::Indirect(cp(g)),
            args: vec![cp(Local(0))],
            dest: l(out),
            target: b1,
            cancel: None,
        },
    );
    f.terminate(b1, ret(out));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::I32, 21)]),
        Ok(iv(IntTy::I32, 42))
    );
    assert!(is_trap(
        &eval(&vm, id, vec![iv(IntTy::I32, 1 << 30)]),
        TrapKind::Overflow
    ));
    // A function value is a result too.
    assert_eq!(
        eval(&vm, double, vec![iv(IntTy::I32, -3)]),
        Ok(iv(IntTy::I32, -6))
    );
}

#[test]
fn writes_through_an_exclusive_address_reach_the_caller() {
    // fn inc(p: &mut i32) { *p = *p + 1 }
    // fn f() -> i32 { let x = 41; inc(&mut x); x }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let unit = m.ty(TyKind::Unit);
    let ref_t = m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: i32t,
    });
    let mut inc = FnBuilder::new("inc", &[ref_t], unit);
    let t = inc.local(i32t);
    let b = inc.block();
    inc.assign(
        b,
        l(t),
        Rvalue::Binary {
            op: BinOp::Add,
            lhs: Operand::Copy(l(Local(0)).deref()),
            rhs: i32c(1),
        },
    );
    inc.assign(b, l(Local(0)).deref(), Rvalue::Use(cp(t)));
    inc.terminate(
        b,
        Terminator::Return {
            value: Operand::Const(ConstValue::Unit),
        },
    );
    let inc = m.function(inc);
    let mut f = FnBuilder::new("f", &[], i32t);
    let x = f.local(i32t);
    let r = f.local(ref_t);
    let u = f.local(unit);
    let (b0, b1) = (f.block(), f.block());
    f.assign(b0, l(x), Rvalue::Use(i32c(41)));
    f.assign(
        b0,
        l(r),
        Rvalue::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            place: l(x),
        },
    );
    f.terminate(
        b0,
        Terminator::Call {
            callee: Callee::Direct(inc),
            args: vec![mv(r)],
            dest: l(u),
            target: b1,
            cancel: None,
        },
    );
    f.terminate(b1, ret(x));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(eval(&vm, id, vec![]), Ok(iv(IntTy::I32, 42)));
    // An address parameter cannot be supplied from outside.
    assert!(matches!(
        eval(&vm, inc, vec![iv(IntTy::I32, 0)]),
        Err(EvalError::InvalidArgument { .. })
    ));
}

#[test]
fn array_indexing_is_bounds_checked() {
    // fn at(i: usize) -> i32 { let a = [10, 20, 30]; a[i] }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let usize_t = m.int(IntTy::Usize);
    let arr = m.ty(TyKind::Array {
        element: i32t,
        length: 3,
    });
    let mut f = FnBuilder::new("at", &[usize_t], i32t);
    let a = f.local(arr);
    let n = f.local(usize_t);
    let out = f.local(i32t);
    let b = f.block();
    f.assign(
        b,
        l(a),
        Rvalue::Aggregate {
            kind: AggregateKind::Array { element: i32t },
            operands: vec![i32c(10), i32c(20), i32c(30)],
        },
    );
    f.assign(b, l(n), Rvalue::Len(l(a)));
    f.assign(
        b,
        l(out),
        Rvalue::Use(Operand::Copy(l(a).project(Projection::Index(Local(0))))),
    );
    f.terminate(b, ret(out));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::Usize, 2)]),
        Ok(iv(IntTy::I32, 30))
    );
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::Usize, 0)]),
        Ok(iv(IntTy::I32, 10))
    );
    assert!(is_trap(
        &eval(&vm, id, vec![iv(IntTy::Usize, 3)]),
        TrapKind::Bounds
    ));
}

#[test]
fn tuples_are_built_and_projected() {
    // fn swap(a: i8, b: u16) -> (u16, i8) { let t = (b, a); t }
    let mut m = ModuleBuilder::new();
    let i8t = m.int(IntTy::I8);
    let u16t = m.int(IntTy::U16);
    let tup = m.ty(TyKind::Tuple(vec![u16t, i8t]));
    let mut f = FnBuilder::new("swap", &[i8t, u16t], tup);
    let t = f.local(tup);
    let b = f.block();
    f.assign(
        b,
        l(t),
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple,
            operands: vec![cp(Local(1)), cp(Local(0))],
        },
    );
    f.terminate(b, Terminator::Return { value: mv(t) });
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::I8, -1), iv(IntTy::U16, 65535)]),
        Ok(Value::Aggregate {
            ty: tup,
            variant: None,
            fields: vec![iv(IntTy::U16, 65535), iv(IntTy::I8, -1)],
        })
    );
}

#[test]
fn error_unions_record_their_arm() {
    // fn f(ok: bool) -> bool { let e = if ok { Ok(7) } else { Err(1u8) }; arm(e) }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let u8t = m.int(IntTy::U8);
    let bool_t = m.ty(TyKind::Bool);
    let eu = m.ty(TyKind::ErrorUnion { ok: i32t, err: u8t });
    let mut f = FnBuilder::new("f", &[bool_t], eu);
    let e = f.local(eu);
    let (b0, okb, errb, done) = (f.block(), f.block(), f.block(), f.block());
    f.terminate(
        b0,
        Terminator::Switch {
            discr: cp(Local(0)),
            arms: vec![(1, okb)],
            otherwise: errb,
        },
    );
    f.assign(
        okb,
        l(e),
        Rvalue::ErrorUnion {
            ok: true,
            value: i32c(7),
            ty: eu,
        },
    );
    f.terminate(okb, goto(done));
    f.assign(
        errb,
        l(e),
        Rvalue::ErrorUnion {
            ok: false,
            value: ic(IntTy::U8, 1),
            ty: eu,
        },
    );
    f.terminate(errb, goto(done));
    f.terminate(done, Terminator::Return { value: mv(e) });
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![Value::Bool(true)]),
        Ok(Value::ErrorUnion {
            ty: eu,
            ok: true,
            payload: Box::new(iv(IntTy::I32, 7)),
        })
    );
    assert_eq!(
        eval(&vm, id, vec![Value::Bool(false)]),
        Ok(Value::ErrorUnion {
            ty: eu,
            ok: false,
            payload: Box::new(iv(IntTy::U8, 1)),
        })
    );
}

#[test]
fn assert_and_trap_terminators_trap_with_their_kind() {
    // fn f(c: bool) -> i32 { assert(c, Bounds); 1 }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let bool_t = m.ty(TyKind::Bool);
    let mut f = FnBuilder::new("f", &[bool_t], i32t);
    let out = f.local(i32t);
    let (b0, b1) = (f.block(), f.block());
    f.terminate(
        b0,
        Terminator::Assert {
            cond: cp(Local(0)),
            expected: true,
            target: b1,
            trap: TrapKind::Bounds,
        },
    );
    f.assign(b1, l(out), Rvalue::Use(i32c(1)));
    f.terminate(b1, ret(out));
    let assert_fn = m.function(f);
    let mut g = FnBuilder::new("g", &[], i32t);
    let b = g.block();
    g.terminate(b, Terminator::Trap(TrapKind::Explicit));
    let trap_fn = m.function(g);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, assert_fn, vec![Value::Bool(true)]),
        Ok(iv(IntTy::I32, 1))
    );
    assert_eq!(
        eval(&vm, assert_fn, vec![Value::Bool(false)]),
        Err(EvalError::Trap {
            kind: TrapKind::Bounds,
            function: assert_fn,
            block: BlockId(0),
        })
    );
    assert_eq!(
        eval(&vm, trap_fn, vec![]),
        Err(EvalError::Trap {
            kind: TrapKind::Explicit,
            function: trap_fn,
            block: BlockId(0),
        })
    );
}

#[test]
fn a_trap_reports_the_function_and_block_where_it_happened() {
    let (vm, id) = sum_module();
    // The overflowing addition is in block 2 (the loop body).
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::U32, 92683)]),
        Err(EvalError::Trap {
            kind: TrapKind::Overflow,
            function: id,
            block: BlockId(2),
        })
    );
}

#[test]
fn arguments_must_match_the_signature() {
    let (vm, id) = sum_module();
    assert!(matches!(
        eval(&vm, id, vec![]),
        Err(EvalError::InvalidArgument { .. })
    ));
    assert!(matches!(
        eval(&vm, id, vec![iv(IntTy::I32, 1)]),
        Err(EvalError::InvalidArgument { .. })
    ));
    // Bits outside the width are not a u32.
    assert!(matches!(
        eval(
            &vm,
            id,
            vec![Value::Int {
                ty: IntTy::U32,
                bits: 1 << 32
            }]
        ),
        Err(EvalError::InvalidArgument { .. })
    ));
    assert!(matches!(
        eval(&vm, FnId(7), vec![]),
        Err(EvalError::InvalidArgument { .. })
    ));
}

#[test]
fn scalar_results_convert_to_constants() {
    assert_eq!(
        iv(IntTy::I8, -1).to_const(),
        Some(ConstValue::Int {
            ty: IntTy::I8,
            bits: 0xff
        })
    );
    assert_eq!(Value::Unit.to_const(), Some(ConstValue::Unit));
    assert_eq!(Value::FnRef(FnId(0)).to_const(), None);
}
