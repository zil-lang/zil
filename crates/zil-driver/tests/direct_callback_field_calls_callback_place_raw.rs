//! Source-derived field places are reconstructed by independent raw/Wire Verify.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
    json_string,
};
use zil_ir::{
    CallableKind, CandidatePlan, CoreModule, Instruction, InstructionKind as I, LocalId, Type,
    TypedFunction, TypedModule, ValueId,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const PREFIX: &str = "resource R{code:i32}package State<F>{manages r:R;callback:F;value:i32}fn pack<F:FnMut(i32)->i32>(callback:F)->State<F>{return State::<F>{r:R{code:84},callback:callback,value:0}}fn make()->impl FnMut(i32)->i32{let r=R{code:65};var n=0;return move(delta:i32)=>{r.code;n+=delta;return n}}";
const LENDING_SOURCE: &str = "fn make()->impl FnMut(i32)->&[u8]{let b=bytes_new();return move(n:i32)=>{bytes_reserve(b,bytes_len(b)+1);bytes_push(b,u8(n));return bytes_view(b)}}package State<F>{manages storage:ByteBuffer;callback:F}fn pack<F:FnMut(i32)->&[u8]>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback}}fn main()->i32{var state=pack(make());let view=state.callback(7);let value=i32(view[0]);let next=state.callback(value);return i32(next.len())-2}";
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "callback199-raw-{label}-{}-{}",
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
fn candidate(source: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "direct_callback_field_calls_callback_place_raw.zil",
        source,
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    (core, module, plan)
}
fn pair(
    label: &str,
    source: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    expected: Option<&str>,
) {
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    let case = Case::new(label);
    fs::write(case.0.join("source.zil"), source).unwrap();
    fs::write(case.0.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(case.0.join("candidate.txt"), format!("{plan:#?}")).unwrap();
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
        match expected {
            None => {
                result.unwrap();
                fs::write(case.0.join(format!("{route}.txt")), "Pass\n").unwrap();
            }
            Some(code) => {
                let errors = result.unwrap_err();
                fs::write(
                    case.0.join(format!("{route}.txt")),
                    format!("{errors:#?}\n"),
                )
                .unwrap();
                assert_eq!(errors[0].phase, Phase::Verify, "{label}: {errors:?}");
                assert_eq!(
                    errors[0].category,
                    ResultCategory::SafeReject,
                    "{label}: {errors:?}"
                );
                assert_eq!(errors[0].code, code, "{label}: {errors:?}");
            }
        }
    }
}
fn main_function(module: &mut TypedModule) -> &mut TypedFunction {
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap()
}

#[test]
fn explicit_callable_borrow_cannot_change_to_a_same_typed_sibling() {
    let source = r#"
resource Token { marker: i32 }
impl Drop for Token { fn drop(self: Self) {} }
package Pair<F: Value> { manages token: Token; left: F; right: F }
fn hold<F: Value>(left: F, right: F) -> Pair<F> {
    return Pair::<F> { token: Token { marker: 0 }, left: left, right: right }
}
fn make() -> impl FnMut(i32) -> i32 {
    var n = 0
    return move(delta: i32) => { n += delta; return n }
}
fn main() -> i32 {
    var pair = hold(make(), make())
    return (&mut pair.left)(1)
}
"#;
    let input = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "explicit-callable-borrow.zil",
        source,
    );
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_semantics(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    assert!(module.semantic.is_some());
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    pair(
        "explicit-sibling-positive",
        source,
        module.clone(),
        plan.clone(),
        None,
    );
    let borrow = first_borrow(main_function(&mut module));
    let I::BorrowPackageField { index, .. } = &mut borrow.kind else {
        unreachable!("source borrows the left callback field");
    };
    assert_eq!(*index, 1);
    *index = 2;
    pair(
        "explicit-sibling-retarget",
        source,
        module,
        plan,
        Some("E-VERIFY-SEMANTIC"),
    );
}

const NESTED_VALUE_PACKAGES: &str = r#"
resource Tag { code: i32 }
impl Drop for Tag { fn drop(self: Self) {} }
package Pair<T: Value> { manages tag: Tag; left: T; right: T }
fn pair<T: Value>(left: T, right: T) -> Pair<T> {
    return Pair::<T> { tag: Tag { code: 0 }, left: left, right: right }
}
fn counter(start: i32) -> impl FnMut(i32) -> i32 {
    var n = start
    return move(delta: i32) => { n += delta; return n }
}
fn main() -> i32 {
    var root = pair(pair(counter(1), counter(2)), pair(counter(3), counter(4)))
    return (&mut root.right.left)(1)
}
"#;

#[test]
fn nested_value_package_borrow_checks_the_complete_field_path() {
    let source = NESTED_VALUE_PACKAGES;
    let limits = Limits::default();
    let module = value_package_module(source);
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    pair("nested-value-control", source, module.clone(), plan.clone(), None);
    use zil_ir::PlaceProjection::Field;
    for (position, sibling) in [(0, Field(1)), (1, Field(2))] {
        let mut changed = module.clone();
        let path = main_function(&mut changed).blocks.iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find_map(|instruction| match &mut instruction.kind {
                I::BorrowPlaceLocal { path, mutable: true, .. } if path == &[Field(2), Field(1)] => Some(path),
                _ => None,
            }).expect("complete original callback borrow");
        path[position] = sibling;
        assert!(zil_analyze::analyze(&changed, &limits).is_err());
        pair("nested-value-sibling", source, changed, plan.clone(), Some("E-VERIFY-SEMANTIC"));
    }
}

#[test]
fn forwarded_child_package_keeps_its_actual_holder_and_receiver() {
    use zil_ir::PlaceProjection::Field;

    let source = NESTED_VALUE_PACKAGES
        .replace("fn main()", "fn relay<F: Value + FnMut(i32)->i32>(root:&mut Pair<F>, n:i32)->i32 {return (root.left)(n)}\nfn main()")
        .replace("(&mut root.right.left)(1)", "relay(&mut root.right, 1)");
    let limits = Limits::default();
    let mut module = value_package_module(&source);
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    pair("forwarded-child-control", &source, module.clone(), plan.clone(), None);

    let path = main_function(&mut module).blocks.iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            I::BorrowPlaceLocal { path, mutable: true, .. } if path == &[Field(2)] => Some(path),
            _ => None,
        }).expect("exclusive borrow of the right child Package");
    path[0] = Field(1);
    assert!(zil_analyze::analyze(&module, &limits).is_err());
    pair("forwarded-child-sibling", &source, module, plan, Some("E-VERIFY-SEMANTIC"));
}

fn value_package_module(source: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "value-package-path.zil", source);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand_with_map_semantics(
        &unit, &syntax, &limits, &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    ).unwrap().core;
    zil_types::check(&core, &target, &limits).unwrap()
}

#[test]
fn borrowed_value_package_helper_preserves_the_original_receiver_field() {
    let source = r#"
resource Tag { code: i32 }
impl Drop for Tag { fn drop(self: Self) {} }
package Pair<T: Value> { manages tag: Tag; left: T; right: T }
fn pair<T: Value>(left: T, right: T) -> Pair<T> {
    return Pair::<T> { tag: Tag { code: 0 }, left: left, right: right }
}
fn counter(start: i32) -> impl FnMut(i32) -> i32 {
    var n = start
    return move(delta: i32) => { n += delta; return n }
}
fn relay<F: Value + FnMut(i32) -> i32>(root: &mut Pair<F>, n: i32) -> i32 {
    return (root.left)(n)
}
fn main() -> i32 {
    var value = pair(counter(1), counter(2))
    return relay(&mut value, 3)
}
"#;
    let limits = Limits::default();
    let mut module = value_package_module(source);
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    pair("borrowed-helper-control", source, module.clone(), plan.clone(), None);
    let helper = module.functions.iter_mut()
        .find(|f| f.parameters.first().is_some_and(|id| matches!(
            &f.locals[id.0 as usize].ty,
            Type::Address { mutable: true, inner } if matches!(inner.as_ref(), Type::Package { .. })
        ))).expect("helper borrowing the concrete Package");
    let index = helper.blocks.iter_mut().flat_map(|block| &mut block.instructions)
        .find_map(|op| match &mut op.kind {
            I::AddressField { index, .. } => Some(index),
            _ => None,
        }).expect("field of actual borrowed Package");
    assert_eq!(*index, 1);
    *index = 2;
    assert!(zil_analyze::analyze(&module, &limits).is_err());
    pair("borrowed-helper-sibling", source, module, plan, Some("E-VERIFY-SEMANTIC"));
}

/// §8.1 and semantic-scopes §可调用值与失败出口的组合. The mutation keeps the
/// original call and receiver identity while changing the physical field.
#[cfg(all(feature = "llvm-api", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn projected_callback_retains_its_field_through_a_generic_helper() {
    use std::process::Command;
    use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage};

    let source = r#"
resource Token { marker: i32 }
impl Drop for Token { fn drop(self: Self) {} }
package Pair<F: Value> { manages token: Token; left: F; right: F }
fn hold<F: Value>(left: F, right: F) -> Pair<F> {
    return Pair::<F> { token: Token { marker: 0 }, left: left, right: right }
}
fn make(start: i32) -> impl FnMut(i32) -> i32 {
    var n = start
    return move(delta: i32) => { n += delta; return n }
}
fn relay<F: FnMut(i32) -> i32>(f: &mut F, n: i32) -> i32 { return f(n) }
fn main() -> i32 {
    var pair = hold(make(40), make(10))
    return relay(&mut pair.left, 2)
}
"#;
    let input = SourceUnit::new(FileId(0), ModuleId(0), "projected-helper.zil", source);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_semantics(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    pair(
        "projected-helper-positive",
        source,
        module.clone(),
        plan.clone(),
        None,
    );
    let borrow = first_borrow(main_function(&mut module));
    let I::BorrowPackageField { index, .. } = &mut borrow.kind else {
        unreachable!()
    };
    assert_eq!(*index, 1);
    *index = 2;
    pair(
        "projected-helper-retarget",
        source,
        module,
        plan,
        Some("E-VERIFY-SEMANTIC"),
    );

    let case = Case::new("projected-helper-native");
    for (field, expected) in [("left", 42), ("right", 12)] {
        let source = source.replace("pair.left", &format!("pair.{field}"));
        let input = SourceUnit::new(FileId(0), ModuleId(0), "projected-helper.zil", source);
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            for optimization in [Optimization::O0, Optimization::O2] {
                let executable = case.0.join(format!("{field}-{backend:?}-{optimization:?}"));
                let report = zil_driver::compile_source(
                    &input,
                    &CompileOptions {
                        stop_after: Stage::Build,
                        output: Some(executable.clone()),
                        backend,
                        optimization,
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let observed = Command::new(executable).output().unwrap();
                assert_eq!(observed.status.code(), Some(expected));
                assert!(observed.stdout.is_empty() && observed.stderr.is_empty());
            }
        }
    }
}

fn first_borrow(function: &mut TypedFunction) -> &mut Instruction {
    function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::BorrowPackageField { .. }))
        .unwrap()
}

#[test]
fn single_source_receiver_lending_matches_the_complete_semantic_entry() {
    use zil_ir::{StorageBase, StorageBinder};

    let source = LENDING_SOURCE;
    let (core, module, plan) = candidate(source);
    assert!(core.semantic.is_none());
    let semantic = value_package_module(source);
    let closure = &module.closures[0];
    let life = closure.storage_life.as_ref().unwrap();
    assert_eq!(closure.storage_life, semantic.closures[0].storage_life);
    assert_eq!(closure.lending_captures, [0]);
    assert_eq!(life.fixed_binder_count, 0);
    assert_eq!(life.invocation_binder_count, 1);
    assert_eq!(
        life.bindings
            .iter()
            .map(|row| (row.position.base, row.binder))
            .collect::<Vec<_>>(),
        [
            (StorageBase::Receiver, StorageBinder::Invocation(0)),
            (StorageBase::Result, StorageBinder::Invocation(0)),
        ],
    );
    assert_eq!(
        module.functions[closure.function.0 as usize]
            .storage_life
            .as_ref(),
        Some(life)
    );
    let returned = module
        .functions
        .iter()
        .find_map(|f| f.callable_return_contract.as_ref())
        .unwrap();
    assert_eq!(returned.life.as_ref(), Some(life));
    assert_eq!(returned.lending_captures, [0]);
    assert_eq!(returned.cleanup, [0]);

    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let verified = zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap();
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};
        use zil_backend_llvm::{BuildOptions, NativeBackend, Optimization};

        let lir = zil_lower::lower(&verified).unwrap();
        let case = Case::new("single-source-receiver-lending");
        fs::write(case.0.join("source.zil"), source).unwrap();
        let clang = std::env::var_os("ZIL_CLANG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "clang".into());
        let probe = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/language/composed-store-closures/allocator_probe.c");
        let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
        let wrapper = case.0.join("link-observer.sh");
        fs::write(&wrapper, format!(
            "#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) exec {} \"$@\" ;; esac\ndone\nexec {} \"$@\" -x c {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
            quote(&clang), quote(&clang), quote(&probe),
        )).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let backends = [
            NativeBackend::Clang,
            #[cfg(feature = "llvm-api")]
            NativeBackend::LlvmApi,
        ];
        for backend in backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let name = format!("{backend:?}-{optimization:?}");
                let mut options = BuildOptions::new(case.0.join(&name));
                options.clang = wrapper.clone();
                options.backend = backend;
                options.optimization = optimization;
                let artifact = zil_backend_llvm::build(&verified, &lir, &options).unwrap();
                assert_eq!(artifact.codegen.backend, backend);
                assert!(artifact.codegen.fallback.is_none());
                let observed = Command::new(artifact.executable)
                    .env_remove("ZIL_TEST_FAIL_AT")
                    .output()
                    .unwrap();
                fs::write(case.0.join(format!("{name}.stdout")), &observed.stdout).unwrap();
                fs::write(case.0.join(format!("{name}.stderr")), &observed.stderr).unwrap();
                assert_eq!(
                    observed.status.code(),
                    Some(0),
                    "{backend:?} {optimization:?}: {observed:?}"
                );
                assert!(observed.stdout.is_empty());
                let cleanup = std::str::from_utf8(&observed.stderr).unwrap();
                assert_eq!(
                    cleanup.trim_end(),
                    "{\"alloc_calls\":2,\"allocated\":1,\"frees\":1,\"failures\":0,\"live\":0,\"peak_live\":1,\"peak_bytes\":2}"
                );
            }
        }
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    drop(verified);
}

#[test]
fn actual_direct_place_has_no_take_and_untrusted_field_mode_changes_reject() {
    let source = format!(
        "{PREFIX}fn main()->i32{{var state=pack(make());return state.callback(1)+state.callback(1)-3}}"
    );
    let (_, original, plan) = candidate(&source);
    let f = original
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    assert_eq!(
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(
                i.kind,
                I::CallClosureBorrowed {
                    kind: CallableKind::FnMut,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(
        !f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, I::TakePackageField { .. }))
    );
    pair(
        "place-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    for (variant, label, code) in [
        (0, "wrong-field", "E-VERIFY-PACKAGE"),
        (1, "shared-permission", "E-VERIFY-CLOSURE-CALL"),
        (2, "borrowed-once-mode", "E-VERIFY-CLOSURE-CALL"),
        (3, "delete-state-cleanup", "E-VERIFY-RESOURCE"),
    ] {
        let mut module = original.clone();
        let f = main_function(&mut module);
        match variant {
            0 => {
                let I::BorrowPackageField { index, .. } = &mut first_borrow(f).kind else {
                    unreachable!()
                };
                *index = 0;
            }
            1 => {
                let i = first_borrow(f);
                let result = i.result.unwrap();
                let I::BorrowPackageField { mutable, .. } = &mut i.kind else {
                    unreachable!()
                };
                *mutable = false;
                let Type::Address { mutable, .. } = &mut f.values[result.0 as usize] else {
                    unreachable!()
                };
                *mutable = false;
            }
            2 => {
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::CallClosureBorrowed { .. }))
                    .unwrap();
                let I::CallClosureBorrowed { kind, .. } = &mut i.kind else {
                    unreachable!()
                };
                *kind = CallableKind::FnOnce;
            }
            _ => {
                let state =
                    LocalId(f.locals.iter().position(|l| l.name == "state").unwrap() as u32);
                let mut removed = 0;
                for b in &mut f.blocks {
                    b.instructions.retain(|i| {
                        let keep = !matches!(i.kind, I::DropPackage { local } if local == state);
                        removed += usize::from(!keep);
                        keep
                    });
                }
                assert_eq!(removed, 1);
            }
        }
        pair(label, &source, module, plan.clone(), Some(code));
    }
}

#[test]
fn stale_field_address_and_retargeted_dead_parent_are_rejected() {
    let source = format!(
        "{PREFIX}fn main()->i32{{var state=pack(make());state.callback(1);state.callback=make();return state.callback(1)-1}}"
    );
    let (_, original, plan) = candidate(&source);
    pair(
        "replacement-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut module = original;
    let f = main_function(&mut module);
    let mut first = None;
    let mut changed = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::CallClosureBorrowed { callee, .. } = &mut i.kind {
            if let Some(old) = first {
                *callee = old;
                changed += 1;
            } else {
                first = Some(*callee);
            }
        }
    }
    assert_eq!(changed, 1);
    pair(
        "stale-replaced-field",
        &source,
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );

    let source = format!(
        "{PREFIX}fn main()->i32{{var state=pack(make());var other=pack(make());let moved=other;return state.callback(1)-1}}"
    );
    let (_, original, plan) = candidate(&source);
    pair(
        "parent-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut module = original;
    let f = main_function(&mut module);
    let other = LocalId(f.locals.iter().position(|l| l.name == "other").unwrap() as u32);
    let I::BorrowPackageField { local, .. } = &mut first_borrow(f).kind else {
        unreachable!()
    };
    *local = other;
    pair(
        "dead-parent",
        &source,
        module,
        plan,
        Some("E-VERIFY-INITIALIZATION"),
    );
}

#[test]
fn actual_once_field_cannot_be_copied_or_borrowed_as_shared() {
    let source = "resource R{n:i32}package State<F>{manages r:R;callback:F}fn pack<F:FnOnce()->i32>(callback:F)->State<F>{return State::<F>{r:R{n:1},callback:callback}}fn make()->impl FnOnce()->i32{let r=R{n:2};return move()=>{let taken=give r;return taken.n}}fn main()->i32{let state=pack(make());return state.callback()-2}";
    let (_, original, plan) = candidate(source);
    pair("once-control", source, original.clone(), plan.clone(), None);
    let mut copied = original.clone();
    let i = main_function(&mut copied)
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::TakePackageField { .. }))
        .unwrap();
    let I::TakePackageField { local, index } = i.kind else {
        unreachable!()
    };
    i.kind = I::ReadPackageField { local, index };
    pair(
        "once-without-take",
        source,
        copied,
        plan.clone(),
        Some("E-VERIFY-PACKAGE"),
    );
    let mut borrowed = original;
    let f = main_function(&mut borrowed);
    let (block, position) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, I::TakePackageField { .. }))
                .map(|p| (b, p))
        })
        .unwrap();
    let take = &f.blocks[block].instructions[position];
    let I::TakePackageField { local, index } = take.kind else {
        unreachable!()
    };
    let inner = f.values[take.result.unwrap().0 as usize].clone();
    let span = take.span;
    let address = ValueId(f.values.len() as u32);
    f.values.push(Type::Address {
        mutable: false,
        inner: Box::new(inner),
    });
    f.blocks[block].instructions.insert(
        position,
        Instruction {
            result: Some(address),
            kind: I::BorrowPackageField {
                local,
                index,
                mutable: false,
            },
            span,
        },
    );
    let call = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::CallClosureOnce { .. }))
        .unwrap();
    let I::CallClosureOnce { arguments, .. } = &mut call.kind else {
        unreachable!()
    };
    call.kind = I::CallClosureBorrowed {
        callee: address,
        kind: CallableKind::Fn,
        arguments: std::mem::take(arguments),
    };
    pair(
        "once-borrowed-shared",
        source,
        borrowed,
        plan,
        Some("E-VERIFY-CLOSURE-CALL"),
    );
}

#[test]
fn generic_modes_are_preserved_when_actual_environment_is_fn() {
    for mode in ["FnMut", "FnOnce"] {
        let source = format!(
            "resource R{{n:i32}}package State<F>{{manages r:R;callback:F}}fn pack<F:{mode}()->i32>(callback:F)->State<F>{{return State::<F>{{r:R{{n:1}},callback:callback}}}}fn make()->impl Fn()->i32{{let r=R{{n:2}};return move()=>r.n}}fn run<G:{mode}()->i32>(state:State<G>)->i32{{var current=state;return current.callback()}}fn main()->i32{{return run(pack(make()))-2}}"
        );
        let (_, module, plan) = candidate(&source);
        let kind = if mode == "FnMut" {
            CallableKind::FnMut
        } else {
            CallableKind::FnOnce
        };
        assert_eq!(module.closures[0].kind, CallableKind::Fn);
        let invoked = module
            .functions
            .iter()
            .filter(|f| f.name.starts_with("@module0:helper"))
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .filter(|i| match i.kind {
                I::CallClosureBorrowed { kind: actual, .. } => actual == kind,
                I::CallClosureOnce { .. } => kind == CallableKind::FnOnce,
                _ => false,
            })
            .count();
        assert_eq!(invoked, 1, "{mode}");
        pair(mode, &source, module, plan, None);
    }
}

#[test]
fn actual_last_lending_use_cannot_be_reordered_after_the_next_callee_borrow() {
    let source = LENDING_SOURCE;
    let (_, original, plan) = candidate(source);
    pair(
        "last-lending-control",
        source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut module = original;
    let function = main_function(&mut module);
    let view = LocalId(
        function
            .locals
            .iter()
            .position(|local| local.name == "view")
            .unwrap() as u32,
    );
    let block = function
        .blocks
        .iter_mut()
        .find(|block| {
            block
                .instructions
                .iter()
                .any(|i| matches!(i.kind, I::Load(local) if local == view))
        })
        .unwrap();
    let read = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::Load(local) if local == view))
        .unwrap();
    let borrow = block
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.kind, I::BorrowPackageField { .. }))
        .nth(1)
        .unwrap()
        .0;
    assert!(read < borrow);
    let instruction = block.instructions.remove(borrow);
    block.instructions.insert(read, instruction);
    pair(
        "callee-before-final-use",
        source,
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );
}

#[test]
fn direct_field_work_is_bounded_in_all_three_fresh_phases() {
    let source = format!(
        "{PREFIX}fn run<G:FnMut(i32)->i32>(state:State<G>,count:i32)->i32{{var current=state;var i=0;var sum=0;while i<count{{sum+=current.callback(1);i+=1}}return sum}}fn main()->i32{{return run(pack(make()),3)-6}}"
    );
    let (core, module, plan) = candidate(&source);
    let case = Case::new("budgets");
    fs::write(case.0.join("source.zil"), &source).unwrap();
    let mut rows = Vec::new();
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let probe = |work| -> Result<(), Vec<Diagnostic>> {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            match phase {
                Phase::Types => {
                    zil_types::check(&core, &module.target, &limits).map(|m| assert_eq!(m, module))
                }
                Phase::Analyze => {
                    zil_analyze::analyze(&module, &limits).map(|p| assert_eq!(p, plan))
                }
                _ => {
                    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                        .map(|_| ())
                }
            }
        };
        let mut high = Limits::default().max_analysis_steps;
        let mut low = 0;
        probe(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match probe(middle) {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors.iter().any(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}/{middle}: {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        probe(high).unwrap();
        let errors = probe(high - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        probe(high).unwrap();
        rows.push(format!(
            "{{\"phase\":\"{phase:?}\",\"N\":{high},\"below\":{},\"fresh_retry\":\"Pass\"}}",
            json_string(&format!("{errors:?}"))
        ));
    }
    fs::write(
        case.0.join("boundaries.json"),
        format!("[{}]\n", rows.join(",")),
    )
    .unwrap();
}
