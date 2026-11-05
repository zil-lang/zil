//! Real Source/Typed replacement witnesses; candidate mutations remain raw/Wire.
use super::*;
use zil_ir::{CandidatePlan, PassingMode, TypedModule};

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

fn candidate(text: &str) -> (TypedModule, CandidatePlan) {
    let typed = typed(text);
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    (typed, plan)
}

fn verify_both(module: &TypedModule, plan: &CandidatePlan) {
    let target = TargetProfile::default();
    let limits = Limits::default();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(module, plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

fn reject_both(module: &TypedModule, plan: &CandidatePlan, code: &str, message: &str) {
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut claim = plan.clone();
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(module, &claim, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(module.clone(), claim, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.code == code && e.message.contains(message)),
            "{errors:?}"
        );
    }
}

#[test]
fn g135_enum_verify_real_runtime_arm_tickets() {
    let text = include_str!("replace-enum-terminal-pending.zil");
    let (module, plan) = candidate(text);
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
        matches!(&i.kind, InstructionKind::ReservePlaceAddress { path, .. } if path.is_empty())
    }));
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
        matches!(i.kind, InstructionKind::CommitPlaceReplace { value, .. }
            if matches!(main.values[value.0 as usize], Type::Enum { .. }))
    }));
    assert!(
        !main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| { matches!(i.kind, InstructionKind::WriteAddress { .. }) })
    );
    verify_both(&module, &plan);
}

const ORIGIN_HEADER: &str = "resource R{code:i32}enum E{Left(manages R,i32),Right(manages R,i32)}";

#[test]
fn g135_enum_verify_actual_rhs_origin_in_both_directions() {
    // Runtime choice keeps both actual RHS arms, so the ordinary match does
    // not rely on an unreachable-arm precision shortcut.
    let fresh = format!(
        "{ORIGIN_HEADER}fn fresh(flag:bool,input:E)->R contract{{result(create)}}{{var packet=input;let target=&mut packet;*target=if flag{{E.Left(R{{code:42}},1)}}else{{E.Right(R{{code:42}},2)}};match packet{{E.Left(r,n)=>{{return give r}},E.Right(r,n)=>{{return give r}}}}}}fn main()->i32{{let r=fresh(false,E.Left(R{{code:65}},0));return r.code}}"
    );
    let (module, plan) = candidate(&fresh);
    verify_both(&module, &plan);

    let incoming = format!(
        "{ORIGIN_HEADER}fn incoming(flag:bool,give input:R)->R contract{{result(give)}}{{var packet=E.Left(R{{code:65}},0);let target=&mut packet;*target=if flag{{E.Left(give input,1)}}else{{E.Right(give input,2)}};match packet{{E.Left(r,n)=>{{return give r}},E.Right(r,n)=>{{return give r}}}}}}fn main()->i32{{let r=incoming(false,R{{code:42}});return r.code}}"
    );
    let (mut module, plan) = candidate(&incoming);
    verify_both(&module, &plan);
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "incoming")
        .unwrap()
        .result_mode = PassingMode::Create;
    reject_both(&module, &plan, "E-VERIFY-RESOURCE", "fresh");
}

#[test]
fn g135_enum_verify_actual_selector_requires_single_root() {
    for same in [true, false] {
        let other = if same { "x" } else { "y" };
        let text = format!(
            "{ORIGIN_HEADER}fn choose(flag:bool)->i32{{var x=E.Left(R{{code:65}},1);var y=E.Right(R{{code:66}},2);let target=if flag{{&mut x}}else{{&mut {other}}};*target=E.Right(R{{code:67}},3);return 42}}fn main()->i32{{return choose(false)}}"
        );
        let module = typed(&text);
        if same {
            let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
            verify_both(&module, &plan);
            continue;
        }
        assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
        // Same actual CFG and honest multi-root Holds; a direct-x reserve
        // supplies a legal candidate without asking Analyze to authorize the
        // address-selected raw candidate this independent kernel must reject.
        let mut direct = module.clone();
        let f = direct
            .functions
            .iter_mut()
            .find(|f| f.name == "choose")
            .unwrap();
        let x = f.locals.iter().find(|l| l.name == "x").unwrap().id;
        let reserve = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
            .unwrap();
        let InstructionKind::ReservePlaceAddress { reservation, .. } = reserve.kind else {
            unreachable!()
        };
        reserve.kind = InstructionKind::ReservePlaceLocal {
            reservation,
            local: x,
            path: vec![],
        };
        let plan = zil_analyze::analyze(&direct, &Limits::default()).unwrap();
        verify_both(&direct, &plan);
        reject_both(
            &module,
            &plan,
            "E-UNSUPPORTED",
            "one independently reconstructed actual target",
        );
    }
}
