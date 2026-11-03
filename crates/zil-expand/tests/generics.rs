use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::Type;

#[test]
fn generic_error_union_binding_and_call_contexts_accept_complete_types() {
    for text in [
        include_str!("../../../tests/generic-error-enum/pending/unique-binding-value.zil"),
        include_str!("../../../tests/generic-error-enum/pending/unique-call-argument.zil"),
        "enum Fault{Bad} fn sink(v:Fault!i32)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{let v:Fault!i32=7;return sink(v)+sink(f())} fn main(){}",
        "enum Choice<T>{None,Some(T)} fn sink<T>(v:Choice<T>!i32)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{f();return sink::<i32>(Choice.None)} fn main(){}",
        "fn sink<T>(value:T!i32,witness:bool!T)->i32{return 0}fn run<F:Fn()->i32>(f:F,v:bool!i64)->i32{f();return sink(7,v)}fn main()->i32{let f=()->i32=>0;let v:bool!i64=true;return run(f,v)}",
        "fn sink<T>(value:T!i32,witness:T,other:(bool!i32)!i64)->i32{return 0}fn run<F:Fn()->i32>(f:F,v:bool!i32)->i32{f();return sink(7,true,v)}fn main()->i32{let f=()->i32=>0;let v:bool!i32=true;return run(f,v)}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-context.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
    }
    for text in [
        "enum Fault{Bad} enum Other{Bad} fn run<F:Fn()->i32>(f:F)->i32{let v:Fault!i32=Other.Bad;return f()} fn main(){}",
        "enum Fault{Bad} fn run<F:Fn()->i32>(f:F)->i32{let v:Fault!Fault=Fault.Bad;return f()} fn main(){}",
        "enum Fault{Bad} fn sink(v:Fault!i64)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{return sink(7)} fn main(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-context-reject.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &parsed).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.phase == Phase::Expand && error.code == "E-GENERIC-ARGUMENT"),
            "{text}: {errors:?}"
        );
    }
}

#[test]
fn generic_error_union_call_constraints_are_independent_of_argument_order() {
    for (parameters, arguments) in [
        ("value:T!i32,witness:T", "7,true"),
        ("witness:T,value:T!i32", "true,7"),
        ("value:Choice<T>!i32,witness:T", "Choice.None,true"),
        ("witness:T,value:Choice<T>!i32", "true,Choice.None"),
        ("value:Choice<T>!i32,witness:T", "Choice.None,id(true)"),
        ("value:T!i32,witness:T", "f(),witness()"),
        ("witness:T,value:T!i32", "witness(),f()"),
        ("value:T!i32,witness:T", "f(),witness_value(7)"),
    ] {
        let text = format!(
            "enum Choice<T>{{None,Some(T)}}fn id<T>(x:T)->T=>x;fn witness()->bool=>true;fn witness_value(value:bool!i32)->bool=>true;fn sink<T>({parameters})->i32{{return 0}}fn run<F:Fn()->i32>(f:F)->i32{{f();return sink({arguments})}}fn main()->i32{{let f=()->i32=>1;return run(f)}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-order.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed)
            .unwrap_or_else(|errors| panic!("{}: {errors:?}", source.text));
    }
    for (parameters, arguments) in [
        ("value:T!i32,witness:T", "7,0"),
        ("witness:T,value:T!i32", "0,7"),
    ] {
        let text = format!(
            "fn sink<T>({parameters})->i32{{return 0}}fn run<F:Fn()->i32>(f:F)->i32{{f();return sink({arguments})}}fn main(){{}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-order-reject.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &parsed).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.phase == Phase::Expand && error.code == "E-GENERIC-ARGUMENT"),
            "{}: {errors:?}",
            source.text
        );
    }
}

#[test]
fn generic_enum_payload_uses_declared_callable_result() {
    for text in [
        "enum Choice<T>{None,Some(T)}fn sink(v:Choice<i32>!i32)->i32{return 0}fn run<F:Fn()->i32>(f:F)->i32{return sink(Choice.Some(f()))}fn main()->i32{let f=()->i32=>7;return run(f)}",
        "enum Choice<T>{None,Some(T)}fn sink(v:Choice<i32>!i32)->i32{return 0}fn run<F:Fn()->i32>(f:F)->i32{return sink(Choice::<i32>.Some(f()))}fn main()->i32{let f=()->i32=>7;return run(f)}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-fn-result.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
    }
    let text = "enum Choice<T>{None,Some(T)}fn sink(v:Choice<i32>!i32)->i32{return 0}fn run<F:Fn()->i64>(f:F)->i32{return sink(Choice.Some(f()))}fn main(){}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "union-fn-wrong-result.zil", text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    assert!(
        zil_expand::expand(&source, &parsed)
            .unwrap_err()
            .iter()
            .all(|error| error.phase == Phase::Expand && error.code == "E-GENERIC-ARGUMENT")
    );
}

#[test]
fn generic_error_union_templates_preserve_symbolic_and_actual_checks() {
    for text in [
        "fn unused<U:Copy>(w:U)->U!U{return w}fn main(){}",
        "fn unused<U:Copy>(w:U)->i32{let value:U!U=w;return 0}fn main(){}",
        "fn sink<T:Copy>(value:T!T,witness:T)->i32{return 0}fn unused<U:Copy>(w:U)->i32{return sink(w,w)}fn main(){}",
        "fn sink<T>(value:T!i32)->i32{return 0}fn unused<U:Copy>(w:U)->i32{return sink(7)}fn main(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-symbolic-reject.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &parsed).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.phase == Phase::Expand && error.code == "E-GENERIC-ARGUMENT"),
            "{text}: {errors:?}"
        );
    }
    for text in [
        "fn sink<T>(value:T!i32,witness:T)->i32{return 0}fn unused<U:Copy>(w:U)->i32{return sink(7,w)}fn main(){}",
        "fn sink<T>(witness:T,value:T!i32)->i32{return 0}fn unused<U:Copy>(w:U)->i32{return sink(w,7)}fn main(){}",
        "fn unused<U:Copy>(w:U)->U!i32{let n:i32=7;return n}fn main(){}",
        "fn unused<U:Copy>(w:U)->U!i32=>7;fn main(){}",
        "fn unused<U:Copy>(w:U)->i32{let n:U!i32=7;return 0}fn main(){}",
        "enum Choice<T>{None,Some(T)}fn unused<U:Copy>(w:U)->Choice<U>!Choice<i32>{let n=Choice::<i32>.None;return n}fn main(){}",
        "fn sink<T>(value:T!i32,witness:T)->i32{return 0}fn unused<U:Copy>(w:U,v:U!i32)->i32{return sink(v,w)}fn main(){}",
        "fn unused<U:Copy>(v:U!i32)->U!i32{let n:U!i32=v;return n}fn main(){}",
        "enum Choice<T>{None,Some(T)}fn unused<U:Copy>(v:Choice<U>)->Choice<U>!i32{let n:Choice<U>!i32=v;return n}fn main(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "union-symbolic.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
    }
}

#[test]
fn generic_error_union_contexts_have_exact_prepaid_boundaries() {
    // Pre322 work/nodes: binding880/301, argument871/290, independent1440/457.
    // Full type-position lifetime validation now pays from the same Budget.
    // Migration and unchanged source controls: changes/322-budget-migration-by324.md.
    for (name, text, steps, nodes) in [
        (
            "binding",
            include_str!("../../../tests/generic-error-enum/pending/unique-binding-value.zil"),
            1036,
            368,
        ),
        (
            "argument",
            include_str!("../../../tests/generic-error-enum/pending/unique-call-argument.zil"),
            1024,
            361,
        ),
        (
            "independent",
            "enum Choice<T>{None,Some(T)}fn sink<T>(value:Choice<T>!i32,witness:T)->i32{return 0}fn run<F:Fn()->i32>(f:F)->i32{f();return sink(Choice.None,true)}fn main()->i32{let f=()->i32=>1;return run(f)}",
            1612,
            536,
        ),
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let expected = zil_expand::expand(&source, &parsed).unwrap();
        for (work, amount) in [(true, steps), (false, nodes)] {
            let mut limits = zil_base::Limits::default();
            if work {
                limits.max_analysis_steps = amount
            } else {
                limits.max_nodes = amount
            }
            let fresh = zil_syntax::parse(&source).into_result().unwrap();
            for syntax in [&parsed, &fresh] {
                assert_eq!(
                    zil_expand::expand_with_map_limits(&source, syntax, &limits)
                        .unwrap()
                        .core,
                    expected
                );
            }
            if work {
                limits.max_analysis_steps -= 1
            } else {
                limits.max_nodes -= 1
            }
            assert!(
                zil_expand::expand_with_map_limits(&source, &parsed, &limits)
                    .unwrap_err()
                    .iter()
                    .all(|error| error.category == ResultCategory::ResourceLimit)
            );
        }
    }
}

#[test]
fn generic_enum_error_context_uses_one_complete_nominal_type() {
    for text in [
        include_str!("../../../tests/generic-error-enum/direct.zil"),
        include_str!("../../../tests/generic-error-enum/concrete.zil"),
        include_str!("../../../tests/generic-error-enum/helper.zil"),
        include_str!("../../../tests/generic-error-enum/annotated.zil"),
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<i32>!Box<i64>{return Box::<i32>.None} fn main(){let f=()->i32=>0;run(f)}",
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<i32>!Box<i64>{return Box::<i64>.None} fn main(){let f=()->i32=>0;run(f)}",
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<i32>!Box<i64>{return Box.Some(7)} fn main(){let f=()->i32=>0;run(f)}",
        include_str!("../../../tests/generic-error-enum/unique-context.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context-if.zil"),
        "enum Choice<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Choice<i32>!i32{f();return (Choice.None)} fn main()->i32{let f=()->i32=>1;run(f);return 0}",
        "enum Fault{NoMemory} enum Note{Ready} fn run<F:Fn()->i32>(f:F)->Fault!Note{return Note.Ready} fn main(){let f=()->i32=>0;run(f)}",
        "enum Fault{NoMemory} fn run<F:Fn()->i32>(f:F)->Fault!i32{return Fault.NoMemory} fn main(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "enum-error.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
    }
    for text in [
        "enum Fault{NoMemory} enum Other{NoMemory} fn run<F:Fn()->i32>(f:F)->Fault!i32{return Other.NoMemory} fn main(){}",
        "enum Fault{NoMemory} fn run<F:Fn()->i32>(f:F)->Fault!Fault{return Fault.NoMemory} fn main(){}",
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<i32>!Box<i64>{return Box.None} fn main(){}",
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<i32>!Box<i32>{return Box.Some(7)} fn main(){}",
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<u8>!Box<i64>{return Box.Some(7)} fn main(){}",
        "enum Box<T>{None;Some(T)} fn run<F:Fn()->i32>(f:F)->Box<i32>!i32{return Box::<i64>.None} fn main(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "enum-error-reject.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &parsed).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.phase == Phase::Expand && error.code == "E-GENERIC-ARGUMENT"),
            "{text}: {errors:?}"
        );
    }
}

#[test]
fn generic_enum_error_context_prepays_new_work() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "enum-error-budget.zil",
        include_str!("../../../tests/generic-error-enum/direct.zil"),
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let expected = zil_expand::expand(&source, &parsed).unwrap();
    // Same frozen source: pre322 work624/nodes242; the paid full-AST life
    // validation now yields work782/nodes310 without resetting the remainder.
    for (work, amount) in [(true, 782), (false, 310)] {
        let mut limits = zil_base::Limits::default();
        if work {
            limits.max_analysis_steps = amount
        } else {
            limits.max_nodes = amount
        }
        let fresh = zil_syntax::parse(&source).into_result().unwrap();
        assert_eq!(
            zil_expand::expand_with_map_limits(&source, &fresh, &limits)
                .unwrap()
                .core,
            expected
        );
        if work {
            limits.max_analysis_steps -= 1
        } else {
            limits.max_nodes -= 1
        }
        assert!(
            zil_expand::expand_with_map_limits(&source, &parsed, &limits)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
    }
}

#[test]
fn generic_records_use_structural_arguments_and_fresh_source_inexpressible_names() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "records.zil",
        r#"
struct Pair<T> { first: T; second: T }
fn first<T>(pair: Pair<T>) -> T => pair.first
fn sample() -> i64 {
    let a: Pair<i64> = Pair::<i64> { first: 1, second: 2 }
    let b: Pair<(i64)> = Pair::<(i64)> { first: 3, second: 4 }
    return first::<i64>(a)
}
"#,
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map(&source, &parsed).unwrap();
    assert!(expanded.core.unsupported.is_empty());
    assert_eq!(expanded.core.aggregates.len(), 1);
    assert!(
        expanded.core.aggregates[0]
            .name
            .starts_with("$generic_Pair_")
    );
    assert!(
        expanded.core.aggregates[0]
            .fields
            .iter()
            .all(|field| field.ty == Type::I64)
    );
    assert_eq!(expanded.core.functions.len(), 2);
    expanded
        .source_map
        .validate(&[source], &zil_base::Limits::default())
        .unwrap();
}

#[test]
fn unused_generic_declaration_attributes_are_not_erased() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "attrs.zil",
        "#[inline]\nfn id<T>(x:T)->T=>x\nfn sample()->i32=>0",
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let errors = zil_expand::expand_with_map(&source, &parsed).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let diagnostic = &errors[0];
    assert_eq!(diagnostic.phase, Phase::Expand);
    assert_eq!(diagnostic.category, ResultCategory::Unsupported);
    assert_eq!(diagnostic.code, "E-UNSUPPORTED");
    assert_eq!(
        diagnostic.message,
        "unexecuted generic declaration attribute"
    );
    assert!(diagnostic.span.unwrap().is_valid_for(&source));
}

#[test]
fn unused_generic_bounds_require_builtin_copy_or_declared_nominal_traits() {
    for (prefix, bound) in [
        ("", "Copy"),
        ("", "MissingRead"),
        ("struct Read { value: i32 }\n", "Read"),
    ] {
        let text = format!("{prefix}fn id<T: {bound}>(x:T)->T=>x\nfn sample()->i32=>0");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "bounds.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        if bound == "Copy" {
            // Keep the original input; structural Copy is now a builtin bound.
            let expanded = zil_expand::expand_with_map(&source, &parsed).unwrap();
            assert!(expanded.core.unsupported.is_empty());
            assert_eq!(expanded.core.functions.len(), 1);
            assert_eq!(expanded.core.functions[0].name, "sample");
            expanded
                .source_map
                .validate(&[source], &zil_base::Limits::default())
                .unwrap();
            continue;
        }
        let errors = zil_expand::expand_with_map(&source, &parsed).unwrap_err();
        assert_eq!(errors.len(), 1, "{bound}: {errors:#?}");
        let diagnostic = &errors[0];
        assert_eq!(diagnostic.phase, Phase::Expand);
        assert_eq!(diagnostic.category, ResultCategory::SafeReject);
        assert_eq!(diagnostic.code, "E-GENERIC-ARGUMENT");
        assert_eq!(
            diagnostic.message,
            format!("generic bound {bound} is not a declared nominal trait")
        );
        let span = diagnostic.span.unwrap();
        assert!(span.is_valid_for(&source));
        assert_eq!(&source.text[span.start..span.end], bound);
    }
}

#[test]
fn unused_generic_with_declared_bound_checks_its_symbolic_method_body() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "declared-bound.zil",
        r#"
trait Read { fn read(self: &Self) -> i32; }
fn get<T: Read>(value: &T) -> i32 => value.read()
fn sample() -> i32 => 0
"#,
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map(&source, &parsed).unwrap();
    assert!(expanded.core.unsupported.is_empty());
    assert_eq!(expanded.core.functions.len(), 1);
    assert_eq!(expanded.core.functions[0].name, "sample");
    expanded
        .source_map
        .validate(&[source], &zil_base::Limits::default())
        .unwrap();
}

#[test]
fn concrete_local_resource_producers_keep_owned_positions_in_callable_helpers() {
    const HEAD: &str = r#"
extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,data:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
resource Mark{n:i32}
package Event{manages mark:Mark;manages bytes:ByteBuffer}
enum Fault{NoMemory}
fn no_memory()->Fault!i32{return Fault.NoMemory}
fn consume(event:Event)->i32{return event.mark.n+i32(bytes_len(event.bytes))}
fn describe(mark:Mark)->Mark{return mark}
fn fresh()->Mark contract{result(create)}{return Mark{n:3}}
fn relay(give mark:Mark)->Mark contract{result(give)}{return give mark}
"#;
    const PREPARE: &str = "let mark=Mark{n:3};let bytes=bytes_new();if !bytes_reserve(bytes,1){return no_memory()};bytes_push(bytes,86u8);";
    const EVENT: &str = "let event=Event{mark:give mark,bytes:give bytes};";
    const WITH: &str = "let body=move ()->i32=>consume(event);unsafe{let result=c_with_callback(reg,close,f,body);return result.1}";
    for body in [
        format!("{PREPARE}{EVENT}{WITH}"),
        format!(
            "{}{EVENT}{WITH}",
            PREPARE.replace("return no_memory()", "return Fault.NoMemory")
        ),
        format!("{PREPARE}{EVENT}return consume(event)+f(1)"),
        format!("{}{EVENT}{WITH}", PREPARE.replace("Mark{n:3}", "fresh()")),
        format!(
            "{PREPARE}let transferred=relay(give mark);{}{WITH}",
            EVENT.replace("give mark", "give transferred")
        ),
    ] {
        let text =
            format!("{HEAD}fn run<F:FnMut(i32)->i32>(f:&mut F)->Fault!i32{{{body}}}fn main(){{}}");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "local-resource.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &parsed).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
    for body in [
        format!(
            "{PREPARE}let alias=mark;let event=Event{{mark:give alias,bytes:give bytes}};{WITH}"
        ),
        format!(
            "{PREPARE}let alias=describe(mark);let event=Event{{mark:give alias,bytes:give bytes}};{WITH}"
        ),
        format!("{PREPARE}{EVENT}let again=Event{{mark:give mark,bytes:bytes_new()}};{WITH}"),
        format!("{PREPARE}{EVENT}let stale=mark.n;{WITH}"),
        format!(
            "{PREPARE}{EVENT}let body=move ()->i32=>consume(event);consume(event);unsafe{{let result=c_with_callback(reg,close,f,body);return result.1}}"
        ),
        format!("{PREPARE}if true{{{EVENT}}}{EVENT}{WITH}"),
        format!(
            "{PREPARE}{EVENT}let body=move ()->i32=>{{f(1);return consume(event)}};unsafe{{let result=c_with_callback(reg,close,f,body);return result.1}}"
        ),
    ] {
        let text =
            format!("{HEAD}fn run<F:FnMut(i32)->i32>(f:&mut F)->Fault!i32{{{body}}}fn main(){{}}");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "local-resource-reject.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        assert!(zil_expand::expand(&source, &parsed).is_err(), "{body}");
    }
    for (kind, passing) in [("FnOnce", "F"), ("FnMut", "&mut F")] {
        let text = format!(
            "{HEAD}fn shadowed<F:{kind}()->Mark>(fresh:{passing})->i32{{let mark=fresh();let event=Event{{mark:give mark,bytes:bytes_new()}};return consume(event)}}fn main(){{}}"
        );
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "shadowed-resource-producer.zil",
            text,
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &parsed).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "E-UNSUPPORTED"
                && error.message == "generic managed give requires a direct Package manages field"
                && error
                    .span
                    .is_some_and(|span| &source.text[span.start..span.end] == "mark")),
            "{kind}: {errors:?}"
        );
    }
    for contract in [
        "",
        "contract{result(copy)}",
        "contract{result(create);result(give)}",
    ] {
        let prepare = PREPARE.replace("Mark{n:3}", "producer()");
        let text = format!(
            "{HEAD}fn producer()->Mark {contract}{{return Mark{{n:3}}}}fn run<F:FnMut(i32)->i32>(f:&mut F)->Fault!i32{{{prepare}{EVENT}{WITH}}}fn main(){{}}"
        );
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "local-resource-contract-reject.zil",
            text,
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        assert!(zil_expand::expand(&source, &parsed).is_err(), "{contract}");
    }
}

#[test]
fn concrete_resource_preparation_prepays_work_and_keeps_old_helper_boundaries() {
    let scalar = "fn run<F:FnMut(i32)->i32>(f:&mut F)->i32{return f(2)}fn main()->i32{var f=(x:i32)->i32=>x+1;return run(&mut f)}";
    let owned = "resource R{n:i32}package P{manages r:R}fn consume(p:P)->i32{return p.r.n}fn run<F:FnMut(i32)->i32>(f:&mut F,p:P)->i32{return f(consume(p))}fn main()->i32{var f=(x:i32)->i32=>x+1;let r=R{n:3};let p=P{r:give r};return run(&mut f,p)}";
    let local = "resource R{n:i32}package P{manages r:R}fn consume(p:P)->i32{return p.r.n}fn run<F:FnMut(i32)->i32>(f:&mut F)->i32{let r=R{n:3};let p=P{r:give r};return f(consume(p))}fn main()->i32{var f=(x:i32)->i32=>x+1;return run(&mut f)}";
    // Historical fa34160 controls were scalar288/144 and owned554/279;
    // local preparation was558/281 before322. Same bytes now also pay the
    // complete type-position lifetime scan (see the attributed migration).
    for (text, work, nodes) in [(scalar, 381, 194), (owned, 735, 376), (local, 730, 372)] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "resource-budget.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let expected = zil_expand::expand(&source, &parsed).unwrap();
        for (check_work, amount) in [(true, work), (false, nodes)] {
            let mut limits = zil_base::Limits::default();
            if check_work {
                limits.max_analysis_steps = amount;
            } else {
                limits.max_nodes = amount;
            }
            for _ in 0..2 {
                assert_eq!(
                    zil_expand::expand_with_map_limits(&source, &parsed, &limits)
                        .unwrap()
                        .core,
                    expected
                );
            }
            if check_work {
                limits.max_analysis_steps -= 1;
            } else {
                limits.max_nodes -= 1;
            }
            let errors = zil_expand::expand_with_map_limits(&source, &parsed, &limits).unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|error| error.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn concrete_local_callables_keep_owned_signatures_and_actual_modes() {
    const DATA: &str = "resource R{n:i32}package Event{manages r:R}package Reply{manages r:R}fn reply(event:Event)->Reply{return Reply{r:give event.r}}fn render(reply:Reply)->i32{return reply.r.n}fn dispatch<H:FnOnce(Event)->Reply>(handler:H,event:Event)->Reply{return handler(event)}";
    for body in [
        "let handler=(value:Event)->Reply=>reply(value);return render(dispatch(handler,event))+f(1)",
        "var n=0;var handler=move(value:Event)->Reply=>{n=n+1;return reply(value)};return render(handler(event))+f(1)",
        "let handler=move(value:Event)->Reply=>{let held=other;return reply(value)};return render(dispatch(handler,event))+f(1)",
    ] {
        let text = format!(
            "{DATA}fn run<F:FnMut(i32)->i32>(f:&mut F,event:Event,other:Event)->i32{{{body}}}fn main(){{}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "concrete-local-callable.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        zil_expand::expand(&source, &syntax).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
    }
    for body in [
        "let handler=(value:Event)->Reply=>{f(1);return reply(value)};return render(dispatch(handler,event))",
        "let handler=move(value:Event)->Reply=>{let held=other;return reply(value)};dispatch(handler,event);return render(dispatch(handler,last))",
        "var n=0;let handler=move(value:Event)->Reply=>{n=n+1;return reply(value)};return render(handler(event))",
        "let handler=(value:F)->i32=>0;return 0",
        "let handler=(value:Unknown)->i32=>0;return 0",
        "let handler=(value:R)->i32=>0;return 0",
        "let handler=(value:&i32)->&i32=>value;return 0",
    ] {
        let text = format!(
            "{DATA}fn run<F:FnMut(i32)->i32>(f:&mut F,event:Event,other:Event,last:Event)->i32{{{body}}}fn main(){{}}"
        );
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "concrete-local-callable-reject.zil",
            text,
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        assert!(zil_expand::expand(&source, &syntax).is_err(), "{body}");
    }
}
