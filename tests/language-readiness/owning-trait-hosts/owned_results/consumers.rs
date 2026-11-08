//! Ordinary owned results retain complete nominal identity and cleanup.
use super::*;

const PROTOCOL: &str = include_str!("protocol.zil");
const LIBRARY: &str = include_str!("library.zil");
const MODEL: &str = include_str!("model.zil");
const MAIN: &str = include_str!("main.zil");

fn files() -> [(&'static str, &'static str); 4] {
    [
        ("main", MAIN),
        ("protocol", PROTOCOL),
        ("library", LIBRARY),
        ("model", MODEL),
    ]
}

fn direct_main() -> String {
    MAIN.replace(
        "let copy=library.duplicate(&original)",
        "let copy:model.Item=original.duplicate()",
    )
    .replace(
        "match library.try_duplicate(&original,true)",
        "let requested:protocol.Error!model.Item=original.try_duplicate(true);match requested",
    )
    .replace(
        "match library.try_duplicate(&original,false)",
        "let completed:protocol.Error!model.Item=original.try_duplicate(false);match completed",
    )
}

#[test]
fn owning_trait_results_reach_concrete_and_symbolic_consumers() {
    for main in [MAIN.to_owned(), direct_main()] {
        let report = compile_project(
            &project(&[
                ("main", &main),
                ("protocol", PROTOCOL),
                ("library", LIBRARY),
                ("model", MODEL),
            ]),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
#[cfg(target_os = "linux")]
fn owning_trait_results_native_values_and_cleanup_on_every_exit() {
    let direct = direct_main();
    let direct_files = [
        ("main", direct.as_str()),
        ("protocol", PROTOCOL),
        ("library", LIBRARY),
        ("model", MODEL),
    ];
    for files in [&files()[..], &direct_files[..]] {
        owning_trait_native_cleanup(
            files,
            &[
                (None, 0, b"4:4:4\n"),
                (Some("1"), 1, b"1:0:0\n"),
                (Some("2"), 2, b"2:1:1\n"),
                (Some("3"), 4, b"3:2:2\n"),
                (Some("4"), 6, b"4:3:3\n"),
            ],
        );
    }
}

#[test]
fn owning_trait_results_check_unused_definitions_and_copy_obligations() {
    let options = CompileOptions::default();
    let report = compile_project(
        &project(&[
            ("main", "fn main(){}"),
            ("protocol", PROTOCOL),
            ("library", LIBRARY),
            ("model", MODEL),
        ]),
        &options,
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for library in [
        format!(
            "{LIBRARY}\npub fn twice<T:protocol.Duplicate>(value:&T)->(T,T){{let fresh=value.duplicate();return (fresh,fresh)}}"
        ),
        format!(
            "{LIBRARY}\npub fn twice<T:protocol.Duplicate>(value:&T)->protocol.Error!(T,T){{let fresh=value.try_duplicate(false)?;return (fresh,fresh)}}"
        ),
    ] {
        let report = compile_project(
            &project(&[
                ("main", "fn main(){}"),
                ("protocol", PROTOCOL),
                ("library", &library),
                ("model", MODEL),
            ]),
            &options,
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT" && d.message.contains("used after move")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn owning_trait_results_preserve_exclusive_receiver_and_shared_owner_permissions() {
    let protocol = PROTOCOL.replace("self:&Self", "self:&mut Self");
    let library = LIBRARY.replace("value:&T", "value:&mut T");
    let model = MODEL.replace("self:&Self", "self:&mut Self");
    let main = MAIN
        .replace(
            ".ok(original)=>{",
            ".ok(original_value)=>{var original=original_value;",
        )
        .replace(
            "library.duplicate(&original)",
            "library.duplicate(&mut original)",
        )
        .replace(
            "library.try_duplicate(&original,",
            "library.try_duplicate(&mut original,",
        );
    let report = compile_project(
        &project(&[
            ("main", &main),
            ("protocol", &protocol),
            ("library", &library),
            ("model", &model),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let bad = MODEL.replace(
        "return Item{data:give bytes,tag:self.tag+1}",
        "return Item{data:give self.data,tag:self.tag+1}",
    );
    let report = compile_project(
        &project(&[
            ("main", "fn main(){}"),
            ("protocol", PROTOCOL),
            ("library", LIBRARY),
            ("model", &bad),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-ADDRESS-PERMISSION"),
        "{}",
        report.to_json()
    );
}

#[test]
fn owning_trait_result_selection_work_and_nodes_remain_bounded_across_replay() {
    let sources = project(&files());
    for nodes in [false, true] {
        let options = |n| {
            let mut options = CompileOptions {
                stop_after: Stage::Core,
                ..CompileOptions::default()
            };
            if nodes {
                options.limits.max_nodes = n
            } else {
                options.limits.max_analysis_steps = n
            }
            options
        };
        let mut low = 1;
        let mut high = if nodes {
            Limits::default().max_nodes
        } else {
            Limits::default().max_analysis_steps
        };
        assert_eq!(
            compile_project(&sources, &options(high)).status,
            ResultCategory::Success
        );
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_project(&sources, &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = zil_driver::CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_project(&sources, &options(n)),
                compile_project(&sources, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
            }
            assert_eq!(session.active_query_count(), 0);
        }
        eprintln!("owned trait result nodes={nodes} N={low}; N-1/cold/warm/fresh agree");
    }
}

#[test]
fn owning_trait_results_keep_actual_provider_and_implementation_signatures() {
    for (model, extra, expected) in [
        (
            MODEL
                .replace(
                    "impl protocol.Duplicate for Item",
                    "impl other.Duplicate for Item",
                )
                .replace("import protocol", "import protocol\nimport other"),
            Some(("other", PROTOCOL)),
            "E-GENERIC-ARGUMENT",
        ),
        (
            MODEL.replace(
                "fn duplicate(self:&Self)->Self",
                "fn duplicate(self:&Self)->bool",
            ),
            None,
            "E-TRAIT-SIGNATURE",
        ),
    ] {
        let mut files = vec![
            ("main", MAIN),
            ("protocol", PROTOCOL),
            ("library", LIBRARY),
            ("model", &model),
        ];
        if let Some(extra) = extra {
            files.push(extra);
        }
        let report = compile_project(&project(&files), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report.diagnostics.iter().any(|d| d.code == expected),
            "{}",
            report.to_json()
        );
    }
}

const RAW: &str = r#"
trait Duplicate{fn duplicate(self:&Self)->Self;}
package Item{manages data:ByteBuffer;tag:i64}
package Other{manages data:ByteBuffer;tag:i64}
impl Duplicate for Item{fn duplicate(self:&Self)->Self{return Item{data:bytes_new(),tag:self.tag}}}
impl Duplicate for Other{fn duplicate(self:&Self)->Self{return Other{data:bytes_new(),tag:self.tag}}}
fn main(){let original=Item{data:bytes_new(),tag:7};let first=original.duplicate();let second=original.duplicate();}
"#;

#[test]
fn owning_trait_results_raw_and_wire_reject_repeated_transfer_and_wrong_nominal() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owned-trait-results.zil", RAW);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let other = typed
        .functions
        .iter()
        .find(|f| f.name.ends_with("Other.duplicate"))
        .unwrap()
        .id;
    for wrong_nominal in [false, true] {
        let mut forged = typed.clone();
        let main = forged
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let calls = main
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(block, b)| {
                b.instructions
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, i)| {
                        if matches!(i.kind, InstructionKind::Call { .. }) {
                            Some((block, index, i.result.unwrap()))
                        } else {
                            None
                        }
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        if wrong_nominal {
            let InstructionKind::Call { function, .. } =
                &mut main.blocks[calls[1].0].instructions[calls[1].1].kind
            else {
                unreachable!()
            };
            *function = other;
        } else {
            let store = main.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| matches!(i.kind, InstructionKind::Store { value, .. } if value == calls[1].2)).unwrap();
            let InstructionKind::Store { value, .. } = &mut store.kind else {
                unreachable!()
            };
            *value = calls[0].2;
        }
        let errors =
            zil_analyze::analyze(&forged, &limits).expect_err("actual owned result authority");
        eprintln!("Analyze wrong_nominal={wrong_nominal}: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for result in [
            zil_verify::verify_module(forged, claim, &target, &limits),
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ] {
            let errors = result.expect_err("independent owned result authority");
            eprintln!("Verify wrong_nominal={wrong_nominal}: {errors:?}");
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject
                        && !d.code.starts_with("E-VERIFY-PLAN")),
                "{errors:?}"
            );
        }
    }
}
