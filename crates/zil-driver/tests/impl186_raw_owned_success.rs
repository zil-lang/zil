//! Raw/Wire attacks start with independently verified source candidates.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind as I, PassingMode, Terminator, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "impl186-raw-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

const SOURCE: &str = r#"
resource R{n:i32}package P{manages r:R}enum E{Fail}
fn make<T:StoreElement>()->E!Store<T> contract{result(create)}{let s:Store<T>=store_new();let alias=s;return give s}
fn pass<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}=>x
fn rewrap<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}{let s=x?;return give s}
fn wrap<T:StoreElement>(give s:Store<T>)->E!Store<T> contract{result(give)}=>give s
fn main()->E!i32{let pending=pass(make::<P>());let s=rewrap(pending)?;let final=wrap(give s);return 0}
"#;

fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw186.zil", source);
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
    fs::write(dir.join("main.zil"), SOURCE).unwrap();
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
        let errors = match result {
            Err(errors) => errors,
            Ok(_) => {
                fs::write(
                    dir.join(format!("{route}-unexpected-accept.txt")),
                    "Verify accepted this exact mutation\n",
                )
                .unwrap();
                panic!("{label}/{route}: mutation unexpectedly accepted")
            }
        };
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
fn instantiated_modes_do_not_authorize_false_creation_sources() {
    let (original, plan) = candidate(SOURCE);
    let mut module = original.clone();
    module
        .functions
        .iter_mut()
        .find(|f| f.result_mode == PassingMode::Create)
        .unwrap()
        .result_mode = PassingMode::Infer;
    reject(
        "missing-mode",
        module,
        plan.clone(),
        "E-VERIFY-RESOURCE",
        "explicit create or give",
    );
    let inputs = original
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, f)| (f.result_mode == PassingMode::Give).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(
        inputs.len(),
        3,
        "all three written generic transfer bodies must instantiate"
    );
    for (n, index) in inputs.into_iter().enumerate() {
        let mut module = original.clone();
        module.functions[index].result_mode = PassingMode::Create;
        reject(
            &format!("input-as-create-{n}"),
            module,
            plan.clone(),
            "E-VERIFY-RESOURCE",
            "proven fresh",
        );
    }
}

#[test]
fn instantiated_union_still_requires_one_consumption() {
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
    for (label, position) in [("duplicate-take", take), ("duplicate-injection", make)] {
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
    let (f, b, i) = take;
    let mut module = original.clone();
    let I::TakeErrorUnion { value, is_error } = module.functions[f].blocks[b].instructions[i].kind
    else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::ReadErrorUnion { value, is_error };
    reject(
        "copy-success",
        module,
        plan.clone(),
        "E-VERIFY-ERROR",
        "consuming TakeErrorUnion",
    );
    let (f, b, i) = locate(
        &original,
        |f, i| matches!(i.kind,I::TakeLocal{local} if matches!(f.locals[local.0 as usize].ty,Type::ErrorUnion{..})),
    );
    let mut module = original;
    let I::TakeLocal { local } = module.functions[f].blocks[b].instructions[i].kind else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::Load(local);
    reject(
        "copy-whole-union",
        module,
        plan,
        "E-VERIFY-ERROR",
        "TakeLocal",
    );
}

#[test]
fn generic_origin_does_not_replace_actual_tag_descriptor_or_cleanup_checks() {
    let (original, plan) = candidate(SOURCE);
    let (f, b, _) = locate(&original, |_, i| matches!(i.kind, I::ErrorTag { .. }));
    let mut module = original.clone();
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
        "opposite-tag",
        module,
        plan.clone(),
        "E-VERIFY-ERROR-TAG",
        "exact active tag",
    );
    let (f, b, i) = locate(&original, |f, i| {
        f.result_mode == PassingMode::Create
            && matches!(
                i.kind,
                I::MakeErrorUnion {
                    is_error: false,
                    ..
                }
            )
    });
    let mut module = original.clone();
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
        "description-as-success",
        module,
        plan.clone(),
        "E-VERIFY-ERROR",
        "selected arm",
    );
    let (f, b, i) = locate(&original, |f, i| {
        matches!(i.kind, I::DropErrorUnion { local }
        if f.name == "main" && f.locals[local.0 as usize].name == "final")
    });
    let mut module = original;
    module.functions[f].blocks[b].instructions.remove(i);
    reject(
        "missing-union-cleanup",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "responsibility",
    );
}
