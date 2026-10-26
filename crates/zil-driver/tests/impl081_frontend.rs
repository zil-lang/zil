//! Real source admission for bytes, checked conversion, and bounded C borrows.
//! Native behavior and independent forged-IR controls belong to 082/083.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, compile_source};
use zil_ir::{CallableKind, Constant, CoreExprKind, InstructionKind, Type};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "impl081.zil", text)
}
fn core(text: &str) -> zil_ir::CoreModule {
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    zil_expand::expand(&input, &ast).unwrap()
}
fn typed(text: &str) -> zil_ir::TypedModule {
    zil_types::check(&core(text), &TargetProfile::default(), &Limits::default())
        .unwrap_or_else(|e| panic!("{text}\n{e:?}"))
}
fn checked(text: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{text}\n{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{text}");
}
fn rejected(text: &str, phase: Phase, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_ne!(report.status, ResultCategory::Success, "{text}");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.code == code),
        "{text}\n{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn byte_literals_preserve_default_and_exact_context() {
    checked(
        "fn main()->i32{let a:u8=0;let b:u8=255;let c=0x80u8;let d=0o377u8;let e=0b1111_1111u8;return i32(a)+i32(b)+i32(c)+i32(d)+i32(e)}",
    );
    let module = typed("fn main()->i32{let default=1;let byte:u8=255;return default+i32(byte)}");
    let constants: Vec<_> = module.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match &i.kind {
            InstructionKind::Constant(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    assert!(constants.contains(&Constant::I32(1)));
    assert!(constants.contains(&Constant::U8(255)));
    for text in [
        "fn main()->i32{let x:u8=256;return 0}",
        "fn main()->i32{let x:u8=-1;return 0}",
        "fn main()->i32{let x=256u8;return 0}",
        "fn main()->i32{let x=0x100u8;return 0}",
    ] {
        rejected(text, Phase::Types, "E-INTEGER-RANGE");
    }
    rejected(
        "fn main()->i32{let x:u8=1;return x}",
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
    rejected(
        "fn main()->i32{let x:i32=1;let y:u8=x;return 0}",
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
    rejected("fn main()->i32{return 1i32}", Phase::Lex, "E-LEX-NUMBER");
    rejected(
        "fn main()->i32{let x:i64=1i64;return 0}",
        Phase::Lex,
        "E-LEX-NUMBER",
    );
}

#[test]
fn conversion_operand_is_typed_before_its_target() {
    // This must reach checked runtime conversion, not reject a u8 literal 256.
    let module = typed("fn main()->i32{let x=u8(256);return i32(x)}");
    let main = &module.functions[0];
    let first = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let InstructionKind::ConvertInteger {
                value,
                target: Type::U8,
            } = &i.kind
            {
                Some(*value)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(main.values[first.0 as usize], Type::I32);
    checked("fn main()->i32{let a:i64=255;let b=u8(a);let c=i64(b);return i32(c)}");
    for text in [
        "fn main()->i32{return i32(true)}",
        "fn main()->i32{let x=1;return i32(&x)}",
        "fn main()->i32{return i32((1,2))}",
    ] {
        rejected(text, Phase::Types, "E-INTEGER-CONVERSION");
    }
    for text in [
        "fn main()->i32{return i32()}",
        "fn main()->i32{return i32(1,2)}",
    ] {
        rejected(text, Phase::Expand, "E-INTEGER-CONVERSION");
    }
}

#[test]
fn byte_inference_reaches_generics_closures_aggregates_and_error_payloads() {
    for text in [
        "fn id<T>(x:T)->T=>x;fn main()->i32{let b=id(255u8);return i32(b)}",
        "fn main()->i32{let f=(x)=>x+%1u8;return i32(f(255u8))}",
        "fn main()->i32{let f=(x)=>u8(x);return i32(f(255))}",
        "struct Bytes{a:u8;b:[u8;2]} fn main()->i32{var x=Bytes{a:128u8,b:[1u8,255u8]};x.b[0]=x.a;return i32(x.b[0])}",
        "enum Fault{Bad} fn byte()->Fault!u8{return 255u8}fn main()->Fault!i32{return i32(byte()?)}",
        "fn main()->i32{let x=255u8;match x{0=>{return 1},255=>{return 2},_=>{return 3}}}",
        "fn main()->i32{var sum=0;for b in 0u8..255u8{sum+=i32(b)}return sum}",
    ] {
        checked(text);
    }
    // Generic validation runs even when the invalid template is never called.
    let report = compile_source(
        &source("fn unused<T>(x:T)->u8{return u8(x)} fn main()->i32{return 0}"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn conversion_walks_keep_owned_capture_write_effects() {
    let text = "resource Byte{value:u8} impl Drop for Byte{fn drop(self:Self){}} fn bump(x:Byte)->u8{x.value=x.value+%1u8;return x.value} fn main()->i32{let x=Byte{value:255u8};var f=move ()=>i32(bump(x));return f()}";
    checked(text);
    let module = typed(text);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, CallableKind::FnMut);
}

#[test]
fn bare_conversion_names_cannot_be_shadowed_but_members_remain_ordinary() {
    for text in [
        "fn u8(x:i32)->i32{return x} fn main()->i32{return 0}",
        "extern \"C\" fn i32(x:i32)->i32;fn main()->i32{return 0}",
        "fn u8<T>(x:T)->T{return x} fn main()->i32{return 0}",
    ] {
        rejected(text, Phase::Expand, "E-INTEGER-CONVERSION-NAME");
    }
    for text in [
        "fn main()->i32{let u8=1;return u8}",
        "fn f(i32:i32)->i32{return i32} fn main()->i32{return 0}",
        "fn main()->i32{for i64 in 0..1{}return 0}",
        "fn main()->i32{let f=(u8:i32)=>u8;return f(1)}",
        "fn main()->i32{match 1{u8=>{return u8}}}",
    ] {
        rejected(text, Phase::Types, "E-INTEGER-CONVERSION-NAME");
    }
    checked(
        "struct Byte{u8:i32} impl Byte{fn i32(self:Self)->i32{return .u8}} fn main()->i32{let x=Byte{u8:7};return x.i32()}",
    );
    for text in [
        "fn main()->i32{return u8::<i32>(1)}",
        "fn main()->i32{let f=u8;return f(1)}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(report.status, ResultCategory::Success, "{text}");
    }
}

const BORROWED: &str = "extern \"C\" fn transfer(a:&[u8;4],b:&mut [i32;2])->i64 contract{provides(noescape(b),noescape(a));effects(ffi);}; fn main()->i32{let a:[u8;4]=[0,128,255,10];var b:[i32;2]=[0,0];unsafe{transfer(&a,&mut b)}return b[0]}";

#[test]
fn foreign_noescape_is_canonical_complete_and_allows_live_reborrows() {
    checked(BORROWED);
    let module = typed(BORROWED);
    assert_eq!(
        module.functions[0]
            .foreign
            .as_ref()
            .unwrap()
            .noescape_parameters,
        [0, 1]
    );
    checked(
        "extern \"C\" fn fill(p:&mut [u8;4]) contract{provides(noescape(p));effects(ffi);}; fn helper(p:&mut [u8;4]){unsafe{fill(p)}}fn main()->i32{var b:[u8;4]=[0,0,0,0];helper(&mut b);helper(&mut b);return i32(b[0])}",
    );
    checked(
        "extern \"C\" fn fill(p:&mut [[u8;2];2]) contract{provides(noescape(p));effects(ffi);}; fn main()->i32{var b:[[u8;2];2]=[[0,1],[2,3]];unsafe{fill(&mut b)}return i32(b[1][1])}",
    );
}

#[test]
fn foreign_contract_rejects_omissions_duplicates_and_unimplemented_terms() {
    for declaration in [
        "extern \"C\" fn f(p:&u8);",
        "extern \"C\" fn f(p:&u8) contract{effects(ffi);};",
        "extern \"C\" fn f(p:&u8) contract{provides(noescape(p));};",
        "extern \"C\" fn f(p:&u8) contract{provides(noescape(p),noescape(p));effects(ffi);};",
        "extern \"C\" fn f(p:&u8) contract{provides(noescape(missing));effects(ffi);};",
        "extern \"C\" fn f(p:i32) contract{provides(noescape(p));effects(ffi);};",
        "extern \"C\" fn f(p:&u8) contract{provides(noescape(p));effects(pure);};",
        "extern \"C\" fn f(p:&u8) contract{provides(noescape(p));effects(ffi);effects(ffi);};",
        "extern \"C\" fn f(p:&u8) contract{provides(retained(p));effects(ffi);};",
        "extern \"C\" fn f(p:&u8) contract{provides(noescape(p.x));effects(ffi);};",
    ] {
        let report = compile_source(
            &source(&format!("{declaration}fn main()->i32{{return 0}}")),
            &CompileOptions::default(),
        );
        assert_ne!(report.status, ResultCategory::Success, "{declaration}");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-FFI-DECLARATION"),
            "{declaration}\n{}",
            report.to_json()
        );
    }
}

#[test]
fn foreign_pointer_shape_and_unsafe_boundary_remain_closed() {
    // Schema15's finite C value ABI admits u8 parameters and results. Preserve
    // the former schema14 rejection as a positive control, with unsafe intact.
    checked("extern \"C\" fn f(p:u8);fn main()->i32{return 0}");
    checked("extern \"C\" fn f(p:u8)->u8;fn main()->i32{unsafe{return i32(f(255u8))}}");
    rejected(
        "extern \"C\" fn f(p:u8)->u8;fn main()->i32{return i32(f(255u8))}",
        Phase::Types,
        "E-FFI-UNSAFE",
    );
    for ty in ["bool", "()", "[u8;0]", "[[u8;0];2]", "(&u8)", "(fn()->i32)"] {
        let text = format!(
            "extern \"C\" fn f(p:&{ty}) contract{{provides(noescape(p));effects(ffi);}};fn main()->i32{{return 0}}"
        );
        rejected(&text, Phase::Types, "E-FFI-DECLARATION");
    }
    rejected(
        "extern \"C\" fn f(p:bool);fn main()->i32{return 0}",
        Phase::Types,
        "E-FFI-DECLARATION",
    );
    rejected(
        "extern \"C\" fn f()->&u8;fn main()->i32{return 0}",
        Phase::Types,
        "E-FFI-DECLARATION",
    );
    rejected(
        "extern \"C\" fn f(p:&u8)contract{provides(noescape(p));effects(ffi);};fn main()->i32{let x=1u8;f(&x);return 0}",
        Phase::Types,
        "E-FFI-UNSAFE",
    );
}

#[test]
fn ffi_address_arguments_cannot_bypass_existing_alias_permissions() {
    let prefix = "extern \"C\" fn f(a:&u8,b:&mut u8)contract{provides(noescape(a),noescape(b));effects(ffi);};";
    rejected(
        &format!("{prefix}fn main()->i32{{var x=1u8;unsafe{{f(&x,&mut x)}}return 0}}"),
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    checked(&format!(
        "{prefix}fn main()->i32{{let x=1u8;var y=2u8;unsafe{{f(&x,&mut y)}}return i32(y)}}"
    ));
}

#[test]
fn raw_core_foreign_indices_and_conversion_targets_are_rechecked() {
    for indices in [vec![], vec![0], vec![1, 0], vec![0, 0, 1], vec![0, 1, 99]] {
        let mut raw = core(BORROWED);
        raw.functions[0]
            .foreign
            .as_mut()
            .unwrap()
            .noescape_parameters = indices;
        let errors =
            zil_types::check(&raw, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-FFI-DECLARATION"),
            "{errors:?}"
        );
    }
    let mut raw = core("fn main()->i32{return i32(1)}");
    let zil_ir::CoreStatementKind::Return(Some(expression)) =
        &mut raw.functions[0].body.statements[0].kind
    else {
        panic!()
    };
    let CoreExprKind::IntegerConvert { target, .. } = &mut expression.kind else {
        panic!()
    };
    *target = Type::Bool;
    let errors = zil_types::check(&raw, &TargetProfile::default(), &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-INTEGER-CONVERSION"),
        "{errors:?}"
    );
}

#[test]
fn raw_new_metadata_is_bounded_before_clone_or_fingerprint() {
    let mut raw = core(BORROWED);
    raw.functions[0]
        .foreign
        .as_mut()
        .unwrap()
        .noescape_parameters = vec![0; 4096];
    let limits = Limits {
        max_analysis_steps: 1024,
        ..Limits::default()
    };
    let errors = zil_types::check(&raw, &TargetProfile::default(), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );

    for forged in [
        Type::Bool,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::U8),
        },
        Type::Unsupported("x".repeat(8192)),
    ] {
        let mut module = typed("fn main()->i32{return i32(1u8)}");
        let target = module.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find_map(|i| {
                if let InstructionKind::ConvertInteger { target, .. } = &mut i.kind {
                    Some(target)
                } else {
                    None
                }
            })
            .unwrap();
        *target = forged;
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Analyze);
    }
}

#[test]
fn source_and_analysis_byte_work_have_exact_logical_budget_boundaries() {
    let raw = core(BORROWED);
    let succeeds = |n| {
        let limits = Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        zil_types::check(&raw, &TargetProfile::default(), &limits)
    };
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    assert!(succeeds(high).is_ok());
    while low < high {
        let mid = low + (high - low) / 2;
        if succeeds(mid).is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    let module = succeeds(high).unwrap();
    assert!(high > 0);
    assert!(
        succeeds(high - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    println!("081 exact Types byte budget N={high}");
    let succeeds = |n| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    assert!(succeeds(high).is_ok());
    while low < high {
        let mid = low + (high - low) / 2;
        if succeeds(mid).is_ok() {
            high = mid
        } else {
            low = mid + 1
        }
    }
    assert!(succeeds(high).is_ok());
    assert!(
        succeeds(high - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    println!("081 exact Analyze byte budget N={high}");
}

#[test]
fn byte_ct_materializes_typed_values_and_conversion_stays_checked() {
    checked("fn byte(x:i32)->u8{return u8(x)}fn main()->i32{let b:u8=#(byte(255));return i32(b)}");
    let report = compile_source(
        &source(
            "fn byte(x:i32)->u8{return u8(x)}fn main()->i32{let b:u8=#(byte(256));return i32(b)}",
        ),
        &CompileOptions::default(),
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
}
