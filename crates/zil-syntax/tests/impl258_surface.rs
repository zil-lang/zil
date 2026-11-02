// Untouched consumer262 source checkpoint8754ae0b8b6db182bc1cd438fc39e59b92068304.
// Syntax parsing only; CT typing, hygiene and execution remain separate.
use zil_base::{FileId, ModuleId, SourceUnit};

#[test]
fn type_meta_candidates_follow_type_productions_in_every_position() {
    for text in [
        "type Word=#(i64)",
        "type Word=#splice(make_type())",
        "type Word=#{yield i64}",
        "fn f(x:#(i64))->#(i64){let y:#(i64)=x;return y}",
        "struct S { value: [&#(i64); 4] }",
        "enum E { Value(#(i64)) }",
        "type F=fn(#(i64))->i64",
        "type R=Box<(i32,#(i64))>",
        "fn f<T:#(Bound)>() where T:#(Other) {}",
        "impl #(Trait) for #(Target) {}",
        "fn f(){let q=quote<Type>{#(i64)}}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "candidate.zil", text);
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.is_ok(), "{text}: {:?}", parsed.diagnostics);
        assert!(parsed.type_meta_candidate, "{text}");
    }
    for text in [
        "type Word=i64",
        "fn f(){let x=#(old())}",
        "fn f(){let x=#splice(old())}",
        "fn f(){let x=#{yield old()}}",
        "type A=[i64;#(old())]",
        "fn f(){let s=\"type Word=#(i64)\"}",
        "// type Word=#(i64)\nfn f(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "candidate.zil", text);
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.is_ok(), "{text}: {:?}", parsed.diagnostics);
        assert!(!parsed.type_meta_candidate, "{text}");
    }
}

#[test]
fn frozen_consumer_surface_parses_without_rewriting_source() {
    let files = [
        (
            "00_frozen_type_value_main.zil",
            include_str!("fixtures/impl258-consumer262/00_frozen_type_value_main.zil"),
        ),
        (
            "01_type_values_main.zil",
            include_str!("fixtures/impl258-consumer262/01_type_values_main.zil"),
        ),
        (
            "02_four_categories_main.zil",
            include_str!("fixtures/impl258-consumer262/02_four_categories_main.zil"),
        ),
        (
            "03_providers_hygiene_main.zil",
            include_str!("fixtures/impl258-consumer262/03_providers_hygiene_main.zil"),
        ),
        (
            "04_generated_ownership_main.zil",
            include_str!("fixtures/impl258-consumer262/04_generated_ownership_main.zil"),
        ),
        (
            "05_generated_allocation_main.zil",
            include_str!("fixtures/impl258-consumer262/05_generated_allocation_main.zil"),
        ),
        (
            "05b_generated_early_error_main.zil",
            include_str!("fixtures/impl258-consumer262/05b_generated_early_error_main.zil"),
        ),
        (
            "06_generated_impl_main.zil",
            include_str!("fixtures/impl258-consumer262/06_generated_impl_main.zil"),
        ),
        (
            "07_fresh_and_named_main.zil",
            include_str!("fixtures/impl258-consumer262/07_fresh_and_named_main.zil"),
        ),
        (
            "common/io.zil",
            include_str!("fixtures/impl258-consumer262/common/io.zil"),
        ),
        (
            "common/model.zil",
            include_str!("fixtures/impl258-consumer262/common/model.zil"),
        ),
        (
            "common/tracked.zil",
            include_str!("fixtures/impl258-consumer262/common/tracked.zil"),
        ),
        (
            "extra_generators.zil",
            include_str!("fixtures/impl258-consumer262/extra_generators.zil"),
        ),
        (
            "generators.zil",
            include_str!("fixtures/impl258-consumer262/generators.zil"),
        ),
        (
            "impl_generators.zil",
            include_str!("fixtures/impl258-consumer262/impl_generators.zil"),
        ),
        (
            "neg_duplicate_export_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_duplicate_export_main.zil"),
        ),
        (
            "neg_duplicate_owned_input_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_duplicate_owned_input_main.zil"),
        ),
        (
            "neg_emit_expr_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_emit_expr_main.zil"),
        ),
        (
            "neg_export_cannot_capture_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_export_cannot_capture_main.zil"),
        ),
        (
            "neg_frozen_emit_scalar_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_frozen_emit_scalar_main.zil"),
        ),
        (
            "neg_ident_in_runtime_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_ident_in_runtime_main.zil"),
        ),
        (
            "neg_item_emit_statements_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_item_emit_statements_main.zil"),
        ),
        (
            "neg_late_import_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_late_import_main.zil"),
        ),
        (
            "neg_layout_cycle_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_layout_cycle_main.zil"),
        ),
        (
            "neg_member_owner_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_member_owner_main.zil"),
        ),
        (
            "neg_nominal_identity_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_nominal_identity_main.zil"),
        ),
        (
            "neg_private_reflection_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_private_reflection_main.zil"),
        ),
        (
            "neg_runtime_read_in_ct_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_runtime_read_in_ct_main.zil"),
        ),
        (
            "neg_splice_wrong_category_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_splice_wrong_category_main.zil"),
        ),
        (
            "neg_syntax_is_not_automatic_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_syntax_is_not_automatic_main.zil"),
        ),
        (
            "neg_type_in_runtime_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_type_in_runtime_main.zil"),
        ),
        (
            "neg_type_requires_type_value_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_type_requires_type_value_main.zil"),
        ),
        (
            "neg_type_splice_expr_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_type_splice_expr_main.zil"),
        ),
        (
            "neg_unknown_provider_export_main.zil",
            include_str!("fixtures/impl258-consumer262/neg_unknown_provider_export_main.zil"),
        ),
        (
            "providers/expressions.zil",
            include_str!("fixtures/impl258-consumer262/providers/expressions.zil"),
        ),
        (
            "providers/items.zil",
            include_str!("fixtures/impl258-consumer262/providers/items.zil"),
        ),
        (
            "providers/scaling.zil",
            include_str!("fixtures/impl258-consumer262/providers/scaling.zil"),
        ),
        (
            "providers/statements.zil",
            include_str!("fixtures/impl258-consumer262/providers/statements.zil"),
        ),
        (
            "providers/support.zil",
            include_str!("fixtures/impl258-consumer262/providers/support.zil"),
        ),
        (
            "providers/types.zil",
            include_str!("fixtures/impl258-consumer262/providers/types.zil"),
        ),
    ];
    assert_eq!(files.len(), 40);
    for (index, (name, text)) in files.into_iter().enumerate() {
        let source = SourceUnit::new(FileId(index as u32), ModuleId(index as u32), name, text);
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.is_ok(), "{name}: {:?}", parsed.diagnostics);
    }
}
