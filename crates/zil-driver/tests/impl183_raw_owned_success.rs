//! Raw/Wire attacks start with independently verified source candidates.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit, TargetProfile};
use zil_ir::{
    CandidatePlan, Constant, InstructionKind as I, PassingMode, Terminator, Type, TypedModule,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "impl183-raw-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

const SOURCE: &str = r#"
resource R{n:i32}enum E{Fail}
fn make()->E!R contract{result(create)}{let r=R{n:1};let alias=r;return give r}
fn pass(give x:E!R)->E!R contract{result(give)}=>x
fn rewrap(give x:E!R)->E!R contract{result(give)}{let r=x?;return give r}
fn wrap(give r:R)->E!R contract{result(give)}=>give r
fn main()->E!i32{let pending=pass(make());let r=rewrap(pending)?;return r.n-1}
"#;

fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw183.zil", source);
    let parsed = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &parsed).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn reject(label: &str, module: TypedModule, mut plan: CandidatePlan, code: &str, message: &str) {
    let limits = Limits::default();
    let target = module.target.clone();
    // Mutations do not rerun Analyze. Neither a new fingerprint nor the old
    // producer's Hold rows can authorize forged responsibility or freshness.
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory(label);
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ),
    ] {
        let errors = result.unwrap_err();
        fs::write(dir.join(format!("{route}.txt")), format!("{errors:#?}\n")).unwrap();
        assert!(
            errors.iter().all(|d| d.phase == Phase::Verify),
            "{label}/{route}: {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.code == code && d.message.contains(message)),
            "{label}/{route}: {errors:#?}"
        );
    }
}

fn locate(
    module: &TypedModule,
    predicate: impl Fn(&zil_ir::TypedFunction, &zil_ir::Instruction) -> bool,
) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if predicate(function, instruction) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("missing mutation target")
}

fn duplicate_result(
    module: &mut TypedModule,
    plan: &mut CandidatePlan,
    position: (usize, usize, usize),
) {
    let (f, b, i) = position;
    let function = &mut module.functions[f];
    let mut instruction = function.blocks[b].instructions[i].clone();
    let old = instruction.result.unwrap();
    let value = zil_ir::ValueId(function.values.len() as u32);
    function
        .values
        .push(function.values[old.0 as usize].clone());
    instruction.result = Some(value);
    function.blocks[b].instructions.insert(i + 1, instruction);
    let mut hold = plan.functions[f]
        .holds
        .iter()
        .find(|h| h.value == old)
        .unwrap()
        .clone();
    hold.value = value;
    plan.functions[f].holds.push(hold);
}

#[test]
fn independent_verify_rejects_missing_result_mode_and_laundered_freshness() {
    let (original, plan) = candidate(SOURCE);
    let mut module = original.clone();
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "make")
        .unwrap()
        .result_mode = PassingMode::Infer;
    reject(
        "missing-result-mode",
        module,
        plan.clone(),
        "E-VERIFY-RESOURCE",
        "explicit create or give",
    );
    for name in ["pass", "rewrap", "wrap"] {
        let mut module = original.clone();
        module
            .functions
            .iter_mut()
            .find(|f| f.name == name)
            .unwrap()
            .result_mode = PassingMode::Create;
        reject(
            &format!("incoming-as-create-{name}"),
            module,
            plan.clone(),
            "E-VERIFY-RESOURCE",
            "proven fresh",
        );
    }
}

#[test]
fn independent_verify_rejects_copying_and_double_transfer_of_managed_success_union() {
    let (original, plan) = candidate(SOURCE);
    let take = locate(&original, |_, i| {
        matches!(
            i.kind,
            I::TakeErrorUnion {
                is_error: false,
                ..
            }
        )
    });
    let make = locate(
        &original,
        |f, i| matches!(i.kind,I::MakeErrorUnion{value,..} if matches!(f.values[value.0 as usize],Type::Managed(_))),
    );
    let local = locate(
        &original,
        |f, i| matches!(i.kind,I::TakeLocal{local} if matches!(f.locals[local.0 as usize].ty,Type::ErrorUnion{..})),
    );
    for (label, position) in [
        ("duplicate-take", take),
        ("duplicate-success-injection", make),
    ] {
        let mut module = original.clone();
        let mut claims = plan.clone();
        duplicate_result(&mut module, &mut claims, position);
        reject(
            label,
            module,
            claims,
            "E-VERIFY-RESOURCE",
            "not available exactly once",
        );
    }
    let mut module = original.clone();
    let (f, b, i) = take;
    let I::TakeErrorUnion { value, is_error } = module.functions[f].blocks[b].instructions[i].kind
    else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::ReadErrorUnion { value, is_error };
    reject(
        "nonconsuming-success",
        module,
        plan.clone(),
        "E-VERIFY-ERROR",
        "consuming TakeErrorUnion",
    );
    let mut module = original;
    let (f, b, i) = local;
    let I::TakeLocal { local } = module.functions[f].blocks[b].instructions[i].kind else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::Load(local);
    reject(
        "copy-union-local",
        module,
        plan,
        "E-VERIFY-ERROR",
        "TakeLocal",
    );
}

#[test]
fn active_tag_and_source_category_are_checked_without_candidate_permission() {
    let (original, plan) = candidate(SOURCE);
    let tag = locate(&original, |_, i| matches!(i.kind, I::ErrorTag { .. }));
    let mut module = original.clone();
    let (f, b, _) = tag;
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = &mut module.functions[f].blocks[b].terminator
    else {
        unreachable!()
    };
    std::mem::swap(then_block, else_block);
    reject(
        "opposite-tag-edge",
        module,
        plan.clone(),
        "E-VERIFY-ERROR-TAG",
        "exact active tag",
    );
    let mut module = original.clone();
    let (f, b, i) = tag;
    module.functions[f].blocks[b].instructions[i].kind = I::Constant(Constant::Bool(false));
    reject(
        "invented-tag",
        module,
        plan.clone(),
        "E-VERIFY-ERROR-TAG",
        "exact active tag",
    );
    let make = locate(&original, |f, i| {
        f.name == "make"
            && matches!(
                i.kind,
                I::MakeErrorUnion {
                    is_error: false,
                    ..
                }
            )
    });
    let mut module = original.clone();
    let (f, b, i) = make;
    let I::MakeErrorUnion { value, .. } = module.functions[f].blocks[b].instructions[i].kind else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::MakeErrorUnion {
        value,
        is_error: true,
    };
    reject(
        "owner-in-error-arm",
        module,
        plan.clone(),
        "E-VERIFY-ERROR",
        "selected arm",
    );
    let mut module = original;
    let (f, b, i) = make;
    let descriptor = module.functions[f]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::DescribeLocal { .. }))
        .unwrap()
        .result
        .unwrap();
    module.functions[f].blocks[b].instructions[i].kind = I::MakeErrorUnion {
        value: descriptor,
        is_error: false,
    };
    reject(
        "descriptor-as-owner",
        module,
        plan,
        "E-VERIFY-ERROR",
        "selected arm",
    );
}

#[test]
fn omitted_union_cleanup_and_post_transfer_tag_are_rejected() {
    let (mut module, plan) = candidate(
        "resource R{n:i32}enum E{Fail}fn make()->E!R contract{result(create)}=>R{n:1};fn main(){let u=make()}",
    );
    let (f, b, i) = locate(&module, |_, i| matches!(i.kind, I::DropErrorUnion { .. }));
    module.functions[f].blocks[b].instructions.remove(i);
    reject(
        "omitted-cleanup",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "responsibility",
    );
    let (mut module, mut plan) = candidate(SOURCE);
    let take = locate(&module, |_, i| {
        matches!(
            i.kind,
            I::TakeErrorUnion {
                is_error: false,
                ..
            }
        )
    });
    duplicate_result(&mut module, &mut plan, take);
    let (f, b, i) = take;
    let I::TakeErrorUnion { value, .. } = module.functions[f].blocks[b].instructions[i].kind else {
        unreachable!()
    };
    let result = module.functions[f].blocks[b].instructions[i + 1]
        .result
        .unwrap();
    module.functions[f].values[result.0 as usize] = Type::Bool;
    module.functions[f].blocks[b].instructions[i + 1].kind = I::ErrorTag { value };
    reject(
        "tag-after-transfer",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "live owned union",
    );
}

#[test]
fn raw_shape_claims_cannot_extend_resource_error_nesting_or_capture_profiles() {
    let (original, plan) = candidate(SOURCE);
    for nested in [false, true] {
        let mut module = original.clone();
        let function = module
            .functions
            .iter_mut()
            .find(|f| f.name == "make")
            .unwrap();
        let Type::ErrorUnion { error, result } = &mut function.return_type else {
            unreachable!()
        };
        if nested {
            **error = Type::ErrorUnion {
                error: error.clone(),
                result: result.clone(),
            };
        } else {
            *error = result.clone();
        }
        reject(
            if nested {
                "nested-owned-error"
            } else {
                "direct-resource-error"
            },
            module,
            plan.clone(),
            "E-UNSUPPORTED",
            "type",
        );
    }
    let mut module = original;
    module.resources.clear();
    reject(
        "unknown-resource",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "nominal definition table",
    );
    let (mut module, plan) = candidate(
        "resource R{n:i32}enum E{Fail}fn make()->E!R contract{result(create)}=>R{n:1};fn main()->i32{let n=42;let f=move()=>n;return f()-42}",
    );
    let union = module
        .functions
        .iter()
        .find(|f| f.name == "make")
        .unwrap()
        .return_type
        .clone();
    assert_eq!(module.closures.len(), 1);
    module.closures[0].captures[0] = union;
    reject(
        "owned-success-capture",
        module,
        plan,
        "E-UNSUPPORTED",
        "owning error-union captures",
    );
}
