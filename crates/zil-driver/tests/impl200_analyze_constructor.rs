//! Real source producers followed by independent Analyze and raw/Wire checks.
//! No native execution is claimed by this component test.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile, json_string,
};
use zil_ir::{
    CandidatePlan, Instruction, InstructionKind as I, LocalId, Type, TypedFunction, TypedModule,
};
const SINGLE: &str = include_str!("impl200_fixtures/00_single_callback_state.zil");
const DOUBLE: &str = include_str!("impl200_fixtures/01_same_f_two_callback_state.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "analyze200-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, saved.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "constructor200.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    (module, plan)
}
fn pair(
    label: &str,
    source: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    rejected: Option<(ResultCategory, &str)>,
) {
    let case = Case::new(label);
    fs::write(case.0.join("source.zil"), source).unwrap();
    fs::write(case.0.join("module.txt"), format!("{module:#?}")).unwrap();
    plan.module_fingerprint = module.fingerprint();
    fs::write(case.0.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    let limits = Limits::default();
    let result = zil_analyze::analyze(&module, &limits);
    fs::write(case.0.join("analyze.txt"), format!("{result:#?}\n")).unwrap();
    match rejected {
        None => {
            assert_eq!(result.unwrap(), plan);
        }
        Some((category, message)) => {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| d.phase == Phase::Analyze
                    && d.category == category
                    && d.message.contains(message)),
                "{label}: {errors:?}"
            );
        }
    }
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    fs::write(case.0.join("candidate.wire"), &bytes).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &module.target, &limits),
        ),
    ] {
        match rejected {
            None => {
                result.unwrap();
                fs::write(case.0.join(format!("{route}.txt")), "Pass\n").unwrap();
            }
            Some(_) => {
                let errors = result.unwrap_err();
                fs::write(
                    case.0.join(format!("{route}.txt")),
                    format!("{errors:#?}\n"),
                )
                .unwrap();
                assert!(
                    errors.iter().any(|d| d.phase == Phase::Verify
                        && matches!(
                            d.category,
                            ResultCategory::SafeReject | ResultCategory::Unsupported
                        )),
                    "{label}/{route}: {errors:?}"
                );
            }
        }
    }
}
fn constructor(module: &mut TypedModule) -> &mut TypedFunction {
    module
        .functions
        .iter_mut()
        .find(|f| {
            f.callable_contract.is_none()
                && matches!(f.return_type, Type::Package { .. })
                && f.parameters
                    .iter()
                    .filter(|p| matches!(f.locals[p.0 as usize].ty, Type::Closure(_)))
                    .count()
                    == 2
        })
        .unwrap()
}

#[test]
fn original_single_and_double_sources_pass_without_an_empty_helper_contract() {
    for (label, source) in [("original00", SINGLE), ("original01", DOUBLE)] {
        let (module, plan) = candidate(source);
        if label == "original01" {
            let mut copied = module.clone();
            let function = constructor(&mut copied);
            assert!(function.callable_contract.is_none());
            assert_eq!(
                function
                    .parameters
                    .iter()
                    .filter(|p| matches!(function.locals[p.0 as usize].ty, Type::Closure(_)))
                    .count(),
                2
            );
        }
        pair(label, source, module, plan, None);
    }
    let renamed = DOUBLE
        .replace("fn pack<", "fn assemble<")
        .replace("pack(", "assemble(");
    let (module, plan) = candidate(&renamed);
    pair("name-independent", &renamed, module, plan, None);
}

#[test]
fn each_actual_owned_assembly_value_and_remaining_field_requires_its_responsibility() {
    let (original, plan) = candidate(DOUBLE);
    pair(
        "assembly-control",
        DOUBLE,
        original.clone(),
        plan.clone(),
        None,
    );
    for duplicate in [false, true] {
        let mut module = original.clone();
        let instruction = constructor(&mut module)
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::MakePackage { .. }))
            .unwrap();
        let I::MakePackage { fields } = &mut instruction.kind else {
            unreachable!()
        };
        if duplicate {
            fields[2] = fields[1];
        } else {
            fields.remove(2);
        }
        pair(
            if duplicate {
                "duplicate-ssa"
            } else {
                "missing-field"
            },
            DOUBLE,
            module,
            plan.clone(),
            Some((ResultCategory::SafeReject, "resource")),
        );
    }
    let mut module = original;
    let function = module
        .functions
        .iter_mut()
        .find(|f| {
            f.parameters
                .iter()
                .any(|p| matches!(f.locals[p.0 as usize].ty, Type::Package { .. }))
        })
        .unwrap();
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::DropPackage { .. }))
        })
        .unwrap();
    let index = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::DropPackage { .. }))
        .unwrap();
    let old = block.instructions.remove(index);
    let I::DropPackage { local } = old.kind else {
        unreachable!()
    };
    block.instructions.insert(
        index,
        Instruction {
            result: None,
            kind: I::DropPackageField { local, index: 1 },
            span: old.span,
        },
    );
    block.instructions.insert(
        index,
        Instruction {
            result: None,
            kind: I::DropPackageField { local, index: 0 },
            span: old.span,
        },
    );
    pair(
        "missing-second-cleanup",
        DOUBLE,
        module,
        plan,
        Some((ResultCategory::SafeReject, "resource")),
    );
}

#[test]
fn new_constructor_does_not_accept_a_borrowed_extra_parameter_or_input() {
    let (original, plan) = candidate(DOUBLE);
    for callback in [false, true] {
        let mut module = original.clone();
        let function = constructor(&mut module);
        let local = if callback {
            function.parameters[1]
        } else {
            function.parameters[2]
        };
        let address = Type::Address {
            mutable: false,
            inner: Box::new(function.locals[local.0 as usize].ty.clone()),
        };
        function.locals[local.0 as usize].ty = address.clone();
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if matches!(instruction.kind, I::Load(p) | I::TakeLocal { local: p } if p == local) {
                function.values[instruction.result.unwrap().0 as usize] = address.clone();
            }
        }
        pair(
            if callback {
                "borrowed-callback"
            } else {
                "borrowed-extra-data"
            },
            DOUBLE,
            module,
            plan.clone(),
            Some((ResultCategory::SafeReject, "no public helper contract")),
        );
    }
}

#[test]
fn concrete_type_identity_and_actual_capture_graph_are_rechecked() {
    let source = format!(
        "{DOUBLE}\nfn other_factory(seed:i32)->impl Fn()->i32{{let log=bytes_new();return move()=>{{bytes_len(log);return seed+1}}}}\n"
    );
    let (original, plan) = candidate(&source);
    pair(
        "identity-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut different = original.clone();
    let function = constructor(&mut different);
    let second = function.parameters[1];
    let Type::Closure(first) = function.locals[second.0 as usize].ty else {
        unreachable!()
    };
    let other = if first.0 == 0 {
        zil_ir::ClosureId(1)
    } else {
        zil_ir::ClosureId(0)
    };
    function.locals[second.0 as usize].ty = Type::Closure(other);
    for i in function.blocks.iter().flat_map(|b| &b.instructions) {
        if matches!(i.kind, I::Load(p) | I::TakeLocal { local: p } if p == second) {
            function.values[i.result.unwrap().0 as usize] = Type::Closure(other);
        }
    }
    pair(
        "different-environment-type",
        &source,
        different,
        plan.clone(),
        Some((ResultCategory::SafeReject, "no public helper contract")),
    );
    let mut borrowed = original;
    let id = {
        let function = constructor(&mut borrowed);
        let Type::Closure(id) = function.locals[function.parameters[0].0 as usize].ty else {
            unreachable!()
        };
        id
    };
    let capture = borrowed.closures[id.0 as usize]
        .captures
        .iter_mut()
        .find(|ty| matches!(ty, Type::Managed(_)))
        .unwrap();
    let Type::Managed(inner) = capture.clone() else {
        unreachable!()
    };
    *capture = Type::Desc(inner);
    pair(
        "forged-self-containment",
        &source,
        borrowed,
        plan,
        Some((ResultCategory::Unsupported, "self-contained")),
    );
}

#[test]
fn ordinary_two_role_helper_cannot_use_the_constructor_exception() {
    let source = "fn make(n:i32)->impl Fn()->i32{let log=bytes_new();return move()=>{bytes_len(log);return n}}fn sum<F:FnOnce()->i32>(first:F,second:F)->i32{return first()+second()}fn main()->i32{return sum(make(17),make(25))-42}";
    let (original, plan) = candidate(source);
    pair(
        "ordinary-helper-control",
        source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut module = original;
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    function.callable_contract = None;
    pair(
        "ordinary-helper-missing-contract",
        source,
        module,
        plan,
        Some((ResultCategory::SafeReject, "no public helper contract")),
    );
}

#[test]
fn real_source_constructor_work_obeys_n_n_minus_one_and_fresh_retry() {
    let case = Case::new("source-budget");
    let mut rows = Vec::new();
    for (label, source) in [("single", SINGLE), ("double", DOUBLE)] {
        let (module, expected) = candidate(source);
        fs::write(case.0.join(format!("{label}.zil")), source).unwrap();
        let probe = |work| {
            zil_analyze::analyze(
                &module,
                &Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                },
            )
        };
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match probe(mid) {
                Ok(_) => high = mid,
                Err(errors) => {
                    assert!(
                        errors.iter().all(|d| d.phase == Phase::Analyze
                            && d.category == ResultCategory::ResourceLimit),
                        "{label}: {errors:?}"
                    );
                    low = mid;
                }
            }
        }
        assert_eq!(probe(high).unwrap(), expected);
        let errors = probe(high - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(probe(high).unwrap(), expected);
        rows.push(format!(
            "{{\"source\":{},\"N\":{high},\"below\":{},\"fresh\":\"Pass\"}}",
            json_string(label),
            json_string(&format!("{errors:?}"))
        ));
    }
    fs::write(
        case.0.join("boundaries.json"),
        format!("[{}]\n", rows.join(",")),
    )
    .unwrap();
}
