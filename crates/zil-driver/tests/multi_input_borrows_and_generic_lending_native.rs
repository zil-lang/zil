//! Dual-backend (`clang-o0`, `clang-o2`, and `llvmapi-o0`/`llvmapi-o2` when
//! `llvm-api` is enabled) empirical tests for multi-input borrowed returns,
//! multi-lifetime structs/enums/error-unions, and generic owning-container
//! lending, paired with nearest safety-negative controls.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zil-multi-borrow-lending-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
}

fn build_and_run_dual(dir: &Path, entry: &Path) -> i32 {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }

    let mut expected_code = None;
    for (label, backend, opt) in configs {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
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
        if let Some(prev) = expected_code {
            assert_eq!(code, prev, "exit code mismatch on {label}");
        } else {
            expected_code = Some(code);
        }
    }
    expected_code.expect("at least one backend ran")
}

fn assert_safe_reject(dir: &Path, name: &str, src: &str) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected SafeReject for `{name}`, got {:?}: {:?}",
        report.status,
        report.diagnostics
    );
}

#[test]
fn multi_input_shared_and_selective_lifetime_returns_native() {
    let dir = temp_dir("shared-and-selective");
    let src = r#"
package Holder<life a> {
    manages buf: ByteBuffer;
    tag: &'a i32;
}

fn make_holder<life a>(tag: &'a i32, byte: u8) -> Holder<'a> {
    let b = bytes_new()
    let _ = bytes_reserve(b, 4)
    let _ = bytes_push(b, byte)
    return Holder::<'a> { buf: give b, tag: tag }
}

fn select_shared<life a>(a: &'a i32, b: &'a i32, take_a: bool) -> &'a i32 {
    if take_a { return a }
    return b
}

fn choose_generic<life a, T: Value>(a: &'a T, b: &'a T, take_a: bool) -> &'a T {
    if take_a { return a }
    return b
}

fn pick_slice<life a>(a: &'a [u8], b: &'a [u8], take_a: bool) -> &'a [u8] {
    if take_a { return a }
    return b
}

fn pick_first<life a, life b>(a: &'a i32, _b: &'b i32) -> &'a i32 {
    return a
}

fn choose_holder<life a, life b>(
    a: &'a Holder<'b>,
    b: &'a Holder<'b>,
    take_a: bool
) -> &'a Holder<'b> {
    if take_a { return a }
    return b
}

fn main() -> i32 {
    let x: i32 = 10
    var y: i32 = 20

    let r1 = select_shared(&x, &y, true)
    let r2 = select_shared(&x, &y, false)
    let g1 = choose_generic(&x, &y, false)
    let s1 = pick_slice("hello", "world!", false)
    let sum1 = *r1 + *r2 + *g1 + i32(s1.len())

    // Selective lifetime: `r_only_x` depends only on `x` ('a), so `y` ('b) can be mutated while `r_only_x` is live.
    let r_only_x = pick_first(&x, &y)
    y += 5
    let sum2 = *r_only_x + y

    // Multi-input shared return of owning Package holding an external borrow.
    let h1 = make_holder(&x, 65u8)
    let h2 = make_holder(&x, 66u8)
    let picked = choose_holder(&h1, &h2, false)
    let pv = bytes_view(picked.buf)
    let sum3 = *picked.tag + i32(pv[0])

    // sum1 = 10 + 20 + 20 + 6 = 56
    // sum2 = 10 + 25 = 35
    // sum3 = 10 + 66 = 76
    // total = 167
    return sum1 + sum2 + sum3
}
"#;
    let entry = write_source(&dir, "pos.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 167);

    // Negative 1: mutating either input (`y`) while `select_shared(&x, &y, true)` is live must fail.
    assert_safe_reject(
        &dir,
        "neg_mutate_second_while_shared_return_live",
        r#"
fn select_shared<life a>(a: &'a i32, b: &'a i32, take_a: bool) -> &'a i32 {
    if take_a { return a }
    return b
}

fn main() -> i32 {
    let x: i32 = 10
    var y: i32 = 20
    let r = select_shared(&x, &y, true)
    y = 99
    return *r
}
"#,
    );

    // Negative 2: returning `'b` where `'a` is declared must fail.
    assert_safe_reject(
        &dir,
        "neg_wrong_lifetime_param_return",
        r#"
fn pick_first<life a, life b>(_a: &'a i32, b: &'b i32) -> &'a i32 {
    return b
}

fn main() -> i32 {
    let x: i32 = 1
    let y: i32 = 2
    return *pick_first(&x, &y)
}
"#,
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn multi_input_exclusive_return_and_pair_swap_native() {
    let dir = temp_dir("mut-and-pair");
    let src = r#"
struct Pair<life a, life b> {
    first: &'a i32;
    second: &'b i32;
}

fn select_mut<life a>(a: &'a mut i32, b: &'a mut i32, take_a: bool) -> &'a mut i32 {
    if take_a { return a }
    return b
}

fn swap_pair<life a, life b>(p: Pair<'a, 'b>) -> Pair<'b, 'a> {
    return Pair::<'b, 'a> { first: p.second, second: p.first }
}

fn main() -> i32 {
    var a: i32 = 10
    var b: i32 = 30
    let m = select_mut(&mut a, &mut b, false)
    *m += 7
    let after_mut = a + b

    var x: i32 = 5
    var y: i32 = 9
    let p = Pair::<'_, '_> { first: &x, second: &y }
    let swapped = swap_pair(p)
    let first_only = swapped.first
    // `first_only` borrows from `y` ('b), so `x` ('a) can be mutated once `swapped.second` is unused.
    let v_y = *first_only
    x += 3
    // after_mut = 10 + 37 = 47, v_y = 9, x = 8 => 64
    return after_mut + v_y + x
}
"#;
    let entry = write_source(&dir, "pos.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 64);

    // Negative 1: reading `a` while `select_mut(&mut a, &mut b, false)` is still live must fail.
    assert_safe_reject(
        &dir,
        "neg_read_input_while_select_mut_live",
        r#"
fn select_mut<life a>(a: &'a mut i32, b: &'a mut i32, take_a: bool) -> &'a mut i32 {
    if take_a { return a }
    return b
}

fn main() -> i32 {
    var a: i32 = 10
    var b: i32 = 20
    let m = select_mut(&mut a, &mut b, false)
    let leak = a
    *m += 1
    return leak + *m
}
"#,
    );

    // Negative 2: mutating `y` while `swapped.first` (which came from `y`) is live must fail.
    assert_safe_reject(
        &dir,
        "neg_mutate_swapped_origin",
        r#"
struct Pair<life a, life b> {
    first: &'a i32;
    second: &'b i32;
}

fn swap_pair<life a, life b>(p: Pair<'a, 'b>) -> Pair<'b, 'a> {
    return Pair::<'b, 'a> { first: p.second, second: p.first }
}

fn main() -> i32 {
    let x: i32 = 5
    var y: i32 = 9
    let p = Pair::<'_, '_> { first: &x, second: &y }
    let swapped = swap_pair(p)
    y = 99
    return *swapped.first
}
"#,
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn borrowed_option_enum_and_error_union_returns_native() {
    let dir = temp_dir("borrowed-sums");
    let src = r#"
enum Lookup<life a> {
    Hit(&'a [u8]),
    Miss,
}

enum PickErr {
    Empty,
}

fn find_positive<life a>(a: &'a i32, b: &'a i32) -> Option<&'a i32> {
    if *a > 0 { return Option::<&'a i32>.Some(a) }
    if *b > 0 { return Option::<&'a i32>.Some(b) }
    return Option::<&'a i32>.None()
}

fn lookup_pair<life a>(a: &'a [u8], b: &'a [u8], which: i32) -> Lookup<'a> {
    if which == 1 { return Lookup::<'a>.Hit(a) }
    if which == 2 { return Lookup::<'a>.Hit(b) }
    return Lookup::<'a>.Miss()
}

fn pick_nonempty<life a>(a: &'a [u8], b: &'a [u8]) -> PickErr!&'a [u8] {
    if a.len() > 0 { return a }
    if b.len() > 0 { return b }
    return PickErr.Empty
}

fn main() -> i32 {
    let neg: i32 = -5
    let pos: i32 = 17
    var score: i32 = 0
    match find_positive(&neg, &pos) {
        Option.Some(v) => { score += *v },
        Option.None() => { return 1 },
    }
    match lookup_pair("abc", "defg", 2) {
        Lookup.Hit(s) => { score += i32(s.len()) },
        Lookup.Miss() => { return 2 },
    }
    match pick_nonempty("", "hello") {
        .ok(s) => { score += i32(s.len()) },
        .err(_) => { return 3 },
    }
    match pick_nonempty("", "") {
        .ok(_) => { return 4 },
        .err(_) => { score += 10 },
    }
    // 17 + 4 + 5 + 10 = 36
    return score
}
"#;
    let entry = write_source(&dir, "pos.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 36);

    // Negative: mutating an input while a borrowed Option<&'a i32> returned from multi-input helper is live.
    assert_safe_reject(
        &dir,
        "neg_mutate_while_borrowed_option_live",
        r#"
fn find_positive<life a>(a: &'a i32, b: &'a i32) -> Option<&'a i32> {
    if *a > 0 { return Option::<&'a i32>.Some(a) }
    if *b > 0 { return Option::<&'a i32>.Some(b) }
    return Option::<&'a i32>.None()
}

fn main() -> i32 {
    var a: i32 = -1
    var b: i32 = 10
    let found = find_positive(&a, &b)
    b = 99
    match found {
        Option.Some(v) => { return *v },
        Option.None() => { return 0 },
    }
}
"#,
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn generic_lending_trait_and_owning_store_lending_native() {
    let dir = temp_dir("generic-lending");
    let src = r#"
trait ByteView {
    fn view_bytes(self: &Self) -> &[u8];
}

package OwnedDoc {
    manages buf: ByteBuffer;
}

impl ByteView for OwnedDoc {
    fn view_bytes(self: &Self) -> &[u8] {
        return bytes_view(self.buf)
    }
}

struct SliceView<life a> {
    data: &'a [u8];
}

impl<life a> ByteView for SliceView<'a> {
    fn view_bytes(self: &Self) -> &[u8] {
        return self.data
    }
}

fn make_doc(text: &[u8]) -> OwnedDoc {
    let b = bytes_new()
    let _ = bytes_reserve(b, text.len())
    var i: i64 = 0
    while i < text.len() {
        let _ = bytes_push(b, text[i])
        i += 1
    }
    return OwnedDoc { buf: give b }
}

fn pick_longer<life a, A: ByteView, B: ByteView>(a: &'a A, b: &'a B) -> &'a [u8] {
    let va = a.view_bytes()
    let vb = b.view_bytes()
    if va.len() >= vb.len() { return va }
    return vb
}

fn pick_store_elem<T: StoreElement>(items: Store<T>, first: i64, second: i64, use_first: bool) -> &T {
    if use_first { return store_at(items, first) }
    return store_at(items, second)
}

fn main() -> i32 {
    let doc = make_doc("zil-generic")
    let slice = SliceView::<'_> { data: "short" }
    let longer = pick_longer(&doc, &slice)
    let len1 = i32(longer.len())

    let docs: Store<OwnedDoc> = store_new()
    let _ = store_reserve(docs, 4)
    let _ = store_push(docs, make_doc("one"))
    let _ = store_push(docs, make_doc("twenty-two"))
    let picked = pick_store_elem(docs, 0, 1, false)
    let len2 = i32(picked.view_bytes().len())

    // 11 + 10 = 21
    return len1 + len2
}
"#;
    let entry = write_source(&dir, "pos.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 21);

    // Negative 1: mutating `Store<OwnedDoc>` via `store_push` while a reference borrowed via
    // generic `pick_store_elem` is still live must be rejected!
    assert_safe_reject(
        &dir,
        "neg_store_push_while_generic_lending_live",
        r#"
package OwnedDoc {
    manages buf: ByteBuffer;
}

fn make_doc() -> OwnedDoc {
    return OwnedDoc { buf: bytes_new() }
}

fn pick_store_elem<T: StoreElement>(items: Store<T>, first: i64, second: i64, use_first: bool) -> &T {
    if use_first { return store_at(items, first) }
    return store_at(items, second)
}

fn main() -> i32 {
    let docs: Store<OwnedDoc> = store_new()
    let _ = store_reserve(docs, 4)
    let _ = store_push(docs, make_doc())
    let _ = store_push(docs, make_doc())
    let picked = pick_store_elem(docs, 0, 1, false)
    let _ = store_push(docs, make_doc())
    return i32(bytes_len(picked.buf))
}
"#,
    );

    // Negative 2: returning a local slice through an impl<life a> unannotated `&[u8]` trait method must be rejected.
    assert_safe_reject(
        &dir,
        "neg_dangling_unannotated_lending_method",
        r#"
trait ByteView {
    fn view_bytes(self: &Self) -> &[u8];
}

struct SliceView<life a> {
    data: &'a [u8];
}

impl<life a> ByteView for SliceView<'a> {
    fn view_bytes(self: &Self) -> &[u8] {
        return self.data
    }
}

fn dangling_view() -> &[u8] {
    let local = [1u8, 2u8, 3u8]
    let sv = SliceView::<'_> { data: &local[..] }
    return sv.view_bytes()
}

fn main() -> i32 {
    return i32(dangling_view().len())
}
"#,
    );

    // Negative 3: mutating the backing array while an unannotated `&[u8]` view from `SliceView<'a>::view_bytes` is live must be rejected.
    assert_safe_reject(
        &dir,
        "neg_mutate_backing_while_unannotated_lending_live",
        r#"
trait ByteView {
    fn view_bytes(self: &Self) -> &[u8];
}

struct SliceView<life a> {
    data: &'a [u8];
}

impl<life a> ByteView for SliceView<'a> {
    fn view_bytes(self: &Self) -> &[u8] {
        return self.data
    }
}

fn main() -> i32 {
    var buf = [1u8, 2u8, 3u8]
    let sv = SliceView::<'_> { data: &buf[..] }
    let view = sv.view_bytes()
    buf[0] = 9u8
    return i32(view[0])
}
"#,
    );

    // Negative 4: mixed named + unannotated tuple return MUST still reject returning `'b` where `'a` is declared.
    assert_safe_reject(
        &dir,
        "neg_mixed_named_and_unannotated_region_sneak",
        r#"
fn sneak<life a, life b>(x: &'a i32, y: &'b i32) -> (&'a i32, &i32) {
    return (y, x)
}

fn main() -> i32 {
    let a: i32 = 1
    let b: i32 = 2
    let (p, _q) = sneak(&a, &b)
    return *p
}
"#,
    );

    // Negative 5: mutating backing array of `SliceView<'_>` while `pick_longer(&doc, &slice)` result is live must be rejected.
    assert_safe_reject(
        &dir,
        "neg_mutate_slice_backing_while_pick_longer_live",
        r#"
trait ByteView {
    fn view_bytes(self: &Self) -> &[u8];
}

package OwnedDoc {
    manages buf: ByteBuffer;
}

impl ByteView for OwnedDoc {
    fn view_bytes(self: &Self) -> &[u8] {
        return bytes_view(self.buf)
    }
}

struct SliceView<life a> {
    data: &'a [u8];
}

impl<life a> ByteView for SliceView<'a> {
    fn view_bytes(self: &Self) -> &[u8] {
        return self.data
    }
}

fn pick_longer<life a, A: ByteView, B: ByteView>(a: &'a A, b: &'a B) -> &'a [u8] {
    let va = a.view_bytes()
    let vb = b.view_bytes()
    if va.len() >= vb.len() { return va }
    return vb
}

fn main() -> i32 {
    let doc = OwnedDoc { buf: bytes_new() }
    var raw = [65u8, 66u8, 67u8]
    let sv = SliceView::<'_> { data: &raw[..] }
    let picked = pick_longer(&doc, &sv)
    raw[0] = 90u8
    return i32(picked[0])
}
"#,
    );

    // Negative 6: outer unannotated `&SliceView<'a>` CANNOT be returned as `&'a SliceView<'a>`.
    assert_safe_reject(
        &dir,
        "neg_unannotated_outer_param_as_named_return",
        r#"
struct SliceView<life a> {
    data: &'a [u8];
}

fn bad_outer<life a>(sv: &SliceView<'a>) -> &'a SliceView<'a> {
    return sv
}

fn main() -> i32 {
    let data = [1u8, 2u8]
    let sv = SliceView::<'_> { data: &data[..] }
    let r = bad_outer(&sv)
    return i32(r.data[0])
}
"#,
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_unannotated_lending_and_region_escape_verified_independently() {
    use zil_base::{FileId, Span};
    use zil_check::{CheckBudget, CheckErrorKind, Violation, ViolationKind, check_module};
    use zil_ir::body::{
        Block, BlockId, Body, Callee, Certificate, ConstValue, FnId, FnRole, FnSig, Function,
        FunctionCertificate, Instruction, InstructionKind, IntTy, Local, LocalDecl, Module,
        Mutability, Operand, ParamEnv, Place, Region, ResumeMode, Rvalue, Terminator, TyKind,
        TyTable, decode_module, encode_module,
    };
    use zil_solve::Budget;

    let sp = Span::new(FileId(0), 0, 0);
    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let ref_a = tys.intern(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Shared,
        pointee: i32_ty,
    });
    let ref_b = tys.intern(TyKind::Ref {
        region: Region::Param(1),
        mutability: Mutability::Shared,
        pointee: i32_ty,
    });
    let ref_erased = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i32_ty,
    });

    // Helper: fn helper<life a>(x: &'a i32) -> &i32 { return x }
    let helper_fn = Function {
        name: "helper".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 1,
            params: vec![ref_a],
            result: ref_erased,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            param_env: ParamEnv::default(),
            closure: None,
            captures: Vec::new(),
            result: ref_erased,
            locals: vec![LocalDecl {
                ty: ref_a,
                name: Some("x".into()),
            }],
            drop_flags: Vec::new(),
            blocks: vec![Block {
                instructions: Vec::new(),
                terminator: Terminator::Return {
                    value: Operand::Copy(Place::local(Local(0))),
                },
                terminator_span: sp,
            }],
        },
    };

    // Caller: fn caller() -> &i32 { let local = 1; let r = &local; return helper(r) }
    // Even though `helper` has `region_params == 1` and unannotated return `&i32`,
    // `zil-check` MUST propagate `&local` across the call and reject `caller` with LoanConflict / ReturnsLocalBorrow.
    let caller_fn = Function {
        name: "caller".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 0,
            params: Vec::new(),
            result: ref_erased,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 0,
            param_env: ParamEnv::default(),
            closure: None,
            captures: Vec::new(),
            result: ref_erased,
            locals: vec![
                LocalDecl {
                    ty: i32_ty,
                    name: Some("local".into()),
                },
                LocalDecl {
                    ty: ref_erased,
                    name: Some("r".into()),
                },
                LocalDecl {
                    ty: ref_erased,
                    name: Some("ret".into()),
                },
            ],
            drop_flags: Vec::new(),
            blocks: vec![
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(0)),
                            span: sp,
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(0)),
                                value: Rvalue::Use(Operand::Const(ConstValue::Int {
                                    ty: IntTy::I32,
                                    bits: 1,
                                })),
                            },
                            span: sp,
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(1)),
                            span: sp,
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::Ref {
                                    place: Place::local(Local(0)),
                                    mutability: Mutability::Shared,
                                    region: Region::Static,
                                },
                            },
                            span: sp,
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(2)),
                            span: sp,
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Direct(FnId(0)),
                        args: vec![Operand::Copy(Place::local(Local(1)))],
                        dest: Place::local(Local(2)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: sp,
                },
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageDead(Local(1)),
                            span: sp,
                        },
                        Instruction {
                            kind: InstructionKind::StorageDead(Local(0)),
                            span: sp,
                        },
                    ],
                    terminator: Terminator::Return {
                        value: Operand::Copy(Place::local(Local(2))),
                    },
                    terminator_span: sp,
                },
            ],
        },
    };

    // Bad region: fn bad<life a, life b>(_a: &'a i32, b: &'b i32) -> &'a i32 { return b }
    let bad_region_fn = Function {
        name: "bad_region".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 2,
            params: vec![ref_a, ref_b],
            result: ref_a,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 2,
            param_env: ParamEnv::default(),
            closure: None,
            captures: Vec::new(),
            result: ref_a,
            locals: vec![
                LocalDecl {
                    ty: ref_a,
                    name: Some("a".into()),
                },
                LocalDecl {
                    ty: ref_b,
                    name: Some("b".into()),
                },
            ],
            drop_flags: Vec::new(),
            blocks: vec![Block {
                instructions: Vec::new(),
                terminator: Terminator::Return {
                    value: Operand::Copy(Place::local(Local(1))),
                },
                terminator_span: sp,
            }],
        },
    };

    // 1. Helper alone passes both Solver and independent zil-check after wire round-trip.
    let helper_mod = Module {
        tys: tys.clone(),
        functions: vec![helper_fn.clone()],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };
    let helper_decoded = decode_module(&encode_module(&helper_mod)).expect("decode helper wire");
    let helper_cert = zil_solve::solve_module(&helper_decoded, Budget { steps: 100_000 })
        .expect("solver must accept unannotated lending helper");
    check_module(helper_decoded, &helper_cert, CheckBudget { steps: 100_000 })
        .expect("independent zil-check must accept unannotated lending helper");

    // 2. Caller escaping local borrow across helper MUST be rejected independently by zil-check.
    let caller_mod = Module {
        tys: tys.clone(),
        functions: vec![helper_fn, caller_fn],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };
    let caller_decoded = decode_module(&encode_module(&caller_mod)).expect("decode caller wire");
    assert!(
        zil_solve::solve_module(&caller_decoded, Budget { steps: 100_000 }).is_err(),
        "solver must reject dangling local borrow across unannotated lending helper"
    );
    let empty_cert = Certificate {
        functions: vec![
            helper_cert.functions[0].clone(),
            FunctionCertificate {
                function: FnId(1),
                invariants: Vec::new(),
            },
        ],
    };
    let caller_err = check_module(caller_decoded, &empty_cert, CheckBudget { steps: 100_000 })
        .expect_err("independent zil-check must reject dangling local borrow across helper");
    assert_eq!(caller_err.function, FnId(1));

    // 3. Returning `'b` as `'a` MUST be rejected independently by zil-check with RegionEscape(1).
    let bad_region_mod = Module {
        tys,
        functions: vec![bad_region_fn],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };
    let bad_decoded = decode_module(&encode_module(&bad_region_mod)).expect("decode bad wire");
    let bad_err = check_module(
        bad_decoded,
        &Certificate {
            functions: vec![FunctionCertificate {
                function: FnId(0),
                invariants: Vec::new(),
            }],
        },
        CheckBudget { steps: 100_000 },
    )
    .expect_err("independent zil-check must reject RegionEscape");
    assert!(
        matches!(
            bad_err.kind,
            CheckErrorKind::Violation(Violation {
                kind: ViolationKind::RegionEscape { region: 1 },
                ..
            })
        ),
        "expected RegionEscape(1), got: {bad_err:?}"
    );
}
