//! Independent review of 2be7aee: source paths and adversarial executable IR.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{Instruction, InstructionKind, Type, TypedModule, ValueId};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review042.zil", text)
}
fn raw(text: &str) -> TypedModule {
    let s = source(text);
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn verify_raw(m: TypedModule) -> Vec<Diagnostic> {
    let limits = Limits::default();
    let candidate = zil_analyze::analyze(&m, &limits).unwrap();
    zil_verify::verify_module(m.clone(), candidate, &m.target, &limits).unwrap_err()
}
fn rejected(text: &str, code: &str) {
    let r = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
fn native(text: &str, expected: i64) {
    let root = std::env::temp_dir().join(format!(
        "zil-review042-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let r = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("answer".into()),
            output: Some(root.join("o0")),
            emit_dir: Some(root.join("ir")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    let llvm = fs::read_to_string(&r.artifacts["llvm"]).unwrap();
    assert!(!llvm.contains("noalias") && !llvm.contains("@malloc"));
    let compile = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&r.artifacts["llvm"])
        .arg("-o")
        .arg(root.join("o2"))
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    for level in ["o0", "o2"] {
        let out = Command::new(root.join(level)).output().unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, format!("{expected}\n").as_bytes());
        assert!(out.stderr.is_empty());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn nested_windows_and_disjoint_control_flow_restore_authority() {
    native(
        "fn answer()->i32{var x=1;let p=&mut x;let q=&mut *p;let r=&mut *q;*r=2;*q=3;*p=42;return x}",
        42,
    );
    native(
        "fn run(flag:bool)->i32 {var x=1;let p=&mut x;if flag {let q=p;*q=40;return x+2}else{let q=p;*q=41;return x+1}} fn answer()->i32{return run(true)+run(false)}",
        84,
    );
    for text in [
        "fn answer()->i32{var x=1;let p=&mut x;let q=&mut *p;let r=&mut *q;*p=2;return *r}",
        "fn answer()->i32{var x=1;let p=&mut x;let q=&mut *p;let r=&mut *q;*q=2;return *r}",
        "fn answer()->i32{var x=1;let p=&mut x;let q=&mut *p;let r=p;*r=3;return *q}",
    ] {
        rejected(text, "E-BORROW-CONFLICT");
    }
    rejected(
        "fn run(flag:bool)->i32 {var x=1;let p=&mut x;if flag {let q=p;*q=40;}return *p} fn answer()->i32{return run(true)}",
        "E-UNINITIALIZED",
    );
}

#[test]
fn captures_preserve_origin_through_aliases_conditions_and_calls() {
    native(
        "fn answer()->i32 { var x=1; let p=&mut x; var f=()=>{let q:&mut i32=p; let r=q; *r=42; *r}; let a=f(); return a+x }",
        84,
    );
    native(
        "fn answer()->i32 { var x=1; var y=2; let p=&mut x; let q=&mut y; var f=(b:bool)=>{let r=if b {p} else {q}; *r=40; *r}; let a=f(true); return a+x+y }",
        82,
    );
    native(
        "fn id(p:&mut i32)->&mut i32{return p} fn answer()->i32{var x=1;let p=&mut x;var f=()=>{*id(p)=42;*p};let n=f();return n+x}",
        84,
    );
    for text in [
        "fn answer()->i32{var x=1;let p=&mut x;var f=()=>{*p=2;*p};let n=*p;return f()+n}",
        "fn answer()->i32{var x=1;var f=()=>{x=2;x};var g=()=>{x=3;x};return f()+g()}",
    ] {
        rejected(text, "E-BORROW-CONFLICT");
    }
}

#[test]
fn readonly_move_capture_acceptance_gap_is_explicit() {
    // Language-level readonly Fn acceptance is still incomplete in this slice.
    rejected(
        "fn answer()->i32 { var x=21; let p=&mut x; let f=move()=>*p; return f()+x }",
        "E-CLOSURE-MUTABILITY",
    );
    native(
        "fn answer()->i32 { var x=21; let p=&mut x; var f=move()=>*p; return f()+x }",
        42,
    );
    native(
        "fn answer()->i32 { var x=21; let p=&mut x; let f=()=>*p; let n=*p; return n+f() }",
        42,
    );
}

#[test]
fn raw_transfer_rejects_live_load_and_projection_views() {
    for projected in [false, true] {
        let mut m = raw(if projected {
            "fn answer()->i32{var x=(1,2);let p=&mut x;let q=p;q.0=2;return x.0}"
        } else {
            "fn answer()->i32{var x=1;let p=&mut x;let q=p;*q=2;return x}"
        });
        let f = &mut m.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
            .unwrap();
        let InstructionKind::TakeLocal { local } = f.blocks[0].instructions[at].kind else {
            unreachable!()
        };
        let span = f.span;
        let load = ValueId(f.values.len() as u32);
        f.values.push(f.locals[local.0 as usize].ty.clone());
        let mut added = vec![Instruction {
            result: Some(load),
            kind: InstructionKind::Load(local),
            span,
        }];
        let view = if projected {
            let projection = ValueId(f.values.len() as u32);
            f.values.push(Type::Address {
                mutable: true,
                inner: Box::new(Type::I32),
            });
            added.push(Instruction {
                result: Some(projection),
                kind: InstructionKind::AddressField {
                    address: load,
                    index: 0,
                },
                span,
            });
            projection
        } else {
            load
        };
        let offset = added.len();
        f.blocks[0].instructions.splice(at..at, added);
        let read = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        f.blocks[0].instructions.insert(
            at + offset + 1,
            Instruction {
                result: Some(read),
                kind: InstructionKind::ReadAddress { address: view },
                span,
            },
        );
        let errors = verify_raw(m);
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    }
}

#[test]
fn raw_transferred_ssa_cannot_read_again() {
    let mut m = raw("fn answer()->i32{var x=1;let p=&mut x;let q=p;*q=2;return x}");
    let f = &mut m.functions[0];
    let transferred = f.blocks[0]
        .instructions
        .iter()
        .find_map(|x| {
            matches!(x.kind, InstructionKind::TakeLocal { .. }).then(|| x.result.unwrap())
        })
        .unwrap();
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(
            |i| matches!(i.kind, InstructionKind::Store { value, .. } if value == transferred),
        )
        .unwrap();
    let result = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    // This Store has consumed the transferred SSA carrier.
    f.blocks[0].instructions.insert(
        at + 1,
        Instruction {
            result: Some(result),
            kind: InstructionKind::ReadAddress {
                address: transferred,
            },
            span: f.span,
        },
    );
    let errors = verify_raw(m);
    assert_eq!(errors[0].code, "E-VERIFY-MOVE", "{errors:?}");
}

#[test]
fn raw_return_cannot_transfer_a_load_view() {
    let mut m =
        raw("fn id(p:&mut i32)->&mut i32{return p} fn answer()->i32{var x=42;return *id(&mut x)}");
    let i = m.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
        .unwrap();
    let InstructionKind::TakeLocal { local } = i.kind else {
        unreachable!()
    };
    i.kind = InstructionKind::Load(local);
    let errors = verify_raw(m);
    assert_eq!(errors[0].code, "E-VERIFY-MOVE", "{errors:?}");
}

#[test]
fn raw_duplicate_readcapture_cannot_forge_independent_qualification() {
    let mut m = raw("fn answer()->i32{var x=1;var f=()=>{x=x+1;x};return f()}");
    let f = m
        .functions
        .iter_mut()
        .find(|f| f.environment.is_some())
        .unwrap();
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReadCapture { .. }))
        .unwrap();
    let first = f.blocks[0].instructions[at].clone();
    let address = ValueId(f.values.len() as u32);
    f.values
        .push(f.values[first.result.unwrap().0 as usize].clone());
    let read = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let write = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::WriteAddress { .. }))
        .unwrap();
    assert!(write > at);
    f.blocks[0].instructions.insert(
        write,
        Instruction {
            result: Some(address),
            kind: first.kind,
            span: f.span,
        },
    );
    let before_cleanup = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    assert!(before_cleanup > write + 1);
    // Keep the duplicate view live across the write, within the active root.
    f.blocks[0].instructions.insert(
        before_cleanup,
        Instruction {
            result: Some(read),
            kind: InstructionKind::ReadAddress { address },
            span: f.span,
        },
    );
    let errors = verify_raw(m);
    assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
}

#[test]
fn raw_mutability_and_candidate_holds_are_not_authority() {
    let mut m = raw("fn answer()->i32{var x=42;let p=&mut x;return *p}");
    let f = &mut m.functions[0];
    let local = f.locals.iter_mut().find(|l| l.name == "x").unwrap();
    local.mutable = false;
    let errors = verify_raw(m);
    assert_eq!(errors[0].code, "E-VERIFY-MUTABILITY", "{errors:?}");

    let m = raw("fn answer()->i32{var x=1;let p=&mut x;let q=&mut *p;*p=2;return *q}");
    let limits = Limits::default();
    let mut plan = zil_analyze::analyze(&m, &limits).unwrap();
    let claim = plan.functions[0]
        .holds
        .iter_mut()
        .find(|h| !h.roots.is_empty())
        .unwrap();
    claim.roots.clear();
    let errors = zil_verify::verify_module(m.clone(), plan, &m.target, &limits).unwrap_err();
    assert_eq!(errors[0].code, "E-VERIFY-HOLD", "{errors:?}");
}

#[test]
fn raw_post_move_local_use_is_checked_without_analyzer_acceptance() {
    let mut m = raw("fn answer()->i32{var x=1;let p=&mut x;let q=p;*q=2;return x}");
    let limits = Limits::default();
    let mut plan = zil_analyze::analyze(&m, &limits).unwrap();
    let f = &mut m.functions[0];
    let p = f.locals.iter().find(|l| l.name == "p").unwrap().id;
    let q = f.locals.iter().find(|l| l.name == "q").unwrap().id;
    let i = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,InstructionKind::Load(l) if l==q))
        .unwrap();
    i.kind = InstructionKind::Load(p);
    plan.module_fingerprint = m.fingerprint();
    let errors = zil_verify::verify_module(m.clone(), plan, &m.target, &limits).unwrap_err();
    assert_eq!(errors[0].code, "E-VERIFY-INITIALIZATION", "{errors:?}");
}

#[test]
fn raw_overlapping_captures_and_shared_child_conflict() {
    for text in [
        "fn answer()->i32{var x=1;let p=&mut x;let q=&mut *p;var f=()=>{*p=2;*q=3;*p+*q};return f()}",
        "fn f(a:&mut i32,b:&i32)->i32{*a=2;return *b} fn answer()->i32{var x=1;let p=&mut x;return f(p,p)}",
        "fn answer()->i32{var x=1;let p=&mut x;let q=&*p;let r=&*q;*p=3;return *r}",
    ] {
        let errors = verify_raw(raw(text));
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    }
}
