use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_syntax::{parse_with_limits, parse_with_limits_and_requirements};

fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(7), ModuleId(9), "qualification.zil", text)
}

#[test]
fn success_requirements_match_real_lexer_and_parser_boundaries() {
    for text in [
        "",
        "fn main()->i32=>0",
        "/* a\nb */ fn f(x: i32)->i32 { return (x + 1) * (x + 2) }",
        "#[doc=\"a\"] #[allow(unused)] struct S { #[align(4)] x: i32; y: i32 }",
        "#[cfg(false)] #[unknown] struct S { #[unknown] x: i32 }",
        "#if (false) { #[unknown] fn f() {} } else { #[other] fn g() {} }",
    ] {
        let source = input(text);
        let limits = Limits::default();
        let (parsed, requirements) = parse_with_limits_and_requirements(&source, &limits);
        assert!(parsed.is_ok(), "{text}: {:?}", parsed.diagnostics);
        let requirements = requirements.unwrap();
        assert_eq!(
            format!("{parsed:?}"),
            format!("{:?}", parse_with_limits(&source, &limits))
        );
        for n in 0..=parsed.tokens.len() + 4 {
            let token_limits = Limits {
                max_tokens: n,
                ..limits.clone()
            };
            assert_eq!(
                requirements.allows(&token_limits),
                parse_with_limits(&source, &token_limits).is_ok(),
                "token n={n}: {text}"
            );
        }
        for n in 0..128 {
            let node_limits = Limits {
                max_nodes: n,
                ..limits.clone()
            };
            assert_eq!(
                requirements.allows(&node_limits),
                parse_with_limits(&source, &node_limits).is_ok(),
                "node n={n}: {text}"
            );
        }
    }
}

#[test]
fn depth_stays_exact_and_failed_parses_never_have_qualification() {
    let source = input("fn main()->i32=>1");
    let limits = Limits::default();
    let (_, requirements) = parse_with_limits_and_requirements(&source, &limits);
    let requirements = requirements.unwrap();
    for depth in [
        0,
        1,
        8,
        limits.max_call_depth - 1,
        limits.max_call_depth + 1,
    ] {
        assert!(!requirements.allows(&Limits {
            max_call_depth: depth,
            ..limits.clone()
        }));
    }
    for text in [
        "fn f(",
        "#[unknown] fn f() {}",
        "fn f()->i32=>\"unterminated",
    ] {
        let (parsed, requirements) = parse_with_limits_and_requirements(&input(text), &limits);
        assert!(!parsed.is_ok());
        assert!(requirements.is_none());
    }
    for limited in [
        Limits {
            max_tokens: 1,
            ..limits.clone()
        },
        Limits {
            max_nodes: 0,
            ..limits.clone()
        },
        Limits {
            max_source_bytes: 0,
            ..limits.clone()
        },
        Limits {
            max_call_depth: 0,
            ..limits.clone()
        },
    ] {
        let (parsed, requirements) = parse_with_limits_and_requirements(&source, &limited);
        assert!(!parsed.is_ok());
        assert!(requirements.is_none());
    }
}

#[test]
fn eof_is_free_for_lexer_but_source_bytes_still_required() {
    let source = input("fn main()->i32=>0");
    let limits = Limits::default();
    let (parsed, requirements) = parse_with_limits_and_requirements(&source, &limits);
    let exact = Limits {
        max_tokens: parsed.tokens.len() - 1,
        max_source_bytes: source.text.len(),
        ..limits
    };
    assert!(requirements.unwrap().allows(&exact));
    assert!(parse_with_limits(&source, &exact).is_ok());
    let short = Limits {
        max_source_bytes: source.text.len() - 1,
        ..exact
    };
    assert!(!requirements.unwrap().allows(&short));
    assert!(!parse_with_limits(&source, &short).is_ok());
}
