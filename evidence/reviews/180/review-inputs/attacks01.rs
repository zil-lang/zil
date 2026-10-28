//! Independent 180 review: actual current-call aliases and forged unique values.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, ProjectSources, compile_project};
use zil_ir::{CandidatePlan, InstructionKind as I, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "review180-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
const MODEL: &str = r#"
resource Ticket{identity:i32} impl Drop for Ticket{fn drop(self:Self){}}
package Entry{manages ticket:Ticket;count:i32;coords:[i32;2]}
fn entry(n:i32)->Entry=>Entry{ticket:Ticket{identity:n},count:n,coords:[2,3]}
fn empty()->Store<Entry> contract{result(create)}{return store_new()}
fn inspect(p:&Entry)->i32=>p.count
"#;
const SETUP: &str =
    "let s=empty();store_reserve(s,4);store_push(s,entry(10));store_push(s,entry(20));";
const STATE: &str = r#"
package State{manages left:Store<Entry>;manages right:Store<Entry>}
fn make()->State{let a=empty();let b=empty();store_reserve(a,1);store_reserve(b,1);store_push(a,entry(3));store_push(b,entry(5));return State{left:give a,right:give b}}
"#;

fn rejected_source(label: &str, source: &str, category: ResultCategory, phase: Phase, code: &str) {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: BTreeMap::new(),
        },
        &CompileOptions::default(),
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{label}: {}",
        report.to_json()
    );
}

fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review180.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn reject_pair(label: &str, module: TypedModule, mut plan: CandidatePlan, code: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
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
        assert!(errors.iter().all(|d| d.phase == Phase::Verify));
        assert!(
            errors.iter().any(|d| d.code == code),
            "{label}/{route}: {errors:#?}"
        );
    }
}

#[test]
fn current_call_reborrows_and_hidden_callback_writes_keep_actual_aliases() {
    let cases = [
        (
            "shared-and-unique-args",
            "fn pair(a:&mut Entry,b:&Entry)->i32{a.count+=b.count;return a.count}",
            "let p=store_at_mut(s,0);return pair(p,p)",
        ),
        (
            "two-unique-args",
            "fn pair(a:&mut Entry,b:&mut Entry)->i32{a.count+=1;b.count+=2;return a.count+b.count}",
            "let p=store_at_mut(s,0);return pair(p,p)",
        ),
        (
            "current-call-structural-effect",
            "fn pair(a:&Entry,b:Store<Entry>)->i32{store_reserve(b,32);return a.count}",
            "let p=store_at_mut(s,0);return pair(p,s)",
        ),
        (
            "captured-store-mut-borrow",
            "",
            "let held=store_at(s,1);var change=()=>{let p=store_at_mut(s,0);p.count=30;return p.count};return change()+held.count",
        ),
        (
            "last-call-captured-parent-conflict",
            "",
            "let p=store_at_mut(s,0);let child=&mut p.count;var change=()=>{store_reserve(s,32);*child+=1;return *child};return change()",
        ),
    ];
    for (label, helpers, body) in cases {
        rejected_source(
            label,
            &format!("{MODEL}{helpers}fn main()->i32{{{SETUP}{body}}}"),
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn escaping_mutable_window_or_child_address_remains_closed() {
    for (label, helper) in [
        (
            "return-element",
            "fn escape(s:Store<Entry>)->&mut Entry{return store_at_mut(s,0)}",
        ),
        (
            "return-copy-child",
            "fn escape(s:Store<Entry>)->&mut i32{let p=store_at_mut(s,0);return &mut p.count}",
        ),
    ] {
        rejected_source(
            label,
            &format!("{MODEL}{helper}fn main()->i32=>0;"),
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn raw_unique_window_cannot_be_copied_or_manufactured_from_access_load() {
    let source = format!(
        "{MODEL}fn main()->i32{{{SETUP}let p=store_at_mut(s,0);let moved=p;return moved.count}}"
    );
    let (module, plan) = candidate(&source);
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let mut bad = module.clone();
    let f = &mut bad.functions[main];
    let address_locals: Vec<_> = f
        .locals
        .iter()
        .filter(|l| l.ty.is_package_address())
        .map(|l| l.id)
        .collect();
    let instruction = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,I::TakeLocal{local} if address_locals.contains(&local)))
        .unwrap();
    if let I::TakeLocal { local } = instruction.kind {
        instruction.kind = I::Load(local);
    }
    reject_pair(
        "copy-loaded-unique-window",
        bad,
        plan.clone(),
        "E-VERIFY-MOVE",
    );

    let mut bad = module;
    let f = &mut bad.functions[main];
    let original = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::OwnedStoreBorrowMut { .. }))
        .unwrap()
        .result
        .unwrap();
    let mut stores = 0;
    for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::Store { local, value } = &mut instruction.kind
            && address_locals.contains(local)
        {
            stores += 1;
            if stores == 2 {
                *value = original;
            }
        }
    }
    assert_eq!(stores, 2);
    reject_pair("duplicate-consumed-unique-ssa", bad, plan, "E-VERIFY-MOVE");
}

#[test]
fn raw_copy_projection_does_not_authorize_package_or_resource_access() {
    let source =
        format!("{MODEL}fn main()->i32{{{SETUP}let p=store_at_mut(s,0);p.count=9;return p.count}}");
    let (module, plan) = candidate(&source);
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let mut bad = module.clone();
    let projection = bad.functions[main]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::AddressField { .. }))
        .unwrap();
    if let I::AddressField { index, .. } = &mut projection.kind {
        *index = u32::MAX;
    }
    reject_pair(
        "invalid-copy-field-index",
        bad,
        plan.clone(),
        "E-VERIFY-ADDRESS",
    );

    let mut bad = module.clone();
    let projection = bad.functions[main]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::AddressField { .. }))
        .unwrap();
    if let I::AddressField { address, .. } = projection.kind {
        projection.kind = I::ReadAddress { address };
    }
    reject_pair("load-entire-package", bad, plan.clone(), "E-UNSUPPORTED");

    let mut bad = module;
    let f = &mut bad.functions[main];
    let package = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::AddressField { .. }))
        .and_then(|i| {
            if let I::AddressField { address, .. } = i.kind {
                Some(address)
            } else {
                None
            }
        })
        .unwrap();
    let write = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::WriteAddress { .. }))
        .unwrap();
    if let I::WriteAddress { address, .. } = &mut write.kind {
        *address = package;
    }
    reject_pair("overwrite-entire-package", bad, plan, "E-UNSUPPORTED");
}

#[test]
fn raw_call_retargeting_cannot_claim_two_unique_actual_sources() {
    let source = format!(
        "{MODEL}{STATE}fn pair(a:&mut Entry,b:&mut Entry)->i32{{a.count+=1;b.count+=2;return a.count+b.count}}fn main()->i32{{let s=make();let a=store_at_mut(s.left,0);let b=store_at_mut(s.right,0);return pair(a,b)}}"
    );
    let (module, plan) = candidate(&source);
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let pair = module
        .functions
        .iter()
        .find(|f| f.name == "pair")
        .unwrap()
        .id;
    let mut bad = module.clone();
    let call = bad.functions[main]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,I::Call{function,..} if function==pair))
        .unwrap();
    if let I::Call { arguments, .. } = &mut call.kind {
        arguments[1] = arguments[0];
    }
    reject_pair(
        "duplicate-current-call-unique-value",
        bad,
        plan.clone(),
        "E-VERIFY-MOVE",
    );

    let mut bad = module;
    let f = &mut bad.functions[main];
    let package_addresses: Vec<_> = f
        .values
        .iter()
        .enumerate()
        .filter(|(_, ty)| {
            matches!(ty, Type::Address { mutable: true, .. }) && ty.is_package_address()
        })
        .map(|(i, _)| zil_ir::ValueId(i as u32))
        .collect();
    let mut first = None;
    let mut changed = 0;
    for inst in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::ReborrowAddress {
            address,
            mutable: true,
        } = &mut inst.kind
            && package_addresses.contains(address)
        {
            if let Some(value) = first {
                *address = value;
                changed += 1;
            } else {
                first = Some(*address);
            }
        }
    }
    assert_eq!(changed, 1);
    reject_pair(
        "retarget-current-call-second-child",
        bad,
        plan,
        "E-BORROW-CONFLICT",
    );
}
