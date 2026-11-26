//! End-to-end native execution, OOM cleanup, source safety, and Raw/Wire IR
//! verification tests for L2 Batch 2 (`OwnedDyn<dyn Trait, Context<'a>>`
//! borrowed providers).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l2-b2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
}

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_and_run_dual_with_c(
    dir: &Path,
    entry: &Path,
    extra_c_sources: &[PathBuf],
) -> (i32, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}{}", std::env::consts::EXE_SUFFIX));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            extra_c_sources: extra_c_sources.to_vec(),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(-1);
        let result = (code, out.stdout);
        if let Some(prev) = &expected {
            assert_eq!(&result, prev, "result mismatch on {label}");
        } else {
            expected = Some(result);
        }
    }
    expected.expect("at least one backend ran")
}

fn check_rejected(dir: &Path, name: &str, src: &str, expected_code_prefix: &str) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "expected `{name}` to be rejected, but it succeeded"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)),
        "expected diagnostic starting with `{expected_code_prefix}` for `{name}`, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn dyn_context_fixtures_execute_natively_on_dual_backends() {
    let root = repo_root();
    let fixtures = [
        "tests/fixtures/dyn-context/exclusive-provider.zil",
        "tests/fixtures/dyn-context/exclusive-factory.zil",
        "tests/fixtures/dyn-context/trait-life-setter.zil",
        "tests/fixtures/dyn-context/captured-owner.zil",
        "tests/fixtures/dyn-context/field-address-factory.zil",
        "tests/fixtures/dyn-context/shared-field-from-mutable.zil",
        "tests/fixtures/dyn-context/long-slice-getter.zil",
        "tests/fixtures/dyn-context/owned-inline-region.zil",
        "tests/fixtures/dyn-context/static-slice-phantom.zil",
        "tests/fixtures/dyn-context/store-owner.zil",
        "tests/fixtures/dyn-context/captured-owner-mut.zil",
        "tests/fixtures/dyn-context/closed-owner-static-callback.zil",
        "tests/fixtures/dyn-context/holder-leaf-factory.zil",
        "tests/fixtures/dyn-context/static-slice-with-input.zil",
        "tests/fixtures/dyn-context/static-slice-without-input.zil",
        "tests/fixtures/dyn-context/library-without-provider.zil",
        "tests/fixtures/dyn-context/static-callback-getter-cycle-control.zil",
        "tests/language-readiness/dynamic-dispatch/dyn-owned-allocation-loop.zil",
    ];
    for (idx, rel) in fixtures.iter().enumerate() {
        let dir = temp_dir(&format!("fixture-{idx}"));
        let path = root.join(rel);
        let (code, stdout) = build_and_run_dual_with_c(&dir, &path, &[]);
        let expected_code = if rel.ends_with("dyn-owned-allocation-loop.zil") {
            if cfg!(target_os = "windows") {
                400_000
            } else {
                128 // 400_000 & 0xff on POSIX waitpid
            }
        } else {
            0
        };
        assert_eq!(
            code, expected_code,
            "fixture {rel} returned unexpected exit code {code}"
        );
        assert!(
            stdout.is_empty(),
            "fixture {rel} produced unexpected stdout"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn borrowed_dyn_provider_drop_order_and_oom_cleanup_native() {
    let dir = temp_dir("drop-and-oom");
    let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource Token { code: i32 }
impl Drop for Token {
    fn drop(self: Self) {
        unsafe { putchar(self.code) }
    }
}

struct Context<life a> { leaf: &'a mut i32 }
package Provider<life a> { pointer: &'a mut i32; manages token: Token }

trait Change {
    fn step(self: &mut Self) -> i32;
}

impl<life a> Change for Provider<'a> {
    fn step(self: &mut Self) -> i32 {
        *self.pointer += 1
        return *self.pointer
    }
}

fn run<life a>(value: &'a mut i32) -> i32 {
    match owned_dyn::<dyn Change, Context<'a>>(Provider::<'a> { pointer: value, token: Token { code: 68 } }) {
        .err(_) => { return 99 },
        .ok(owner) => {
            var handle = owner
            let a = handle.step()
            let b = handle.step()
            return a + b
        }
    }
}

fn main() -> i32 {
    var value = 10
    let sum = run(&mut value)
    if sum == 99 {
        return 99
    }
    value += 100
    if sum == 23 && value == 112 {
        unsafe { putchar(79) }
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);

    // 1. Normal execution: Provider drops ('D') at the end of run() BEFORE
    //    main() resumes mutating `value` and prints 'O'.
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"DO");

    // 2. OOM injection: calloc() returns NULL; OwnedDynCreate must drop the
    //    unboxed concrete Provider<'a> exactly once ('D') and return 99.
    let oom_dir = temp_dir("drop-and-oom-fail");
    let oom_probe = oom_dir.join("oom_calloc.c");
    std::fs::write(
        &oom_probe,
        r#"
#include <stddef.h>
void *calloc(size_t nmemb, size_t size) {
    (void)nmemb;
    (void)size;
    return NULL;
}
"#,
    )
    .expect("write oom_calloc.c");
    let (oom_code, oom_stdout) = build_and_run_dual_with_c(&oom_dir, &entry, &[oom_probe]);
    assert_eq!(oom_code, 99);
    assert_eq!(oom_stdout.as_slice(), b"D");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&oom_dir);
}

#[test]
fn borrowed_dyn_provider_safety_negatives_are_rejected() {
    let dir = temp_dir("safety-negatives");

    // 1. Local borrow escaping via returned OwnedDyn<dyn Change, Context<'a>>.
    check_rejected(
        &dir,
        "local_borrow_escape",
        r#"
struct Context<life a> { leaf: &'a mut i32 }
struct Provider<life a> { pointer: &'a mut i32 }
trait Change { fn step(self: &mut Self) -> i32; }
impl<life a> Change for Provider<'a> {
    fn step(self: &mut Self) -> i32 { *self.pointer += 1; return *self.pointer }
}
fn escape<life a>(_unused: &'a mut i32) -> DynCreateError!OwnedDyn<dyn Change, Context<'a>> {
    var local = 7
    return owned_dyn::<dyn Change, Context<'a>>(Provider::<'a> { pointer: &mut local })
}
fn main() -> i32 {
    var x = 1
    match escape(&mut x) { .err(_) => { return 0 }, .ok(_) => { return 1 } }
}
"#,
        "E-SAFETY-SOLVE",
    );

    // 2. Conflicting write to borrowed owner while OwnedDyn is still live.
    check_rejected(
        &dir,
        "conflicting_write_while_live",
        r#"
struct Context<life a> { leaf: &'a mut i32 }
struct Provider<life a> { pointer: &'a mut i32 }
trait Change { fn step(self: &mut Self) -> i32; }
impl<life a> Change for Provider<'a> {
    fn step(self: &mut Self) -> i32 { *self.pointer += 1; return *self.pointer }
}
fn main() -> i32 {
    var x = 10
    match owned_dyn::<dyn Change, Context<'a>>(Provider { pointer: &mut x }) {
        .err(_) => { return 1 }
        .ok(owner) => {
            var h = owner
            x = 99
            return h.step()
        }
    }
}
"#,
        "E-SAFETY-SOLVE",
    );

    // 3. Invariant region mismatch on OwnedDyn<dyn Set<'a>, Context<'a>>.
    check_rejected(
        &dir,
        "invariant_region_mismatch",
        r#"
struct Context<life a> { value: &'a i32 }
struct Pointer<life a> { value: &'a i32 }
trait Set<life a> {
    fn set(self: &mut Self, next: &'a i32);
    fn read(self: &Self) -> i32;
}
impl<life p> Set<'p> for Pointer<'p> {
    fn set(self: &mut Self, next: &'p i32) { self.value = next }
    fn read(self: &Self) -> i32 { return *self.value }
}
fn create<life a>(initial: &'a i32) -> DynCreateError!OwnedDyn<dyn Set<'a>, Context<'a>> {
    return owned_dyn::<dyn Set<'a>, Context<'a>>(Pointer::<'a> { value: initial })
}
fn update<life a>(owner: &mut OwnedDyn<dyn Set<'a>, Context<'a>>, next: &'a i32) {
    owner.set(next)
}
fn bad<life a, life b>(initial: &'a i32, shorter: &'b i32) -> i32 {
    match create(initial) {
        .err(_) => { return 0 }
        .ok(owner) => {
            var h = owner
            update(&mut h, shorter)
            return h.read()
        }
    }
}
fn main() -> i32 {
    let a = 1
    let b = 2
    return bad(&a, &b)
}
"#,
        "E-SAFETY-SOLVE",
    );

    // 4. Erasing borrowed Provider<'a> into context-free OwnedDyn<dyn Change>.
    check_rejected(
        &dir,
        "erasing_borrow_without_context",
        r#"
struct Provider<life a> { pointer: &'a mut i32 }
trait Change { fn step(self: &mut Self) -> i32; }
impl<life a> Change for Provider<'a> {
    fn step(self: &mut Self) -> i32 { *self.pointer += 1; return *self.pointer }
}
fn bad<life a>(value: &'a mut i32) -> DynCreateError!OwnedDyn<dyn Change> {
    return owned_dyn::<dyn Change>(Provider::<'a> { pointer: value })
}
fn main() -> i32 {
    var x = 1
    match bad(&mut x) { .err(_) => { return 0 }, .ok(_) => { return 1 } }
}
"#,
        "E-SAFETY-SOLVE",
    );

    // 5. Provider carries region 'b not covered by Context<'a>.
    check_rejected(
        &dir,
        "provider_region_exceeds_context",
        r#"
struct Context<life a> { leaf: &'a i32 }
struct Provider<life a, life b> { first: &'a i32, second: &'b i32 }
trait Read { fn read(self: &Self) -> i32; }
impl<life a, life b> Read for Provider<'a, 'b> {
    fn read(self: &Self) -> i32 { return *self.first + *self.second }
}
fn bad<life a, life b>(x: &'a i32, y: &'b i32) -> DynCreateError!OwnedDyn<dyn Read, Context<'a>> {
    return owned_dyn::<dyn Read, Context<'a>>(Provider::<'a, 'b> { first: x, second: y })
}
fn main() -> i32 {
    let a = 1
    let b = 2
    match bad(&a, &b) { .err(_) => { return 0 }, .ok(_) => { return 1 } }
}
"#,
        "E-SAFETY-SOLVE",
    );

    // 6. Double move / use-after-move of borrowed OwnedDyn.
    check_rejected(
        &dir,
        "double_move_owned_dyn",
        r#"
struct Context<life a> { leaf: &'a mut i32 }
struct Provider<life a> { pointer: &'a mut i32 }
trait Change { fn step(self: &mut Self) -> i32; }
impl<life a> Change for Provider<'a> {
    fn step(self: &mut Self) -> i32 { *self.pointer += 1; return *self.pointer }
}
fn take<life a>(h: OwnedDyn<dyn Change, Context<'a>>) -> i32 {
    var owned = h
    return owned.step()
}
fn run<life a>(value: &'a mut i32) -> i32 {
    match owned_dyn::<dyn Change, Context<'a>>(Provider::<'a> { pointer: value }) {
        .err(_) => { return 0 }
        .ok(owner) => {
            let a = take(owner)
            let b = take(owner)
            return a + b
        }
    }
}
fn main() -> i32 { var x = 1; return run(&mut x) }
"#,
        "E-SAFETY-SOLVE",
    );

    // 7. Context-free OwnedDyn<dyn Change> cannot unify with OwnedDyn<dyn Change, Context<'a>>.
    check_rejected(
        &dir,
        "context_vs_contextless_type_mismatch",
        r#"
struct Context<life a> { leaf: &'a i32 }
struct Plain { value: i32 }
trait Read { fn read(self: &Self) -> i32; }
impl Read for Plain { fn read(self: &Self) -> i32 { return self.value } }
fn bad<life a>(_unused: &'a i32) -> DynCreateError!OwnedDyn<dyn Read, Context<'a>> {
    return owned_dyn::<dyn Read>(Plain { value: 1 })
}
fn main() -> i32 {
    let x = 1
    match bad(&x) { .err(_) => { return 0 }, .ok(_) => { return 1 } }
}
"#,
        "E-TYPE-MISMATCH",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_forged_owned_dyn_context_bypass_is_rejected_by_checker() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, DynDef, DynImpl, DynSlotSig,
        FieldDef, FnId, FnRole, FnSig, Function, GenericArgs, IntTy, IntrinsicOp, Local, LocalDecl,
        Module, Mutability, Operand, ParamEnv, Place, Region, Terminator, TyKind, TyTable,
        VariantDef, decode_module, encode_module,
    };

    // Build a forged raw Body IR module where OwnedDynCreate erases a borrowed
    // Provider<'a> (&'a mut i32) into context-free OwnedDyn { def, context: None }.
    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let ref_mut_r0_i32 = tys.intern(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Exclusive,
        pointee: i32_ty,
    });
    let err_adt = tys.add_adt(AdtDef {
        name: "DynCreateError".into(),
        kind: AdtKind::Enum,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "OutOfMemory".into(),
            fields: vec![],
        }],
        user_drop: false,
    });
    let empty_args = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![],
    });
    let err_ty = tys.intern(TyKind::Adt {
        adt: err_adt,
        args: empty_args,
    });
    let dyn_id = tys.add_dyn(DynDef {
        name: "Change".into(),
        slots: vec![DynSlotSig {
            name: "step".into(),
            receiver: Some(Mutability::Exclusive),
            params: vec![],
            result: i32_ty,
        }],
    });
    let forged_owned_dyn = tys.intern(TyKind::OwnedDyn {
        def: dyn_id,
        regions: Vec::new(),
        context: None,
    });
    let result_ty = tys.intern(TyKind::ErrorUnion {
        ok: forged_owned_dyn,
        err: err_ty,
    });
    let prov_adt = tys.add_adt(AdtDef {
        name: "Provider".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 1,
        variants: vec![VariantDef {
            name: "Provider".into(),
            fields: vec![FieldDef {
                name: "pointer".into(),
                ty: ref_mut_r0_i32,
            }],
        }],
        user_drop: false,
    });
    let r0_args = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![Region::Param(0)],
    });
    let prov_ty = tys.intern(TyKind::Adt {
        adt: prov_adt,
        args: r0_args,
    });
    let recv_ty = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: prov_ty,
    });

    // Slot impl function (fn 0)
    let slot_fn = Function {
        name: "Provider::step".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 1,
            params: vec![recv_ty],
            result: i32_ty,
            role: FnRole::DynSlotImpl {
                dyn_id,
                concrete_ty: prov_ty,
                slot: 0,
            },
            resume_mode: zil_ir::body::ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: recv_ty,
                name: Some("self".into()),
            }],
            captures: vec![],
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };

    // Forged creator function (fn 1)
    let forged_fn = Function {
        name: "forged_create".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 1,
            params: vec![prov_ty],
            result: result_ty,
            role: FnRole::Normal,
            resume_mode: zil_ir::body::ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: prov_ty,
                    name: Some("p".into()),
                },
                LocalDecl {
                    ty: result_ty,
                    name: Some("ret".into()),
                },
            ],
            captures: vec![],
            closure: None,
            result: result_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![
                Block {
                    instructions: vec![],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::OwnedDynCreate),
                        args: vec![Operand::Move(Place::local(Local(0)))],
                        dest: Place::local(Local(1)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: zil_base::Span::default(),
                },
                Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Move(Place::local(Local(1))),
                    },
                    terminator_span: zil_base::Span::default(),
                },
            ],
        },
    };

    let module = Module {
        tys,
        functions: vec![slot_fn, forged_fn],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![DynImpl {
            dyn_id,
            concrete_ty: prov_ty,
            methods: vec![FnId(0)],
        }],
    };

    // Round-trip through binary Wire encoding to verify wire format preserves
    // OwnedDyn.context and independent solver + zil-check reject the forgery.
    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire decode forged module");
    let solved = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 });
    assert!(
        solved.is_err(),
        "solver must reject raw/wire OwnedDynCreate erasing borrowed Provider<'a> into context-free OwnedDyn"
    );
    let valid_cert = zil_ir::body::Certificate {
        functions: vec![
            zil_ir::body::FunctionCertificate {
                function: FnId(0),
                invariants: Vec::new(),
            },
            zil_ir::body::FunctionCertificate {
                function: FnId(1),
                invariants: Vec::new(),
            },
        ],
    };
    let checked = zil_check::check_module(
        decoded,
        &valid_cert,
        zil_check::CheckBudget { steps: 1_000_000 },
    );
    assert!(
        checked.is_err(),
        "independent zil-check must reject raw/wire OwnedDynCreate erasing borrowed Provider<'a> into context-free OwnedDyn"
    );
}

#[test]
fn coborrowed_store_owned_dyn_push_pop_and_safety_negatives() {
    let dir = temp_dir("store-owned-dyn");

    // 1. Positive native consumer: Store<OwnedDyn<dyn Read, Context<'a>>> with push,
    // store_at read, store_pop (releasing Store lock while keeping provider loan),
    // second push, and deterministic LIFO Drop sequence across all 4 backend/opt configs.
    let pos_src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource DropTag { code: i32 }
impl Drop for DropTag {
    fn drop(self: Self) {
        unsafe { putchar(.code) }
    }
}

struct Context<life a> { leaf: &'a i32 }

package Provider<life a> {
    manages tag: DropTag,
    pointer: &'a i32,
}

trait Read {
    fn read(self: &Self) -> i32;
}

impl<life a> Read for Provider<'a> {
    fn read(self: &Self) -> i32 {
        return *self.pointer
    }
}

fn run<life a>(x: &'a i32, y: &'a i32) -> i32 {
    let items: Store<OwnedDyn<dyn Read, Context<'a>>> = store_new();
    if !store_reserve(items, 2) { return 90 }
    match owned_dyn::<dyn Read, Context<'a>>(Provider::<'a> {
        tag: DropTag { code: 65 },
        pointer: x,
    }) {
        .err(_) => { return 91 }
        .ok(o1) => {
            match store_push(items, o1) {
                .err(_) => { return 92 }
                .ok(_) => {}
            }
            let first = store_at(items, 0).read();
            // Pop o1: releases Store borrow so we can push o2, while o1 still borrows x.
            match store_pop(items) {
                .err(_) => { return 93 }
                .ok(popped) => {
                    match owned_dyn::<dyn Read, Context<'a>>(Provider::<'a> {
                        tag: DropTag { code: 66 },
                        pointer: y,
                    }) {
                        .err(_) => { return 94 }
                        .ok(o2) => {
                            match store_push(items, o2) {
                                .err(_) => { return 95 }
                                .ok(_) => {}
                            }
                            let second = store_at(items, 0).read();
                            let again = popped.read();
                            return (first + second + again) - (10 + 32 + 10)
                        }
                    }
                }
            }
        }
    }
}

fn main() -> i32 {
    let a: i32 = 10;
    let b: i32 = 32;
    return run(&a, &b)
}
"#;
    let pos_entry = write_source(&dir, "coborrowed_store_pop.zil", pos_src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &pos_entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(
        String::from_utf8(stdout).expect("utf8 stdout"),
        "AB",
        "popped OwnedDyn ('A') and Store element ('B') must each drop exactly once"
    );

    // 2. Negative: mutating provider referent while Stored inside Store<OwnedDyn<..., Context<'a>>>
    // MUST be rejected (LoanConflict).
    check_rejected(
        &dir,
        "store_owned_dyn_mutate_provider_conflict",
        r#"
struct Context<life a> { leaf: &'a i32 }
struct Provider<life a> { pointer: &'a i32 }
trait Read { fn read(self: &Self) -> i32; }
impl<life a> Read for Provider<'a> {
    fn read(self: &Self) -> i32 { return *self.pointer }
}
fn main() -> i32 {
    var data: i32 = 42;
    let items: Store<OwnedDyn<dyn Read, Context<'a>>> = store_new();
    if !store_reserve(items, 1) { return 1 }
    match owned_dyn::<dyn Read, Context<'a>>(Provider::<'a> { pointer: &data }) {
        .err(_) => { return 2 }
        .ok(owner) => {
            match store_push(items, owner) {
                .err(_) => { return 3 }
                .ok(_) => {}
            }
            data = 99;
            return store_at(items, 0).read()
        }
    }
}
"#,
        "E-SAFETY",
    );

    // 3. Negative: mutating provider referent after StorePop while popped owner is still live
    // MUST be rejected (LoanConflict).
    check_rejected(
        &dir,
        "store_owned_dyn_pop_mutate_provider_conflict",
        r#"
struct Context<life a> { leaf: &'a i32 }
struct Provider<life a> { pointer: &'a i32 }
trait Read { fn read(self: &Self) -> i32; }
impl<life a> Read for Provider<'a> {
    fn read(self: &Self) -> i32 { return *self.pointer }
}
fn main() -> i32 {
    var data: i32 = 42;
    let items: Store<OwnedDyn<dyn Read, Context<'a>>> = store_new();
    if !store_reserve(items, 1) { return 1 }
    match owned_dyn::<dyn Read, Context<'a>>(Provider::<'a> { pointer: &data }) {
        .err(_) => { return 2 }
        .ok(owner) => {
            match store_push(items, owner) {
                .err(_) => { return 3 }
                .ok(_) => {}
            }
            match store_pop(items) {
                .err(_) => { return 4 }
                .ok(popped) => {
                    data = 99;
                    return popped.read()
                }
            }
        }
    }
}
"#,
        "E-SAFETY",
    );

    // 4. Negative: erasing an exclusive-ref provider (&'a mut i32) into a shared-only
    // Context<'a> { leaf: &'a i32 } MUST be rejected by opspec/Solver/Verify.
    check_rejected(
        &dir,
        "exclusive_provider_into_shared_context_rejected",
        r#"
struct SharedCtx<life a> { leaf: &'a i32 }
struct MutProvider<life a> { pointer: &'a mut i32 }
trait Bump { fn bump(self: &mut Self) -> i32; }
impl<life a> Bump for MutProvider<'a> {
    fn bump(self: &mut Self) -> i32 {
        *self.pointer += 1;
        return *self.pointer
    }
}
fn bad<life a>(x: &'a mut i32) -> i32 {
    match owned_dyn::<dyn Bump, SharedCtx<'a>>(MutProvider::<'a> { pointer: x }) {
        .ok(owner) => {
            var current = owner;
            return current.bump()
        }
        .err(_) => { return 0 }
    }
}
fn main() -> i32 {
    var v: i32 = 1;
    return bad(&mut v)
}
"#,
        "E-SAFETY",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_valid_cert_forged_exclusive_provider_in_shared_context_rejected() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, Certificate, ConstValue, DynDef, DynImpl,
        DynSlotSig, FieldDef, FnId, FnRole, FnSig, Function, FunctionCertificate, GenericArgs,
        IntTy, IntrinsicOp, Local, LocalDecl, Module, Mutability, Operand, ParamEnv, Place, Region,
        Terminator, TyKind, TyTable, VariantDef, decode_module, encode_module,
    };

    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let mut_ref_a_i32 = tys.intern(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Exclusive,
        pointee: i32_ty,
    });
    let shared_ref_a_i32 = tys.intern(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Shared,
        pointee: i32_ty,
    });
    let prov_adt = tys.add_adt(AdtDef {
        name: "MutProvider".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 1,
        user_drop: false,
        variants: vec![VariantDef {
            name: "MutProvider".into(),
            fields: vec![FieldDef {
                name: "pointer".into(),
                ty: mut_ref_a_i32,
            }],
        }],
    });
    let ctx_adt = tys.add_adt(AdtDef {
        name: "SharedCtx".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 1,
        user_drop: false,
        variants: vec![VariantDef {
            name: "SharedCtx".into(),
            fields: vec![FieldDef {
                name: "leaf".into(),
                ty: shared_ref_a_i32,
            }],
        }],
    });
    let err_adt = tys.add_adt(AdtDef {
        name: "DynCreateError".into(),
        kind: AdtKind::Enum,
        type_params: 0,
        region_params: 0,
        user_drop: false,
        variants: vec![VariantDef {
            name: "OutOfMemory".into(),
            fields: vec![],
        }],
    });
    let reg_args = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![Region::Param(0)],
    });
    let empty_args = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![],
    });
    let prov_ty = tys.intern(TyKind::Adt {
        adt: prov_adt,
        args: reg_args,
    });
    let ctx_ty = tys.intern(TyKind::Adt {
        adt: ctx_adt,
        args: reg_args,
    });
    let err_ty = tys.intern(TyKind::Adt {
        adt: err_adt,
        args: empty_args,
    });
    let dyn_id = tys.add_dyn(DynDef {
        name: "Bump".into(),
        slots: vec![DynSlotSig {
            name: "bump".into(),
            receiver: Some(Mutability::Exclusive),
            params: vec![],
            result: i32_ty,
        }],
    });
    let forged_dyn_ty = tys.intern(TyKind::OwnedDyn {
        def: dyn_id,
        regions: Vec::new(),
        context: Some(ctx_ty),
    });
    let result_ty = tys.intern(TyKind::ErrorUnion {
        ok: forged_dyn_ty,
        err: err_ty,
    });
    let recv_ty = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: prov_ty,
    });

    let slot_fn = Function {
        name: "MutProvider::bump".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 1,
            params: vec![recv_ty],
            result: i32_ty,
            role: FnRole::DynSlotImpl {
                dyn_id,
                concrete_ty: prov_ty,
                slot: 0,
            },
            resume_mode: zil_ir::body::ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: recv_ty,
                name: Some("self".into()),
            }],
            captures: vec![],
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: zil_base::Span::default(),
            }],
        },
    };

    let forged_fn = Function {
        name: "forged_exclusive_in_shared_ctx".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 1,
            params: vec![prov_ty],
            result: result_ty,
            role: FnRole::Normal,
            resume_mode: zil_ir::body::ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: prov_ty,
                    name: Some("p".into()),
                },
                LocalDecl {
                    ty: result_ty,
                    name: Some("ret".into()),
                },
            ],
            captures: vec![],
            closure: None,
            result: result_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![
                Block {
                    instructions: vec![],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::OwnedDynCreate),
                        args: vec![Operand::Move(Place::local(Local(0)))],
                        dest: Place::local(Local(1)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: zil_base::Span::default(),
                },
                Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Move(Place::local(Local(1))),
                    },
                    terminator_span: zil_base::Span::default(),
                },
            ],
        },
    };

    let module = Module {
        tys,
        functions: vec![slot_fn, forged_fn],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![DynImpl {
            dyn_id,
            concrete_ty: prov_ty,
            methods: vec![FnId(0)],
        }],
    };

    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire decode forged exclusive-in-shared module");
    assert!(
        zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 }).is_err(),
        "solver must reject OwnedDynCreate erasing &mut T provider into shared-only Context"
    );
    let valid_cert = Certificate {
        functions: vec![
            FunctionCertificate {
                function: FnId(0),
                invariants: Vec::new(),
            },
            FunctionCertificate {
                function: FnId(1),
                invariants: Vec::new(),
            },
        ],
    };
    assert!(
        zil_check::check_module(
            decoded,
            &valid_cert,
            zil_check::CheckBudget { steps: 1_000_000 }
        )
        .is_err(),
        "independent zil-check must reject OwnedDynCreate erasing &mut T provider into shared-only Context"
    );
}

#[test]
fn dyn_trait_lifetime_return_multi_provider_and_after_owner_drop_native() {
    let dir = temp_dir("dyn-lifetime-return-positive");
    let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource DropTag { code: i32 }
impl Drop for DropTag {
    fn drop(self: Self) {
        unsafe { putchar(self.code) }
    }
}

struct Context<life a> { text: &'a [u8] }
package LeftProvider<life a> { text: &'a [u8]; manages tag: DropTag }
package RightProvider<life a> { prefix: i32; text: &'a [u8]; manages tag: DropTag }

trait Text<life a> {
    fn text(self: &Self) -> &'a [u8];
}

impl<life a> Text<'a> for LeftProvider<'a> {
    fn text(self: &Self) -> &'a [u8] { return self.text }
}

impl<life a> Text<'a> for RightProvider<'a> {
    fn text(self: &Self) -> &'a [u8] { return self.text }
}

fn make_provider<life a>(
    pick_left: bool,
    left: &'a [u8],
    right: &'a [u8],
) -> DynCreateError!OwnedDyn<dyn Text<'a>, Context<'a>> {
    if pick_left {
        return owned_dyn::<dyn Text<'a>, Context<'a>>(LeftProvider::<'a> {
            text: left,
            tag: DropTag { code: 76 },
        });
    }
    return owned_dyn::<dyn Text<'a>, Context<'a>>(RightProvider::<'a> {
        prefix: 7,
        text: right,
        tag: DropTag { code: 82 },
    });
}

fn extract_after_drop<life a>(pick_left: bool, left: &'a [u8], right: &'a [u8]) -> &'a [u8] {
    match make_provider(pick_left, left, right) {
        .err(_) => { return left },
        .ok(owner) => { return owner.text() }
    }
}

fn pick_second_region<life x, life y>(_first: &'x [u8], second: &'y [u8]) -> &'y [u8] {
    match make_provider(false, second, second) {
        .err(_) => { return second },
        .ok(owner) => { return owner.text() }
    }
}

fn forward_owner_ref<life a>(owner: &OwnedDyn<dyn Text<'a>, Context<'a>>) -> &'a [u8] {
    return owner.text()
}

fn forward_dyn_ref<life a>(v: &dyn Text<'a>) -> &'a [u8] {
    return v.text()
}

fn main() -> i32 {
    let a = extract_after_drop(true, "alpha", "omega");
    let b = extract_after_drop(false, "alpha", "omega");
    let c = pick_second_region("ignored", "zeta");
    var d_byte: i32 = 0;
    match make_provider(true, "beta", "gamma") {
        .err(_) => { return 99 },
        .ok(owner) => {
            let s = forward_owner_ref(&owner);
            d_byte = i32(s[0]);
        }
    }
    if a[0] == 97u8 && b[0] == 111u8 && c[0] == 122u8 && d_byte == 98 {
        unsafe { putchar(79) }
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    // Drop sequence:
    // 1. LeftProvider ('L') drops when extract_after_drop(true, ...) returns &'a [u8]
    // 2. RightProvider ('R') drops when extract_after_drop(false, ...) returns &'a [u8]
    // 3. RightProvider ('R') drops when pick_second_region(...) returns &'y [u8]
    // 4. LeftProvider ('L') drops at the end of match arm after forward_owner_ref
    // 5. Final check reads all returned slices and prints 'O'
    assert_eq!(stdout.as_slice(), b"LRRLO");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dyn_trait_lifetime_return_safety_negatives_rejected() {
    let dir = temp_dir("dyn-lifetime-return-negatives");

    // 1. Returning a &self-tied borrow (&i32) after local OwnedDyn drops MUST be rejected
    //    even when coerced into a &'a i32 local binding.
    check_rejected(
        &dir,
        "self_tied_borrow_after_owner_drop",
        r#"
struct Context<life a> { text: &'a [u8] }
struct Provider { local: i32 }
trait LocalView { fn view(self: &Self) -> &i32; }
impl LocalView for Provider {
    fn view(self: &Self) -> &i32 { return &self.local }
}
fn bad<life a>(seed: &'a [u8]) -> &'a i32 {
    match owned_dyn::<dyn LocalView, Context<'a>>(Provider { local: 42 }) {
        .err(_) => { return bad(seed) },
        .ok(owner) => {
            let p: &'a i32 = owner.view();
            return p
        }
    }
}
fn main() -> i32 { let p = bad("abc"); return *p }
"#,
        "E-SAFETY",
    );

    // 2. Impl claiming to return &'a i32 from &self.local MUST be rejected.
    check_rejected(
        &dir,
        "impl_returns_self_field_as_trait_lifetime",
        r#"
struct Context<life a> { text: &'a [u8] }
struct Provider<life a> { text: &'a [u8], local: i32 }
trait Bad<life a> { fn get_local(self: &Self) -> &'a i32; }
impl<life a> Bad<'a> for Provider<'a> {
    fn get_local(self: &Self) -> &'a i32 { return &self.local }
}
fn bad<life a>(seed: &'a [u8]) -> &'a i32 {
    match owned_dyn::<dyn Bad<'a>, Context<'a>>(Provider::<'a> { text: seed, local: 42 }) {
        .err(_) => { return bad(seed) },
        .ok(owner) => { return owner.get_local() }
    }
}
fn main() -> i32 { let p = bad("abc"); return *p }
"#,
        "E-SAFETY",
    );

    // 3. Returning &'b [u8] from OwnedDyn<dyn Text<'b>, Context<'b>> when &'a [u8]
    //    is required ('b does not outlive 'a) MUST be rejected.
    check_rejected(
        &dir,
        "owned_dyn_wrong_trait_lifetime_return",
        r#"
struct Context<life a> { text: &'a [u8] }
struct Provider<life a> { text: &'a [u8] }
trait Text<life a> { fn text(self: &Self) -> &'a [u8]; }
impl<life a> Text<'a> for Provider<'a> {
    fn text(self: &Self) -> &'a [u8] { return self.text }
}
fn bad<life a, life b>(first: &'a [u8], second: &'b [u8]) -> &'a [u8] {
    match owned_dyn::<dyn Text<'b>, Context<'b>>(Provider::<'b> { text: second }) {
        .err(_) => { return first },
        .ok(owner) => { return owner.text() }
    }
}
fn main() -> i32 { let t = bad("xyz", "abc"); return i32(t[0]) - 97 }
"#,
        "E-TYPE",
    );

    // 4. Returning &'b [u8] from &dyn Text<'b> when &'a [u8] is required MUST be rejected.
    check_rejected(
        &dir,
        "ref_dyn_wrong_trait_lifetime_return",
        r#"
trait Text<life a> { fn text(self: &Self) -> &'a [u8]; }
fn bad<life a, life b>(_first: &'a [u8], v: &dyn Text<'b>) -> &'a [u8] {
    return v.text()
}
fn main() -> i32 { return 0 }
"#,
        "E-TYPE",
    );

    // 5. Mutating the external referent after owner drops while returned &'a i32
    //    is still live MUST be rejected (LoanConflict).
    check_rejected(
        &dir,
        "mutate_external_referent_while_returned_trait_life_ref_live",
        r#"
struct Context<life a> { val: &'a i32 }
struct Provider<life a> { val: &'a i32 }
trait View<life a> { fn view(self: &Self) -> &'a i32; }
impl<life a> View<'a> for Provider<'a> {
    fn view(self: &Self) -> &'a i32 { return self.val }
}
fn create<life a>(v: &'a i32) -> DynCreateError!OwnedDyn<dyn View<'a>, Context<'a>> {
    return owned_dyn::<dyn View<'a>, Context<'a>>(Provider::<'a> { val: v })
}
fn extract<life a>(v: &'a i32) -> &'a i32 {
    match create(v) {
        .err(_) => { return v },
        .ok(owner) => { return owner.view() }
    }
}
fn main() -> i32 {
    var data: i32 = 10;
    let r = extract(&data);
    data = 99;
    return *r
}
"#,
        "E-SAFETY",
    );

    // 6. Local stack buffer escaping via owner.text() after owner drops MUST be rejected.
    check_rejected(
        &dir,
        "local_buffer_escapes_via_trait_lifetime_return",
        r#"
struct Context<life a> { val: &'a i32 }
struct Provider<life a> { val: &'a i32 }
trait View<life a> { fn view(self: &Self) -> &'a i32; }
impl<life a> View<'a> for Provider<'a> {
    fn view(self: &Self) -> &'a i32 { return self.val }
}
fn create<life a>(v: &'a i32) -> DynCreateError!OwnedDyn<dyn View<'a>, Context<'a>> {
    return owned_dyn::<dyn View<'a>, Context<'a>>(Provider::<'a> { val: v })
}
fn escape() -> &i32 {
    let local: i32 = 42;
    match create(&local) {
        .err(_) => { return escape() },
        .ok(owner) => { return owner.view() }
    }
}
fn main() -> i32 { let r = escape(); return *r }
"#,
        "E-SAFETY",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_forged_dyn_lifetime_return_attacks_rejected() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, Certificate, ConstValue, DynDef, DynImpl,
        DynSlotSig, FieldDef, FieldIdx, FieldRef, FnId, FnRole, FnSig, Function,
        FunctionCertificate, GenericArgs, Instruction, InstructionKind, IntTy, Local, LocalDecl,
        Module, Mutability, Operand, ParamEnv, Place, Projection, Region, Rvalue, Terminator,
        TyKind, TyTable, VariantDef, VariantIdx, decode_module, encode_module,
    };

    let build_attack = |slot_result_region: Region,
                        impl_result_region: Region,
                        impl_recv_region: Region,
                        caller_dest_region: Region|
     -> Module {
        let mut tys = TyTable::new();
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let ref_a_i32 = tys.intern(TyKind::Ref {
            region: Region::Param(0),
            mutability: Mutability::Shared,
            pointee: i32_ty,
        });
        let slot_ret_ty = tys.intern(TyKind::Ref {
            region: slot_result_region,
            mutability: Mutability::Shared,
            pointee: i32_ty,
        });
        let impl_ret_ty = tys.intern(TyKind::Ref {
            region: impl_result_region,
            mutability: Mutability::Shared,
            pointee: i32_ty,
        });
        let caller_dest_ty = tys.intern(TyKind::Ref {
            region: caller_dest_region,
            mutability: Mutability::Shared,
            pointee: i32_ty,
        });
        let prov_adt = tys.add_adt(AdtDef {
            name: "Provider".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            user_drop: false,
            variants: vec![VariantDef {
                name: "Provider".into(),
                fields: vec![FieldDef {
                    name: "local".into(),
                    ty: i32_ty,
                }],
            }],
        });
        let ctx_adt = tys.add_adt(AdtDef {
            name: "Context".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 1,
            user_drop: false,
            variants: vec![VariantDef {
                name: "Context".into(),
                fields: vec![FieldDef {
                    name: "leaf".into(),
                    ty: ref_a_i32,
                }],
            }],
        });
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let reg_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![Region::Param(0)],
        });
        let prov_ty = tys.intern(TyKind::Adt {
            adt: prov_adt,
            args: empty_args,
        });
        let ctx_ty = tys.intern(TyKind::Adt {
            adt: ctx_adt,
            args: reg_args,
        });
        let dyn_id = tys.add_dyn(DynDef {
            name: "Bad".into(),
            slots: vec![DynSlotSig {
                name: "get".into(),
                receiver: Some(Mutability::Shared),
                params: vec![],
                result: slot_ret_ty,
            }],
        });
        let dyn_regions = match slot_result_region {
            Region::Param(_) => vec![Region::Param(0)],
            Region::Static | Region::Var(_) => vec![],
        };
        let owned_dyn_ty = tys.intern(TyKind::OwnedDyn {
            def: dyn_id,
            regions: dyn_regions,
            context: Some(ctx_ty),
        });
        let ref_owned_dyn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: owned_dyn_ty,
        });
        let impl_recv_ty = tys.intern(TyKind::Ref {
            region: impl_recv_region,
            mutability: Mutability::Shared,
            pointee: prov_ty,
        });

        // Slot impl: fn Provider::get(self: &Provider) -> impl_ret_ty { return &self.local }
        let mut local_field = Place::local(Local(0));
        local_field.projections.push(Projection::Deref);
        local_field.projections.push(Projection::Field(FieldRef {
            adt: prov_adt,
            variant: VariantIdx(0),
            field: FieldIdx(0),
            args: empty_args,
        }));
        let slot_fn = Function {
            name: "Provider::get".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 1,
                params: vec![impl_recv_ty],
                result: impl_ret_ty,
                role: FnRole::DynSlotImpl {
                    dyn_id,
                    concrete_ty: prov_ty,
                    slot: 0,
                },
                resume_mode: zil_ir::body::ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: impl_recv_ty,
                        name: Some("self".into()),
                    },
                    LocalDecl {
                        ty: impl_ret_ty,
                        name: Some("ret".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: impl_ret_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(1)),
                            value: Rvalue::Ref {
                                region: impl_result_region,
                                mutability: Mutability::Shared,
                                place: local_field,
                            },
                        },
                        span: zil_base::Span::default(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Copy(Place::local(Local(1))),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };

        // Caller: fn forged<life a>(owner: OwnedDyn<dyn Bad, Context<'a>>) -> &'a i32
        //   r = &owner
        //   out = DynSlot(r, slot 0)
        //   Drop(owner)
        //   return out
        let caller_fn = Function {
            name: "forged_caller".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 1,
                params: vec![owned_dyn_ty],
                result: caller_dest_ty,
                role: FnRole::Normal,
                resume_mode: zil_ir::body::ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: owned_dyn_ty,
                        name: Some("owner".into()),
                    },
                    LocalDecl {
                        ty: ref_owned_dyn,
                        name: Some("ref_owner".into()),
                    },
                    LocalDecl {
                        ty: caller_dest_ty,
                        name: Some("out".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: caller_dest_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::Ref {
                                    region: Region::Static,
                                    mutability: Mutability::Shared,
                                    place: Place::local(Local(0)),
                                },
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Call {
                            callee: Callee::DynSlot {
                                object: Operand::Copy(Place::local(Local(1))),
                                slot: 0,
                            },
                            args: vec![],
                            dest: Place::local(Local(2)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Drop {
                                place: Place::local(Local(0)),
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Return {
                            value: Operand::Copy(Place::local(Local(2))),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };

        let _ = ConstValue::Unit;
        Module {
            tys,
            functions: vec![slot_fn, caller_fn],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![DynImpl {
                dyn_id,
                concrete_ty: prov_ty,
                methods: vec![FnId(0)],
            }],
        }
    };

    let empty_cert = Certificate {
        functions: vec![
            FunctionCertificate {
                function: FnId(0),
                invariants: Vec::new(),
            },
            FunctionCertificate {
                function: FnId(1),
                invariants: Vec::new(),
            },
        ],
    };

    // Attack A: Slot returns &self (Region::Static), impl returns &self.local, caller
    // forges dest type &'a i32 (Region::Param(0)) and drops owner before returning.
    let attack_a = decode_module(&encode_module(&build_attack(
        Region::Static,
        Region::Static,
        Region::Static,
        Region::Param(0),
    )))
    .expect("wire decode attack A");
    assert!(
        zil_solve::solve_module(&attack_a, zil_solve::Budget { steps: 1_000_000 }).is_err(),
        "solver must reject Attack A (caller forging &'a dest on &self-borrowing DynSlot)"
    );
    assert!(
        zil_check::check_module(
            attack_a,
            &empty_cert,
            zil_check::CheckBudget { steps: 1_000_000 }
        )
        .is_err(),
        "independent zil-check must reject Attack A even with forged certificate"
    );

    // Attack B: Slot claims fixed Region::Param(0), but DynImpl erases return to Region::Static
    // to return &self.local.
    let attack_b = decode_module(&encode_module(&build_attack(
        Region::Param(0),
        Region::Static,
        Region::Static,
        Region::Param(0),
    )))
    .expect("wire decode attack B");
    assert!(
        zil_solve::solve_module(&attack_b, zil_solve::Budget { steps: 1_000_000 }).is_err(),
        "solver must reject Attack B (DynImpl erasing fixed trait-lifetime return contract)"
    );
    assert!(
        zil_check::check_module(
            attack_b,
            &empty_cert,
            zil_check::CheckBudget { steps: 1_000_000 }
        )
        .is_err(),
        "independent zil-check must reject Attack B"
    );

    // Attack C: Slot claims fixed Region::Param(0), and DynImpl forges self: &'a Provider
    // (Region::Param(0)) to smuggle &self.local out as &'a i32.
    let attack_c = decode_module(&encode_module(&build_attack(
        Region::Param(0),
        Region::Param(0),
        Region::Param(0),
        Region::Param(0),
    )))
    .expect("wire decode attack C");
    assert!(
        zil_solve::solve_module(&attack_c, zil_solve::Budget { steps: 1_000_000 }).is_err(),
        "solver must reject Attack C (DynImpl forging self: &'a Self to alias fixed return)"
    );
    assert!(
        zil_check::check_module(
            attack_c,
            &empty_cert,
            zil_check::CheckBudget { steps: 1_000_000 }
        )
        .is_err(),
        "independent zil-check must reject Attack C"
    );
}
