//! Raw and Wire admission never trust a producer's slice signature or Hold.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{CandidatePlan, Instruction, InstructionKind as I, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SHARED: &str =
    "fn main()->i32{let a=[20,22];let f=(p:&[i32])=>p[0]+p[1];return f(&a[..])-42}";

fn baseline(text: &str) -> (TypedModule, CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw193.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    (module, plan)
}
fn record(name: &str, module: &TypedModule, plan: &CandidatePlan, results: &[Vec<Diagnostic>]) {
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = PathBuf::from(root).join(format!(
            "raw193-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("module.txt"), format!("{module:#?}")).unwrap();
        fs::write(dir.join("candidate.txt"), format!("{plan:#?}")).unwrap();
        fs::write(dir.join("diagnostics.txt"), format!("{results:#?}")).unwrap();
    }
}
fn reject(
    name: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    category: ResultCategory,
    code: &str,
) {
    // Executable operands must be checked without stale identity being the reason.
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    let results = [
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err(),
    ];
    record(name, &module, &plan, &results);
    for diagnostics in results {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == category && d.code == code),
            "{name}: {diagnostics:?}"
        );
    }
}

#[test]
fn entry_and_actual_parent_permissions_are_checked_independently() {
    let (mut module, plan) = baseline(SHARED);
    let Type::Address { mutable, .. } = &mut module.closures[0].parameters[0] else {
        unreachable!()
    };
    *mutable = true;
    reject(
        "entry-disagrees",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE-ENTRY",
    );
    let (mut module, plan) = baseline(SHARED);
    let mut changed = false;
    for function in &mut module.functions {
        if function.environment.is_some() {
            continue;
        }
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::ReborrowAddress { mutable, .. } = &mut instruction.kind {
                *mutable = true;
                let Type::Address { mutable, .. } =
                    &mut function.values[instruction.result.unwrap().0 as usize]
                else {
                    unreachable!()
                };
                *mutable = true;
                changed = true;
            }
        }
    }
    assert!(changed);
    reject(
        "shared-parent-forged-exclusive",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-ADDRESS",
    );
}

#[test]
fn a_raw_write_through_a_shared_slice_element_is_not_admitted() {
    let (mut module, plan) = baseline(SHARED);
    let entry = module.closures[0].function.0 as usize;
    let mut inserted = false;
    for block in &mut module.functions[entry].blocks {
        let mut next = Vec::new();
        for instruction in std::mem::take(&mut block.instructions) {
            let write = if let I::ReadAddress { address } = instruction.kind {
                Some(Instruction {
                    result: None,
                    kind: I::WriteAddress {
                        address,
                        value: instruction.result.unwrap(),
                    },
                    span: instruction.span,
                })
            } else {
                None
            };
            next.push(instruction);
            if let Some(write) = write {
                next.push(write);
                inserted = true;
            }
        }
        block.instructions = next;
    }
    assert!(inserted);
    reject(
        "shared-element-write",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-ADDRESS",
    );
}

#[test]
fn raw_range_operand_types_and_hold_rows_are_independent() {
    let (mut module, plan) = baseline(SHARED);
    let mut changed = false;
    for function in &mut module.functions {
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::AddressSlice { address, start, .. } = &mut instruction.kind {
                *start = Some(*address);
                changed = true;
            }
        }
    }
    assert!(changed);
    reject(
        "address-as-range-endpoint",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-ADDRESS",
    );
    let (module, mut plan) = baseline(SHARED);
    let mut changed = false;
    for function in &mut plan.functions {
        for hold in &mut function.holds {
            if module.functions[function.function.0 as usize].values[hold.value.0 as usize]
                .is_slice_address()
            {
                hold.roots.clear();
                changed = true;
            }
        }
    }
    assert!(changed);
    reject(
        "erased-slice-hold",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-HOLD",
    );
}

#[test]
fn replacing_an_explicit_operand_with_its_exclusive_sibling_preserves_aliases() {
    let source = "fn main()->i32{var a=[20,22];var b=[1,2];let f=(p:&mut[i32],q:&mut[i32])=>{p[0]+=q[0];return p[0]};return f(&mut a[..],&mut b[..])-21}";
    let (mut module, plan) = baseline(source);
    let mut changed = false;
    for function in &mut module.functions {
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            let arguments = match &mut instruction.kind {
                I::CallClosure { arguments, .. }
                | I::CallClosureShared { arguments, .. }
                | I::CallClosureMut { arguments, .. }
                | I::CallClosureOnce { arguments, .. }
                | I::CallClosureBorrowed { arguments, .. } => arguments,
                _ => continue,
            };
            assert_eq!(arguments.len(), 2);
            arguments[1] = arguments[0];
            changed = true;
        }
    }
    assert!(changed);
    reject(
        "same-exclusive-operand",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-MOVE",
    );

    // Two distinct SSA carriers must also fail when their executable parents
    // share storage. Fresh producer Hold rows cannot authorize that overlap.
    let (mut module, _) = baseline(source);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let a = main
        .locals
        .iter()
        .find(|local| local.name == "a")
        .unwrap()
        .id;
    let b = main
        .locals
        .iter()
        .find(|local| local.name == "b")
        .unwrap()
        .id;
    let mut changed = false;
    for instruction in main
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        if let I::BorrowLocalMut { local } = &mut instruction.kind
            && *local == b
        {
            *local = a;
            changed = true;
        }
    }
    assert!(changed);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    reject(
        "distinct-exclusive-operands-same-parent",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn captured_desc_aliasing_is_rebuilt_from_actual_executable_backing() {
    for helper in [false, true] {
        let declaration = if helper {
            "fn apply<F:Fn(&[u8])->i32>(f:&F,p:&[u8])->i32{return f(p)}"
        } else {
            ""
        };
        let call = if helper { "apply(&f,p)" } else { "f(p)" };
        let source = format!(
            "{declaration}fn main()->i32{{let edited=bytes_new();let other=bytes_new();bytes_push(other,7u8);let p=bytes_view(other);let f=(q:&[u8])=>{{bytes_reserve(edited,8);return i32(q[0])}};return {call}-7}}"
        );
        let (mut module, plan) = baseline(&source);
        let main = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let edited = main.locals.iter().find(|l| l.name == "edited").unwrap().id;
        let other = main.locals.iter().find(|l| l.name == "other").unwrap().id;
        let mut changed = false;
        for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::DescribeLocal { local } = &mut instruction.kind
                && *local == edited
            {
                *local = other;
                changed = true;
            }
        }
        assert!(changed);
        let current_plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        reject(
            if helper {
                "helper-stale-backing-hold"
            } else {
                "direct-stale-backing-hold"
            },
            module.clone(),
            plan,
            ResultCategory::SafeReject,
            "E-VERIFY-HOLD",
        );
        // Updating a producer claim to the real provenance still cannot make
        // the captured descriptor's write compatible with the explicit window.
        reject(
            if helper {
                "helper-desc-now-same-backing"
            } else {
                "direct-desc-now-same-backing"
            },
            module,
            current_plan,
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn raw_unsupported_shapes_and_capture_or_lending_claims_stay_closed() {
    for (name, ty) in [
        (
            "bare-slice",
            Type::Slice {
                element: Box::new(Type::I32),
            },
        ),
        (
            "nested-address",
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                }),
            },
        ),
        (
            "nested-slice",
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Slice {
                    element: Box::new(Type::Slice {
                        element: Box::new(Type::I32),
                    }),
                }),
            },
        ),
        (
            "noncopy-elements",
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Slice {
                    element: Box::new(Type::ByteBuffer),
                }),
            },
        ),
    ] {
        let (mut module, plan) = baseline(SHARED);
        module.closures[0].parameters[0] = ty;
        reject(
            name,
            module,
            plan,
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        );
    }
    let (mut module, plan) = baseline(SHARED);
    let parameter = module.closures[0].parameters[0].clone();
    module.closures[0].captures.push(parameter);
    reject(
        "slice-capture",
        module,
        plan,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    let (mut module, plan) = baseline(SHARED);
    module.closures[0].result = module.closures[0].parameters[0].clone();
    reject(
        "explicit-argument-return",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-LENDING",
    );
    let (mut module, plan) = baseline(SHARED);
    module.closures[0].lending_captures.push(0);
    reject(
        "parameter-is-not-capture",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE",
    );
}

#[test]
fn actual_slice_work_has_measured_raw_and_wire_n_and_n_minus_one_boundaries() {
    let source = "fn apply<F:Fn(&mut[i32],&[i32])->i32>(f:&F,p:&mut[i32],q:&[i32])->i32{return f(p,q)}fn main()->i32{var a=[20,21];let b=[22];let f=(p:&mut[i32],q:&[i32])=>{p[0]+=q[0];return p[0]};return apply(&f,&mut a[..],&b[..])-42}";
    let (module, plan) = baseline(source);
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for wire in [false, true] {
        let works = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &module.target, &limits)
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert!(works(high).is_ok());
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if works(middle).is_ok() {
                high = middle;
            } else {
                low = middle;
            }
        }
        let failed = works(low).unwrap_err();
        assert!(
            failed
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::ResourceLimit),
            "{failed:?}"
        );
        assert!(works(high).is_ok());
        record(
            &format!(
                "{}-budget-{low}-reject-{high}-accept",
                if wire { "wire" } else { "raw" }
            ),
            &module,
            &plan,
            &[failed],
        );
    }
}

#[test]
fn raw_parameter_count_and_slice_type_depth_exhaust_bounds_before_entry_checks() {
    let (mut module, mut plan) = baseline(SHARED);
    let parameter = module.closures[0].parameters[0].clone();
    module.closures[0].parameters.resize(3, parameter);
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits {
        max_locals: 2,
        ..Limits::default()
    };
    let diagnostics =
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
            .unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{diagnostics:?}"
    );
    record("bounded-parameter-count", &module, &plan, &[diagnostics]);
    let (module, plan) = baseline("fn main(){let f=(p:&[[[i32;1];1]])=>0;}");
    let limits = Limits {
        max_call_depth: 2,
        ..Limits::default()
    };
    let diagnostics =
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
            .unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{diagnostics:?}"
    );
    record("bounded-slice-type-depth", &module, &plan, &[diagnostics]);
}
