//! End-to-end dual-backend native execution, safety rejection, and Raw/Wire
//! valid-certificate `zil-check` proof tests for Wave 3 Batch U5:
//! 1. Per-field borrow tracking for `Struct`/`Package`/`Tuple` (G3.4):
//!    - Overwriting field `a` with a new borrow releases the old borrow on `x`
//!      so `x` can be mutated while field `b` remains live.
//!    - Soundness negatives: dangling borrowed field escape (`ReturnsLocalBorrow`),
//!      mutating `y` while borrowed field `b` still holds `&y` (`LoanConflict`).
//!    - Raw/Wire valid-certificate `zil-check` proof: overwriting field 0 releases
//!      loan 0 (accepted by Solver + `zil-check` with valid cert) vs mutating
//!      field 1's still-borrowed target (rejected by Solver + `zil-check` with
//!      valid cert and exact `ViolationKind::LoanConflict`).
//! 2. Closure capture precision (G3.5):
//!    - Read-only capture of `var` outer locals (`&T` instead of `&mut T`),
//!      allowing simultaneous reads and multiple read-only closures over the same `var`.
//!    - By-value capture of moved owned outer locals (`ByteBuffer`, `Store`) in
//!      non-`move` closures (`FnOnce`) + `Fn`/`FnMut`/`FnOnce` consumer matrix
//!      with deterministic `Drop`.
//!    - Nested `OuterCapture` consumption for non-`Copy` owned types and `give cap.field`.
//!    - Soundness negatives: mutating `var` while read-only closure still borrows it
//!      (`LoanConflict`), double-consuming an owned captured local (`SafeReject`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{FileId, ResultCategory, Span};
use zil_check::{CheckBudget, CheckErrorKind, Violation, ViolationKind, check_module};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};
use zil_ir::body::{
    AdtDef, AdtKind, AggregateKind, BinOp, Block, Body, Certificate, ConstValue, FieldDef,
    FieldIdx, FieldRef, FnId, FnRole, FnSig, Function, FunctionCertificate, GenericArgs,
    Instruction, InstructionKind, IntTy, Local, LocalDecl, Module, Mutability, Operand, ParamEnv,
    Place, PlaceRoot, Projection, Region, ResumeMode, Rvalue, Terminator, TyKind, TyTable,
    VariantDef, VariantIdx, decode_module, encode_module,
};
use zil_solve::{Budget, SolveError};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-u5-{name}-{}", std::process::id()));
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

fn check_rejected(dir: &Path, name: &str, src: &str, expected_code_prefix: &str) {
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
        "expected `{name}` to be SafeReject, got {:?} with diagnostics: {:?}",
        report.status,
        report.diagnostics
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
fn per_field_borrow_tracking_struct_package_tuple_and_negatives() {
    let dir = temp_dir("per-field-borrows");
    let pos_src = r#"
struct Pair<life a, life b> {
    first: &'a i32,
    second: &'b i32,
}

package Session<life a, life b> {
    manages bytes: ByteBuffer
    reader: &'a i32
    writer: &'b mut i32
}

fn make_session<life a, life b>(reader: &'a i32, writer: &'b mut i32) -> Session<'a, 'b> {
    var bytes = bytes_new()
    bytes_reserve(bytes, 4)
    bytes_push(bytes, 1u8)
    bytes_push(bytes, 2u8)
    return Session::<'a, 'b> {
        bytes: give bytes,
        reader: reader,
        writer: writer,
    }
}

fn main() -> i32 {
    // 1. Struct with two independent shared borrows: overwriting `first` releases `x`.
    var x: i32 = 10
    let y: i32 = 7
    let z: i32 = 3
    var p = Pair::<'_, '_> { first: &x, second: &y }
    p.first = &z
    // `x` is no longer borrowed by `p`, while `p.second` still borrows `y`.
    x += 5
    let s1 = *p.first + *p.second + x // 3 + 7 + 15 = 25

    // 2. Package with owned ByteBuffer + shared & exclusive borrowed fields.
    var a: i32 = 4
    var b: i32 = 6
    let c: i32 = 2
    var s2: i32 = 0
    if true {
        var sess = make_session(&a, &mut b)
        sess.reader = &c
        // `a` is released by overwriting `sess.reader`, while `sess.writer` stays live!
        a += 8
        *sess.writer += 3
        let blen: i32 = if bytes_len(sess.bytes) == 2 { 2 } else { 0 }
        s2 = *sess.reader + a + blen // 2 + 12 + 2 = 16
    }

    // 3. Tuple of two borrows: overwriting `.0` releases `u` while `.1` stays live.
    var u: i32 = 1
    let v: i32 = 2
    let w: i32 = 3
    var tup = (&u, &v)
    tup.0 = &w
    u += 4
    let s3 = *tup.0 + *tup.1 + u // 3 + 2 + 5 = 10

    // s1 (25) + s2 (16) + b (9) + s3 (10) = 60
    return s1 + s2 + b + s3
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 60);

    // Negative 1: Dangling borrowed field escape from function (`ReturnsLocalBorrow`).
    check_rejected(
        &dir,
        "neg_dangling_field_escape",
        r#"
struct Pair<life a, life b> {
    first: &'a i32,
    second: &'b i32,
}

fn bad<life a>(ext: &'a i32) -> Pair<'a, 'a> {
    let local_val: i32 = 99
    let p = Pair::<'_, '_> { first: ext, second: &local_val }
    return p
}

fn main() -> i32 {
    let x: i32 = 1
    let _p = bad(&x)
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 2: Overwriting `first` releases `x`, NOT `y` — mutating `y` while `second`
    // still holds `&y` must still be rejected with `LoanConflict`.
    check_rejected(
        &dir,
        "neg_mutate_still_borrowed_second_field",
        r#"
struct Pair<life a, life b> {
    first: &'a i32,
    second: &'b i32,
}

fn main() -> i32 {
    var x: i32 = 10
    var y: i32 = 20
    let z: i32 = 30
    var p = Pair::<'_, '_> { first: &x, second: &y }
    p.first = &z
    y += 1
    return *p.first + *p.second
}
"#,
        "E-SAFETY",
    );
}

#[test]
fn closure_capture_precision_readonly_var_fnonce_and_negatives() {
    let dir = temp_dir("closure-captures");
    let pos_src = r#"
resource DropCounter {
    delta: i32,
}

impl Drop for DropCounter {
    fn drop(self: Self) {
        let _ = self.delta
    }
}

package Token {
    manages dc: DropCounter
    weight: i32
}

fn consume_bytes(b: Managed<ByteBuffer>) -> i32 {
    if bytes_len(b) == 3 {
        return 3
    }
    return 0
}

fn consume_store(s: Managed<Store<Token>>) -> i32 {
    if store_len(s) == 2 {
        return 2
    }
    return 0
}

fn consume_token(t: Token) -> i32 {
    return t.weight
}

fn consume_dc(d: Managed<DropCounter>) -> i32 {
    return d.delta
}

fn call_fn<F: Fn() -> i32>(f: &F) -> i32 {
    return f() + f()
}

fn call_fn_mut<F: FnMut() -> i32>(f: &mut F) -> i32 {
    let a = f()
    let b = f()
    return a + b
}

fn call_fn_once<F: FnOnce() -> i32>(f: F) -> i32 {
    return f()
}

fn main() -> i32 {
    // 1. Read-only capture of `var` outer local: two read-only closures + direct read coexist!
    var count: i32 = 10
    let r1 = () -> i32 => count + 1
    let r2 = () -> i32 => count + 2
    let ro_sum = count + call_fn(&r1) + call_fn(&r2) // 10 + 22 + 24 = 56
    // After read-only closures end (NLL), `count` can be mutated again.
    count += 5

    // 2. Read-only capture of `var` ByteBuffer via `bytes_len`.
    var ro_buf = bytes_new()
    bytes_reserve(ro_buf, 4)
    bytes_push(ro_buf, 10u8)
    bytes_push(ro_buf, 20u8)
    let rb = () -> i32 => if bytes_len(ro_buf) == 2 { 2 } else { 0 }
    let direct_len: i32 = if bytes_len(ro_buf) == 2 { 2 } else { 0 }
    let buf_ro_sum = direct_len + call_fn(&rb) // 2 + 4 = 6

    // 3. FnMut closure mutating a `var` outer local.
    var acc: i32 = 1
    var mut_c = () -> i32 => {
        acc += 3
        return acc
    }
    let mut_sum = call_fn_mut(&mut mut_c) // 4 + 7 = 11

    // 4. Non-`move` closure consuming an owned `ByteBuffer` (`FnOnce`).
    var owned_buf = bytes_new()
    bytes_reserve(owned_buf, 4)
    bytes_push(owned_buf, 1u8)
    bytes_push(owned_buf, 2u8)
    bytes_push(owned_buf, 3u8)
    let once_buf = () -> i32 => consume_bytes(give owned_buf)
    let once_buf_val = call_fn_once(once_buf) // 3

    // 5. Non-`move` closure consuming an owned `Store<Token>` (`FnOnce`) with deterministic Drop.
    var owned_store: Store<Token> = store_new()
    store_reserve(owned_store, 2)
    store_push(owned_store, Token { dc: DropCounter { delta: 1 }, weight: 7 })
    store_push(owned_store, Token { dc: DropCounter { delta: 2 }, weight: 8 })
    let once_store = () -> i32 => consume_store(give owned_store)
    let once_store_val = call_fn_once(once_store) // 2

    // 6. Nested closure consuming an outer captured owned `Package` (checks !is_copy, not just Managed).
    let tok = Token { dc: DropCounter { delta: 3 }, weight: 9 }
    let outer_once = () -> i32 => {
        let inner_once = () -> i32 => consume_token(tok)
        return call_fn_once(inner_once)
    }
    let nested_val = call_fn_once(outer_once) // 9

    // 7. Non-`move` closure consuming `give cap.field` (`FnOnce`).
    let tok2 = Token { dc: DropCounter { delta: 4 }, weight: 1 }
    let give_field_once = () -> i32 => consume_dc(give tok2.dc)
    let give_field_val = call_fn_once(give_field_once) // 4

    // 56 + 15 + 6 + 11 + 3 + 2 + 9 + 4 = 106
    return ro_sum + count + buf_ro_sum + mut_sum + once_buf_val + once_store_val + nested_val + give_field_val
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 106);

    // Negative 1: Mutating `var` while read-only closure still borrows it (`LoanConflict`).
    check_rejected(
        &dir,
        "neg_mutate_var_while_ro_closure_live",
        r#"
fn main() -> i32 {
    var x: i32 = 10
    let c = () -> i32 => x + 1
    x = 20
    return c()
}
"#,
        "E-SAFETY",
    );

    // Negative 2: Double-consuming an owned captured local by calling an `FnOnce` closure twice.
    check_rejected(
        &dir,
        "neg_double_call_fnonce_capture",
        r#"
fn consume_bytes(b: Managed<ByteBuffer>) -> i32 {
    if bytes_len(b) == 1 {
        return 1
    }
    return 0
}

fn main() -> i32 {
    var buf = bytes_new()
    bytes_reserve(buf, 2)
    bytes_push(buf, 1u8)
    let c = () -> i32 => consume_bytes(give buf)
    let a = c()
    let b = c()
    return a + b
}
"#,
        "E-SAFETY",
    );

    // Negative 3: Double-consuming an owned captured local inside the closure body itself.
    check_rejected(
        &dir,
        "neg_double_consume_inside_closure",
        r#"
fn consume_bytes(b: Managed<ByteBuffer>) -> i32 {
    if bytes_len(b) == 1 {
        return 1
    }
    return 0
}

fn main() -> i32 {
    var buf = bytes_new()
    bytes_reserve(buf, 2)
    bytes_push(buf, 1u8)
    let c = () -> i32 => {
        let a = consume_bytes(give buf)
        let b = consume_bytes(give buf)
        return a + b
    }
    return c()
}
"#,
        "E-SAFETY",
    );
}

/// Builds a raw IR module with `struct Pair { a: &i64, b: &i64 }` where:
/// - `x = 1; y = 2; z = 3;`
/// - `r1 = &x; r2 = &y; p = Pair { a: r1, b: r2 };`
/// - `r3 = &z; p.a = r3;` (overwrites field 0, releasing loan on `x` while `p.b` still borrows `y`)
/// - If `mutate_y` is false: mutates `x = 10` (LEGAL — loan on `x` was released!)
/// - If `mutate_y` is true: mutates `y = 20` (ILLEGAL — `p.b` still holds loan on `y`!)
/// - Reads `*p.a + *p.b + x + y` and returns.
fn build_raw_pair_field_overwrite_module(mutate_y: bool) -> Module {
    let sp = Span::new(FileId(0), 0, 0);
    let mut tys = TyTable::new();
    let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
    let ref_i64 = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i64_ty,
    });
    let pair_adt = tys.add_adt(AdtDef {
        name: "Pair".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Pair".into(),
            fields: vec![
                FieldDef {
                    name: "a".into(),
                    ty: ref_i64,
                },
                FieldDef {
                    name: "b".into(),
                    ty: ref_i64,
                },
            ],
        }],
        user_drop: false,
    });
    let empty_args = tys.intern_args(GenericArgs {
        tys: Vec::new(),
        regions: Vec::new(),
    });
    let pair_ty = tys.intern(TyKind::Adt {
        adt: pair_adt,
        args: empty_args,
    });

    // Locals:
    // 0: x: i64
    // 1: y: i64
    // 2: z: i64
    // 3: r1: &i64
    // 4: r2: &i64
    // 5: p: Pair
    // 6: r3: &i64
    // 7: ra: &i64
    // 8: rb: &i64
    // 9: va: i64
    // 10: vb: i64
    // 11: sum: i64
    let decl = |ty| LocalDecl { ty, name: None };
    let locals = vec![
        decl(i64_ty),
        decl(i64_ty),
        decl(i64_ty),
        decl(ref_i64),
        decl(ref_i64),
        decl(pair_ty),
        decl(ref_i64),
        decl(ref_i64),
        decl(ref_i64),
        decl(i64_ty),
        decl(i64_ty),
        decl(i64_ty),
    ];

    let field_a = Place {
        root: PlaceRoot::Local(Local(5)),
        projections: vec![Projection::Field(FieldRef {
            adt: pair_adt,
            variant: VariantIdx(0),
            field: FieldIdx(0),
            args: empty_args,
        })],
    };
    let field_b = Place {
        root: PlaceRoot::Local(Local(5)),
        projections: vec![Projection::Field(FieldRef {
            adt: pair_adt,
            variant: VariantIdx(0),
            field: FieldIdx(1),
            args: empty_args,
        })],
    };

    let i64_const = |bits: u128| {
        Rvalue::Use(Operand::Const(ConstValue::Int {
            ty: IntTy::I64,
            bits,
        }))
    };

    let mut insts = vec![
        Instruction {
            kind: InstructionKind::StorageLive(Local(0)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(0)),
                value: i64_const(1),
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
                value: i64_const(2),
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(2)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(2)),
                value: i64_const(3),
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(3)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(3)),
                value: Rvalue::Ref {
                    region: Region::Static,
                    mutability: Mutability::Shared,
                    place: Place::local(Local(0)),
                },
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(4)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(4)),
                value: Rvalue::Ref {
                    region: Region::Static,
                    mutability: Mutability::Shared,
                    place: Place::local(Local(1)),
                },
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(5)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(5)),
                value: Rvalue::Aggregate {
                    kind: AggregateKind::Adt {
                        adt: pair_adt,
                        variant: VariantIdx(0),
                        args: empty_args,
                    },
                    operands: vec![
                        Operand::Copy(Place::local(Local(3))),
                        Operand::Copy(Place::local(Local(4))),
                    ],
                },
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(6)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(6)),
                value: Rvalue::Ref {
                    region: Region::Static,
                    mutability: Mutability::Shared,
                    place: Place::local(Local(2)),
                },
            },
            span: sp,
        },
        // Overwrite `p.a = r3`, releasing the loan on `x` while `p.b` still borrows `y`.
        Instruction {
            kind: InstructionKind::Assign {
                dest: field_a.clone(),
                value: Rvalue::Use(Operand::Copy(Place::local(Local(6)))),
            },
            span: sp,
        },
    ];

    // Mutate either `x` (legal!) or `y` (LoanConflict!).
    let mutated_local = if mutate_y { Local(1) } else { Local(0) };
    insts.push(Instruction {
        kind: InstructionKind::Assign {
            dest: Place::local(mutated_local),
            value: i64_const(10),
        },
        span: sp,
    });

    // Keep both `p.a` and `p.b` live past the mutation by reading `*p.a + *p.b`.
    insts.extend([
        Instruction {
            kind: InstructionKind::StorageLive(Local(7)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(7)),
                value: Rvalue::Use(Operand::Copy(field_a)),
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(8)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(8)),
                value: Rvalue::Use(Operand::Copy(field_b)),
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(9)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(9)),
                value: Rvalue::Use(Operand::Copy(Place {
                    root: PlaceRoot::Local(Local(7)),
                    projections: vec![Projection::Deref],
                })),
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(10)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(10)),
                value: Rvalue::Use(Operand::Copy(Place {
                    root: PlaceRoot::Local(Local(8)),
                    projections: vec![Projection::Deref],
                })),
            },
            span: sp,
        },
        Instruction {
            kind: InstructionKind::StorageLive(Local(11)),
            span: sp,
        },
        Instruction {
            kind: InstructionKind::Assign {
                dest: Place::local(Local(11)),
                value: Rvalue::Binary {
                    op: BinOp::Add,
                    lhs: Operand::Copy(Place::local(Local(9))),
                    rhs: Operand::Copy(Place::local(Local(10))),
                },
            },
            span: sp,
        },
    ]);

    let body = Body {
        param_count: 0,
        locals,
        captures: Vec::new(),
        closure: None,
        result: i64_ty,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![Block {
            instructions: insts,
            terminator: Terminator::Return {
                value: Operand::Copy(Place::local(Local(11))),
            },
            terminator_span: sp,
        }],
    };

    Module {
        tys,
        functions: vec![Function {
            name: "main".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: Vec::new(),
                result: i64_ty,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    }
}

#[test]
fn raw_wire_valid_cert_per_field_borrow_release_and_conflict() {
    // 1. Positive Raw/Wire proof: overwriting field 0 (`p.a`) releases loan on `x`,
    // so mutating `x` while `p.b` still borrows `y` is accepted by Solver AND `zil-check`.
    let pos_mod = build_raw_pair_field_overwrite_module(false);
    let pos_bytes = encode_module(&pos_mod);
    let pos_decoded = decode_module(&pos_bytes).expect("wire roundtrip must decode");
    let pos_cert = zil_solve::solve_module(&pos_decoded, Budget { steps: 200_000 })
        .expect("solver must admit per-field loan release");
    check_module(pos_decoded, &pos_cert, CheckBudget { steps: 200_000 })
        .expect("zil-check must accept valid cert for per-field loan release");

    // 2. Negative Raw/Wire proof: mutating `y` while `p.b` still borrows `y` MUST be
    // rejected by Solver AND by `zil-check` (even with a structurally valid certificate)
    // with exact `ViolationKind::LoanConflict`.
    let neg_mod = build_raw_pair_field_overwrite_module(true);
    let neg_bytes = encode_module(&neg_mod);
    let neg_decoded = decode_module(&neg_bytes).expect("wire roundtrip must decode");
    let solve_errs = zil_solve::solve_module(&neg_decoded, Budget { steps: 200_000 })
        .expect_err("solver must reject mutation of still-borrowed field target y");
    assert!(
        solve_errs.iter().any(|e| matches!(
            e,
            SolveError::Violation {
                violation: Violation {
                    kind: ViolationKind::LoanConflict { .. },
                    ..
                },
                ..
            }
        )),
        "expected LoanConflict from solver, got: {solve_errs:?}"
    );

    let valid_cert = Certificate {
        functions: vec![FunctionCertificate {
            function: FnId(0),
            invariants: Vec::new(),
        }],
    };
    let check_err = check_module(neg_decoded, &valid_cert, CheckBudget { steps: 200_000 })
        .expect_err("zil-check must reject mutation of still-borrowed field target y");
    assert!(
        matches!(
            check_err.kind,
            CheckErrorKind::Violation(Violation {
                kind: ViolationKind::LoanConflict { .. },
                ..
            })
        ),
        "expected exact ViolationKind::LoanConflict from zil-check, got: {check_err:?}"
    );
}
