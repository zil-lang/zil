//! Storage effects, replacement, drops and closures.

mod common;

use common::*;
use zil_interp::{EvalError, Value};
use zil_ir::body::*;

#[test]
fn field_wise_initialization_and_storage_markers() {
    // fn f(a: i32) -> (i32, i32) {
    //   scope { live t; t = (0, 0); t.1 = a; t.0 = 2; let u = t; dead t } u
    // }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let tup = m.ty(TyKind::Tuple(vec![i32t, i32t]));
    let mut f = FnBuilder::new("f", &[i32t], tup);
    let t = f.local(tup);
    let u = f.local(tup);
    let b = f.block();
    f.push(b, InstructionKind::ScopeEnter(ScopeId(0)));
    f.push(b, InstructionKind::StorageLive(t));
    f.assign(
        b,
        l(t),
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple,
            operands: vec![ic(IntTy::I32, 0), ic(IntTy::I32, 0)],
        },
    );
    f.assign(
        b,
        l(t).project(Projection::TupleField(1)),
        Rvalue::Use(cp(Local(0))),
    );
    f.assign(
        b,
        l(t).project(Projection::TupleField(0)),
        Rvalue::Use(ic(IntTy::I32, 2)),
    );
    f.assign(b, l(u), Rvalue::Use(cp(t)));
    f.push(b, InstructionKind::StorageDead(t));
    f.push(
        b,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            exit: ScopeExit::Fallthrough,
        },
    );
    f.terminate(b, Terminator::Return { value: mv(u) });
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(
        eval(&vm, id, vec![iv(IntTy::I32, -9)]),
        Ok(Value::Aggregate {
            ty: tup,
            variant: None,
            fields: vec![iv(IntTy::I32, 2), iv(IntTy::I32, -9)],
        })
    );
}

#[test]
fn replace_and_drop_of_an_owning_package() {
    // package P { a: (i32,) }  — affine by declaration.
    // fn f() -> i32 { let p = P((1,)); replace p with P((2,)) -> old; drop old; p.a.0 }
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let inner = m.ty(TyKind::Tuple(vec![i32t]));
    let (adt, p_ty) = m.adt("m.P", AdtKind::Package, &[("P", &[inner])]);
    let empty = m.empty;
    let mut f = FnBuilder::new("f", &[], i32t);
    let p = f.local(p_ty);
    let q = f.local(p_ty);
    let old = f.local(p_ty);
    let t1 = f.local(inner);
    let t2 = f.local(inner);
    let out = f.local(i32t);
    let b = f.block();
    let mk = |v: Local| Rvalue::Aggregate {
        kind: AggregateKind::Adt {
            adt,
            variant: VariantIdx(0),
            args: empty,
        },
        operands: vec![mv(v)],
    };
    f.assign(
        b,
        l(t1),
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple,
            operands: vec![ic(IntTy::I32, 1)],
        },
    );
    f.assign(
        b,
        l(t2),
        Rvalue::Aggregate {
            kind: AggregateKind::Tuple,
            operands: vec![ic(IntTy::I32, 2)],
        },
    );
    f.assign(b, l(p), mk(t1));
    f.assign(b, l(q), mk(t2));
    f.push(
        b,
        InstructionKind::Replace {
            place: l(p),
            value: mv(q),
            old,
        },
    );
    f.push(b, InstructionKind::Drop { place: l(old) });
    f.assign(
        b,
        l(out),
        Rvalue::Use(Operand::Copy(
            l(p).field(m.field(adt, 0, 0))
                .project(Projection::TupleField(0)),
        )),
    );
    f.push(b, InstructionKind::Drop { place: l(p) });
    f.terminate(b, ret(out));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(eval(&vm, id, vec![]), Ok(iv(IntTy::I32, 2)));
}

#[test]
fn a_closure_reads_its_capture() {
    // let k = 40; let c = |x: i32| k + x; c(2)
    let mut m = ModuleBuilder::new();
    let i32t = m.int(IntTy::I32);
    let empty = m.empty;
    let def = m.tys.add_closure(ClosureDef {
        kind: ClosureKind::Fn,
        type_params: 0,
        captures: vec![i32t],
        params: vec![i32t],
        result: i32t,
    });
    let closure_ty = m.ty(TyKind::Closure { def, args: empty });
    let mut body = FnBuilder::new("closure", &[i32t], i32t);
    let o = body.local(i32t);
    let b = body.block();
    body.assign(
        b,
        l(o),
        Rvalue::Binary {
            op: BinOp::Add,
            lhs: Operand::Copy(Place::capture(0)),
            rhs: cp(Local(0)),
        },
    );
    body.terminate(b, ret(o));
    let closure_fn = m.function(body);
    m.functions[closure_fn.index()].body.closure = Some(def);
    m.functions[closure_fn.index()].body.captures = vec![i32t];
    let mut f = FnBuilder::new("f", &[], i32t);
    let k = f.local(i32t);
    let c = f.local(closure_ty);
    let r = f.local(i32t);
    let (b0, b1) = (f.block(), f.block());
    f.assign(b0, l(k), Rvalue::Use(ic(IntTy::I32, 40)));
    f.assign(
        b0,
        l(c),
        Rvalue::Aggregate {
            kind: AggregateKind::Closure { def, args: empty },
            operands: vec![cp(k)],
        },
    );
    f.terminate(
        b0,
        Terminator::Call {
            callee: Callee::Closure {
                closure: l(c),
                mode: ClosureCallMode::Shared,
            },
            args: vec![ic(IntTy::I32, 2)],
            dest: l(r),
            target: b1,
            cancel: None,
        },
    );
    f.terminate(b1, ret(r));
    let id = m.function(f);
    let vm = m.verify();
    assert_eq!(eval(&vm, id, vec![]), Ok(iv(IntTy::I32, 42)));
    // The closure body itself needs an environment; it cannot be an entry.
    assert!(matches!(
        eval(&vm, closure_fn, vec![iv(IntTy::I32, 1)]),
        Err(EvalError::Internal { .. })
    ));
}
