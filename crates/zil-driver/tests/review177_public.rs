//! Independent public21 review. Manual byte oracles exercise the real transport;
//! Public-only semantic stub probes are frozen in tests/frozen/177_public/.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, Span, TargetProfile,
};
use zil_driver::{CompileOptions, ProjectSources, compile_project};
use zil_ir::{CandidatePlan, Instruction, InstructionKind, LocalId, TypedModule, ValueId, wire};

fn record(name: &str, bytes: impl AsRef<[u8]>) {
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let root = PathBuf::from(root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(name), bytes).unwrap();
    }
}

fn base() -> (TypedModule, CandidatePlan) {
    let source = "fn main()->i32{let n=0;return n}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review177.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &TargetProfile::host_llvm(),
        &limits,
    )
    .unwrap();
    (module, plan)
}

fn append(instruction: Instruction) -> (TypedModule, CandidatePlan) {
    let (mut module, mut plan) = base();
    module.functions[0].blocks[0].instructions.push(instruction);
    plan.module_fingerprint = module.fingerprint();
    (module, plan)
}

fn span() -> Span {
    Span::new(FileId(0x1413_1211), 0x1817_1615, 0x1c1b_1a19)
}

fn cases() -> [(InstructionKind, Vec<u8>); 3] {
    [
        (
            InstructionKind::OwnedStoreBorrow {
                store: ValueId(0x4433_2211),
                index: ValueId(0x8877_6655),
            },
            vec![86, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        ),
        (
            InstructionKind::BorrowPackageField {
                local: LocalId(0x4a39_2817),
                index: 0x8e7d_6c5b,
                mutable: false,
            },
            vec![87, 0x17, 0x28, 0x39, 0x4a, 0x5b, 0x6c, 0x7d, 0x8e, 0],
        ),
        (
            InstructionKind::BorrowPackageField {
                local: LocalId(0x4a39_2817),
                index: 0x8e7d_6c5b,
                mutable: true,
            },
            vec![87, 0x17, 0x28, 0x39, 0x4a, 0x5b, 0x6c, 0x7d, 0x8e, 1],
        ),
    ]
}

fn error(errors: &[Diagnostic], category: ResultCategory, code: &str) {
    assert!(
        errors
            .iter()
            .any(|d| d.category == category && d.code == code),
        "{errors:?}"
    );
}

#[test]
fn literal_instruction_bytes_survive_real_module_envelopes_and_reject_corruptions() {
    let limits = Limits::default();
    for (case, (kind, body)) in cases().into_iter().enumerate() {
        for present in [false, true] {
            let instruction = Instruction {
                result: present.then_some(ValueId(0x0d0c_0b0a)),
                kind: kind.clone(),
                span: span(),
            };
            let (module, plan) = append(instruction.clone());
            let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            // All expected bytes are literal protocol values, never produced by
            // a second encoder or copied from an actual encoded snapshot.
            let mut expected = if present {
                vec![1, 0x0a, 0x0b, 0x0c, 0x0d]
            } else {
                vec![0]
            };
            let opcode = expected.len();
            expected.extend(&body);
            expected.extend([
                0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0, 0, 0, 0, 0x19, 0x1a, 0x1b, 0x1c,
                0, 0, 0, 0,
            ]);
            let offsets: Vec<_> = bytes
                .windows(expected.len())
                .enumerate()
                .filter_map(|(i, row)| (row == expected).then_some(i))
                .collect();
            assert_eq!(offsets.len(), 1);
            let offset = offsets[0];
            let (decoded, recovered) = wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(
                decoded.functions[0].blocks[0].instructions.last(),
                Some(&instruction)
            );
            assert_eq!(decoded.fingerprint(), module.fingerprint());
            assert_eq!(recovered.module_fingerprint, plan.module_fingerprint);
            record(&format!("manual-{case}-{present}.bin"), &bytes);
            record(
                &format!("manual-{case}-{present}.txt"),
                format!("offset={offset}\nexpected={expected:02x?}\n"),
            );

            let mut bad = bytes.clone();
            bad[offset] = 2;
            error(
                &wire::decode_module_candidate(&bad, &limits).unwrap_err(),
                ResultCategory::SafeReject,
                "E-WIRE-BOOL",
            );
            for unknown in [89, 254] {
                let mut bad = bytes.clone();
                bad[offset + opcode] = unknown;
                error(
                    &wire::decode_module_candidate(&bad, &limits).unwrap_err(),
                    ResultCategory::SafeReject,
                    "E-WIRE-OPCODE",
                );
            }
            if body[0] == 87 {
                for invalid in [2, 128, 255] {
                    let mut bad = bytes.clone();
                    bad[offset + opcode + 9] = invalid;
                    error(
                        &wire::decode_module_candidate(&bad, &limits).unwrap_err(),
                        ResultCategory::SafeReject,
                        "E-WIRE-BOOL",
                    );
                }
            }
            let mut bad_span = bytes.clone();
            let end = offset + expected.len();
            bad_span[end - 8..end].fill(0);
            error(
                &wire::decode_module_candidate(&bad_span, &limits).unwrap_err(),
                ResultCategory::SafeReject,
                "E-WIRE-SPAN",
            );
            for cut in offset..end {
                error(
                    &wire::decode_module_candidate(&bytes[..cut], &limits).unwrap_err(),
                    ResultCategory::SafeReject,
                    "E-WIRE-TRUNCATED",
                );
            }
            for version in [0u32, 20, 21, 23, u32::MAX] {
                let mut bad = bytes.clone();
                bad[8..12].copy_from_slice(&version.to_le_bytes());
                error(
                    &wire::decode_module_candidate(&bad, &limits).unwrap_err(),
                    ResultCategory::SafeReject,
                    "E-WIRE-VERSION",
                );
            }
            let mut trailing = bytes;
            trailing.push(0);
            error(
                &wire::decode_module_candidate(&trailing, &limits).unwrap_err(),
                ResultCategory::SafeReject,
                "E-WIRE-TRAILING",
            );
        }
    }
}

#[test]
fn whole_module_fingerprints_bind_new_operands_result_presence_and_span() {
    let mut count = 0;
    for (kind, _) in cases() {
        let instruction = Instruction {
            result: None,
            kind: kind.clone(),
            span: span(),
        };
        let (module, plan) = append(instruction);
        let mut changes = Vec::new();
        match kind {
            InstructionKind::OwnedStoreBorrow { store, index } => {
                changes.push(InstructionKind::OwnedStoreBorrow {
                    store: ValueId(store.0 ^ 0x8000_0000),
                    index,
                });
                changes.push(InstructionKind::OwnedStoreBorrow {
                    store,
                    index: ValueId(index.0 ^ 0x8000_0000),
                });
            }
            InstructionKind::BorrowPackageField {
                local,
                index,
                mutable,
            } => {
                changes.push(InstructionKind::BorrowPackageField {
                    local: LocalId(local.0 ^ 0x8000_0000),
                    index,
                    mutable,
                });
                changes.push(InstructionKind::BorrowPackageField {
                    local,
                    index: index ^ 0x8000_0000,
                    mutable,
                });
                changes.push(InstructionKind::BorrowPackageField {
                    local,
                    index,
                    mutable: !mutable,
                });
            }
            _ => unreachable!(),
        }
        for changed_kind in changes {
            let mut changed = module.clone();
            changed.functions[0].blocks[0]
                .instructions
                .last_mut()
                .unwrap()
                .kind = changed_kind;
            assert_ne!(changed.fingerprint(), plan.module_fingerprint);
            count += 1;
        }
        for change in 0..3 {
            let mut changed = module.clone();
            let operation = changed.functions[0].blocks[0]
                .instructions
                .last_mut()
                .unwrap();
            match change {
                0 => operation.result = Some(ValueId(0)),
                1 => operation.span.file = FileId(7),
                _ => operation.span.end += 1,
            }
            assert_ne!(changed.fingerprint(), plan.module_fingerprint);
            count += 1;
        }
    }
    record(
        "fingerprints.txt",
        format!("{count} independent module fingerprint mutations differ\n"),
    );
}

fn minimum(mut succeeds: impl FnMut(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, Limits::default().max_nodes);
    assert!(succeeds(hi));
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if succeeds(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    assert!(lo > 0);
    lo
}

#[test]
fn new_instruction_envelopes_obey_exact_byte_node_and_instruction_limits() {
    for (case, (kind, _)) in cases().into_iter().enumerate() {
        let (module, plan) = append(Instruction {
            result: None,
            kind,
            span: span(),
        });
        let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let exact = Limits {
            max_source_bytes: bytes.len(),
            ..Limits::default()
        };
        assert!(wire::encode_module_candidate(&module, &plan, &exact).is_ok());
        assert!(wire::decode_module_candidate(&bytes, &exact).is_ok());
        let short = Limits {
            max_source_bytes: bytes.len() - 1,
            ..Limits::default()
        };
        for errors in [
            wire::encode_module_candidate(&module, &plan, &short).unwrap_err(),
            wire::decode_module_candidate(&bytes, &short).unwrap_err(),
        ] {
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
        }
        let enc = minimum(|n| {
            wire::encode_module_candidate(
                &module,
                &plan,
                &Limits {
                    max_nodes: n,
                    ..Limits::default()
                },
            )
            .is_ok()
        });
        let dec = minimum(|n| {
            wire::decode_module_candidate(
                &bytes,
                &Limits {
                    max_nodes: n,
                    ..Limits::default()
                },
            )
            .is_ok()
        });
        for (decode, n) in [(false, enc), (true, dec)] {
            let low = Limits {
                max_nodes: n - 1,
                ..Limits::default()
            };
            let errors = if decode {
                wire::decode_module_candidate(&bytes, &low).unwrap_err()
            } else {
                wire::encode_module_candidate(&module, &plan, &low).unwrap_err()
            };
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
        }
        let instructions = module.functions[0].blocks[0].instructions.len();
        let exact = Limits {
            max_instructions: instructions,
            ..Limits::default()
        };
        assert!(wire::encode_module_candidate(&module, &plan, &exact).is_ok());
        assert!(wire::decode_module_candidate(&bytes, &exact).is_ok());
        let low = Limits {
            max_instructions: instructions - 1,
            ..Limits::default()
        };
        for errors in [
            wire::encode_module_candidate(&module, &plan, &low).unwrap_err(),
            wire::decode_module_candidate(&bytes, &low).unwrap_err(),
        ] {
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
        }
        record(
            &format!("budget-{case}.txt"),
            format!(
                "bytes={}\nencode_nodes={enc}\ndecode_nodes={dec}\ninstructions={instructions}\nN passes, N-1 ResourceLimit in both directions\n",
                bytes.len()
            ),
        );
    }
}

fn check(files: &[(&str, &str)], label: &str) -> zil_driver::CompileReport {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *source,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    };
    let report = compile_project(&project, &CompileOptions::default());
    record(&format!("{label}.json"), report.to_json());
    for (name, source) in files {
        record(&format!("{label}-{name}.zil"), source);
    }
    report
}

#[test]
fn reserved_store_at_cannot_hide_in_unused_templates_patterns_or_closure_parameters() {
    for (case, (source, phase, code)) in [
        (
            "resource store_at{x:i32}fn main(){}",
            Phase::Expand,
            "E-NAME-DUPLICATE",
        ),
        (
            "package store_at{x:i32}fn main(){}",
            Phase::Expand,
            "E-NAME-DUPLICATE",
        ),
        (
            "enum store_at{A}fn main(){}",
            Phase::Expand,
            "E-NAME-DUPLICATE",
        ),
        (
            "type Identity<store_at> = store_at;fn main(){}",
            Phase::Expand,
            "E-OWNED-STORE-NAME",
        ),
        (
            "fn unused<T>(store_at:T)->T{return store_at}fn main(){}",
            Phase::Expand,
            "E-OWNED-STORE-NAME",
        ),
        (
            "fn unused<T>(x:T)->T{let store_at=x;return store_at}fn main(){}",
            Phase::Expand,
            "E-OWNED-STORE-NAME",
        ),
        (
            "extern \"C\" fn store_at(x:i32)->i32;fn main(){}",
            Phase::Expand,
            "E-NAME-DUPLICATE",
        ),
        (
            "fn main(){for store_at in 0..1{}}",
            Phase::Types,
            "E-OWNED-STORE-NAME",
        ),
        (
            "fn main(){let f=(store_at:i32)=>store_at;}",
            Phase::Types,
            "E-OWNED-STORE-NAME",
        ),
        (
            "enum E{Some(i32),None}fn main(){let e=E.Some(1);if let E.Some(store_at)=e{}}",
            Phase::Types,
            "E-OWNED-STORE-NAME",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let report = check(&[("main", source)], &format!("reserved-{case}"));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == phase && d.code == code),
            "{source}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        if code == "E-NAME-DUPLICATE" {
            // The project symbol table rejects these before canonicalization;
            // the standalone Expand API independently retains its precise rule.
            let unit = SourceUnit::new(FileId(0), ModuleId(0), "standalone.zil", source);
            let syntax = zil_syntax::parse(&unit).into_result().unwrap();
            let errors = zil_expand::expand(&unit, &syntax).unwrap_err();
            error(&errors, ResultCategory::SafeReject, "E-OWNED-STORE-NAME");
            record(&format!("standalone-{case}.txt"), format!("{errors:#?}\n"));
        }
    }
}

#[test]
fn ordinary_members_longer_identifiers_and_module_qualifiers_keep_their_namespace() {
    for (case, source) in [
        "struct Row{store_at:i32}fn main()->i32{let row=Row{store_at:3};return row.store_at-3}",
        "fn store_at_suffix()->i32{return 0}fn main()->i32{return store_at_suffix()}",
        "struct Row{n:i32}impl Row{fn store_at(self:Self)->i32=>.n;}fn main()->i32{let row=Row{n:0};return row.store_at()}",
    ].into_iter().enumerate() {
        let report = check(&[("main",source)], &format!("ordinary-{case}"));
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1);
    }
    let report = check(
        &[
            (
                "main",
                "import support as store_at\nfn main()->i32{return store_at.answer()}",
            ),
            ("support", "pub fn answer()->i32{return 0}"),
        ],
        "module-qualifier",
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
