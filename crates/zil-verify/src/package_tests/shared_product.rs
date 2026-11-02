//! Independently built synchronous shared Product parameter candidates.
//! No producer/type/analyzer solver supplies these roots or Hold rows.
use super::*;

fn address() -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(package()),
    }
}

// 0: borrowed parameter, 1: owning parameter's local storage, 2: ordinary local.
fn fixture(mode: u8) -> (TypedModule, CandidatePlan) {
    let mut render = Body::new(Type::I32);
    let panel = if mode == 0 {
        render.parameter(address())
    } else if mode == 1 {
        render.parameter(package())
    } else {
        render.package_local()
    };
    let body = render.storage(Type::Closure(ClosureId(0)));
    let root = if mode == 0 {
        HoldRoot::Parameter(0)
    } else {
        HoldRoot::Local(panel)
    };
    let input = render.value(
        address(),
        if mode == 0 {
            InstructionKind::Load(panel)
        } else {
            InstructionKind::BorrowLocal { local: panel }
        },
    );
    let borrowed = render.value(
        address(),
        InstructionKind::ReborrowAddress {
            address: input,
            mutable: false,
        },
    );
    let closure = render.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![borrowed],
        },
    );
    render.op(InstructionKind::Store {
        local: body,
        value: closure,
    });
    let result = render.value(
        Type::I32,
        InstructionKind::CallClosureShared {
            callee: body,
            arguments: vec![],
        },
    );
    if mode != 0 {
        render.op(InstructionKind::DropPackage { local: panel });
    }
    let mut module = module(render.finish(Some(result), CleanupExitKind::Return));
    let mut body = Body::new(Type::I32);
    body.0.id = FunctionId(1);
    body.0.name = "read_panel".into();
    body.0.environment = Some(ClosureId(0));
    let captured = body.value(address(), InstructionKind::ReadCapture { index: 0 });
    let field = body.value(
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        InstructionKind::AddressField {
            address: captured,
            index: 2,
        },
    );
    let result = body.value(Type::I32, InstructionKind::ReadAddress { address: field });
    module
        .functions
        .push(body.finish(Some(result), CleanupExitKind::Return));
    module.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(1),
        kind: CallableKind::Fn,
        captures: vec![address()],
        parameters: vec![],
        result: Type::I32,
        lending_captures: vec![],
    });
    let mut plan = resource_tests::plan(&module);
    for value in [input, borrowed, closure] {
        plan.functions[0].holds[value.0 as usize].roots = vec![root];
    }
    for value in [captured, field] {
        plan.functions[1].holds[value.0 as usize].roots = vec![HoldRoot::Capture(0)];
    }
    (module, plan)
}

fn both(module: TypedModule, plan: CandidatePlan) -> [CompileResult<VerifiedModule>; 2] {
    let target = module.target.clone();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    [
        verify_module(module, plan, &target, &limits),
        verify_module_bytes(&bytes, &target, &limits),
    ]
}

#[test]
fn shared_product_parameter_keeps_its_actual_root_and_local_controls() {
    for mode in 0..3 {
        let (module, plan) = fixture(mode);
        for result in both(module, plan) {
            result.unwrap();
        }
    }
    let (module, mut plan) = fixture(0);
    for hold in &mut plan.functions[0].holds {
        if !hold.roots.is_empty() {
            hold.roots = vec![HoldRoot::Local(LocalId(0))];
        }
    }
    for result in both(module, plan) {
        let ds = result.unwrap_err();
        assert!(ds.iter().any(|d| d.code == "E-VERIFY-HOLD"), "{ds:?}");
    }
}
