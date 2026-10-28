//! Real source exclusive addresses, non-Copy transfers and captured external state.
use std::{
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{InstructionKind, TypedModule};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "impl042.zil", text)
}
fn run(path: &Path, want: i64) {
    let out = Command::new(path).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(out.stdout, format!("{want}\n").as_bytes());
    assert!(out.stderr.is_empty());
}
fn native(text: &str, want: i64) -> String {
    let dir = std::env::temp_dir().join(format!(
        "zil-042-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("answer".into()),
            output: Some(dir.join("program")),
            emit_dir: Some(dir.join("ir")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    run(&report.artifacts["executable"], want);
    let ir = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(!ir.contains("@malloc") && !ir.contains("noalias"));
    let out = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(dir.join("optimized"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    run(&dir.join("optimized"), want);
    fs::remove_dir_all(dir).unwrap();
    ir
}
fn reject(text: &str, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
}
fn raw(text: &str) -> TypedModule {
    let s = source(text);
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
#[test]
fn native_exclusive_call_and_reborrow() {
    let ir = native(include_str!("../../../examples/impl042/main.zil"), 42);
    assert!(ir.contains("store i32 %v."));
}
#[test]
fn independent_and_sequential_reborrows() {
    native(
        "fn bump(p:&mut i32) { *p=*p+1 } fn answer()->i32 { var x=1; var y=2; let p=&mut x; let q=&mut *p; *q=3; *p=*p+1; bump(p); bump(p); let s=&*p; let n=*s; *p=7; return n*100+x*10+y }",
        672,
    );
}
#[test]
fn exclusive_nearest_conflicts() {
    for text in [
        "fn answer()->i32 { var x=1; let p=&mut x; let n=x; return *p+n }",
        "fn answer()->i32 { var x=1; let p=&mut x; let q=&mut *p; *p=2; return *q }",
        "fn answer()->i32 { var x=1; let p=&mut x; let q=&mut *p; let n=*p; return *q+n }",
        "fn alter(a:&mut i32,b:&mut i32) { *a=1; *b=2 } fn answer()->i32 { var x=1; alter(&mut x,&mut x); return x }",
        "fn answer()->i32 { var x=1; let p=&mut x; let q=&*p; *p=2; return *q }",
    ] {
        reject(text, "E-BORROW-CONFLICT");
    }
    reject(
        "fn answer()->i32 { var x=1; let p=&x; let q=&mut *p; return *q }",
        "E-ADDRESS-PERMISSION",
    );
    reject(
        "fn answer()->i32 { let x=1; let p=&mut x; return *p }",
        "E-IMMUTABLE-ASSIGNMENT",
    );
}
#[test]
fn address_transfer_clears_the_source() {
    native(
        "fn answer()->i32 { var x=1; let p=&mut x; let q=p; *q=42; return x }",
        42,
    );
    reject(
        "fn answer()->i32 { var x=1; let p=&mut x; let q=p; *q=2; return *p }",
        "E-UNINITIALIZED",
    );
}
#[test]
fn wire_roundtrip_preserves_executable_permissions() {
    let m = raw(include_str!("../../../examples/impl042/main.zil"));
    let limits = Limits::default();
    let p = zil_analyze::analyze(&m, &limits).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (m.clone(), p)
    );
    assert!(
        m.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::ReborrowAddress { mutable: true, .. }
            ))
    );
}

#[test]
fn default_closure_modifies_the_external_position() {
    native(
        "fn answer()->i32 { var x=1; var f=()=>{ x=x+1; x }; let a=f(); let b=f(); return a*100+b*10+x }",
        233,
    );
    native(
        "fn answer()->i32 { var a=[1,2]; var f=()=>{ a[0]=a[0]+2; a[0] }; let x=f(); let y=f(); return x*100+y*10+a[0] }",
        355,
    );
}
#[test]
fn default_address_captures_reborrow_and_move_address_captures_transfer() {
    native(
        "fn answer()->i32 { var x=1; let p=&mut x; var f=()=>{ *p=*p+1; *p }; let a=f(); let b=f(); *p=4; return a*100+b*10+x }",
        234,
    );
    native(
        "fn answer()->i32 { var x=1; let p=&mut x; var f=move ()=>{ *p=*p+1; *p }; let a=f(); let b=f(); return a*100+b*10+x }",
        233,
    );
    native(
        "fn answer()->i32 { var x=40; let p=&mut x; let f=()=>*p; let a=f(); *p=2; return a+x }",
        42,
    );
}
#[test]
fn closure_hold_and_exclusive_environment_move() {
    native(
        "fn answer()->i32 { var x=1; var f=()=>{ x=x+1; x }; var g=f; let a=g(); return a+x }",
        4,
    );
    reject(
        "fn answer()->i32 { var x=1; var f=()=>{ x=x+1; x }; let a=x; return f()+a }",
        "E-BORROW-CONFLICT",
    );
    reject(
        "fn answer()->i32 { var x=1; var f=()=>{ x=x+1; x }; var g=f; let a=g(); return f()+a }",
        "E-UNINITIALIZED",
    );
    reject(
        "fn answer()->i32 { var x=1; let p=&mut x; var f=()=>{ *p=2; *p }; *p=3; return f() }",
        "E-BORROW-CONFLICT",
    );
    reject(
        "fn answer()->i32 { var x=1; let p=&mut x; var f=move ()=>{ *p=2; *p }; let a=f(); return *p+a }",
        "E-UNINITIALIZED",
    );
}

#[test]
fn projected_address_writes_and_method_receiver() {
    native(
        "struct Pair { left:i32, right:i32 } impl Pair { fn bump(self:&mut Self) { .left=.left+1; } } fn answer()->i32 { var p=Pair{left:1,right:2}; p.bump(); let a=&mut p; a.left=a.left+3; let q=&mut a.right; *q=4; return p.left*10+p.right }",
        54,
    );
    native(
        "fn answer()->i32 { var a=[[1,2],[3,4]]; let p=&mut a; p[0][1]=p[0][1]+5; let q=&mut p[1][0]; *q=9; return a[0][1]*10+a[1][0] }",
        79,
    );
}
#[test]
fn return_contracts_and_control_flow_keep_parent_windows() {
    native(
        "fn id(p:&mut i32)->&mut i32 { return p } fn answer()->i32 { var x=1; let p=&mut x; let q=id(p); *q=2; *p=3; var i=0; while i<3 { let r=&mut *p; *r=*r+1; i=i+1; } return x }",
        6,
    );
    native(
        "fn choose(a:&mut i32,b:&mut i32,flag:bool)->&mut i32 { return if flag { a } else { b } } fn answer()->i32 { var x=1; var y=2; let q=choose(&mut x,&mut y,false); *q=40; return x+y }",
        41,
    );
    reject(
        "fn id(p:&mut i32)->&mut i32 { return p } fn answer()->i32 { var x=1; let p=&mut x; let q=id(p); *p=3; return *q }",
        "E-BORROW-CONFLICT",
    );
    reject(
        "fn bad()->&mut i32 { var x=1; return &mut x } fn answer()->i32{return 0}",
        "E-BORROW-ESCAPE",
    );
    reject(
        "fn answer()->i32 { var x=1; let p=&mut x; let q=&*p; let n=x; return n+*q }",
        "E-BORROW-CONFLICT",
    );
}
#[test]
fn capture_permissions_follow_calls_and_do_not_escape_scope() {
    native(
        "fn bump(p:&mut i32) { *p=*p+2 } fn answer()->i32 { var x=1; let p=&mut x; var f=()=>{ bump(p); *p }; let a=f(); return a+x }",
        6,
    );
    native(
        "fn answer()->i32 { var x=1; let p=&mut x; var f=()=>{ let q=p; *q=42; *q }; let a=f(); return a+x }",
        84,
    );
}

fn verify_raw(module: TypedModule) -> Result<(), Vec<zil_base::Diagnostic>> {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &limits).map(|_| ())
}
#[test]
fn raw_load_cannot_copy_exclusive_address_or_environment() {
    for text in [
        "fn answer()->i32 { var x=1; let p=&mut x; let q=p; *q=2; return x }",
        "fn answer()->i32 { var x=1; var f=()=>{x=x+1;x}; var g=f; return g() }",
    ] {
        let mut module = raw(text);
        let take = module.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
            .unwrap();
        let InstructionKind::TakeLocal { local } = take.kind else {
            unreachable!()
        };
        take.kind = InstructionKind::Load(local);
        let error = verify_raw(module).unwrap_err();
        assert_eq!(error[0].code, "E-VERIFY-MOVE");
    }
}
#[test]
fn raw_shared_upgrade_and_false_closure_call_kind_fail() {
    let mut module = raw("fn answer()->i32 { var x=1; let p=&x; let q=&*p; return *q }");
    let f = &mut module.functions[0];
    let instruction = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::ReborrowAddress { .. }))
        .unwrap();
    if let InstructionKind::ReborrowAddress { mutable, .. } = &mut instruction.kind {
        *mutable = true;
    }
    let value = instruction.result.unwrap();
    if let zil_ir::Type::Address { mutable, .. } = &mut f.values[value.0 as usize] {
        *mutable = true;
    }
    let error = verify_raw(module).unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-ADDRESS");
    let mut module = raw("fn answer()->i32 { var x=1; var f=()=>{x=x+1;x}; return f() }");
    module.closures[0].kind = zil_ir::CallableKind::Fn;
    let error = verify_raw(module).unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-CLOSURE-CALL");
}
#[test]
fn raw_permission_conflicts_are_independent_of_candidate_inference() {
    for text in [
        "fn answer()->i32 { var x=1; let p=&mut x; let q=&mut *p; *p=2; return *q }",
        "fn pair(a:&mut i32,b:&mut i32){*a=1;*b=2;} fn answer()->i32{var x=1;let p=&mut x;pair(p,p);return x;}",
        "fn answer()->i32 { var x=1; var f=()=>{x=x+1;x}; let y=x; return f()+y }",
    ] {
        let error = verify_raw(raw(text)).unwrap_err();
        assert_eq!(error[0].code, "E-BORROW-CONFLICT");
    }
}
#[test]
fn raw_exclusive_ssa_transfer_is_single_use_even_without_source_local() {
    let mut module = raw("fn answer()->i32 { var x=1; let p=&mut x; let q=p; *q=2; return x }");
    let f = &mut module.functions[0];
    let copied = f.blocks[0]
        .instructions
        .iter()
        .find_map(|i| {
            if let InstructionKind::Store { local, .. } = i.kind {
                if matches!(
                    f.locals[local.0 as usize].ty,
                    zil_ir::Type::Address { mutable: true, .. }
                ) {
                    Some((*i).clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| *i == copied)
        .unwrap();
    // Reuse the exact SSA carrier in a distinct, structurally live local. All
    // operands dominate and a regenerated candidate cannot grant affine authority.
    let mut copy = copied.clone();
    let new = zil_ir::LocalId(f.locals.len() as u32);
    let InstructionKind::Store {
        local: original, ..
    } = copied.kind
    else {
        unreachable!()
    };
    let mut decl = f.locals[original.0 as usize].clone();
    decl.id = new;
    decl.name = "forged_copy".into();
    f.locals.push(decl);
    if let InstructionKind::Store { local, .. } = &mut copy.kind {
        *local = new;
    }
    f.blocks[0].instructions.splice(
        at + 1..at + 1,
        [
            zil_ir::Instruction {
                result: None,
                kind: InstructionKind::StorageLive(new),
                span: copied.span,
            },
            copy,
            zil_ir::Instruction {
                result: None,
                kind: InstructionKind::StorageDead(new),
                span: copied.span,
            },
        ],
    );
    let error = verify_raw(module).unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-MOVE");
}
#[test]
fn low_budget_is_a_limit_and_whole_object_aliases_are_conservative() {
    let module = raw(include_str!("../../../examples/impl042/main.zil"));
    let defaults = Limits::default();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let limits = Limits {
        max_analysis_steps: 1,
        ..defaults
    };
    assert_eq!(
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
    reject(
        "fn answer()->i32 { var pair=(1,2); let a=&mut pair.0; let b=&mut pair.1; *a=3; return *b }",
        "E-BORROW-CONFLICT",
    );
    reject(
        "fn answer()->i32 { var x=1; var p=&mut x; if true { var inner=2; p=&mut inner; } return *p }",
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn compound_dereference_keeps_one_authority_and_evaluates_address_once() {
    native(
        "fn answer()->i32 { var x=20; let p=&mut x; *p+=1; *p+=*p; return x }",
        42,
    );
    native(
        "fn select(p:&mut i32,count:&mut i32)->&mut i32 { *count+=1; return p } fn answer()->i32 { var x=1; var count=0; *select(&mut x,&mut count)+=5; return x*10+count }",
        61,
    );
    reject(
        "fn answer()->i32 { var x=1; let p=&x; *p+=1; return x }",
        "E-ADDRESS-PERMISSION",
    );
}
#[test]
fn raw_closure_address_parameter_cannot_disagree_with_its_entry() {
    let mut module = raw("fn answer()->i32 { let f=(x:i32)=>x+1; return f(1) }");
    let limits = Limits::default();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    module.closures[0].parameters[0] = zil_ir::Type::Address {
        mutable: true,
        inner: Box::new(zil_ir::Type::I32),
    };
    // 189 permits the signature shape, but the original i32 entry remains unchanged.
    plan.module_fingerprint = module.fingerprint();
    let error =
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err();
    assert_eq!(error[0].category, ResultCategory::SafeReject);
    assert_eq!(error[0].phase, zil_base::Phase::Verify);
    assert_eq!(error[0].code, "E-VERIFY-CLOSURE-ENTRY");
    assert_eq!(
        error[0].message,
        "closure entry parameter differs from its definition"
    );
}
