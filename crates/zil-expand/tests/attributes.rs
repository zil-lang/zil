use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};

#[test]
fn unexecuted_attributes_cannot_disappear_from_core_parameters_or_fields() {
    for (text, attachment) in [
        (
            "fn f(#[doc = \"parameter\"] x: i32) -> i32 => x",
            "parameter",
        ),
        ("fn f(#[cfg(false)] x: i32) -> i32 => x", "parameter"),
        (
            "struct S {}\nimpl S { fn f(#[doc = \"receiver\"] self: &Self) {} }",
            "parameter",
        ),
        ("struct S { #[align(8)] x: i32 }", "field"),
        ("package P { #[cfg(false)] x: i32 }", "field"),
        ("#[inline(always)]\nfn f() {}", "declaration"),
        ("impl S { #[doc = \"method\"] fn f() {} }", "declaration"),
    ] {
        let source = SourceUnit::new(FileId(19), ModuleId(6), "attributes.zil", text);
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.is_ok(), "{text}\n{:#?}", parsed.diagnostics);
        let errors = zil_expand::expand(&source, &parsed.module).unwrap_err();
        assert_eq!(errors.len(), 1, "{text}\n{errors:#?}");
        let diagnostic = &errors[0];
        assert_eq!(diagnostic.phase, Phase::Expand);
        assert_eq!(diagnostic.category, ResultCategory::Unsupported);
        assert_eq!(diagnostic.code, "E-UNSUPPORTED");
        assert!(diagnostic.message.contains(attachment));
        let span = diagnostic.span.unwrap();
        assert!(span.is_valid_for(&source));
        assert_eq!(span.start, text.find("#[").unwrap());
        assert_eq!(span.end, text.find(']').unwrap() + 1);
    }
}

#[test]
fn scoped_foreign_primitive_rejects_value_bindings_but_preserves_member_names() {
    for text in [
        "fn main(){let c_with_callback=1}",
        "fn main(c_with_callback:i32){}",
        "fn main(){let f=(c_with_callback:i32)->i32=>0}",
        "fn main(){for c_with_callback in 0..1{}}",
        "enum E{A(i32)}fn main(e:E){match e{E.A(c_with_callback)=>{}}}",
        "fn c_with_callback(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "scoped-name.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &parsed).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Expand && e.code == "E-FFI-SCOPED-NAME"),
            "{text}: {errors:?}"
        );
    }
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "member.zil",
        "struct S{c_with_callback:i32}fn read(s:S)->i32{return s.c_with_callback}",
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &parsed).unwrap();
}
