//! Independent transport probes. Raw claims never grant execution permission.
use std::{collections::BTreeSet, fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, TypedModule};

const SOURCE: &str = "fn make(a:i32,b:i32)->impl Fn()->i32{return move()=>a+b}fn main()->i32{let f=make(2,3);return f()-5}";

fn fixture() -> (TypedModule, CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "review185.zil", SOURCE);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    assert_eq!(module.closures[0].captures.len(), 2);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    (module, plan)
}

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("zil-review185"));
    let dir = root.join(label);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.zil"), SOURCE).unwrap();
    dir
}

fn contract(module: &mut TypedModule) -> &mut zil_ir::CallableReturnContract {
    module
        .functions
        .iter_mut()
        .find_map(|f| f.callable_return_contract.as_mut())
        .unwrap()
}

#[test]
fn reviewer_in_range_duplicate_and_unsorted_claims_cannot_hide_behind_bounds() {
    for (label, indices, category) in [
        ("duplicate", vec![0, 0], ResultCategory::SafeReject),
        ("unsorted", vec![1, 0], ResultCategory::SafeReject),
        ("first", vec![0], ResultCategory::Unsupported),
        ("second", vec![1], ResultCategory::Unsupported),
        ("both", vec![0, 1], ResultCategory::Unsupported),
    ] {
        let (mut module, mut plan) = fixture();
        module.closures[0].lending_captures = indices;
        plan.module_fingerprint = module.fingerprint();
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let dir = directory(&format!("review185-{label}"));
        fs::write(dir.join("candidate.bin"), &bytes).unwrap();
        let target = module.target.clone();
        for (path, result) in [
            (
                "raw",
                zil_verify::verify_module(module, plan, &target, &Limits::default()),
            ),
            (
                "wire",
                zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()),
            ),
        ] {
            let diagnostics = result.unwrap_err();
            fs::write(dir.join(format!("{path}.txt")), format!("{diagnostics:#?}")).unwrap();
            assert!(
                diagnostics.iter().any(|d| d.phase == Phase::Verify
                    && d.category == category
                    && d.message.contains("lending")),
                "{label}/{path}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn reviewer_both_wire_count_fields_are_checked_before_allocation() {
    let narrow = Limits {
        max_locals: 16,
        ..Limits::default()
    };
    for contract_only in [false, true] {
        let (mut module, plan) = fixture();
        let indices: Vec<_> = (0..17).map(|i| 0x1851_0000 + i).collect();
        if contract_only {
            contract(&mut module).lending_captures = indices;
        } else {
            module.closures[0].lending_captures = indices;
        }
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let label = if contract_only {
            "contract"
        } else {
            "definition"
        };
        let dir = directory(&format!("review185-count-{label}"));
        fs::write(dir.join("count17.bin"), &bytes).unwrap();
        let diagnostics = zil_ir::wire::decode_module_candidate(&bytes, &narrow).unwrap_err();
        fs::write(dir.join("count17.txt"), format!("{diagnostics:#?}")).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );

        // Locate our unique payload, then forge its immediately preceding count.
        // No semantic producer or declaration count is trusted by this attack.
        let marker: Vec<u8> = [0x1851_0000u32, 0x1851_0001]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let positions: Vec<_> = bytes
            .windows(marker.len())
            .enumerate()
            .filter_map(|(i, part)| (part == marker).then_some(i))
            .collect();
        assert_eq!(positions.len(), 1);
        let offset = positions[0] - 4;
        assert_eq!(&bytes[offset..offset + 4], &17u32.to_le_bytes());
        let mut huge = bytes;
        huge[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(dir.join("forged-count.bin"), &huge).unwrap();
        let diagnostics =
            zil_ir::wire::decode_module_candidate(&huge, &Limits::default()).unwrap_err();
        fs::write(dir.join("forged-count.txt"), format!("{diagnostics:#?}")).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
    }
}

#[test]
fn reviewer_same_length_source_indices_and_record_positions_have_distinct_identity() {
    let mut module_hashes = BTreeSet::new();
    let mut contract_hashes = BTreeSet::new();
    let dir = directory("review185-fingerprints");
    let mut observations = String::new();
    for (definition, exported) in [
        (vec![], vec![]),
        (vec![0], vec![]),
        (vec![1], vec![]),
        (vec![], vec![0]),
        (vec![], vec![1]),
        (vec![0], vec![1]),
        (vec![1], vec![0]),
    ] {
        let (mut module, mut plan) = fixture();
        module.closures[0].lending_captures = definition.clone();
        contract(&mut module).lending_captures = exported.clone();
        let fingerprint = module.fingerprint();
        assert!(module_hashes.insert(fingerprint));
        plan.module_fingerprint = fingerprint;
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let (decoded, decoded_plan) =
            zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
        assert_eq!(decoded, module);
        assert_eq!(decoded_plan, plan);
        observations.push_str(&format!("{definition:?}/{exported:?}: {fingerprint:?}\n"));
    }
    for indices in [vec![], vec![0], vec![1], vec![0, 1], vec![1, 0]] {
        let (mut module, _) = fixture();
        let c = contract(&mut module);
        c.lending_captures = indices.clone();
        let fingerprint = zil_ir::callable_return_contract_fingerprint(c);
        assert!(contract_hashes.insert(fingerprint));
        observations.push_str(&format!("contract {indices:?}: {fingerprint:?}\n"));
    }
    fs::write(dir.join("observations.txt"), observations).unwrap();
}
