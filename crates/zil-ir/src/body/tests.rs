//! Schema tests. Expectations are written by hand from the contract text,
//! not computed by any consumer of the schema.

use super::*;
use zil_base::Span;

struct Fixture {
    tys: TyTable,
    i32: TyId,
    usize: TyId,
    unit: TyId,
    pair: AdtId,
    pair_ty: TyId,
    pair_args: ArgsId,
    res: AdtId,
    managed_res: TyId,
    pkg: AdtId,
    pkg_ty: TyId,
}

fn fixture() -> Fixture {
    let mut tys = TyTable::new();
    let i32 = tys.intern(TyKind::Int(IntTy::I32));
    let usize = tys.intern(TyKind::Int(IntTy::Usize));
    let unit = tys.intern(TyKind::Unit);
    tys.intern(TyKind::Bool);
    let empty = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![],
    });
    let pair = tys.add_adt(AdtDef {
        name: "m.Pair".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Pair".into(),
            fields: vec![
                FieldDef {
                    name: "a".into(),
                    ty: i32,
                },
                FieldDef {
                    name: "b".into(),
                    ty: i32,
                },
            ],
        }],
        user_drop: false,
    });
    let pair_ty = tys.intern(TyKind::Adt {
        adt: pair,
        args: empty,
    });
    let res = tys.add_adt(AdtDef {
        name: "m.File".into(),
        kind: AdtKind::Resource,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "File".into(),
            fields: vec![FieldDef {
                name: "fd".into(),
                ty: i32,
            }],
        }],
        user_drop: false,
    });
    let res_ty = tys.intern(TyKind::Adt {
        adt: res,
        args: empty,
    });
    let managed_res = tys.intern(TyKind::Managed(res_ty));
    tys.intern(TyKind::Desc(res_ty));
    let pkg = tys.add_adt(AdtDef {
        name: "m.Owner".into(),
        kind: AdtKind::Package,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Owner".into(),
            fields: vec![
                FieldDef {
                    name: "file".into(),
                    ty: managed_res,
                },
                FieldDef {
                    name: "count".into(),
                    ty: i32,
                },
            ],
        }],
        user_drop: false,
    });
    let pkg_ty = tys.intern(TyKind::Adt {
        adt: pkg,
        args: empty,
    });
    Fixture {
        tys,
        i32,
        usize,
        unit,
        pair,
        pair_ty,
        pair_args: empty,
        res,
        managed_res,
        pkg,
        pkg_ty,
    }
}

fn field(adt: AdtId, args: ArgsId, index: u32) -> FieldRef {
    FieldRef {
        adt,
        variant: VariantIdx(0),
        field: FieldIdx(index),
        args,
    }
}

fn body(locals: &[TyId], params: u32, result: TyId, blocks: Vec<Block>) -> Body {
    Body {
        param_count: params,
        locals: locals
            .iter()
            .map(|t| LocalDecl { ty: *t, name: None })
            .collect(),
        captures: vec![],
        closure: None,
        result,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks,
    }
}

fn block(instructions: Vec<InstructionKind>, terminator: Terminator) -> Block {
    Block {
        instructions: instructions
            .into_iter()
            .map(|kind| Instruction {
                kind,
                span: Span::default(),
            })
            .collect(),
        terminator,
        terminator_span: Span::default(),
    }
}

#[test]
fn field_identity_is_the_declaration_not_the_name() {
    let f = fixture();
    let a = field(f.pair, f.pair_args, 0);
    let b = field(f.pair, f.pair_args, 1);
    let p = Place::local(Local(0));
    assert_eq!(
        p.clone().field(a).relation(&p.clone().field(b)),
        PlaceRelation::Disjoint
    );
    assert_eq!(
        p.clone().field(a).relation(&p.clone().field(a)),
        PlaceRelation::Equal
    );
    assert_eq!(p.relation(&p.clone().field(a)), PlaceRelation::Contains);
    // A field of a different declaration never names this instance's field.
    let wrong = field(f.pkg, f.pair_args, 1);
    let b0 = body(&[f.pair_ty], 1, f.unit, vec![]);
    let err = type_place(&Place::local(Local(0)).field(wrong), &b0.cx(&f.tys)).unwrap_err();
    assert!(matches!(err, PlaceTyError::FieldInstance { step: 0, .. }));
}

#[test]
fn place_typing_follows_declarations_and_indirections() {
    let f = fixture();
    let b0 = body(&[f.pkg_ty, f.managed_res, f.usize], 1, f.unit, vec![]);
    let cx = b0.cx(&f.tys);
    let t = type_place(
        &Place::local(Local(0)).field(field(f.pkg, f.pair_args, 0)),
        &cx,
    )
    .unwrap();
    assert_eq!(t.ty(), f.managed_res);
    let through = type_place(
        &Place::local(Local(1))
            .deref()
            .field(field(f.res, f.pair_args, 0)),
        &cx,
    )
    .unwrap();
    assert_eq!(through.ty(), f.i32);
    assert_eq!(through.derefs, vec![DerefBase::Managed]);
    // Deref of a non-indirection is a mismatch, not an empty step.
    let err = type_place(&Place::local(Local(2)).deref(), &cx).unwrap_err();
    assert!(matches!(err, PlaceTyError::Mismatch { step: 0, .. }));
}

#[test]
fn distinct_roots_alias_only_through_indirection() {
    let a = Place::local(Local(0));
    let b = Place::local(Local(1));
    assert_eq!(a.relation(&b), PlaceRelation::Disjoint);
    assert_eq!(a.relation(&b.clone().deref()), PlaceRelation::MayOverlap);
    let i = Place::local(Local(0)).project(Projection::Index(Local(2)));
    assert_eq!(i.relation(&i), PlaceRelation::MayOverlap);
    let c0 = Place::local(Local(0)).project(Projection::ConstIndex(0));
    let c1 = Place::local(Local(0)).project(Projection::ConstIndex(1));
    assert_eq!(c0.relation(&c1), PlaceRelation::Disjoint);
}

#[test]
fn accesses_follow_evaluation_order_and_include_index_reads() {
    let f = fixture();
    let fa = field(f.pair, f.pair_args, 0);
    let dest = Place::local(Local(3)).project(Projection::Index(Local(4)));
    let inst = InstructionKind::Assign {
        dest: dest.clone(),
        value: Rvalue::Binary {
            op: BinOp::Add,
            lhs: Operand::Copy(Place::local(Local(0)).field(fa)),
            rhs: Operand::Move(Place::local(Local(1))),
        },
    };
    let got: Vec<(PlaceAccess, OperandRole)> =
        inst.accesses().iter().map(|a| (a.access, a.role)).collect();
    assert_eq!(
        got,
        vec![
            (PlaceAccess::Copy, OperandRole::Lhs),
            (PlaceAccess::Move, OperandRole::Rhs),
            (PlaceAccess::Copy, OperandRole::Index),
            (PlaceAccess::Init, OperandRole::Target),
        ]
    );
    let e = inst.effects();
    for effect in [
        Effect::Read,
        Effect::Move,
        Effect::Write,
        Effect::Init,
        Effect::MayTrap,
    ] {
        assert!(e.contains(effect), "{effect:?}");
    }
    assert!(!e.contains(Effect::Borrow));
}

#[test]
fn call_destination_is_initialized_on_the_return_edge_only() {
    let term = Terminator::Call {
        callee: Callee::Direct(FnId(0)),
        args: vec![Operand::Move(Place::local(Local(0)))],
        dest: Place::local(Local(1)),
        target: BlockId(1),
        cancel: Some(BlockId(2)),
    };
    assert_eq!(
        term.edges(),
        vec![
            (EdgeKind::CallReturn, BlockId(1)),
            (EdgeKind::CallCancel, BlockId(2))
        ]
    );
    assert_eq!(term.edge_accesses(EdgeKind::CallReturn).len(), 1);
    assert!(term.edge_accesses(EdgeKind::CallCancel).is_empty());
    assert!(term.effects().contains(Effect::Suspend));
}

#[test]
fn constraints_reject_copy_of_affine_and_resource_aggregates() {
    let f = fixture();
    let b0 = body(
        &[f.pkg_ty, f.pkg_ty],
        1,
        f.unit,
        vec![block(vec![], Terminator::Unreachable)],
    );
    let cx = ConstraintCx {
        body: b0.cx(&f.tys),
        sigs: Sigs {
            functions: &[],
            foreigns: &[],
            static_count: 0,
            return_summaries: None,
        },
    };
    let copy_pkg = InstructionKind::Assign {
        dest: Place::local(Local(1)),
        value: Rvalue::Use(Operand::Copy(Place::local(Local(0)))),
    };
    assert!(matches!(
        check_instruction(&copy_pkg, &cx),
        Err(ConstraintError::OperandClass {
            class: ValueClass::Affine,
            ..
        })
    ));
    let move_pkg = InstructionKind::Assign {
        dest: Place::local(Local(1)),
        value: Rvalue::Use(Operand::Move(Place::local(Local(0)))),
    };
    assert_eq!(check_instruction(&move_pkg, &cx), Ok(()));
    let forged = InstructionKind::Assign {
        dest: Place::local(Local(1)),
        value: Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt: f.res,
                variant: VariantIdx(0),
                args: f.pair_args,
            },
            operands: vec![Operand::Const(ConstValue::Int {
                ty: IntTy::I32,
                bits: 3,
            })],
        },
    };
    assert!(matches!(
        check_instruction(&forged, &cx),
        Err(ConstraintError::Aggregate { .. })
    ));
}

#[test]
fn value_classes_follow_the_category_table() {
    let f = fixture();
    let env = ParamEnv::default();
    assert_eq!(f.tys.class(f.pair_ty, &env), ValueClass::Copy);
    assert_eq!(f.tys.class(f.pkg_ty, &env), ValueClass::Affine);
    assert_eq!(f.tys.class(f.managed_res, &env), ValueClass::Affine);
    let res_ty = f
        .tys
        .lookup(&TyKind::Adt {
            adt: f.res,
            args: f.pair_args,
        })
        .unwrap();
    assert_eq!(f.tys.class(res_ty, &env), ValueClass::NotValue);
    let desc = f.tys.lookup(&TyKind::Desc(res_ty)).unwrap();
    assert_eq!(f.tys.class(desc, &env), ValueClass::Copy);
    assert!(f.tys.needs_cleanup(f.pkg_ty, &env));
    assert!(!f.tys.needs_cleanup(desc, &env));
}

#[test]
fn generic_field_types_substitute_through_nested_arguments() {
    let mut tys = TyTable::new();
    let i32 = tys.intern(TyKind::Int(IntTy::I32));
    let p0 = tys.intern(TyKind::Param(0));
    let boxed = tys.add_adt(AdtDef {
        name: "m.Cell".into(),
        kind: AdtKind::Record,
        type_params: 1,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Cell".into(),
            fields: vec![FieldDef {
                name: "v".into(),
                ty: p0,
            }],
        }],
        user_drop: false,
    });
    let inner_args = tys.intern_args(GenericArgs {
        tys: vec![p0],
        regions: vec![],
    });
    let cell_p = tys.intern(TyKind::Adt {
        adt: boxed,
        args: inner_args,
    });
    let outer = tys.add_adt(AdtDef {
        name: "m.Wrap".into(),
        kind: AdtKind::Record,
        type_params: 1,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Wrap".into(),
            fields: vec![FieldDef {
                name: "c".into(),
                ty: cell_p,
            }],
        }],
        user_drop: false,
    });
    let args = tys.intern_args(GenericArgs {
        tys: vec![i32],
        regions: vec![],
    });
    // Not interned yet: lookup must fail closed.
    assert_eq!(tys.field_ty(outer, VariantIdx(0), FieldIdx(0), args), None);
    let produced = tys.subst(cell_p, args);
    assert_eq!(
        tys.field_ty(outer, VariantIdx(0), FieldIdx(0), args),
        Some(produced)
    );
    assert_eq!(
        tys.field_ty(boxed, VariantIdx(0), FieldIdx(0), args),
        Some(i32)
    );
    assert_eq!(*tys.kind(produced), TyKind::Adt { adt: boxed, args });
}
