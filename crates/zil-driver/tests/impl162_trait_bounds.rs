//! Ordinary nominal bounds are checked symbolically and at each instantiation.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn reviewer330_unused_and_instantiated_reference_defaults_keep_the_same_boundary() {
    let cases = [
        (
            "local-address",
            "trait Leak{fn bad(self:&Self)->&i32{let local=7;return &local}}",
            "struct P{}impl Leak for P{}",
        ),
        (
            "byvalue-receiver",
            "trait Leak{fn borrow(self:&Self)->&i32;fn bad(self:Self)->&i32{return self.borrow()}}",
            "struct P{n:i32}impl Leak for P{fn borrow(self:&Self)->&i32{return &self.n}}",
        ),
        (
            "concrete-local",
            "trait Leak{fn borrow(self:&Self)->&i32;fn bad(self:&Self)->&i32{let local=P{n:7};return local.borrow()}}",
            "struct P{n:i32}impl Leak for P{fn borrow(self:&Self)->&i32{return &self.n}}",
        ),
        (
            "shadowed-reference",
            "trait Leak{fn bad(self:&Self,value:&i32)->&i32{let value=7;return &value}}",
            "struct P{}impl Leak for P{}",
        ),
    ];
    let mut mismatches = Vec::new();
    for (label, declaration, implementation) in cases {
        let unused = if label == "concrete-local" {
            "struct P{n:i32}impl P{fn borrow(self:&Self)->&i32{return &self.n}}trait Leak{fn bad(self:&Self)->&i32{let local=P{n:7};return local.borrow()}}fn main(){}".to_owned()
        } else {
            format!("{declaration}fn main(){{}}")
        };
        let implemented = format!("{declaration}{implementation}fn main(){{}}");
        let called = match label {
            "local-address" => format!(
                "{declaration}{implementation}fn main()->i32{{let p=P{{}};return *p.bad()-7}}"
            ),
            "shadowed-reference" => format!(
                "{declaration}{implementation}fn main()->i32{{let p=P{{}};let value=1;return *p.bad(&value)-7}}"
            ),
            _ => format!(
                "{declaration}{implementation}fn main()->i32{{let p=P{{n:7}};return *p.bad()-7}}"
            ),
        };
        for (form, text) in [
            ("unused", unused),
            ("implemented", implemented),
            ("called", called),
        ] {
            let report = check(&text);
            eprintln!(
                "review330 {label}/{form}: {:?}/{:?}/{:?}",
                report.status,
                report.phase,
                report
                    .diagnostics
                    .iter()
                    .map(|d| &d.code)
                    .collect::<Vec<_>>()
            );
            if report.status != ResultCategory::Unsupported
                || report.phase != Phase::Expand
                || report
                    .diagnostics
                    .first()
                    .is_none_or(|d| d.code != "E-UNSUPPORTED")
                || report.verification_runs != 0
            {
                mismatches.push((
                    label,
                    form,
                    format!(
                        "{:?}/{:?}/{:?}",
                        report.status,
                        report.phase,
                        report
                            .diagnostics
                            .iter()
                            .map(|d| &d.code)
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "unused or instantiated default escaped the finite declaration boundary: {mismatches:?}"
    );
}

#[test]
fn reviewer330_receiver_shadow_is_rejected_before_an_instance_is_selected() {
    for suffix in [
        "fn main(){}",
        "struct P{}impl Shadow for P{}fn main()->i32{let p=P{};return p.read()}",
    ] {
        let report = check(&format!(
            "trait Shadow{{fn read(self:&Self)->i32{{let self=7;return 7}}}}{suffix}"
        ));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, "E-RECEIVER-RESERVED-NAME");
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn reviewer330_real_reference_forwarding_and_parameter_shadow_keep_native_meaning() {
    native_both(&[(
        "main",
        "trait Borrow{fn required(self:&Self)->&i32;fn forward(self:&Self)->&i32{return self.required()}fn shadow(self:&Self,value:i32)->i32{let value=value+1;return value}}struct P{n:i32}impl Borrow for P{fn required(self:&Self)->&i32{return &self.n}}fn main()->i32{let p=P{n:41};let q=p.forward();return *q+p.shadow(0)-42}",
    )]);
    native_both(&[(
        "main",
        "trait Score<Tag>{fn required(self:&Self)->i32;fn score(self:&Self)->i32{return self.required()+1}}struct P{}impl Score<bool> for P{fn required()->i32{return 10}}impl Score<i32> for P{fn required()->i32{return 20}}fn left<T:Score<bool>>(p:&T)->i32{return p.score()}fn right<T:Score<i32>>(p:&T)->i32{return p.score()}fn main()->i32{let p=P{};if left(&p)!=11{return 1}return right(&p)-21}",
    )]);
    native_both(&[
        (
            "main",
            "import rules;import data;fn main()->i32{let p=data.P{};return rules.run(&p)-43}",
        ),
        (
            "rules",
            "fn hidden()->i32{return 41}pub trait Score{fn score(self:&Self)->i32{return hidden()+2}}pub fn run<T:Score>(p:&T)->i32{return p.score()}",
        ),
        (
            "data",
            "import rules;fn hidden()->i32{return 99}pub struct P{}impl rules.Score for P{}",
        ),
    ]);
}

#[test]
fn reviewer330_required_body_escape_cannot_gain_a_verify_proof() {
    let unit = source(
        "trait Borrow{fn required(self:&Self)->&i32;fn forward(self:&Self)->&i32{return self.required()}}struct P{n:i32}impl Borrow for P{fn required(self:&Self)->&i32{let local=7;let view=&local;let consumed=*view;return &self.n}}fn main()->i32{let p=P{n:7};return *p.forward()-7}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let mut module = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let function=module.functions.iter_mut().find(|f|f.blocks.iter().flat_map(|b|&b.instructions).any(|i|matches!(i.kind,zil_ir::InstructionKind::BorrowLocal{local} if f.locals[local.0 as usize].ty==zil_ir::Type::I32))).unwrap();
    let function_id = function.id;
    let root=function.blocks.iter().flat_map(|b|&b.instructions).find_map(|i|matches!(i.kind,zil_ir::InstructionKind::BorrowLocal{local} if function.locals[local.0 as usize].ty==zil_ir::Type::I32).then_some(i.result).flatten()).unwrap();
    for block in &mut function.blocks {
        if matches!(block.terminator, zil_ir::Terminator::Return(_)) {
            block.terminator = zil_ir::Terminator::Return(Some(root));
        }
    }
    // Unannotated ordinary returns use the established architecture: Analyze
    // produces a candidate and records the local root; Verify independently
    // refuses to turn that candidate into a safety proof.
    plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(
        plan.functions
            .iter()
            .find(|f| f.function == function_id)
            .unwrap()
            .holds[root.0 as usize]
            .roots
            .iter()
            .any(|r| matches!(r, zil_ir::HoldRoot::Local(_)))
    );
    eprintln!("review330 raw Analyze: CandidatePlan, local origin retained");
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()),
    ] {
        let errors = result.unwrap_err();
        eprintln!(
            "review330 raw Verify: {:?}",
            errors.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-ESCAPE"),
            "{errors:?}"
        );
    }
}

#[test]
fn trait330_original_pending_sources() {
    // Original Unsupported/Types baseline remains in commit 43bcafbd0; preserve source bytes.
    for text in [
        include_str!("../../../tests/language-readiness/pending/default-trait-method.zil"),
        include_str!("../../../tests/language-readiness/recovery/pending/trait-default-method.zil"),
    ] {
        let report = check(text);
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
fn trait330_default_delegation_and_instance_override_are_native() {
    native_both(&[(
        "main",
        include_str!("../../../tests/language-readiness/recovery/pending/trait-default-method.zil"),
    )]);
    native_both(&[(
        "main",
        r#"
trait Read<R:Copy>{
 fn read(self:&Self)->R;
 fn delegated(self:&Self)->R{return self.read()}
}
struct Point{value:i32}
impl Read<i32> for Point{fn read()->i32{return .value}}
impl Read<i64> for Point{
 fn read()->i64{return i64(.value)+2}
 fn delegated()->i64{return 29}
}
fn narrow<T:Read<i32>>(x:&T)->i32{return x.delegated()}
fn wide<T:Read<i64>>(x:&T)->i64{return x.delegated()}
fn main()->i32{let p=Point{value:7};if narrow(&p)!=7{return 1}if wide(&p)!=29{return 2}return 0}
"#,
    )]);
}

#[test]
fn trait330_same_self_two_default_instances_keep_ordered_phantom_identity() {
    native_both(&[(
        "main",
        r#"
trait Tag<A:Copy,B:Copy>{
 fn base(self:&Self)->i32;
 fn delegated(self:&Self)->i32{return self.base()}
}
struct Point{}
impl Tag<i32,bool> for Point{fn base()->i32{return 17}}
impl Tag<bool,i32> for Point{fn base()->i32{return 29}}
fn first<T:Tag<i32,bool>>(x:&T)->i32{return x.delegated()}
fn second<T>(x:&T)->i32 where T:Tag<bool,i32>{return x.delegated()}
fn main()->i32{let p=Point{};if first(&p)!=17{return 1}if second(&p)!=29{return 2}return 0}
"#,
    )]);
}

#[test]
fn trait330_default_body_keeps_private_helper_module_and_actual_host_dependencies() {
    native_both(&[
        (
            "main",
            "import rules;import data;fn main()->i32{let p=data.Point{};return rules.apply(&p)-7}",
        ),
        (
            "rules",
            "fn helper()->i32{return 7}pub trait Read<R:Copy>{fn read(self:&Self)->i32{return helper()}}pub fn apply<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "data",
            "import rules;pub struct Point{}fn helper()->i32{return 99}impl rules.Read<i32> for Point{}",
        ),
    ]);
}

#[test]
fn trait336_original_global_function_value_reaches_both_native_backends() {
    native_both(&[(
        "main",
        include_str!(
            "../../../tests/language-readiness/followup334/default-global-fnptr-known-incomplete.zil"
        ),
    )]);
}

#[test]
fn trait336_private_function_values_keep_trait_module_and_full_signature() {
    native_both(&[
        (
            "main",
            include_str!("../../../tests/language-readiness/followup336/private-fnptr/main.zil"),
        ),
        (
            "rules",
            include_str!("../../../tests/language-readiness/followup336/private-fnptr/rules.zil"),
        ),
        (
            "component",
            include_str!(
                "../../../tests/language-readiness/followup336/private-fnptr/component.zil"
            ),
        ),
    ]);
}

#[test]
fn trait336_real_local_bindings_and_existing_fnptr_forwarding_take_precedence() {
    native_both(&[(
        "main",
        include_str!("../../../tests/language-readiness/followup336/local-shadow.zil"),
    )]);
}

#[test]
fn trait336_unused_and_overridden_function_values_keep_complete_symbolic_identity() {
    for text in [
        "fn identity(value:i32)->i32{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
        "fn identity(value:i32)->i32{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(i32)->i32{return identity}}struct P{}impl Factory<bool> for P{fn handler()->fn(i32)->i32{return identity}}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for declaration in [
        "fn identity(value:i64)->i32{return 0}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}",
        "fn identity(value:i32)->i64{return 0}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}",
        "fn identity(value:i32,other:i32)->i32{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}",
        "struct A{n:i32}struct B{n:i32}fn identity(value:A)->A{return value}trait Factory{fn handler(self:&Self)->fn(B)->B{return identity}}",
        "struct Tag<A:Copy,B:Copy>{}fn identity(value:Tag<i32,bool>)->Tag<i32,bool>{return value}trait Factory{fn handler(self:&Self)->fn(Tag<bool,i32>)->Tag<bool,i32>{return identity}}",
        "fn identity(value:i32)->i32{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{let identity=7;return identity}}",
    ] {
        for suffix in [
            "fn main(){}",
            "struct P{}impl Factory for P{fn handler()->fn(i32)->i32{return fallback}}fn fallback(value:i32)->i32{return value}fn main(){}",
        ] {
            // An unrelated/wrong override cannot hide the original bad default.
            let report = check(&format!("{declaration}{suffix}"));
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
            assert_eq!(
                report.diagnostics[0].code,
                "E-GENERIC-ARGUMENT",
                "{}",
                report.to_json()
            );
            assert!(
                report.diagnostics[0]
                    .message
                    .contains("before concrete instantiation"),
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 0);
        }
    }
    for symbolic in [
        "fn identity(value:i32)->i32{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(R)->R{return identity}}",
        "struct R{n:i32}fn identity(value:R)->R{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(R)->R{return identity}}",
    ] {
        for suffix in [
            "fn main(){}",
            "struct P{}impl Factory<i32> for P{}fn main(){}",
            "struct P{}impl Factory<i32> for P{fn handler()->fn(i32)->i32{return identity}}fn main(){}",
        ] {
            reject(
                &format!("{symbolic}{suffix}"),
                "before concrete instantiation",
            );
        }
    }
}

#[test]
fn trait336_foreign_generic_inferred_and_noncopy_function_values_stay_explicitly_unsupported() {
    for text in [
        "fn identity<T:Copy>(value:T)->T{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
        "fn identity(value:i32){return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
        "fn identity(value:&i32)->i32{return *value}trait Factory{fn handler(self:&Self)->fn(&i32)->i32{return identity}}fn main(){}",
        "resource R{}fn identity(value:R)->R{return value}trait Factory{fn handler(self:&Self)->fn(R)->R{return identity}}fn main(){}",
        "resource R{}fn identity(value:R)->R{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(R)->R{return identity}}fn main(){}",
        "extern \"C\" fn identity(value:i32)->i32;trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            "E-UNSUPPORTED",
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn trait336_function_value_lookup_keeps_private_names_and_unknown_names_closed() {
    let report = compile_project(
        &project(&[
            (
                "main",
                "import hidden;trait Factory{fn handler(self:&Self)->fn(i32)->i32{return hidden.identity}}fn main(){}",
            ),
            ("hidden", "fn identity(value:i32)->i32{return value}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    // Qualified module values in default bodies still stop at the existing
    // symbolic Member boundary. This slice only admits resolved Name values.
    assert_eq!(
        report.diagnostics[0].code,
        "E-GENERIC-ARGUMENT",
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics[0]
            .message
            .contains("unknown value hidden"),
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    let ordinary = compile_project(
        &project(&[
            (
                "main",
                "import hidden;fn main()->i32{let handler=hidden.identity;return handler(0)}",
            ),
            ("hidden", "fn identity(value:i32)->i32{return value}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        ordinary.status,
        ResultCategory::SafeReject,
        "{}",
        ordinary.to_json()
    );
    assert_eq!(
        ordinary.diagnostics[0].code,
        "E-MODULE-PRIVATE",
        "{}",
        ordinary.to_json()
    );
    reject(
        "trait Factory{fn handler(self:&Self)->fn(i32)->i32{return missing}}fn main(){}",
        "unknown value",
    );
    reject(
        "fn identity(value:i32)->i32{return value}fn ordinary<T:Copy>(value:T)->fn(i32)->i32{return identity}fn main(){}",
        "unknown value",
    );
}

#[test]
fn trait336_cached_default_values_recheck_the_actual_global_signature() {
    use zil_driver::CompilationSession;
    let valid = "fn identity(value:i32)->i32{return 7}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}struct P{}impl Factory for P{}fn main(){}";
    let invalid = valid.replace("value:i32", "value:i64");
    let mut session = CompilationSession::default();
    for (text, expected) in [
        (valid, ResultCategory::Success),
        (invalid.as_str(), ResultCategory::SafeReject),
        (valid, ResultCategory::Success),
    ] {
        let unit = source(text);
        for report in [
            session.compile_source(&unit, &CompileOptions::default()),
            CompilationSession::default().compile_source(&unit, &CompileOptions::default()),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::Success {
                assert_eq!(report.verification_runs, 1);
            } else {
                assert_eq!(report.phase, Phase::Expand);
                assert_eq!(report.diagnostics[0].code, "E-GENERIC-ARGUMENT");
                assert!(
                    report.diagnostics[0]
                        .message
                        .contains("before concrete instantiation")
                );
                assert_eq!(report.verification_runs, 0);
            }
        }
        assert_eq!(session.active_query_count(), 0);
    }
}

#[test]
fn trait336_default_function_value_request_budgets_are_exact_across_cache_replay() {
    use zil_driver::CompilationSession;
    let unit = source(include_str!(
        "../../../tests/language-readiness/followup334/default-global-fnptr-known-incomplete.zil"
    ));
    for ledger in ["work", "nodes", "functions"] {
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            }
            options
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1);
                } else {
                    assert!(!report.diagnostics.is_empty());
                    assert!(
                        report
                            .diagnostics
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit)
                    );
                }
            }
            assert_eq!(session.active_query_count(), 0);
        }
        println!(
            "336 default function value request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree"
        );
    }
}

#[test]
fn trait330_recursive_default_is_a_static_call_edge() {
    native_both(&[(
        "main",
        r#"
trait Down{fn down(self:&Self,n:i32)->i32{if n==0{return 0}return self.down(n-1)}}
struct P{}
impl Down for P{}
fn main()->i32{let p=P{};return p.down(4)}
"#,
    )]);
}

#[test]
fn trait330_default_substitutes_trait_arguments_through_record_bodies_and_signatures() {
    native_both(&[(
        "main",
        r#"
struct Box<T:Copy>{value:T}
trait Wrap<R:Copy>{fn wrap(self:&Self,value:R)->Box<R>{return Box::<R>{value:value}}}
struct P{}
impl Wrap<i32> for P{}
fn wrapped<T:Wrap<i32>>(x:&T)->Box<i32>{return x.wrap(7)}
fn main()->i32{let p=P{};return wrapped(&p).value-7}
"#,
    )]);
}

#[test]
fn trait330_direct_reference_inputs_and_required_borrow_delegation_are_native() {
    native_both(&[(
        "main",
        r#"
trait Read{
 fn raw(self:&Self)->&i32;
 fn same(self:&Self)->&Self{return self}
 fn delegated(self:&Self)->&i32{return self.raw()}
}
struct P{n:i32}
impl Read for P{fn raw()->&i32{return &.n}}
fn main()->i32{let p=P{n:7};let same=p.same();let raw=p.delegated();if same.n!=7{return 1}return *raw-7}
"#,
    )]);
}

#[test]
fn trait330_missing_wrong_override_duplicate_and_ambiguous_targets_reject() {
    for (text, phase, code) in [
        (
            "trait Read{fn required(self:&Self)->i32;fn delegated(self:&Self)->i32{return self.required()}}struct P{}impl Read for P{}fn main(){}",
            Phase::Expand,
            "E-TRAIT-METHOD-SET",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return 7}}struct P{}impl Read for P{fn read()->bool{return true}}fn main(){}",
            Phase::Expand,
            "E-TRAIT-SIGNATURE",
        ),
        (
            "type Int=i32;trait Read<R>{fn read(self:&Self)->i32{return 7}}struct P{}impl Read<Int> for P{}impl Read<i32> for P{}fn main(){}",
            Phase::Expand,
            "E-TRAIT-DUPLICATE",
        ),
        (
            "trait Read<R>{fn read(self:&Self)->i32{return 7}}struct P{}impl Read<i32> for P{}impl Read<i64> for P{}fn main()->i32{let p=P{};return p.read()}",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn trait330_default_request_budgets_are_exact_across_cache_replay() {
    use zil_driver::CompilationSession;
    let unit = source(
        "trait Tag<A:Copy,B:Copy>{fn base(self:&Self)->i32;fn delegated(self:&Self)->i32{return self.base()}}struct P{}impl Tag<i32,bool> for P{fn base()->i32{return 17}}impl Tag<bool,i32> for P{fn base()->i32{return 29}}fn a<T:Tag<i32,bool>>(x:&T)->i32{return x.delegated()}fn b<T:Tag<bool,i32>>(x:&T)->i32{return x.delegated()}fn main()->i32{let p=P{};return a(&p)+b(&p)-46}",
    );
    for ledger in ["work", "nodes", "functions"] {
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            }
            options
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::ResourceLimit {
                    assert!(!report.diagnostics.is_empty());
                } else {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(session.active_query_count(), 0);
        }
        println!("330 default request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree");
    }
}

#[test]
fn trait330_unused_and_overridden_defaults_check_symbolic_permissions() {
    for (text, category, code) in [
        (
            "trait Read{fn read(self:&Self)->&i32{let x=7;return &x}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read{fn raw(self:&Self)->&i32;fn leak(self:Self)->&i32{return self.raw()}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "struct P{n:i32}impl P{fn borrow(self:&Self)->&i32{return &.n}}trait Read{fn read(self:&Self)->&i32{let p=P{n:7};return p.borrow()}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "struct P{n:i32}impl P{fn borrow(self:&Self)->&i32{return &.n}}trait Read{fn read(self:&Self,p:&i32)->&i32{let p=P{n:7};return p.borrow()}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{let self=7;return 7}}fn main(){}",
            ResultCategory::SafeReject,
            "E-RECEIVER-RESERVED-NAME",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return self.secret}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read<R:Copy>{fn read(self:&Self,x:R)->R{return x+1}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return true}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "struct R{n:i32}fn concrete(x:R)->i32{return x.n}trait Read<R:Copy>{fn read(self:&Self,x:R)->i32{return concrete(x)}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return self.other()}}struct P{}impl Read for P{fn read()->i32{return 7}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "trait Read<R>{fn read(self:&Self,x:R)->R{let y=x;return x}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "struct Box<T:Copy>{value:T}trait Read<R>{fn read(self:&Self,x:R)->Box<R>{return Box::<R>{value:x}}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        let report = check(text);
        assert_eq!(report.status, category, "{text}: {}", report.to_json());
        assert_eq!(report.phase, Phase::Expand, "{text}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(!report.diagnostics.is_empty());
        assert!(
            report.diagnostics.iter().all(|d| d.code == code),
            "{}",
            report.to_json()
        );
    }
    for text in [
        "trait Read<R>{fn read(self:&Self)->R;fn delegated(self:&Self)->R{return self.read()}}fn main(){}",
        "trait Again{fn again(self:&Self)->i32{return self.again()}}fn main(){}",
        "trait Read<R:Copy>{fn read(self:&Self,x:R)->R{let y=x;return x}}fn main(){}",
    ] {
        let report = check(text);
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
fn review327_generic_trait_declaration_and_ordered_argument_identity() {
    native(&[
        (
            "main",
            "import left;import right;import data;fn main()->i32{let p=data.P{n:7};if left.apply(&p)!=7{return 1}if right.apply(&p)!=9{return 2}return 0}",
        ),
        (
            "left",
            "pub trait Read<R>{fn read(self:&Self)->R;}pub fn apply<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "right",
            "pub trait Read<R>{fn read(self:&Self)->R;}pub fn apply<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "data",
            "import left;import right;pub struct P{pub n:i32}impl left.Read<i32> for P{fn read()->i32{return .n}}impl right.Read<i32> for P{fn read()->i32{return .n+2}}",
        ),
    ]);
    native(&[(
        "main",
        r#"
trait Tag<A,B>{fn value(self:&Self)->i32;}
struct P{}
impl Tag<i32,bool> for P{fn value()->i32{return 17}}
impl Tag<bool,i32> for P{fn value()->i32{return 29}}
fn first<T:Tag<i32,bool>>(x:&T)->i32{return x.value()}
fn second<T>(x:&T)->i32 where T:Tag<bool,i32>{return x.value()}
fn main()->i32{let p=P{};if first(&p)!=17{return 1}if second(&p)!=29{return 2}return 0}
"#,
    )]);
}

#[test]
fn review327_unused_trait_obligations_and_alias_coherence() {
    let declaration = "struct Box<T:Copy>{value:T}struct Wrap<T:Copy>{value:T}trait Read<R>{fn read(self:&Self)->Wrap<Box<R>>;}fn main(){}";
    let bad = check(declaration);
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert_eq!(bad.phase, Phase::Expand, "{}", bad.to_json());
    assert!(
        bad.diagnostics.iter().any(|d| d.message.contains("Copy")),
        "{}",
        bad.to_json()
    );
    let good = check(&declaration.replace("trait Read<R>", "trait Read<R:Copy>"));
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let duplicate = check(
        "type Int=i32;trait Flag<A,B>{}struct P{}impl Flag<Int,bool> for P{}impl Flag<i32,bool> for P{}fn main(){}",
    );
    assert_eq!(
        duplicate.status,
        ResultCategory::SafeReject,
        "{}",
        duplicate.to_json()
    );
    assert_eq!(duplicate.phase, Phase::Expand, "{}", duplicate.to_json());
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TRAIT-DUPLICATE"),
        "{}",
        duplicate.to_json()
    );
    let missing = check(
        "trait Flag<A,B>{}struct P{}impl Flag<i32,bool> for P{}fn needed<T:Flag<bool,i32>>(x:&T){}fn main(){let p=P{};needed(&p)}",
    );
    assert_eq!(
        missing.status,
        ResultCategory::SafeReject,
        "{}",
        missing.to_json()
    );
    assert_eq!(missing.phase, Phase::Expand, "{}", missing.to_json());
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|d| d.message.contains("does not satisfy nominal trait bound")),
        "{}",
        missing.to_json()
    );
}

#[test]
fn concrete_generic_trait_arguments_and_bounds_call_real_instances() {
    native(&[(
        "main",
        r#"
trait Read<R>{fn read(self:&Self)->R;}
struct Point{n:i32}
impl Read<i32> for Point{fn read()->i32{return .n}}
impl Read<i64> for Point{fn read()->i64{return i64(.n)+1}}
fn read32<T:Read<i32>>(x:&T)->i32{return x.read()}
fn read64<T>(x:&T)->i64 where T:Read<i64>{return x.read()}
fn main()->i32{let p=Point{n:7};if read32(&p)!=7{return 1}if read64(&p)!=8{return 2}return 0}
"#,
    )]);
}

#[test]
fn original_concrete_generic_trait_probe_calls_directly() {
    native(&[(
        "main",
        include_str!("../../../tests/language-readiness/recovery/pending/trait-type-argument.zil"),
    )]);
}

#[test]
fn generic_trait_cross_module_bound_calls_keep_identity_and_exports() {
    native(&[
        (
            "main",
            "import api;import data;fn main()->i32{let p=data.Point{n:7};return api.get(&p)-7}",
        ),
        (
            "api",
            "pub trait Read<R>{fn read(self:&Self)->R;}pub fn get<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "data",
            "import api;pub struct Point{pub n:i32}impl api.Read<i32> for Point{fn read()->i32{return .n}}",
        ),
    ]);
}

#[test]
fn generic_trait_same_layout_nominals_and_phantom_arguments_stay_distinct() {
    native(&[(
        "main",
        r#"
struct A{n:i32}struct B{n:i32}
trait Build<R,P>{fn build(self:&Self)->R;}
struct Point{n:i32}
impl Build<A,bool> for Point{fn build()->A{return A{n:.n}}}
impl Build<B,bool> for Point{fn build()->B{return B{n:.n+1}}}
impl Build<A,i32> for Point{fn build()->A{return A{n:.n+2}}}
fn a<T:Build<A,bool>>(x:&T)->A{return x.build()}
fn b<T:Build<B,bool>>(x:&T)->B{return x.build()}
fn c<T:Build<A,i32>>(x:&T)->A{return x.build()}
fn main()->i32{let p=Point{n:7};if a(&p).n!=7{return 1}if b(&p).n!=8{return 2}return c(&p).n-9}
"#,
    )]);
}

#[test]
fn generic_trait_record_arguments_normalize_required_signatures_from_declaration() {
    native(&[(
        "main",
        r#"
struct Box<T:Copy>{value:T}
trait Read<R:Copy>{fn read(self:&Self)->R;}
struct Point{n:i32}
impl Read<Box<i32>> for Point{fn read()->Box<i32>{return Box::<i32>{value:.n}}}
fn get<T:Read<Box<i32>>>(x:&T)->Box<i32>{return x.read()}
fn id<T:Copy>(value:T)->T{return value}
fn main()->i32{let p=Point{n:7};let a=get(&p);let b=id(p.read());return a.value+b.value-14}
"#,
    )]);
    for (actual, value) in [("B", "B{n:1}"), ("i64", "1")] {
        let text = format!(
            "struct A{{n:i32}}struct B{{n:i32}}struct Box<T:Copy>{{value:T}}trait Read<R>{{fn read(self:&Self)->R;}}struct P{{}}impl Read<Box<A>> for P{{fn read()->Box<{actual}>{{return Box::<{actual}>{{value:{value}}}}}}}fn main(){{}}"
        );
        let report = check(&text);
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
                .any(|d| d.code == "E-TRAIT-SIGNATURE"),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn generic_trait_direct_static_and_value_call_arguments_keep_context() {
    native(&[(
        "main",
        r#"
trait Add<R>{fn add(self:&Self,value:R)->R;}
struct Point{n:i64}
impl Add<i64> for Point{fn add(value:i64)->i64{return .n+value}}
fn id<T:Copy>(value:T)->T{return value}
fn main()->i32{let p=Point{n:1};let a=p.add(id(6));let b=Point.add(&p,id(6));if a!=7{return 1}if b!=7{return 2}return 0}
"#,
    )]);
}

#[test]
fn generic_trait_direct_ambiguity_does_not_reject_distinct_impl_headers() {
    let declarations = "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl Read<i64> for P{fn read()->i64{return i64(.n)}}";
    let good = check(&format!("{declarations}fn main()->i32{{return 0}}"));
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let bad = check(&format!(
        "{declarations}fn main()->i32{{let p=P{{n:7}};return p.read()}}"
    ));
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.message.contains("ambiguous")),
        "{}",
        bad.to_json()
    );
    let text = "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl P{fn read(self:&Self)->i32{return .n}}fn get<T:Read<i32>>(x:&T)->i32{return x.read()}fn main()->i32{let p=P{n:7};return get(&p)-7}";
    assert_eq!(check(text).status, ResultCategory::Success);
    let bad = check(&text.replace("get(&p)-7", "p.read()-7"));
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.message.contains("ambiguous"))
    );
}

#[test]
fn generic_trait_request_budgets_are_exact_across_cache_replay() {
    use zil_driver::CompilationSession;
    let unit = source(
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl Read<i64> for P{fn read()->i64{return i64(.n)}}fn a<T:Read<i32>>(x:&T)->i32{return x.read()}fn b<T:Read<i64>>(x:&T)->i64{return x.read()}fn main()->i32{let p=P{n:7};let x=a(&p);let y=b(&p);return x-7}",
    );
    for ledger in ["work", "nodes", "instances"] {
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            };
            options
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            assert_eq!(session.compile_source(&unit, &options(n)).status, expected);
            assert_eq!(
                CompilationSession::default()
                    .compile_source(&unit, &options(n))
                    .status,
                expected
            );
            assert_eq!(session.active_query_count(), 0);
        }
        println!("323 trait request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree");
    }
}

#[test]
fn generic_trait_unused_signatures_and_instantiated_conformance_reject() {
    for source in [
        "trait Read<R>{fn read(self:&Self)->Missing;}fn main(){}",
        "struct Box<T:Copy>{value:T}trait Read<R>{fn read(self:&Self)->Box<R>;}fn main(){}",
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i64{return 7}}fn main(){}",
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{}fn main(){}",
        "trait Read<R>{fn read(self:&Self)->R;}fn unused<T:Read<i32>>(x:&T)->i64{return x.read()}fn main(){}",
        "trait Flag<R>{}struct P{}impl Flag<i32> for P{}fn needed<T:Flag<i64>>(x:&T){}fn main(){let p=P{};needed(&p)}",
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl Read<i32> for P{fn read()->i32{return .n}}fn main(){}",
        "type Int=i32;trait Flag<R>{}struct P{}impl Flag<Int> for P{}impl Flag<i32> for P{}fn main(){}",
    ] {
        let report = check(source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{source}: {}",
            report.to_json()
        );
    }
    let good = check(
        "struct Box<T:Copy>{value:T}trait Read<R:Copy>{fn read(self:&Self)->Box<R>;}fn main(){}",
    );
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
}

#[test]
fn trait_argument_lexical_binders_and_structural_capabilities_are_separate() {
    for source in [
        "trait Flag<R>{}struct U{}fn unused<U,T:Flag<U>>(x:&T){}fn main(){}",
        "trait Flag<R>{}struct P{}impl Flag<&i32> for P{}fn main(){}",
        "trait Flag<R>{}resource R{n:i32}struct P{}impl Flag<R> for P{}fn main(){}",
        "trait Transfer<R>{}struct P{}impl Transfer<i32> for P{}fn main(){}",
        "trait Copy<R>{}struct P{}impl Copy<i32> for P{}fn main(){}",
        "trait Share<R>{}struct P{}impl Share<i32> for P{}fn main(){}",
    ] {
        let report = check(source);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{source}: {}",
            report.to_json()
        );
    }
    native(&[(
        "main",
        "type R=i64;trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}fn get<T:Read<i32>>(x:&T)->i32{return x.read()}fn main()->i32{let p=P{n:7};return get(&p)-7}",
    )]);
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)
}
fn check(text: &str) -> CompileReport {
    compile_source(&source(text), &CompileOptions::default())
}
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn native(files: &[(&str, &str)]) {
    native_backend(files, zil_driver::NativeBackend::default());
}
fn native_both(files: &[(&str, &str)]) {
    native_backend(files, zil_driver::NativeBackend::Clang);
    if cfg!(feature = "llvm-api") {
        native_backend(files, zil_driver::NativeBackend::LlvmApi);
    }
}
fn native_backend(files: &[(&str, &str)], backend: zil_driver::NativeBackend) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "bounds162-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let options = CompileOptions {
            backend,
            stop_after: Stage::Build,
            optimization,
            output: Some(executable.clone()),
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        };
        let report = if files.len() == 1 {
            compile_source(&source(files[0].1), &options)
        } else {
            compile_project(&project(files), &options)
        };
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == zil_driver::NativeBackend::LlvmApi {
            assert!(!codegen.object_reports.is_empty());
            assert!(
                codegen
                    .object_reports
                    .iter()
                    .all(|object| object.contains("\"input_kind\":\"builder\""))
            );
        }
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}
fn reject(text: &str, fragment: &str) {
    let report = check(text);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(fragment)),
        "{}",
        report.to_json()
    );
}

#[test]
fn shared_bounds_dispatch_for_multiple_nominal_types_and_forward_symbolically() {
    native(&[(
        "main",
        r#"
trait Measure { fn measure(self:&Self)->i32; }
struct Point { value:i32 }
struct Area { width:i32,height:i32 }
impl Measure for Point { fn measure()->i32=>.value }
impl Measure for Area { fn measure()->i32=>.width*.height }
fn measured<T:Measure>(value:&T)->i32=>value.measure()
fn relay<U:Measure>(value:&U)->i32=>measured(value)
fn main()->i32 { let p=Point{value:7};let a=Area{width:5,height:7};return relay(&p)+measured::<Area>(&a)-42 }
"#,
    )]);
}

#[test]
fn mutable_multiple_bounds_and_value_receiver_keep_passing_modes() {
    native(&[(
        "main",
        r#"
trait Read { fn get(self:&Self)->i32; }
trait Change { fn set(self:&mut Self,n:i32); }
trait Consume { fn finish(self:Self)->i32; }
struct Counter { n:i32 }
impl Read for Counter { fn get()->i32=>.n }
impl Change for Counter { fn set(n:i32){.n=n} }
impl Consume for Counter { fn finish()->i32=>.n }
fn update<T:Read+Change>(value:&mut T,n:i32)->i32{value.set(n);return value.get()}
fn local<T:Read+Change>(value:T,n:i32)->i32{var x=value;x.set(n);return x.get()}
fn finish<T:Consume>(value:T)->i32=>value.finish()
fn main()->i32 {var c=Counter{n:1};if update(&mut c,20)!=20{return 1};if local(c,40)!=40{return 2};return finish(Counter{n:42})-42}
"#,
    )]);
}

#[test]
fn bound_method_named_len_and_self_parameters_are_symbolic() {
    native(&[(
        "main",
        r#"
trait Measure {fn len(self:&Self)->i32;fn plus(self:&Self,other:&Self)->i32;}
struct Number {n:i32}
impl Measure for Number {fn len()->i32=>.n;fn plus(other:&Self)->i32=>.n+other.n}
fn combine<T:Measure>(a:&T,b:&T)->i32=>a.len()+a.plus(b)
fn main()->i32{let a=Number{n:7};let b=Number{n:28};return combine(&a,&b)-42}
"#,
    )]);
}

#[test]
fn imported_public_bounds_keep_definition_and_nominal_module_identity() {
    native(&[
        (
            "main",
            "import left;import right;import helpers;fn main()->i32{let a=left.make(7);let b=right.make(35);return helpers.apply(&a)+right.apply(&b)-42}",
        ),
        (
            "left",
            "pub trait Measure{fn measure(self:&Self)->i32;}pub struct Value{n:i32}impl Measure for Value{fn measure()->i32=>.n}pub fn make(n:i32)->Value=>Value{n:n}",
        ),
        (
            "right",
            "pub trait Measure{fn measure(self:&Self)->i32;}pub struct Value{n:i32}impl Measure for Value{fn measure()->i32=>.n}pub fn make(n:i32)->Value=>Value{n:n};pub fn apply<T:Measure>(x:&T)->i32=>x.measure()",
        ),
        (
            "helpers",
            "import left;pub fn apply<T:left.Measure>(x:&T)->i32=>x.measure()",
        ),
    ]);
}

#[test]
fn same_named_inherent_method_does_not_satisfy_nominal_bound() {
    reject(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl C{fn get(self:&Self)->i32=>.n}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
        "does not satisfy nominal trait bound",
    );
}

#[test]
fn unused_definitions_cannot_gain_members_types_or_copy_from_instances() {
    for (body, reason) in [
        (
            "fn bad<T:Read>(x:&T)->i32=>x.missing()",
            "not provided by the declared trait bounds",
        ),
        (
            "fn bad<T:Read>(x:&T)->bool=>x.get()",
            "generic body types must agree",
        ),
        (
            "fn bad<T:Read>(x:&T)->i32=>x.get(true)",
            "argument count differs",
        ),
        ("fn bad<T:Read>(x:T)->(T,T)=>(x,x)", "used after move"),
        ("fn bad<T:Read>(x:T)->T=>x+1", "requires a declared bound"),
    ] {
        let text = format!("trait Read{{fn get(self:&Self)->i32;}}{body};fn main(){{}}");
        let report = check(&text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(reason)),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn forwarding_requires_a_prior_bound_even_when_unused() {
    reject(
        "trait Read{fn get(self:&Self)->i32;}fn read<T:Read>(x:&T)->i32=>x.get();fn bad<U>(x:&U)->i32=>read(x);fn main(){}",
        "does not satisfy nominal trait bound",
    );
    reject(
        "trait Read{fn get(self:&Self)->i32;}struct T{n:i32}impl Read for T{fn get()->i32=>.n}fn read<U:Read>(x:&U)->i32=>x.get();fn bad<T>(x:&T)->i32=>read(x);fn main(){}",
        "does not satisfy nominal trait bound",
    );
}

#[test]
fn missing_trait_wrong_arguments_and_ambiguous_bounds_reject_before_specialization() {
    reject(
        "fn bad<T:Missing>(x:T)->T=>x;fn main(){}",
        "not a declared nominal trait",
    );
    reject(
        "trait Read{fn get(self:&Self,n:i32)->i32;}fn bad<T:Read>(x:&T)->i32=>x.get(true);fn main(){}",
        "generic body types must agree",
    );
    reject(
        "trait A{fn get(self:&Self)->i32;}trait B{fn get(self:&Self)->i32;}fn bad<T:A+B>(x:&T)->i32=>x.get();fn main(){}",
        "ambiguous between declared trait bounds",
    );
}

#[test]
fn borrowed_receivers_do_not_gain_mutation_or_move_permissions() {
    reject(
        "trait Change{fn set(self:&mut Self,n:i32);}fn bad<T:Change>(x:&T){x.set(1)}fn main(){}",
        "shared reference cannot grant exclusive access",
    );
    reject(
        "trait Change{fn set(self:&mut Self,n:i32);}fn bad<T:Change>(x:T){x.set(1)}fn main(){}",
        "requires a writable place",
    );
    reject(
        "trait Consume{fn take(self:Self)->i32;}fn bad<T:Consume>(x:&T)->i32=>x.take();fn main(){}",
        "cannot move an unknown non-Copy trait receiver",
    );
    reject(
        "trait Consume{fn take(self:Self)->i32;}fn bad<T:Consume>(x:T)->i32{x.take();return x.take()}fn main(){}",
        "used after move",
    );
}

#[test]
fn actual_impl_signatures_and_independent_loan_checks_still_run() {
    let report = check(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->bool=>true}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
    );
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
            .any(|d| d.code == "E-TRAIT-SIGNATURE"),
        "{}",
        report.to_json()
    );
    let report = check(
        "trait Change{fn set(self:&mut Self,n:i32);}struct C{n:i32}impl Change for C{fn set(n:i32){.n=n}}fn update<T:Change>(x:&mut T){x.set(1)}fn main()->i32{var c=C{n:7};let old=&c;update(&mut c);return old.n}",
    );
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
            .any(|d| d.code.starts_with("E-BORROW") || d.code.starts_with("E-ADDRESS")),
        "{}",
        report.to_json()
    );
}

#[test]
fn private_and_same_spelling_foreign_traits_cannot_satisfy_bound() {
    let files = [
        (
            "main",
            "import model;fn read<T:model.Secret>(x:&T)->i32=>x.get();fn main(){}",
        ),
        ("model", "trait Secret{fn get(self:&Self)->i32;}"),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics.iter().any(|d| d.code.contains("PRIVATE")),
        "{}",
        r.to_json()
    );
    let files = [
        (
            "main",
            "import left;import right;fn main()->i32{let c=right.make();return left.read(&c)}",
        ),
        (
            "left",
            "pub trait Read{fn get(self:&Self)->i32;}pub fn read<T:Read>(x:&T)->i32=>x.get()",
        ),
        (
            "right",
            "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make()->C=>C{n:7}",
        ),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("does not satisfy nominal trait bound")),
        "{}",
        r.to_json()
    );
}

#[test]
fn trait_bound_scans_obey_actual_work_budget() {
    let source = source(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->i32=>.n}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    assert!(zil_expand::expand_with_map_limits(&source, &ast, &Limits::default()).is_ok());
    let mut low = 0usize;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_expand::expand_with_map_limits(&source, &ast, &limits) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                low = middle;
            }
        }
    }
    assert!(
        zil_expand::expand_with_map_limits(
            &source,
            &ast,
            &Limits {
                max_analysis_steps: high,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let errors = zil_expand::expand_with_map_limits(
        &source,
        &ast,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}
