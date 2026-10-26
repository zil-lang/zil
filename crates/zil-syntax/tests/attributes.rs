use zil_base::{FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_syntax::ast::ItemKind;
use zil_syntax::attributes::{
    self, AttributeContext, AttributeTarget, ConfigValue, CustomAttribute, CustomAttributeKind,
    DeferredReason, RepeatRule,
};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(21), ModuleId(5), "attributes.zil", text)
}

fn accepted(text: &str) -> zil_syntax::ParseResult {
    let parsed = zil_syntax::parse(&source(text));
    assert!(parsed.is_ok(), "{text}\n{:#?}", parsed.diagnostics);
    parsed
}

fn rejected(text: &str, code: &str) {
    let input = source(text);
    let parsed = zil_syntax::parse(&input);
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == code),
        "{text}\nexpected {code}, got {:#?}",
        parsed.diagnostics
    );
    for diagnostic in &parsed.diagnostics {
        assert_eq!(diagnostic.category, ResultCategory::SafeReject);
        assert!(diagnostic.span.unwrap().is_valid_for(&input));
    }
}

#[test]
fn plan_t01_005_registry_matches_frozen_normative_input() {
    let registry = include_bytes!("../../../spec/attributes.json");
    assert_eq!(
        Fingerprint::of(registry).to_hex(),
        attributes::REGISTRY_SHA256
    );
    let schedule: Vec<_> = attributes::BUILTIN_ATTRIBUTES
        .iter()
        .map(|a| (a.name, a.phase, a.repeat))
        .collect();
    assert_eq!(
        schedule,
        [
            ("cfg", 0, RepeatRule::And),
            ("repr", 3, RepeatRule::Error),
            ("align", 3, RepeatRule::Error),
            ("derive", 2, RepeatRule::MergeUniqueTraits),
            ("inline", 5, RepeatRule::Error),
            ("doc", 1, RepeatRule::Concat),
            ("allow", 4, RepeatRule::Union),
            ("test", 4, RepeatRule::Error),
        ]
    );
}

#[test]
fn plan_t01_005_all_builtin_attachment_targets_are_checked() {
    // Independent source fixtures cover all declared attachment categories,
    // including resource versus package and fn-shaped associated/extern items.
    let templates = [
        "@\nfn f() {}",
        "@\nstruct S {}",
        "@\nenum E { A }",
        "@\nresource R {}",
        "@\npackage P {}",
        "@\ntrait T { fn f(); }",
        "@\nimpl S { fn f() {} }",
        "@\ntype Number = i32;",
        "@\nconst value: i32 = 1;",
        "struct S {\n@\nx: i32\n}",
        "enum E {\n@\nA\n}",
        "fn f(@ x: i32) {}",
        "@\nextern \"C\" fn f();",
        "struct S {\n@\nfn f() {}\n}",
    ];
    let all: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    let cases: &[(&str, &[usize])] = &[
        ("#[cfg(true)]", all),
        ("#[repr(C)]", &[1, 2, 4]),
        ("#[align(8)]", &[1, 2, 4, 9]),
        ("#[derive(Debug)]", &[1, 2, 4]),
        ("#[inline]", &[0, 12, 13]),
        ("#[doc = \"help\"]", all),
        ("#[allow(unused)]", all),
        ("#[test]", &[0, 12, 13]),
    ];
    for &(attribute, targets) in cases {
        for (index, template) in templates.iter().enumerate() {
            let text = template.replace('@', attribute);
            let parsed = zil_syntax::parse(&source(&text));
            if targets.contains(&index) {
                assert!(parsed.is_ok(), "{text}\n{:#?}", parsed.diagnostics);
            } else {
                assert_eq!(
                    parsed.diagnostics.len(),
                    1,
                    "{text}\n{:#?}",
                    parsed.diagnostics
                );
                assert_eq!(
                    parsed.diagnostics[0].code, "E-PARSE-ATTRIBUTE-TARGET",
                    "{text}"
                );
                assert_eq!(parsed.diagnostics[0].phase, Phase::Parse);
            }
        }
    }
}

#[test]
fn plan_t01_005_unknown_and_preregistered_custom_attributes_are_distinct() {
    rejected("#[typo]\nfn f() {}", "E-PARSE-ATTRIBUTE-UNKNOWN");
    rejected("#[schema.Record]\nstruct S {}", "E-PARSE-ATTRIBUTE-UNKNOWN");
    let mut context = AttributeContext::default();
    let custom = CustomAttribute {
        path: vec!["schema".into(), "Record".into()],
        kind: CustomAttributeKind::Generate,
        targets: vec![AttributeTarget::Struct],
        repeat: RepeatRule::Error,
    };
    context.register_custom(custom.clone()).unwrap();
    let text = source("#[schema.Record]\nstruct S {}");
    let parsed = zil_syntax::parse_with_attribute_context(&text, &Limits::default(), &context);
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed.module.items[0].attributes[0].path,
        ["schema", "Record"]
    );
    // Registration is explicit data. It cannot load or execute an implementation
    // and successful syntax does not remove the still-unexecuted request.
    for invalid in [
        CustomAttribute {
            path: vec!["Record".into()],
            ..custom.clone()
        },
        CustomAttribute {
            targets: vec![],
            ..custom.clone()
        },
        custom,
    ] {
        let errors = context.register_custom(invalid).unwrap_err();
        assert_eq!(errors[0].code, "E-PARSE-ATTRIBUTE-REGISTRY");
    }
    let text = source("#[schema.Record]\nfn f() {}");
    let parsed = zil_syntax::parse_with_attribute_context(&text, &Limits::default(), &context);
    assert_eq!(parsed.diagnostics[0].code, "E-PARSE-ATTRIBUTE-TARGET");
}

#[test]
fn plan_t01_005_repeat_rules_and_inline_conflicts_are_separate() {
    for (attributes, target) in [
        ("#[repr(C)]\n#[repr(C)]", "struct S {}"),
        ("#[align(8)]\n#[align(8)]", "struct S {}"),
        ("#[inline(always)]\n#[inline(always)]", "fn f() {}"),
        ("#[test]\n#[test]", "fn f() {}"),
    ] {
        rejected(
            &format!("{attributes}\n{target}"),
            "E-PARSE-ATTRIBUTE-DUPLICATE",
        );
    }
    for attributes in [
        "#[inline(always)]\n#[inline(never)]",
        "#[inline(never)]\n#[inline]",
        "#[inline(never)]\n#[inline(never)]",
    ] {
        rejected(
            &format!("{attributes}\nfn f() {{}}"),
            "E-PARSE-ATTRIBUTE-CONFLICT",
        );
    }
    let parsed = accepted(
        "#[cfg(true)]\n#[cfg(true)]\n#[doc = \"first\"]\n#[doc(\"second\")]\n#[allow(unused)]\n#[allow(unused, style)]\nfn f() {}",
    );
    let attrs = &parsed.module.items[0].attributes;
    assert_eq!(attrs.len(), 6);
    assert_eq!(attrs[2].string_value.as_deref(), Some("\"first\""));
    assert_eq!(attrs[3].path, ["doc"]);
    assert!(
        attrs[2].span.start < attrs[3].span.start,
        "doc concatenation must retain source order"
    );
}

#[test]
fn plan_t01_005_derive_rejects_repeated_written_trait_paths() {
    for attrs in [
        "#[derive(Debug, Debug)]",
        "#[derive(Debug)]\n#[derive(Eq, Debug)]",
        "#[derive(standard.Debug, standard.Debug)]",
    ] {
        rejected(
            &format!("{attrs}\nstruct S {{}}"),
            "E-PARSE-ATTRIBUTE-DUPLICATE-TRAIT",
        );
    }
    accepted("#[derive(first.Debug)]\n#[derive(second.Debug, Eq)]\nstruct S {}");
}

#[test]
fn plan_t01_005_registered_argument_structure_is_enforced() {
    for (attribute, target) in [
        ("#[cfg()]", "fn f() {}"),
        ("#[cfg(true, false)]", "fn f() {}"),
        ("#[repr(C, transparent)]", "struct S {}"),
        ("#[align()]", "struct S {}"),
        ("#[derive(42)]", "struct S {}"),
        ("#[derive(make_trait())]", "struct S {}"),
        ("#[inline(always, never)]", "fn f() {}"),
        ("#[doc]", "fn f() {}"),
        ("#[allow(make_lint())]", "fn f() {}"),
        ("#[test(1)]", "fn f() {}"),
    ] {
        rejected(
            &format!("{attribute}\n{target}"),
            "E-PARSE-ATTRIBUTE-ARGUMENTS",
        );
    }
    rejected("#[doc = 42]\nfn f() {}", "E-PARSE-ATTRIBUTE-VALUE");
}

#[test]
fn plan_t01_005_attribute_value_types_are_not_claimed_by_the_parser() {
    // Structure parses; later registered phases still owe value types, enum
    // choices, actual trait identities, lint membership, and target layout.
    for (attribute, target) in [
        ("#[align(3)]", "struct S {}"),
        ("#[align(\"eight\")]", "struct S {}"),
        ("#[align(#(8))]", "struct S {}"),
        ("#[repr(unknown_repr)]", "struct S {}"),
        ("#[doc(123)]", "fn f() {}"),
        ("#[inline(unknown_mode)]", "fn f() {}"),
        ("#[derive(UnknownTrait)]", "struct S {}"),
        ("#[allow(UnknownLint)]", "fn f() {}"),
    ] {
        let parsed = accepted(&format!("{attribute}\n{target}"));
        assert_eq!(parsed.module.items[0].attributes.len(), 1);
    }
}

#[test]
fn plan_t01_005_cfg_removal_precedes_other_attribute_resolution() {
    for attrs in [
        "#[cfg(false)]\n#[unknown]",
        "#[unknown]\n#[cfg(false)]",
        "#[cfg(target.unknown)]\n#[unknown]\n#[cfg(false)]",
        "#[inline]\n#[inline]\n#[unknown]\n#[cfg(false)]",
    ] {
        let parsed = accepted(&format!("{attrs}\nstruct S {{ #[also_unknown] x: i32 }}"));
        assert_eq!(parsed.cfg_removed, [parsed.module.items[0].span]);
        assert!(parsed.deferred_attributes.is_empty());
        assert_eq!(
            parsed.module.items.len(),
            1,
            "source-shaped AST preserves removed text"
        );
    }
    let parsed = accepted(
        "struct S { #[unknown] #[cfg(false)] x: i32 }\nenum E { #[unknown] #[cfg(false)] A }\nfn f(#[unknown] #[cfg(false)] x: i32) {}",
    );
    assert_eq!(parsed.cfg_removed.len(), 3);
    rejected(
        "#[unknown]\n#[cfg(true)]\nfn f() {}",
        "E-PARSE-ATTRIBUTE-UNKNOWN",
    );
}

#[test]
fn plan_t01_005_unresolved_configuration_defers_until_snapshot_is_supplied() {
    let input =
        source("#[unknown]\n#[cfg(target.os == \"linux\")]\nstruct S { #[unknown] x: i32 }");
    let parsed = zil_syntax::parse(&input);
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.deferred_attributes.len(), 2);
    assert!(
        parsed
            .deferred_attributes
            .iter()
            .all(|d| d.reason == DeferredReason::Configuration)
    );
    assert!(parsed.cfg_removed.is_empty());
    let mut context = AttributeContext::default();
    context
        .configuration
        .insert("target.os".into(), ConfigValue::String("windows".into()));
    let validation = attributes::validate(&parsed.module, &context, &Limits::default());
    assert!(validation.diagnostics.is_empty());
    assert_eq!(validation.removed, [parsed.module.items[0].span]);
    context
        .configuration
        .insert("target.os".into(), ConfigValue::String("linux".into()));
    let parsed = zil_syntax::parse_with_attribute_context(&input, &Limits::default(), &context);
    assert_eq!(parsed.diagnostics.len(), 2);
    assert!(
        parsed
            .diagnostics
            .iter()
            .all(|d| d.code == "E-PARSE-ATTRIBUTE-UNKNOWN")
    );
    let deferred = accepted("#[cfg(config_call())]\n#[unknown]\nfn f() {}");
    assert_eq!(
        deferred.deferred_attributes.len(),
        1,
        "a parser never runs a configuration call"
    );
}

#[test]
fn plan_t01_005_unmaterialized_compile_time_items_defer_attribute_resolution() {
    let parsed = accepted("#if (false) { #[unknown] fn f() {} } else { #[other] fn g() {} }");
    assert_eq!(parsed.deferred_attributes.len(), 2);
    assert!(
        parsed
            .deferred_attributes
            .iter()
            .all(|d| d.reason == DeferredReason::CompileTimeSelection)
    );
    let ItemKind::Meta(_) = &parsed.module.items[0].kind else {
        panic!()
    };
    // Dead branches still undergo lexical/delimiter/parameter-structure parsing.
    let malformed = zil_syntax::parse(&source("#[cfg(false)]\n#[unknown(]\nfn f() {}"));
    assert!(!malformed.is_ok());
    assert!(
        malformed
            .diagnostics
            .iter()
            .any(|d| d.phase == Phase::Parse)
    );
    let malformed = zil_syntax::parse(&source("#if (false) { #[unknown(] fn f() {} }"));
    assert!(!malformed.is_ok());
}

#[test]
fn plan_t01_005_attributes_cannot_float_or_attach_to_statements_or_values() {
    for text in [
        "#[doc = \"floating\"]",
        "struct S { #[doc = \"floating\"] }",
        "enum E { #[doc = \"floating\"] }",
        "fn f(#[doc = \"floating\"]) {}",
        "#[doc = \"wrong\"]\nlet x = 1;",
        "fn f() { #[doc = \"wrong\"] let x = 1; }",
        "fn f() { let x = #[doc = \"wrong\"] 1; }",
    ] {
        rejected(text, "E-PARSE-ATTRIBUTE-ATTACHMENT");
    }
    rejected("#![doc = \"inner\"]\nfn f() {}", "E-PARSE-HASH-DISPATCH");
    rejected("#[doc(\"a\"), doc(\"b\")]\nfn f() {}", "E-PARSE-EXPECTED");
}
