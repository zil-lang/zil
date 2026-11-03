//! Exact original223 attack bytes, plus an explicitly migrated schema32 envelope.
use std::{fs, path::PathBuf};
use zil_base::{Fingerprint, Limits, Phase, ResultCategory, TargetProfile};

const ORIGINAL: &[u8] = include_bytes!("review223_fixtures/mutable-product-original.wire");
const SCHEMA29: &[u8] = include_bytes!("review223_fixtures/mutable-product-schema29.wire");
const SCHEMA30: &[u8] = include_bytes!("review223_fixtures/schema30-mutable-product.wire");
const SCHEMA32: &[u8] = include_bytes!("review223_fixtures/schema32-mutable-product.wire");

#[test]
fn frozen_schema27_29_and30_wire_are_rejected_as_old_transports() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (wire, hash) in [
        (
            ORIGINAL,
            "5ba4b35d45093a324e31ab8971d92788a1e9c18b57cbdc17fdd317c7d2f59de9",
        ),
        (
            SCHEMA29,
            "3d5bf5e48a6a0856d3fad6c8b7997e8ba5ccb7adfbb0327a2e7e0852d4da4198",
        ),
        (
            SCHEMA30,
            "783db64403cec14468080ce9708d4f25a4cbd315aaed36d2f994994f7f70e3c9",
        ),
    ] {
        assert_eq!(Fingerprint::of(wire).to_hex(), hash);
        for errors in [
            zil_ir::wire::decode_module_candidate(wire, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(wire, &target, &limits).unwrap_err(),
        ] {
            assert_eq!(errors.len(), 1, "{errors:#?}");
            let error = &errors[0];
            assert_eq!(error.phase, Phase::Verify);
            assert_eq!(error.category, ResultCategory::SafeReject);
            assert_eq!(error.code, "E-WIRE-VERSION");
        }
    }
}

// The archived attack contains no foreign declarations or return contracts.
// Decode its separately migrated Wire32 fixture and check every unchanged byte
// against the original; no legacy decoder is exposed in production.
fn migrate_original_envelope(limits: &Limits) -> Vec<u8> {
    assert_eq!(&ORIGINAL[..8], b"ZILMODP\0");
    assert_eq!(&ORIGINAL[8..12], &27u32.to_le_bytes());
    assert_eq!(zil_ir::wire::WIRE_VERSION, 32);
    assert_eq!(zil_base::SCHEMA_VERSION, 32);
    let (module, plan) = zil_ir::wire::decode_module_candidate(SCHEMA32, limits).unwrap();
    assert_eq!(module.schema_version, 32);
    assert_eq!(plan.schema_version, 32);
    assert_eq!(plan.target_fingerprint, module.target.fingerprint());
    assert_eq!(plan.module_fingerprint, module.fingerprint());
    assert!(
        module
            .functions
            .iter()
            .all(|f| f.foreign.is_none() && f.callable_return_contract.is_none())
    );
    let migrated = zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap();
    let mut expected = ORIGINAL.to_vec();
    for offset in [8, 12, 1538] {
        assert_eq!(&ORIGINAL[offset..offset + 4], &27u32.to_le_bytes());
        expected[offset..offset + 4].copy_from_slice(&32u32.to_le_bytes());
    }
    expected[1542..1574].copy_from_slice(&plan.module_fingerprint.0);
    assert_eq!(migrated, expected);
    assert_eq!(migrated, SCHEMA32);
    migrated
}

#[test]
fn frozen_mutable_product_raw_and_wire_is_promoted_by_315() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    assert_eq!(
        Fingerprint::of(SCHEMA32).to_hex(),
        "8cc0e4a565e57b71135b3c692079dcaa65dfa4ce6f49dbe36c253182df497862"
    );
    assert_eq!(
        Fingerprint::of(SCHEMA30).to_hex(),
        "783db64403cec14468080ce9708d4f25a4cbd315aaed36d2f994994f7f70e3c9"
    );
    let current = migrate_original_envelope(&limits);
    let (module, plan) = zil_ir::wire::decode_module_candidate(&current, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap(),
        current
    );
    assert_eq!(module.fingerprint(), plan.module_fingerprint);
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("frozen-mutable-replay");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("schema32-mutable-product.wire"), &current).unwrap();
    let results = [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&current, &target, &limits),
        ),
    ];
    for (route, result) in &results {
        fs::write(root.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
    }
    for (route, result) in results {
        // The exact original223 current envelope now has an explicit315
        // synchronous incoming &mut contract. It writes only the outer Copy
        // tag; historical ORACLE.json and all frozen Wire bytes remain intact.
        let verified = result.unwrap_or_else(|e| panic!("{route}: {e:?}"));
        assert_eq!(verified.fingerprint(), module.fingerprint());
    }
    fs::write(
        root.join("current-result.json"),
        "{\"raw\":\"Success\",\"wire\":\"Success\",\"rule\":\"8.1.2.1\"}\n",
    )
    .unwrap();

    for owned in [false, true] {
        let mut changed = module.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "write")
            .unwrap();
        if owned {
            let zil_ir::Type::Address { inner, .. } = &f.locals[f.parameters[0].0 as usize].ty
            else {
                unreachable!()
            };
            let zil_ir::Type::Package { fields, .. } = inner.as_ref() else {
                unreachable!()
            };
            let child = fields[0].1.clone();
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let zil_ir::InstructionKind::AddressField { index, .. } = &mut i.kind {
                    *index = 0;
                    f.values[i.result.unwrap().0 as usize] = zil_ir::Type::Address {
                        mutable: true,
                        inner: Box::new(child.clone()),
                    };
                }
            }
        } else {
            for ty in f
                .locals
                .iter_mut()
                .map(|l| &mut l.ty)
                .chain(f.values.iter_mut())
            {
                if let zil_ir::Type::Address { mutable, .. } = ty {
                    *mutable = false;
                }
            }
        }
        let mut p = plan.clone();
        p.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &p, &limits).unwrap();
        let label = if owned { "owned-child" } else { "shared-write" };
        fs::write(root.join(format!("{label}.wire")), &wire).unwrap();
        for (route, result) in [
            (
                "raw",
                zil_verify::verify_module(changed, p, &target, &limits),
            ),
            (
                "wire",
                zil_verify::verify_module_bytes(&wire, &target, &limits),
            ),
        ] {
            let errors = result.expect_err(label);
            assert_eq!(errors.len(), 1, "{label}/{route}: {errors:?}");
            assert_eq!(errors[0].phase, Phase::Verify);
            assert_eq!(
                errors[0].code,
                if owned {
                    "E-UNSUPPORTED"
                } else {
                    "E-VERIFY-ADDRESS"
                },
                "{label}/{route}: {errors:?}"
            );
            fs::write(
                root.join(format!("{label}-{route}.json")),
                errors[0].to_json(),
            )
            .unwrap();
        }
    }
    let text = include_str!("review223_fixtures/mutable_control.zil").replace("write(p:&mut Leaf)","write(p:&mut Root)").replace("fn main(){}","fn main(){var root=Root{child:Leaf{owner:R{n:1},tag:2},tag:3};let child=root.child;write(&mut root)}");
    fs::write(root.join("incomplete-call.zil"), &text).unwrap();
    let r = zil_driver::compile_source(
        &zil_base::SourceUnit::new(
            zil_base::FileId(0),
            zil_base::ModuleId(0),
            "incomplete223.zil",
            text,
        ),
        &zil_driver::CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..Default::default()
        },
    );
    fs::write(root.join("incomplete-call.json"), r.to_json()).unwrap();
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Analyze, "{}", r.to_json());
    assert!(
        r.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"),
        "{}",
        r.to_json()
    );
}
