use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_syntax::{ParseResult, parse, parse_with_limits};

fn same_syntax(a: &ParseResult, b: &ParseResult) {
    assert_eq!(a.module, b.module);
    assert_eq!(a.tokens, b.tokens);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.deferred_attributes, b.deferred_attributes);
    assert_eq!(a.cfg_removed, b.cfg_removed);
    assert_eq!(a.usage, b.usage);
}

#[test]
fn successful_receipts_cover_parser_and_attribute_node_boundaries() {
    for text in [
        "",
        "// 中文\n/* outer /* inner */ */\nfn answer()->i32=>1+2*3\n",
        "#[doc=\"a\"]\n#[doc=\"b\"]\n#[doc=\"c\"]\nfn f(){}",
        "struct S{x:i32;y:i32;z:i32}",
        "#[cfg(false)]\n#[unknown]\nfn removed(){}\n#[cfg(feature)]\n#[unknown]\nfn pending(){}",
    ] {
        let source = SourceUnit::new(FileId(7), ModuleId(9), "receipt.zil", text);
        let full = parse(&source);
        assert!(full.is_ok(), "{text}: {:?}", full.diagnostics);
        for nodes in 0..=full.usage.nodes_required + 1 {
            let limits = Limits {
                max_nodes: nodes,
                ..Limits::default()
            };
            let fresh = parse_with_limits(&source, &limits);
            assert_eq!(
                full.fits_limits(&limits),
                fresh.is_ok(),
                "nodes={nodes}: {text}"
            );
            if full.fits_limits(&limits) {
                same_syntax(&full, &fresh);
            }
        }
        let exact = Limits {
            max_tokens: full.tokens.len(),
            ..Limits::default()
        };
        assert!(full.fits_limits(&exact));
        same_syntax(&full, &parse_with_limits(&source, &exact));
        assert!(!full.fits_limits(&Limits {
            max_tokens: full.tokens.len() - 1,
            ..Limits::default()
        }));
    }
}

#[test]
fn recovered_or_invalid_syntax_never_supplies_a_success_receipt() {
    for text in ["fn f( ->", "#[unknown]\nfn f(){}", "fn f()->i32=>((((1))))"] {
        let source = SourceUnit::new(FileId(2), ModuleId(3), "invalid.zil", text);
        let low = Limits {
            max_call_depth: 1,
            ..Limits::default()
        };
        let parsed = parse_with_limits(&source, &low);
        assert!(!parsed.is_ok(), "{text}");
        assert!(!parsed.fits_limits(&Limits::default()));
    }
}
