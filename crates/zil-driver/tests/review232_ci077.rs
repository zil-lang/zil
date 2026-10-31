//! Independent diagnosis of the original077 raw selector after232 lowering.
//! The original source bytes are retained; forged IR never reaches codegen.
use std::{
    fs,
    path::PathBuf,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind as I, TypedModule};

const DEFINITIONS: &str = "resource R{n:i32} package P{manages a:R,manages b:R,n:i32} fn make()->P{return P{a:R{n:20},b:R{n:21},n:1}} fn consume(give r:R)->i32{return r.n} fn keep(give r:R,n:i32)->R contract{result(give)}{return give r} fn step(ok:bool)->bool!i32{if ok{return 0}return true} fn bump(r:R){r.n+=1}";
const READER: &str = "fn factory(p:P)->impl Fn()->i32{return move ()=>p.a.n+p.n}fn main()->i32{let f=factory(make());let moved=f;return moved()+moved()-42}";
const EXPLICIT_DESC_READER: &str = "fn factory(p:P)->impl Fn()->i32{return move ()=>{let d=p.a;return d.n+p.n}}fn main()->i32{let f=factory(make());let moved=f;return moved()+moved()-42}";
static RUN: OnceLock<String> = OnceLock::new();

fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(RUN.get_or_init(|| {
            format!(
                "review077-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        }))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}

fn typed(body: &str) -> (String, TypedModule) {
    let text = format!("{DEFINITIONS} {body}");
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "captures077.zil", &text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    (text, module)
}

fn semantic_rejection<T>(result: Result<T, Vec<Diagnostic>>, code: &str, message: &str) {
    let errors = result
        .err()
        .expect("a changed, malformed whole-view descriptor must be rejected");
    assert!(!errors.is_empty());
    assert!(
        errors
            .iter()
            .all(|d| d.category == ResultCategory::SafeReject
                && d.code == code
                && d.message == message),
        "{errors:#?}"
    );
    assert!(
        errors
            .iter()
            .all(|d| d.category != ResultCategory::ResourceLimit
                && !d.code.starts_with("E-VERIFY-PLAN")),
        "{errors:#?}"
    );
}

fn observe(label: &str, body: &str, require_one_hit: bool) -> usize {
    let out = directory(label);
    let (source, original) = typed(body);
    fs::write(out.join("source.zil"), source).unwrap();
    fs::write(out.join("original-typed.txt"), format!("{original:#?}")).unwrap();
    let limits = Limits::default();
    let original_plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(
        original.clone(),
        original_plan.clone(),
        &original.target,
        &limits,
    )
    .unwrap();
    let original_wire =
        zil_ir::wire::encode_module_candidate(&original, &original_plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&original_wire, &original.target, &limits).unwrap();
    fs::write(out.join("original.wire"), &original_wire).unwrap();
    let mut module = original.clone();
    assert_eq!(module.closures.len(), 1);
    let entry_index = module.closures[0].function.0 as usize;
    let entry = &mut module.functions[entry_index];
    let views = entry
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i.kind {
            I::BindPackageCapture { local, index: 0 } => Some(local),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(views.len(), 1);
    let local = views[0];
    let mut hit_count = 0usize;
    let mut projected_address_count = 0usize;
    let mut field_address_count = 0usize;
    let mut read_address_count = 0usize;
    for instruction in entry.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        projected_address_count +=
            usize::from(matches!(instruction.kind, I::BorrowPackageField { .. }));
        field_address_count += usize::from(matches!(instruction.kind, I::AddressField { .. }));
        read_address_count += usize::from(matches!(instruction.kind, I::ReadAddress { .. }));
        if matches!(instruction.kind, I::DescribePackageField { .. }) {
            instruction.kind = I::DescribeLocal { local };
            hit_count += 1;
        }
    }
    let typed_equal = original == module;
    let mut plan = original_plan;
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let wire_equal = original_wire == wire;
    fs::write(out.join("mutated-typed.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("mutated-plan.txt"), format!("{plan:#?}")).unwrap();
    fs::write(out.join("mutated.wire"), &wire).unwrap();
    let analyze = zil_analyze::analyze(&module, &limits);
    let raw = zil_verify::verify_module(module.clone(), plan, &module.target, &limits);
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &limits);
    fs::write(out.join("analyze.txt"), format!("{analyze:#?}")).unwrap();
    fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
    fs::write(out.join("wire.txt"), format!("{decoded:#?}")).unwrap();
    fs::write(out.join("OBSERVATION.json"), format!("{{\"describe_package_field_hits\":{hit_count},\"borrow_package_field_count\":{projected_address_count},\"address_field_count\":{field_address_count},\"read_address_count\":{read_address_count},\"typed_unchanged\":{typed_equal},\"wire_unchanged\":{wire_equal},\"analyze_accepted\":{},\"raw_accepted\":{},\"wire_accepted\":{}}}\n", analyze.is_ok(), raw.is_ok(), decoded.is_ok())).unwrap();
    if require_one_hit {
        assert_eq!(
            hit_count, 1,
            "the invalid-IR attack must select exactly one real descriptor producer"
        );
    }
    assert!(hit_count <= 1);
    if hit_count == 0 {
        // Observation of the old selector, not an invalid-IR safety claim.
        assert!(!require_one_hit);
        assert!(typed_equal && wire_equal);
        assert!(analyze.is_ok() && raw.is_ok() && decoded.is_ok());
    } else {
        assert!(!typed_equal && !wire_equal);
        semantic_rejection(
            analyze,
            "E-ANALYZE-IR",
            "resource closure: Package view requires a precise field description",
        );
        semantic_rejection(
            raw,
            "E-VERIFY-RESOURCE",
            "DescribeLocal requires an actual managed position",
        );
        semantic_rejection(
            decoded,
            "E-VERIFY-RESOURCE",
            "DescribeLocal requires an actual managed position",
        );
    }
    hit_count
}

#[test]
fn original077_source_and_selector_have_explicit_mutation_evidence() {
    let hits = observe("original077-selector", READER, false);
    if let Ok(expected) = std::env::var("ZIL_REVIEW077_EXPECT_ORIGINAL_HITS") {
        assert_eq!(hits, expected.parse::<usize>().unwrap());
    }
}

#[test]
fn whole_view_disguised_as_managed_descriptor_still_rejects_a_real_mutation() {
    observe("explicit-descriptor-attack", EXPLICIT_DESC_READER, true);
}
