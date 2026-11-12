//! Returned environments keep each actual input instance and its live loans.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind, Type};
const SOURCE: &str = "fn make(pointer:&i32)->impl Fn()->i32{return move()=>*pointer}\nfn keep<F:Fn()->i32>(input:F)->F{return input}\nfn relay<F:Fn()->i32>(input:F)->F{return keep(input)}\nfn main()->i32{var a=20;var b=22;let f=relay(make(&a));let g=relay(make(&b));let first=f();a=99;return first+g()-42}";
#[test]
fn independent_capture_instances_keep_their_actual_loans_through_forwarding() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let input = SourceUnit::new(FileId(0), ModuleId(0), "returned-instances.zil", SOURCE);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    if let Some(dir) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::PathBuf::from(dir).join("capture-instances-typed.txt"),
            format!("{module:#?}"),
        )
        .unwrap();
    }
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let f = &module.functions[main];
    let block = f
        .blocks
        .iter()
        .position(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::CallClosure { .. }))
        })
        .unwrap();
    let instructions = &f.blocks[block].instructions;
    let factory = module
        .functions
        .iter()
        .find(|f| f.name == "make")
        .unwrap()
        .id;
    let environments: Vec<_> = instructions
        .iter()
        .filter_map(|i| {
            if let InstructionKind::Call {
                function,
                arguments,
            } = &i.kind
            {
                if *function == factory {
                    Some((i.result.unwrap(), arguments[0]))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    assert_eq!(environments.len(), 2);
    assert_ne!(environments[0].1, environments[1].1);
    assert_eq!(
        f.values[environments[0].0.0 as usize],
        f.values[environments[1].0.0 as usize]
    );
    let write = instructions
        .iter()
        .rposition(|i| matches!(i.kind, InstructionKind::Store { .. }))
        .unwrap();
    let first_call = instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosure { .. }))
        .unwrap();
    assert!(write > first_call);
    let mut changed = module.clone();
    let instructions = &mut changed.functions[main].blocks[block].instructions;
    // This write is legal after f's last use while the same-type g is live.
    // Moving it before f's call must conflict with f's actual input loan.
    let value = match instructions[write].kind {
        InstructionKind::Store { value, .. } => value,
        _ => unreachable!(),
    };
    assert_eq!(instructions[write - 1].result, Some(value));
    assert_eq!(f.values[value.0 as usize], Type::I32);
    let write_pair: Vec<_> = instructions.drain(write - 1..=write).collect();
    instructions.splice(first_call..first_call, write_pair);
    let mut forged = plan.clone();
    forged.module_fingerprint = changed.fingerprint();
    for decoded in [false, true] {
        let (raw, candidate) = if decoded {
            let wire = zil_ir::wire::encode_module_candidate(&changed, &forged, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
        } else {
            (changed.clone(), forged.clone())
        };
        let errors = zil_verify::verify_module(raw, candidate, &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-VERIFY-STORED"
                && e.message
                    == "stored target proof: actual target intersects a live incompatible loan"),
            "Wire={decoded}: {errors:?}"
        );
    }
}
