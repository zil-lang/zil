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

#[test]
fn scoped_generic_local_bodies_are_checked_before_instantiation() {
    const ROLES: &str = r#"
extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,data:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
"#;
    for body in [
        "let body=()->i32=>3;unsafe{return c_with_callback(reg,close,f,body)}",
        "let n=3;let body=()->i32=>n;unsafe{return c_with_callback(reg,close,f,body)}",
        "var n=3;let body=()->i32=>{n=n+1;return n};unsafe{return c_with_callback(reg,close,f,body)}",
        "unsafe{return c_with_callback(reg,close,f,()->i32=>3)}",
        "let ignored=()->i32=>3;unsafe{return c_with_callback(reg,close,f,()->i32=>4)}",
        "let body=()->i32=>3;unsafe{let first=c_with_callback(reg,close,f,body);return c_with_callback(reg,close,f,body)}",
        "var n=3;let body=move ()->i32=>{n=n+1;return n};unsafe{let first=c_with_callback(reg,close,f,body);return c_with_callback(reg,close,f,body)}",
    ] {
        let text = format!("{ROLES}fn run<F:FnMut(i32)->i32>(f:&mut F)->(bool,i32){{{body}}}");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "local-body.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
    for body in [
        "let body=()->i32=>f(1);unsafe{return c_with_callback(reg,close,f,body)}",
        "let body=(x:i32)->i32=>x;unsafe{return c_with_callback(reg,close,f,body)}",
        "let body=()->bool=>true;unsafe{return c_with_callback(reg,close,f,body)}",
        "let n=3;let body=()->i32=>n;unsafe{let first=c_with_callback(reg,close,f,body);return c_with_callback(reg,close,f,body)}",
    ] {
        let text = format!("{ROLES}fn run<F:FnMut(i32)->i32>(f:&mut F)->(bool,i32){{{body}}}");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "local-body-reject.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        assert!(zil_expand::expand(&source, &parsed).is_err(), "{body}");
    }
    for (calls, accepted) in [
        ("return c_with_callback(reg,close,f,body)", true),
        (
            "let first=c_with_callback(reg,close,f,body);return c_with_callback(reg,close,f,body)",
            false,
        ),
    ] {
        let text = format!(
            "{ROLES}resource R{{n:i32}}package P{{manages r:R}}fn consume(p:P)->i32{{return p.r.n}}fn run<F:FnMut(i32)->i32>(f:&mut F,p:P)->(bool,i32){{let body=move ()->i32=>consume(p);unsafe{{{calls}}}}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "local-once-body.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        assert_eq!(
            zil_expand::expand(&source, &parsed).is_ok(),
            accepted,
            "{calls}"
        );
    }
}
