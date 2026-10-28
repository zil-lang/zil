//! A local flat capture must not acquire the ordinary Product profile through
//! the raw entry. This oracle follows the frozen 206 capture exclusion.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

const CONTROL: &str = "resource R{n:i32}\npackage Leaf{manages a:R;n:i32}\nfn local_capture(p:Leaf)->i32{let f=move()=>p.n;return f()}\nfn main(){}\n";

struct Records(PathBuf, bool);
impl Records {
    fn new() -> Self {
        let output = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let path = output
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("independent206-capture-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path, output.is_some())
    }
    fn write(&self, name: &str, content: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), content).unwrap();
    }
}
impl Drop for Records {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn pair(
    records: &Records,
    label: &str,
    typed: &TypedModule,
    candidate: &CandidatePlan,
    accept: bool,
) {
    let limits = Limits::default();
    records.write(&format!("{label}.typed.txt"), format!("{typed:#?}"));
    records.write(&format!("{label}.candidate.txt"), format!("{candidate:#?}"));
    let wire = zil_ir::wire::encode_module_candidate(typed, candidate, &limits).unwrap();
    records.write(&format!("{label}.wire"), &wire);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (typed.clone(), candidate.clone())
    );
    // Both actual entry points run before the assertion, including a failure.
    let raw = zil_verify::verify_module(typed.clone(), candidate.clone(), &typed.target, &limits);
    let bytes = zil_verify::verify_module_bytes(&wire, &typed.target, &limits);
    records.write(
        &format!("{label}.verify.txt"),
        format!("raw={raw:#?}\nwire={bytes:#?}"),
    );
    for result in [raw, bytes] {
        if accept {
            result.unwrap();
        } else {
            let errors = result.expect_err("206 ordinary Product capture must remain closed");
            assert!(errors.iter().all(|e| e.code != "E-VERIFY-PLAN-IDENTITY"));
            assert!(errors.iter().any(|e| e.phase == Phase::Verify
                && matches!(
                    e.category,
                    ResultCategory::Unsupported | ResultCategory::SafeReject
                )));
            assert!(
                errors
                    .iter()
                    .all(|e| e.category != ResultCategory::ResourceLimit)
            );
        }
    }
}

#[test]
fn raw_local_flat_capture_cannot_be_coordinately_retyped_as_product() {
    let records = Records::new();
    records.write("control.zil", CONTROL);
    let source = SourceUnit::new(FileId(0), ModuleId(0), "capture206.zil", CONTROL);
    let syntax = zil_syntax::parse(&source).into_result();
    records.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&source, &syntax.unwrap());
    records.write("core.txt", format!("{core:#?}"));
    let typed = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    records.write("types.txt", format!("{typed:#?}"));
    let original = typed.unwrap();
    assert_eq!(original.closures.len(), 1);
    assert_eq!(original.closures[0].kind, CallableKind::Fn);
    assert_eq!(original.closures[0].captures.len(), 1);
    assert!(
        original
            .functions
            .iter()
            .all(|f| f.callable_return_contract.is_none())
    );
    let candidate = zil_analyze::analyze(&original, &Limits::default());
    records.write("control.analyze.txt", format!("{candidate:#?}"));
    let mut candidate = candidate.unwrap();
    pair(&records, "control", &original, &candidate, true);

    let leaf = original.closures[0].captures[0].clone();
    let Type::Package { fields, .. } = &leaf else {
        panic!("flat Package control")
    };
    assert_eq!(fields.len(), 2);
    assert!(matches!(fields[0].1, Type::Managed(_)));
    assert_eq!(fields[1], ("n".into(), Type::I32));
    let product = Type::Package {
        name: "Independent206Root".into(),
        fields: vec![("child".into(), leaf.clone()), ("n".into(), Type::I32)],
    };
    let mut changed = original.clone();
    let mut replacements = 0;
    for f in &mut changed.functions {
        for ty in f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values) {
            if *ty == leaf {
                *ty = product.clone();
                replacements += 1;
            }
        }
    }
    changed.closures[0].captures[0] = product;
    assert!(
        replacements >= 3,
        "parameter, actual SSA, and bound view must agree"
    );
    // No opcode, field index, CFG, scalar use, Bind/End, return contract or
    // initialization claim changes. Copy field n stays at index 1, while a
    // complete child occupies the old direct Managed responsibility's slot.
    let entry = &changed.functions[changed.closures[0].function.0 as usize];
    assert!(
        entry
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::BindPackageCapture { index: 0, .. }))
    );
    assert!(
        entry
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::EndPackageCapture { index: 0, .. }))
    );
    let analyze = zil_analyze::analyze(&changed, &Limits::default());
    records.write("product.analyze.txt", format!("{analyze:#?}"));
    let errors = analyze.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.phase == Phase::Analyze && e.category == ResultCategory::Unsupported)
    );
    candidate.module_fingerprint = changed.fingerprint();
    pair(&records, "product", &changed, &candidate, false);
}
