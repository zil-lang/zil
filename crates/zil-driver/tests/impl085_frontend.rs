//! Source-to-Types/Analyze coverage for the finite Win64 platform prerequisites.
//! These checks grant no Verify capability and claim no window execution.
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{Constant, CoreExprKind, CoreStatementKind, InstructionKind, Type};

fn core(text: &str) -> Result<zil_ir::CoreModule, Vec<Diagnostic>> {
    let input = SourceUnit::new(FileId(0), ModuleId(0), "impl085.zil", text);
    let ast = zil_syntax::parse(&input).into_result()?;
    zil_expand::expand(&input, &ast)
}
fn typed_result(text: &str) -> Result<zil_ir::TypedModule, Vec<Diagnostic>> {
    zil_types::check(&core(text)?, &TargetProfile::default(), &Limits::default())
}
fn typed(text: &str) -> zil_ir::TypedModule {
    typed_result(text).unwrap_or_else(|e| panic!("{text}\n{e:?}"))
}
fn rejected(text: &str, code: &str) {
    let errors = typed_result(text).expect_err(text);
    assert!(errors.iter().any(|e| e.code == code), "{text}\n{errors:?}");
}
fn constants(module: &zil_ir::TypedModule) -> Vec<&Constant> {
    module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match &i.kind {
            InstructionKind::Constant(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn u16_is_a_real_unsigned_integer_through_inference_and_conversion() {
    let module = typed(
        "fn main()->i32{let a:u16=65535;let b=0xffffu16;let c=0o177777u16;let d=0b1111_1111_1111_1111u16;return i32(a)+i32(b)+i32(c)+i32(d)}",
    );
    assert_eq!(
        constants(&module)
            .iter()
            .filter(|c| ***c == Constant::U16(65535))
            .count(),
        4
    );
    for text in [
        "fn id<T>(x:T)->T=>x;fn main()->i32{return i32(id(65535u16))}",
        "fn main()->i32{let f=(x)=>x+%1u16;return i32(f(65535u16))}",
        "fn main()->i32{let f=(x)=>u16(x);return i32(f(65535))}",
        "fn main()->i32{var sum=0;for x in 0u16..2u16{sum+=i32(x)}return sum}",
        "fn main()->i32{let x=65535u16;match x{0=>{return 1},65535=>{return 2},_=>{return 3}}}",
        "enum Fault{Bad}fn value()->Fault!u16{return 65535u16}fn main()->Fault!i32{return i32(value()?)}",
    ] {
        typed(text);
    }
    for text in [
        "fn main()->i32{let x:u16=65536;return 0}",
        "fn main()->i32{let x=65536u16;return 0}",
        "fn main()->i32{let x:u16=-1;return 0}",
    ] {
        rejected(text, "E-INTEGER-RANGE");
    }
    rejected("fn main()->i32{let x=1u16;return x}", "E-TYPE-MISMATCH");
    let module = typed("fn main()->i32{let x=u16(65536);return i32(x)}");
    let main = &module.functions[0];
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(&i.kind,
        InstructionKind::ConvertInteger{value,target:Type::U16} if main.values[value.0 as usize] == Type::I32)));
}

#[test]
fn static_unicode_literals_have_exact_utf16_and_distinct_null() {
    let module = typed(
        "fn main()->i32{let a=c_wstr(\"ZIL 中文 😀\");let empty=c_wstr(\"\");let none=c_wstr_null();let h=c_null();if h==c_null(){return 0}return 1}",
    );
    let mut expected: Vec<_> = "ZIL 中文 😀".encode_utf16().collect();
    expected.push(0);
    let values = constants(&module);
    assert!(values.contains(&&Constant::WideString(expected)));
    assert!(values.contains(&&Constant::WideString(vec![0])));
    assert!(values.contains(&&Constant::NullWideString));
    assert!(values.contains(&&Constant::NullHandle));
    for text in [
        "fn main(){let a=c_wstr(\"a\\0b\")}",
        "fn main(){let a=c_wstr(\"a\\u{0}b\")}",
        "fn main(){let a=c_wstr(1)}",
        "fn main(){let a=c_wstr(c_wstr(\"x\"))}",
        "fn main(){let a=c_null(1)}",
        "fn main(){let a=c_wstr_null(1)}",
    ] {
        rejected(text, "E-FFI-CONSTRUCTOR");
    }
    rejected(
        "fn main()->i32{return i32(c_null())}",
        "E-INTEGER-CONVERSION",
    );
    rejected(
        "fn main()->i32{let h=c_null();let x=h+1;return 0}",
        "E-TYPE-MISMATCH",
    );
    assert!(typed_result("fn main(){let h=c_null();let x=*h}").is_err());
    assert!(typed_result("fn main(){let s=c_wstr(\"x\");let b=s==s}").is_err());
}

const CALLBACK: &str =
    "fn callback(h:c_handle,m:i32,w:i64,l:i64)->i64{if h!=c_null(){return w+l}return i64(m)}";

#[test]
fn callback_is_static_foreign_code_and_never_an_indirect_zil_call() {
    let text = format!(
        "{CALLBACK}fn main()->i32{{let cb:extern \"C\" fn(c_handle,i32,i64,i64)->i64=c_callback(callback);return 0}}"
    );
    let module = typed(&text);
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::ForeignFunctionRef { .. }))
    );
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::CallIndirect { .. }))
    );
    for suffix in [
        "fn main(){let f=callback;let c=c_callback(f)}",
        "fn main(){let c=c_callback((x:i32)=>x)}",
        "fn main(){let value=1;let f=()=>value;let c=c_callback(f)}",
        "fn main(){let c=c_callback()}",
        "fn main(){let c=c_callback(callback,callback)}",
    ] {
        rejected(&format!("{CALLBACK}{suffix}"), "E-FFI-CALLBACK");
    }
    rejected(
        "extern \"C\" fn external(x:i32)->i32;fn main(){let c=c_callback(external)}",
        "E-FFI-CALLBACK",
    );
    rejected(
        "fn bad(x:bool)->i32{return 0}fn main(){let c=c_callback(bad)}",
        "E-FFI-CALLBACK",
    );
    rejected(
        "fn bad(x:u16)->i32{return 0}fn main(){let c=c_callback(bad)}",
        "E-FFI-CALLBACK",
    );
    rejected(
        "fn bad(x:&i32)->i32{return *x}fn main(){let c=c_callback(bad)}",
        "E-FFI-CALLBACK",
    );
    assert!(
        typed_result(&format!(
            "{CALLBACK}fn main(){{let cb=c_callback(callback);cb(c_null(),0,0,0)}}"
        ))
        .is_err()
    );
    for ty in [
        "extern \"C\" fn(bool)->i32",
        "extern \"C\" fn(u16)->i32",
        "extern \"C\" fn(&i32)->i32",
    ] {
        rejected(
            &format!("fn unused(x:{ty}){{}}fn main(){{}}"),
            "E-FFI-CALLBACK-TYPE",
        );
    }
}

#[test]
fn callback_and_handle_survive_ordinary_copy_storage_and_closure_inference() {
    typed(&format!(
        "{CALLBACK}fn main()->i32{{let h=c_null();let f=()=>{{let cb=c_callback(callback);if h==c_null(){{return 1}}return 0}};return f()}}"
    ));
    typed(
        "fn main()->i32{var h=c_null();let p=&mut h;*p=c_null();if h!=c_null(){return 1}return 0}",
    );
    typed(&format!(
        "{CALLBACK}fn id<T>(x:T)->T=>x;fn main()->i32{{let cb=id(c_callback(callback));let title=id(c_wstr(\"ZIL\"));let h=id(c_null());if h==c_null(){{return 0}}return 1}}"
    ));
}

#[test]
fn strict_c_records_and_explicit_static_callbacks_cover_normal_window_shapes() {
    let text = format!(
        r#"{CALLBACK}
struct Point{{x:i32;y:i32}}
struct Rect{{left:i32;top:i32;right:i32;bottom:i32}}
struct Message{{hwnd:c_handle;message:i32;wparam:i64;lparam:i64;time:i32;point:Point;private:i32}}
struct Paint{{hdc:c_handle;erase:i32;rect:Rect;restore:i32;increment:i32;reserved:[u8;32]}}
struct Class{{size:i32;style:i32;proc:extern "C" fn(c_handle,i32,i64,i64)->i64;extra:i32;window_extra:i32;instance:c_handle;icon:c_handle;cursor:c_handle;background:c_handle;menu:c_wstr;name:c_wstr;small_icon:c_handle}}
extern "C" fn Register(p:&Class)->u16 contract{{provides(noescape(p),static_callbacks());effects(ffi);}};
extern "C" fn MessageNext(p:&mut Message,h:c_handle,a:i32,b:i32)->i32 contract{{provides(noescape(p),static_callbacks());effects(ffi);}};
extern "C" fn PaintBegin(h:c_handle,p:&mut Paint)->c_handle contract{{provides(noescape(p),static_callbacks());effects(ffi);}};
extern "C" fn Text(h:c_handle,s:c_wstr,n:i32)->i32;
fn main()->i32{{let c=Class{{size:80,style:0,proc:c_callback(callback),extra:0,window_extra:0,instance:c_null(),icon:c_null(),cursor:c_null(),background:c_null(),menu:c_wstr_null(),name:c_wstr("ZIL"),small_icon:c_null()}};unsafe{{let atom=Register(&c);return i32(atom)}}}}
"#
    );
    let module = typed(&text);
    let declaration = module
        .functions
        .iter()
        .find(|f| f.name == "Register")
        .unwrap()
        .foreign
        .as_ref()
        .unwrap();
    assert!(declaration.static_callbacks);
    assert_eq!(declaration.noescape_parameters, [0]);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    typed(
        "extern \"C\" fn Pump()->i32 contract{provides(static_callbacks());effects(ffi);};fn main()->i32{unsafe{return Pump()}}",
    );
    typed(
        "extern \"C\" fn bytes(a:u8,b:u16)->u16;fn main()->i32{unsafe{return i32(bytes(1u8,2u16))}}",
    );
}

#[test]
fn imported_platform_foreign_signatures_resolve_private_record_names() {
    // Exercise the real project normalizer, including an unrelated same-spelled
    // root type. Expanding each module in isolation would hide this regression.
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import platform\nstruct Rect{unrelated:bool}\nfn main()->i32{return platform.run()}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "platform.zil",
            r#"
struct Point{x:i32;y:i32}
struct Rect{left:i32;top:i32;right:i32;bottom:i32}
struct Message{window:c_handle;message:i32;wparam:i64;lparam:i64;time:i32;point:Point;private_data:i32}
struct Paint{dc:c_handle;erase:i32;area:Rect;restore:i32;update:i32;reserved:[u8;2]}
struct WindowClass{procedure:extern "C" fn(c_handle,i32,i64,i64)->i64;name:c_wstr}
extern "C" fn RegisterClassExW(value:&WindowClass)->u16 contract{provides(noescape(value),static_callbacks());effects(ffi);};
extern "C" fn GetMessageW(value:&mut Message)->i32 contract{provides(noescape(value),static_callbacks());effects(ffi);};
extern "C" fn BeginPaint(value:&mut Paint)->c_handle contract{provides(noescape(value),static_callbacks());effects(ffi);};
extern "C" fn FillRect(value:&Rect)->i32 contract{provides(noescape(value));effects(ffi);};
fn window_proc(h:c_handle,m:i32,w:i64,l:i64)->i64{return i64(m)}
pub fn run()->i32{
    let class=WindowClass{procedure:c_callback(window_proc),name:c_wstr("ZIL 中文")}
    var message=Message{window:c_null(),message:0,wparam:0,lparam:0,time:0,point:Point{x:0,y:0},private_data:0}
    var paint=Paint{dc:c_null(),erase:0,area:Rect{left:0,top:0,right:0,bottom:0},restore:0,update:0,reserved:[0u8,0]}
    unsafe{
        let atom=RegisterClassExW(&class)
        GetMessageW(&mut message)
        let dc=BeginPaint(&mut paint)
        FillRect(&paint.area)
        return i32(atom)
    }
}
"#,
        ),
    ];
    let syntax = sources
        .iter()
        .map(|source| zil_syntax::parse(source).into_result().unwrap())
        .collect::<Vec<_>>();
    let limits = Limits::default();
    let expanded = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &[
            vec![zil_ir::CoreImport {
                alias: "platform".into(),
                module: ModuleId(1),
                span: syntax[0].items[0].span,
            }],
            Vec::new(),
        ],
        &limits,
    )
    .unwrap();
    let program = zil_ir::CoreProgram {
        root: ModuleId(0),
        source_fingerprint: zil_base::Fingerprint::of(b"impl085-private-platform-records"),
        modules: expanded.into_iter().map(|(module, _)| module).collect(),
    };
    let typed = zil_types::check_program(&program, &TargetProfile::default(), &limits)
        .unwrap_or_else(|errors| panic!("{errors:#?}"));
    for symbol in ["RegisterClassExW", "GetMessageW", "BeginPaint", "FillRect"] {
        let function = typed
            .module
            .functions
            .iter()
            .find(|f| {
                f.foreign
                    .as_ref()
                    .is_some_and(|foreign| foreign.symbol == symbol)
            })
            .unwrap_or_else(|| panic!("foreign symbol was not restored: {symbol}"));
        assert!(matches!(
            &function.locals[function.parameters[0].0 as usize].ty,
            Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Record { .. })
        ));
    }
    zil_analyze::analyze(&typed.module, &limits).unwrap();
}

#[test]
fn mutable_static_fields_and_missing_reentry_contracts_are_rejected() {
    for declaration in [
        "extern \"C\" fn f(x:extern \"C\" fn(i32)->i32);",
        "struct R{f:extern \"C\" fn(i32)->i32}extern \"C\" fn f(x:&R)contract{provides(noescape(x));effects(ffi);};",
        "struct R{s:c_wstr}extern \"C\" fn f(x:&mut R)contract{provides(noescape(x));effects(ffi);};",
        "struct R{f:extern \"C\" fn(i32)->i32}extern \"C\" fn f(x:&mut R)contract{provides(noescape(x),static_callbacks());effects(ffi);};",
        "struct R{x:bool}extern \"C\" fn f(x:&R)contract{provides(noescape(x));effects(ffi);};",
        "struct R{x:i32}extern \"C\" fn f(x:R);",
        "extern \"C\" fn f()->c_wstr;",
        "extern \"C\" fn f()->extern \"C\" fn(i32)->i32;",
    ] {
        rejected(&format!("{declaration}fn main(){{}}"), "E-FFI-DECLARATION");
    }
    for declaration in [
        "extern \"C\" fn f()contract{provides(static_callbacks());};",
        "extern \"C\" fn f()contract{provides(static_callbacks(),static_callbacks());effects(ffi);};",
        "extern \"C\" fn f(x:i32)contract{provides(static_callbacks(x));effects(ffi);};",
    ] {
        rejected(&format!("{declaration}fn main(){{}}"), "E-FFI-DECLARATION");
    }
    rejected(
        "extern \"C\" fn f(h:c_handle);fn main(){f(c_null())}",
        "E-FFI-UNSAFE",
    );
}

#[test]
fn raw_static_constants_are_validated_and_budgeted_before_cloning() {
    for units in [
        vec![],
        vec![65],
        vec![0, 65, 0],
        vec![0xd800, 0],
        vec![0xdc00, 0],
    ] {
        let mut module = core("fn main()->c_wstr{return c_wstr(\"ok\")}").unwrap();
        let CoreStatementKind::Return(Some(expression)) =
            &mut module.functions[0].body.statements[0].kind
        else {
            panic!()
        };
        expression.kind = CoreExprKind::Literal(Constant::WideString(units));
        let errors =
            zil_types::check(&module, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-FFI-WIDE-STRING"),
            "{errors:?}"
        );
    }
    let mut module = core("fn main()->c_wstr{return c_wstr(\"ok\")}").unwrap();
    let CoreStatementKind::Return(Some(expression)) =
        &mut module.functions[0].body.statements[0].kind
    else {
        panic!()
    };
    expression.kind = CoreExprKind::Literal(Constant::WideString(vec![65; 4096]));
    let limits = Limits {
        max_analysis_steps: 1024,
        ..Limits::default()
    };
    let errors = zil_types::check(&module, &TargetProfile::default(), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );

    let mut module = typed("fn main()->c_wstr{return c_wstr(\"ok\")}");
    let literal = module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Constant(Constant::WideString(_))))
        .unwrap();
    literal.kind = InstructionKind::Constant(Constant::WideString(vec![65; 4096]));
    let errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );
}

#[test]
fn constructor_names_cannot_be_shadowed() {
    for name in ["c_null", "c_wstr_null", "c_wstr", "c_callback"] {
        rejected(
            &format!("fn {name}(){{}}fn main(){{}}"),
            "E-FFI-CONSTRUCTOR-NAME",
        );
        rejected(
            &format!("fn main(){{let {name}=1}}"),
            "E-FFI-CONSTRUCTOR-NAME",
        );
        rejected(
            &format!("fn unused({name}:i32){{}}fn main(){{}}"),
            "E-FFI-CONSTRUCTOR-NAME",
        );
    }
}

#[test]
fn erased_aliases_and_unused_templates_preserve_callback_formation_errors() {
    for declaration in [
        "fn unused<T>(cb:extern \"C\" fn(bool)->i32){}",
        "fn unused<T>(cb:extern \"C\" fn(())->i32){}",
        "fn unused<T>(cb:extern \"C\" fn(T,bool)->T){}",
        "fn unused<T>(cb:extern \"system\" fn(i32)->i32){}",
        "fn unused<T>(cb:extern \"C\" fn(i32)->c_wstr){}",
        "type Bad=extern \"C\" fn(bool)->i32;",
        "type Bad=extern \"C\" fn(i32)->c_wstr;",
        "type Bad=(i32,extern \"C\" fn(())->i32);",
        "type Flag=bool;type Bad=extern \"C\" fn(Flag)->i32;",
        "type Bad=extern \"system\" fn(i32)->i32;",
        "struct Unused<T>{cb:extern \"C\" fn(bool)->i32}",
        "struct Unused<T>{cb:[extern \"C\" fn(T,())->T;2]}",
        "struct Unused<T>{cb:(T,extern \"system\" fn(i32)->i32)}",
        "struct R{x:i32}type Bad=extern \"C\" fn(R)->i32;",
        "struct R{x:i32}fn unused<T>(cb:extern \"C\" fn(R)->i32){}",
        "struct R{x:i32}struct Unused<T>{cb:extern \"C\" fn(R)->i32}",
        "struct Wrapper<T>{cb:extern \"C\" fn(T)->T}fn unused(x:Wrapper<bool>){}",
    ] {
        rejected(
            &format!("{declaration}fn main()->i32=>0"),
            "E-FFI-CALLBACK-TYPE",
        );
    }
    for declaration in [
        "fn unused<T>(cb:extern \"C\" fn(T)->T){}",
        "fn unused<T>(cb:extern \"C\" fn(T)->()){}",
        "struct Unused<T>{cb:extern \"C\" fn(T)->T}",
        "struct Wrapper<T>{cb:extern \"C\" fn(T)->T}fn unused(x:Wrapper<i32>){}",
        "type Word=i32;type Callback=extern \"C\" fn(Word)->Word;fn unused(cb:Callback){}",
        "type Callback=extern \"C\" fn()->();",
    ] {
        typed(&format!("{declaration}fn main()->i32=>0"));
    }
    // Structural argument matching inside callback signatures remains outside
    // this repair, even with a written type argument; generic aliases do too.
    rejected(
        "fn ignore<T>(cb:extern \"C\" fn(T)->T){}fn callback(x:i32)->i32=>x;fn main()->i32{ignore::<i32>(c_callback(callback));return 0}",
        "E-GENERIC-ARGUMENT",
    );
}
