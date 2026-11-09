//! Ordinary owned results retain complete nominal identity and cleanup.
use super::*;

#[path = "stored_composition.rs"]
mod stored_composition;

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

const UNUSED_LENDING: &str =
    "pub trait UnusedLending{type Item;fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;}";

#[test]
fn unrelated_life_trait_preserves_concrete_owned_provider_bodies() {
    let protocol = format!("{PROTOCOL}\n{UNUSED_LENDING}");
    for main in [MAIN, "fn main(){}"] {
        let report = compile_project(
            &project(&[
                ("main", main),
                ("protocol", &protocol),
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
fn unrelated_life_trait_keeps_unused_provider_permissions_and_ownership() {
    let protocol = format!("{PROTOCOL}\n{UNUSED_LENDING}");
    for (model, code, reason) in [
        (
            MODEL.replacen(
                "let bytes=bytes_new()",
                "self.tag=0;let bytes=bytes_new()",
                1,
            ),
            "E-ADDRESS-PERMISSION",
            "indirect write requires an exclusive address",
        ),
        (
            MODEL.replace(
                "return Item{data:give bytes,tag:self.tag+1}",
                "return Item{data:self.data,tag:self.tag+1}",
            ),
            "E-TYPE-MISMATCH",
            "expected Managed(ByteBuffer), found Desc(ByteBuffer)",
        ),
    ] {
        let report = compile_project(
            &project(&[
                ("main", "fn main(){}"),
                ("protocol", &protocol),
                ("library", LIBRARY),
                ("model", &model),
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
                .any(|d| d.phase == zil_base::Phase::Types
                    && d.code == code
                    && d.message == reason),
            "{}",
            report.to_json()
        );
        eprintln!("unused concrete owned provider: {}", report.to_json());
    }
    let shared_write = MODEL.replacen("bytes_reserve(bytes,1)", "bytes_reserve(self.data,1)", 1);
    let report = compile_project(
        &project(&[
            (
                "main",
                "import model;import protocol;import library;fn main(){}",
            ),
            ("protocol", &protocol),
            ("library", LIBRARY),
            ("model", &shared_write),
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
    assert!(report.diagnostics.iter().any(|d| d.phase == zil_base::Phase::Analyze && d.code == "E-ANALYZE-IR" && d.message == "shared borrowed resource head cannot authorize a write, including through forwarding or captures"), "{}", report.to_json());
    let shared_give = MODEL.replace(
        "return Item{data:give bytes,tag:self.tag+1}",
        "return Item{data:give self.data,tag:self.tag+1}",
    );
    let report = compile_project(
        &project(&[
            (
                "main",
                "import model;import protocol;import library;fn main(){}",
            ),
            ("protocol", &protocol),
            ("library", LIBRARY),
            ("model", &shared_give),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == zil_base::Phase::Types
                && d.code == "E-UNSUPPORTED"
                && d.message == "give requires a managed local or a direct Managed package field"),
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn unrelated_life_trait_preserves_native_owned_results_and_cleanup() {
    let protocol = format!("{PROTOCOL}\n{UNUSED_LENDING}");
    owning_trait_native_cleanup(
        &[
            ("main", MAIN),
            ("protocol", &protocol),
            ("library", LIBRARY),
            ("model", MODEL),
        ],
        &[
            (None, 0, b"4:4:4\n"),
            (Some("1"), 1, b"1:0:0\n"),
            (Some("2"), 2, b"2:1:1\n"),
            (Some("3"), 4, b"3:2:2\n"),
            (Some("4"), 6, b"4:3:3\n"),
        ],
    );
}

#[test]
fn concrete_provider_keeps_generic_helper_selected_error_arms() {
    let protocol = "pub trait Read{fn read(self:&Self,fail:bool)->i32;}";
    let main = "import model;fn main()->i32{if model.run(true)!=13{return 1}if model.run(false)!=25{return 2}return 0}";
    let library = "import protocol;pub fn read<P:protocol.Read>(value:&P,fail:bool)->i32{return value.read(fail)}";
    for helper in [
        "fn choose<E:Copy,T:Copy>(first:E,second:T,fail:bool)->E!T{if fail{return first}return second}",
        "fn relay<E:Copy,T:Copy>(value:E!T)->E!T{return value}fn choose<E:Copy,T:Copy>(first:E,second:T,fail:bool)->E!T{if fail{return relay::<E,T>(first)}return relay::<E,T>(second)}",
    ] {
        let model = format!(
            "import protocol;import library;package Item{{manages data:ByteBuffer}}{helper}impl protocol.Read for Item{{fn read(self:&Item,fail:bool)->i32{{let result:i32!i32=choose::<i32,i32>(3,5,fail);match result{{.err(value)=>{{return value+10}},.ok(value)=>{{return value+20}}}}}}}}pub fn run(fail:bool)->i32{{let value=Item{{data:bytes_new()}};return library.read(&value,fail)}}"
        );
        for protocol in [protocol.to_owned(), format!("{protocol}\n{UNUSED_LENDING}")] {
            let files = [
                ("main", main),
                ("protocol", protocol.as_str()),
                ("library", library),
                ("model", model.as_str()),
            ];
            let report = compile_project(&project(&files), &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            #[cfg(target_os = "linux")]
            owning_trait_native_cleanup(&files, &[(None, 0, b"0:0:0\n")]);
        }
    }
}

#[test]
fn concrete_provider_resolves_actual_generic_trait_method() {
    let protocol = format!(
        "pub trait Select{{fn selected<T:Copy>(self:&Self,value:T)->i32;}}pub trait Read{{fn read(self:&Self)->i32;}}{UNUSED_LENDING}"
    );
    let model = "import protocol;struct Item{}impl protocol.Select for Item{fn selected<T:Copy>(self:&Self,value:T)->i32{return 42}}impl protocol.Read for Item{fn read(self:&Item)->i32{return self.selected::<i32>(7)}}pub fn run()->i32{let value=Item{};return value.read()}";
    let main = "import model;fn main()->i32{return model.run()-42}";
    let report = compile_project(
        &project(&[("main", main), ("protocol", &protocol), ("model", model)]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    #[cfg(target_os = "linux")]
    owning_trait_native_cleanup(
        &[("main", main), ("protocol", &protocol), ("model", model)],
        &[(None, 0, b"0:0:0\n")],
    );
    let ambiguous =
        format!("{protocol}\npub trait Other{{fn selected<T:Copy>(self:&Self,value:T)->i32;}}");
    let other = format!(
        "{model}\nimpl protocol.Other for Item{{fn selected<T:Copy>(self:&Self,value:T)->i32{{return 9}}}}"
    );
    let report = compile_project(
        &project(&[
            ("main", "fn main(){}"),
            ("protocol", &ambiguous),
            ("model", &other),
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
            .any(|d| d.message.contains("ambiguous")),
        "{}",
        report.to_json()
    );
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
    for (bad, expected, code, reason) in [
        (
            MODEL.replace(
                "return Item{data:give bytes,tag:self.tag+1}",
                "return Item{data:give self.data,tag:self.tag+1}",
            ),
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            "give requires a managed local or a direct Managed package field",
        ),
        (
            MODEL.replace("let bytes=bytes_new()", "self.tag=0;let bytes=bytes_new()"),
            ResultCategory::SafeReject,
            "E-ADDRESS-PERMISSION",
            "",
        ),
    ] {
        let report = compile_project(
            &project(&[
                ("main", "fn main(){}"),
                ("protocol", PROTOCOL),
                ("library", LIBRARY),
                ("model", &bad),
            ]),
            &CompileOptions::default(),
        );
        assert_eq!(report.status, expected, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == code && d.message.contains(reason)),
            "{}",
            report.to_json()
        );
    }
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
        let analyzed = zil_analyze::analyze(&forged, &limits);
        if wrong_nominal {
            // Analyze supplies an untrusted plan; Verify independently checks
            // the complete actual call signature and nominal identity.
            assert!(analyzed.is_ok(), "{analyzed:?}");
        } else {
            let errors = analyzed.expect_err("actual owned result authority");
            eprintln!("Analyze repeated transfer: {errors:?}");
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject
                        && d.code == "E-ANALYZE-IR"
                        && d.message
                            .contains("responsibility was already consumed or absent")),
                "{errors:?}"
            );
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for result in [
            zil_verify::verify_module(forged, claim, &target, &limits),
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ] {
            let errors = result.expect_err("independent owned result authority");
            eprintln!("Verify wrong_nominal={wrong_nominal}: {errors:?}");
            let expected = if wrong_nominal {
                "E-VERIFY-CALL"
            } else {
                "E-VERIFY-RESOURCE"
            };
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject && d.code == expected),
                "{errors:?}"
            );
        }
    }
}
