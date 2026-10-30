use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_syntax::{
    cache::{self, CacheError},
    parse, parse_with_limits,
};

fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(7), ModuleId(9), "memo.zil", text)
}
fn roundtrip(text: &str) {
    let source = unit(text);
    let parsed = parse(&source);
    assert!(parsed.is_ok(), "{text}: {:?}", parsed.diagnostics);
    let bytes = cache::encode(&source, &parsed).unwrap();
    let decoded = cache::decode(&source, &bytes, &Limits::default()).unwrap();
    assert_eq!(parsed.module, decoded.module);
    assert_eq!(parsed.tokens, decoded.tokens);
    assert_eq!(parsed.deferred_attributes, decoded.deferred_attributes);
    assert_eq!(parsed.cfg_removed, decoded.cfg_removed);
    assert_eq!(parsed.usage, decoded.usage);
    assert!(decoded.is_ok());
}
#[test]
fn source_roundtrips_preserve_syntax_positions_attributes_and_tokens() {
    for text in [
        "",
        "// Unicode 中文\n/* nested /* comment */ */\nfn 答案()->i32=>42\n",
        "import a.b as c\nsyntax use html as h\nfn f() {}",
        "struct S<T> { x:T; pub y:i32=1; fn f(){} }\nresource R { x:i32 }\npackage P { x:i32 }\nenum E { A, B(i32), C{x:i32} }",
        "trait T<A> { fn f(x:A)->i32; }\nimpl T<i32> for S { fn f(x:i32)->i32=>x }\ntype X = i32\nconst x:i32=1\nextern \"C\" fn foreign(x:i32)->i32;",
        "fn f(){let a=move(x:i32)=>x+1;let b=()=>{return 2};let c=3|>(x=>x+1);let d=.{x:2};let e=.{...d,x:3}}",
        "type A = fn()->i32?\ntype B = (fn()->i32)?\ntype C = fn(i32)->E!i32\nfn make(n:i64)->impl Fn()->i64=>move()=>n",
        "fn f(){let a=[1,2];let b=(1,2);let c=a[0];let d=a[0..1];if true {return} else {loop {break}};while false {continue};for x in a {defer {x;}};match true {true=>1,false=>2}}",
        "#[cfg(false)]\n#[unknown]\nfn removed(){}\n#[cfg(feature)]\n#[unknown]\nfn pending(){}\n#if(true){#[doc=\"text\"]fn selected(){}}",
    ] {
        roundtrip(text);
    }
    for op in [
        "+", "-", "*", "/", "%", "+%", "-%", "*%", "<<", ">>", "&", "|", "^", "==", "!=", "<",
        "<=", ">", ">=", "&&", "||", "..",
    ] {
        roundtrip(&format!("fn f(a:i32,b:i32)=>a{op}b"));
    }
}
#[test]
fn receipts_cover_parser_and_attribute_budgets_and_preserve_low_budget_errors() {
    for text in [
        "fn f()->i32=>1+2*3",
        "#[doc=\"a\"]\n#[doc=\"b\"]\n#[doc=\"c\"]\nfn f(){}",
        "struct S{x:i32;y:i32;z:i32}",
    ] {
        let source = unit(text);
        let full = parse(&source);
        assert!(full.is_ok(), "{:?}", full.diagnostics);
        let bytes = cache::encode(&source, &full).unwrap();
        for nodes in 0..=full.usage.nodes_required + 1 {
            let limits = Limits {
                max_nodes: nodes,
                ..Limits::default()
            };
            let fresh = parse_with_limits(&source, &limits);
            let loaded = cache::decode(&source, &bytes, &limits);
            assert_eq!(loaded.is_ok(), fresh.is_ok(), "node budget {nodes}: {text}");
        }
        let low = Limits {
            max_tokens: full.tokens.len() - 1,
            ..Limits::default()
        };
        assert_eq!(
            cache::decode(&source, &bytes, &low).unwrap_err(),
            CacheError::Budget
        );
        let low = Limits {
            max_source_bytes: source.text.len() - 1,
            ..Limits::default()
        };
        assert_eq!(
            cache::decode(&source, &bytes, &low).unwrap_err(),
            CacheError::Budget
        );
    }
}
#[test]
fn malformed_or_incomplete_bytes_never_construct_a_successful_tree() {
    let source = unit("fn f()->i32=>42");
    let bytes = cache::encode(&source, &parse(&source)).unwrap();
    for end in 0..bytes.len() {
        assert!(
            cache::decode(&source, &bytes[..end], &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut huge = bytes.clone();
    huge[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(cache::decode(&source, &huge, &Limits::default()).is_err());
    let mut unknown = bytes.clone();
    unknown[32] = 255;
    assert!(cache::decode(&source, &unknown, &Limits::default()).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(cache::decode(&source, &trailing, &Limits::default()).is_err());
    let different = SourceUnit {
        module_id: ModuleId(10),
        ..source.clone()
    };
    assert!(cache::decode(&different, &bytes, &Limits::default()).is_err());
    let bad = unit("fn f( ->");
    assert!(cache::encode(&bad, &parse(&bad)).is_err());
}
#[test]
fn deep_valid_syntax_bypasses_memo_without_changing_parse_acceptance() {
    let source = unit(&format!(
        "fn f()->i32=>{}42{}",
        "(".repeat(70),
        ")".repeat(70)
    ));
    let parsed = parse(&source);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    assert_eq!(
        cache::encode(&source, &parsed).unwrap_err(),
        CacheError::Limit
    );
}
