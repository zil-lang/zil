//! Raw context inputs only: neither analyzer summaries nor certificates.
use super::*;
use crate::place_replacements::fixture;
pub(super) fn public_callback_input_fixture() -> TypedModule {
    use zil_ir::{
        CallEvent, CallableCleanup, CallableExit, CallableParameterContract, CallableRole,
        CallableTrace, CallableTraces, LocalDecl, ReservationId,
    };
    let mut module = fixture::module();
    let template = module.functions.remove(0);
    let target = template.locals[0].ty.clone();
    let resource = template.locals[1].ty.clone();
    let Type::Managed(payload) = &resource else {
        unreachable!()
    };
    let desc = Type::Desc(payload.clone());
    let callback_address = Type::Address {
        mutable: true,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    let mut callback = template.clone();
    callback.id = FunctionId(2);
    callback.name = "actual_callback_entry".into();
    callback.parameters.clear();
    callback.locals.clear();
    callback.environment = Some(ClosureId(0));
    callback.return_type = Type::I32;
    callback.result_mode = PassingMode::Infer;
    callback.values = vec![desc.clone(), Type::I32].into();
    callback.blocks[0].instructions = vec![
        fixture::instruction(Some(ValueId(0)), InstructionKind::ReadCapture { index: 0 }),
        fixture::instruction(
            Some(ValueId(1)),
            InstructionKind::ReadDesc {
                desc: ValueId(0),
                index: 0,
            },
        ),
    ];
    callback.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    let mut helper = template.clone();
    helper.id = FunctionId(1);
    helper.name = "ordinary_input_and_callback".into();
    helper.parameters = vec![LocalId(0), LocalId(1), LocalId(2)];
    helper.locals = vec![callback_address.clone(), target.clone(), resource.clone()]
        .into_iter()
        .enumerate()
        .map(|(index, ty)| LocalDecl {
            id: LocalId(index as u32),
            name: format!("input{index}"),
            ty,
            mutable: true,
            span: template.span,
        })
        .collect();
    helper.values = vec![callback_address, target, resource, Type::I32].into();
    helper.return_type = Type::I32;
    helper.result_mode = PassingMode::Infer;
    helper.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(0),
            kind: CallableKind::FnMut,
            hold: vec![],
            call: vec![0],
            writes: vec![],
            cleanup: CallableCleanup::Borrowed,
        }],
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            }],
            exit: CallableExit::Return,
        }]),
    });
    helper.blocks[0].instructions = vec![
        fixture::instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        fixture::instruction(Some(ValueId(1)), InstructionKind::Load(LocalId(1))),
        fixture::instruction(
            Some(ValueId(2)),
            InstructionKind::TakeLocal { local: LocalId(2) },
        ),
        fixture::instruction(
            None,
            InstructionKind::ReservePlaceAddress {
                reservation: ReservationId(71),
                address: ValueId(1),
                path: vec![PlaceProjection::Field(0)],
            },
        ),
        fixture::instruction(
            None,
            InstructionKind::CommitPlaceReplace {
                reservation: ReservationId(71),
                value: ValueId(2),
            },
        ),
        fixture::instruction(
            Some(ValueId(3)),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(0),
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        ),
    ];
    helper.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    let mut forward = helper.clone();
    forward.id = FunctionId(0);
    forward.name = "forward_public_helper".into();
    forward.blocks[0].instructions.truncate(3);
    forward.blocks[0].instructions.push(fixture::instruction(
        Some(ValueId(3)),
        InstructionKind::Call {
            function: FunctionId(1),
            arguments: vec![ValueId(0), ValueId(1), ValueId(2)],
        },
    ));
    module.functions = vec![forward, helper, callback].into();
    module.closures = vec![ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(2),
        captures: vec![desc],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::FnMut,
        lending_captures: vec![],
    }];
    module
}
pub(super) fn old_descriptor_fixture() -> TypedModule {
    let mut module = fixture::module();
    let f = &mut module.functions[0];
    let resource = f.locals[1].ty.clone();
    let payload = if let Type::Managed(payload) = &resource {
        payload.clone()
    } else {
        unreachable!()
    };
    f.values.extend([
        Type::Address {
            mutable: true,
            inner: Box::new(resource),
        },
        Type::Desc(payload),
        Type::I32,
    ]);
    f.blocks[0].instructions.splice(
        1..1,
        vec![
            fixture::instruction(
                Some(ValueId(2)),
                InstructionKind::AddressField {
                    address: ValueId(0),
                    index: 0,
                },
            ),
            fixture::instruction(
                Some(ValueId(3)),
                InstructionKind::DescribeAddress {
                    address: ValueId(2),
                },
            ),
        ],
    );
    f.blocks[0].instructions.push(fixture::instruction(
        Some(ValueId(4)),
        InstructionKind::ReadDesc {
            desc: ValueId(3),
            index: 0,
        },
    ));
    module
}
