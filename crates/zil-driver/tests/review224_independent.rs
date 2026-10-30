//! Independent reviewer /root/ct_for probes, promoted after review of 0614831.
//! The original executed harness and exact source bytes remain in review evidence.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, HoldRoot, InstructionKind, TypedModule};
fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review224-independent-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(name: &str) -> TypedModule {
    let text = match name {
        "distinct-instances" => DISTINCT,
        "child-nll-control" => {
            include_str!("../../../examples/mainline/shared_product224/child-nll-control.zil")
        }
        _ => panic!("unknown frozen source {name}"),
    };
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent224.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn check(label: &str, module: TypedModule, mut plan: CandidatePlan, code: Option<&str>) {
    plan.module_fingerprint = module.fingerprint();
    let out = directory(label);
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    fs::write(out.join("input.wire"), &wire).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits::default(),
            ),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()),
        ),
    ] {
        fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
        if let Some(code) = code {
            let errors = result.expect_err(label);
            assert!(
                errors.iter().all(|d| d.phase == Phase::Verify
                    && d.category != ResultCategory::ResourceLimit
                    && !d.message.contains("fingerprint")),
                "{label}/{route}: {errors:?}"
            );
            assert!(
                errors.iter().any(|d| d.code == code),
                "{label}/{route}: {errors:?}"
            );
        } else {
            result.unwrap_or_else(|e| panic!("{label}/{route}: {e:?}"));
        }
    }
}
#[test]
fn same_type_other_instance_and_incomplete_child_are_not_authority() {
    let mut module = typed("distinct-instances");
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    check(
        "distinct-instances-control",
        module.clone(),
        plan.clone(),
        None,
    );
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let a = main.locals.iter().find(|l| l.name == "first").unwrap().id;
    let b = main.locals.iter().find(|l| l.name == "second").unwrap().id;
    let i = main
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,InstructionKind::BorrowLocal{local}if local==a))
        .unwrap();
    i.kind = InstructionKind::BorrowLocal { local: b };
    // Coherently retarget the public Hold as well; stale fingerprints and a
    // trivial producer-metadata mismatch cannot stand in for the live loan.
    let candidate = plan
        .functions
        .iter_mut()
        .find(|f| f.function == main.id)
        .unwrap();
    let mut changed = 0;
    for hold in &mut candidate.holds {
        for root in &mut hold.roots {
            if *root == HoldRoot::Local(a) {
                *root = HoldRoot::Local(b);
                changed += 1;
            }
        }
    }
    assert!(changed > 0);
    check(
        "retarget-same-type-instance-with-coherent-hold",
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );

    let mut module = typed("child-nll-control");
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    check(
        "child-before-take-control",
        module.clone(),
        plan.clone(),
        None,
    );
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let pair = main.locals.iter().find(|l| l.name == "pair").unwrap().id;
    let block = main
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions.iter().any(
                |i| matches!(i.kind,InstructionKind::BorrowPackageField{local,..}if local==pair),
            )
        })
        .unwrap();
    let take = block
        .instructions
        .iter()
        .position(
            |i| matches!(i.kind,InstructionKind::TakePackageField{local,index:0}if local==pair),
        )
        .unwrap();
    let instruction = block.instructions.remove(take);
    let borrow=block.instructions.iter().position(|i|matches!(i.kind,InstructionKind::BorrowPackageField{local,index:0,..}if local==pair)).unwrap();
    block.instructions.insert(borrow, instruction);
    check(
        "borrow-child-after-take",
        module,
        plan,
        Some("E-VERIFY-RESOURCE"),
    );
}

const DISTINCT: &str = r#"extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32){unsafe{putchar(c)}}
resource Token{code:i32;value:i32}
impl Drop for Token{fn drop(self:Self){emit(.code)}}
package Leaf{manages owned:Token;weight:i32}
package Pair{left:Leaf;right:Leaf;tag:i32}
fn leaf(p:&Leaf)->i32{return p.weight+p.owned.value}
fn both(p:&Pair,turn:bool)->i32{let alias=p;var n=0;for i in 0..2{if turn{n+=leaf(&alias.left)}else{n+=leaf(&alias.right)}}return n+alias.tag}
fn main()->i32{let first=Pair{left:Leaf{owned:Token{code:65,value:5},weight:7},right:Leaf{owned:Token{code:66,value:11},weight:13},tag:3};let second=Pair{left:Leaf{owned:Token{code:67,value:5},weight:7},right:Leaf{owned:Token{code:68,value:11},weight:13},tag:3};let p=&first;let moved=second;let n=both(p,true)+both(&moved,false);emit(88);return n-78}
"#;
