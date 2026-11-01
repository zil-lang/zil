//! Raw/Wire follow-ups for the ordinary data boundary, not new callback types.
use super::*;

fn data() -> Type {
    Type::Enum {
        name: "ExtraEnum".into(),
        variants: vec![("V".into(), vec![owned()]), ("Nil".into(), vec![])],
    }
}
fn wrapped() -> Type {
    Type::Package {
        name: "ExtraWrapper".into(),
        fields: vec![("state".into(), data())],
    }
}
fn union() -> Type {
    Type::ErrorUnion {
        error: Box::new(Type::Unit),
        result: Box::new(wrapped()),
    }
}
fn drop_data(ty: &Type, local: LocalId) -> InstructionKind {
    match ty {
        Type::Enum { .. } => InstructionKind::DropEnum { local },
        Type::ErrorUnion { .. } => InstructionKind::DropErrorUnion { local },
        Type::Package { .. } => InstructionKind::DropPackage { local },
        _ => unreachable!(),
    }
}
fn extra(ty: Type) -> Raw {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, false, 0, Type::I32);
    let f = &mut raw.module.functions[0];
    let local = LocalId(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        id: local,
        name: "data".into(),
        ty: ty.clone(),
        mutable: false,
        span: span(),
    });
    f.parameters.push(local);
    f.blocks[0].instructions.insert(
        0,
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    f.blocks[0].instructions.extend([
        op(None, drop_data(&ty, local)),
        op(None, InstructionKind::StorageDead(local)),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    raw
}
fn state(ty: Option<Type>) -> Raw {
    let mut raw = if let Some(ty) = ty {
        extra(ty)
    } else {
        let mut r = fixture(CallableKind::Fn, CallableKind::Fn, false, 0, Type::I32);
        r.module.functions[0].blocks[0].instructions.insert(
            0,
            op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        r.module.functions[0].blocks[0].instructions.extend([
            op(None, InstructionKind::StorageDead(LocalId(0))),
            op(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
        r
    };
    let f = &mut raw.module.functions[0];
    f.callable_contract = None;
    let result = Type::Package {
        name: "OwningState".into(),
        fields: vec![("owner".into(), owned()), ("callback".into(), env())],
    };
    f.return_type = result.clone();
    let env_value = ValueId(f.values.len() as u32);
    f.values.push(env());
    let n = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let owner = ValueId(f.values.len() as u32);
    f.values.push(owned());
    let output = ValueId(f.values.len() as u32);
    f.values.push(result);
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::StorageDead(_)
                    | InstructionKind::DropEnum { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::DropErrorUnion { .. }
            )
        })
        .unwrap();
    f.blocks[0].instructions.splice(
        at..at,
        [
            op(
                Some(env_value),
                InstructionKind::TakeLocal { local: LocalId(0) },
            ),
            op(Some(n), InstructionKind::Constant(Constant::I32(7))),
            op(
                Some(owner),
                InstructionKind::MakeManaged { fields: vec![n] },
            ),
            op(
                Some(output),
                InstructionKind::MakePackage {
                    fields: vec![owner, env_value],
                },
            ),
        ],
    );
    f.blocks[0].terminator = Terminator::Return(Some(output));
    raw
}

#[test]
fn ordinary_helper_data_accepts_complete_sums_and_keeps_callback_contract() {
    for ty in [
        data(),
        wrapped(),
        union(),
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(Type::Package {
                name: "Flat".into(),
                fields: vec![("r".into(), owned())],
            }),
        },
    ] {
        extra(ty).accept();
    }
}
#[test]
fn ordinary_sum_data_does_not_open_owning_state_constructor_extras() {
    state(None).accept();
    for ty in [
        data(),
        wrapped(),
        union(),
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(owned()),
        },
    ] {
        state(Some(ty)).reject("owning State");
    }
}
