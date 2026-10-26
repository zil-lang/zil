//! Language-only borrowed windows. These source/CT/Types/Analyze/Verify tests
//! do not claim a GUI, allocator, or independent verifier implementation.
use std::collections::BTreeMap;
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, ProjectSources, compile_project, compile_source};
use zil_ir::{HoldRoot, InstructionKind, Type};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "impl090.zil", text)
}
fn core(text: &str) -> zil_ir::CoreModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    zil_expand::expand(&input, &syntax).unwrap()
}
fn typed(text: &str) -> zil_ir::TypedModule {
    let expanded = core(text);
    let evaluated =
        zil_ct::evaluate_source_constants(&expanded, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&evaluated, &TargetProfile::default(), &Limits::default())
        .unwrap_or_else(|d| panic!("{text}\n{d:#?}"))
}
fn passes(text: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{text}\n{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
}
fn rejects(text: &str, category: ResultCategory, phase: Phase, prefix: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{text}\n{}", report.to_json());
    assert_eq!(report.phase, phase, "{text}\n{}", report.to_json());
    assert!(
        report.diagnostics[0].code.starts_with(prefix),
        "{text}\n{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}

const WINDOW: &str = "fn total(s:&[i32])->i64{var i:i64=0;var n:i64=0;while i<s.len(){n=n+i64(s[i]);i=i+1;}return n;}fn edit(s:&mut[i32]){s[0]=20;}fn tail(s:&[i32])->&[i32]{return &s[1..];}fn result()->i64{var a=[1,2,3,4];edit(&mut a[1..3]);let s=&a[..];return total(tail(s))+s.len();}";

#[test]
fn one_ordinary_signature_handles_runtime_windows_and_preserves_origins() {
    passes(WINDOW);
    let module = typed(WINDOW);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let tail = module.functions.iter().find(|f| f.name == "tail").unwrap();
    let facts = &plan.functions[tail.id.0 as usize];
    let range = tail
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::AddressSlice { .. }))
        .unwrap();
    assert_eq!(
        facts
            .holds
            .iter()
            .find(|v| Some(v.value) == range.result)
            .unwrap()
            .roots,
        vec![HoldRoot::Parameter(0)]
    );
    assert!(tail.return_type.is_slice_address());
    for i in module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
    {
        if let InstructionKind::AddressSlice { address, .. } = i.kind {
            assert!(i.result.is_some());
            assert!(address.0 < u32::MAX);
        }
    }
}

#[test]
fn all_endpoint_forms_reborrow_and_existing_copy_shapes_are_admitted() {
    for text in [
        "fn result()->i64{let a=[1,2,3];let a0=&a[..];let a1=&a[..2];let a2=&a[1..];let a3=&a[1..2];return a0.len()+a1.len()+a2.len()+a3.len();}",
        "fn relay(s:&mut[i32])->&mut[i32]{return &mut *s;}fn result()->i32{var a=[1,2];let s=relay(&mut a[..]);s[0]=3;return s[0];}",
        "fn result()->i64{let a=[(),()];let s=&a[..];s[0];return s.len();}",
        "struct Empty{}fn result()->i64{let a=[Empty{},Empty{}];return (&a[..]).len();}",
        "fn result()->i64{let a:[[i32;0];2]=[[],[]];let s=&a[..];let x=s[0];return s.len();}",
        "struct P{x:i32;y:i32;}fn result()->i32{var a=[P{x:1,y:2},P{x:3,y:4}];let s=&mut a[..];s[1].x=8;return s[1].x;}",
        "fn result()->i64{let a:[u8;0]=[];return (&a[..]).len();}",
        "fn result()->i64{let a=[1u16,2u16];let s=&a[..];return i64(s[1])+s.len();}",
        "fn result()->i64{let a=[1,2];let s=&a[..];return (if true{s}else{s}).len();}",
    ] {
        passes(text);
    }
}

#[test]
fn endpoint_order_is_base_then_left_then_right_without_implicit_reborrow() {
    let module = typed(
        "fn edge(p:&mut i32,v:i32)->i32{*p=*p+1;return v;}fn result()->i64{var n=0;let a=[1,2,3];let s=&a[edge(&mut n,0)..edge(&mut n,2)];return s.len()+i64(n);}",
    );
    let main = module
        .functions
        .iter()
        .find(|f| f.name == "result")
        .unwrap();
    let instructions: Vec<_> = main.blocks.iter().flat_map(|b| &b.instructions).collect();
    let slice_pos = instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::AddressSlice { .. }))
        .unwrap();
    let InstructionKind::AddressSlice {
        address,
        start: Some(start),
        end: Some(end),
    } = instructions[slice_pos].kind
    else {
        panic!("explicit endpoints disappeared")
    };
    let position = |value| {
        instructions
            .iter()
            .position(|i| i.result == Some(value))
            .unwrap()
    };
    assert!(position(address) < position(start));
    assert!(position(start) < position(end));
    assert!(position(end) < slice_pos);
    assert!(matches!(
        instructions[slice_pos + 1].kind,
        InstructionKind::ReborrowAddress { .. }
    ));
}

#[test]
fn ordinary_len_methods_and_non_slice_closures_keep_their_meaning() {
    passes(
        "struct Q{x:i64;}impl Q{fn len(self:Self)->i64{return .x;}}fn result()->i64{let q=Q{x:5};let a=[1,2,3];let s=&a[1..];return q.len()+s.len();}",
    );
    passes("fn result()->i64{let a=[1,2,3];let s=&a[1..];let f=()=>3;return s.len()+i64(f());}");
    passes(
        "type Bytes=[u8];fn count(s:&Bytes)->i64{return s.len();}fn result()->i64{let a=[1u8,2u8];return count(&a[..]);}",
    );
}

#[test]
fn cross_module_parameter_and_result_contracts_keep_element_identity() {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "root.zil",
                "import seq;fn result()->i64{var a=[1u8,2u8,3u8];let s=seq.tail(&a[..]);return seq.count(s)+i64(s[0]);}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "seq.zil",
                "pub fn count(s:&[u8])->i64{return s.len();}pub fn tail(s:&[u8])->&[u8]{return &s[1..];}",
            ),
        ],
        paths: BTreeMap::from([(vec!["seq".into()], ModuleId(1))]),
    };
    let report = compile_project(&project, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn views_require_real_places_integer_bounds_and_matching_mutability() {
    for (text, code) in [
        (
            "fn result(){let a=[1,2];let s=&a[true..];}",
            "E-SLICE-BOUND-TYPE",
        ),
        (
            "fn result(){let a=[1,2];let s=&a[..false];}",
            "E-SLICE-BOUND-TYPE",
        ),
        (
            "fn result(){let a=[1,2];let s=&mut a[..];}",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result(){var a=[1,2];let s=&a[..];let t=&mut s[..];}",
            "E-ADDRESS-PERMISSION",
        ),
        (
            "fn result(){let a=[1,2];let s=&a[..];s.len(1);}",
            "E-CALL-ARITY",
        ),
        ("fn result(){let a=1;let s=&a[..];}", "E-SLICE-TYPE"),
    ] {
        rejects(text, ResultCategory::SafeReject, Phase::Types, code);
    }
    for text in [
        "fn result(){let a=[1,2];let s=a[..];}",
        "fn result(){let a=[1,2];let s=&a[..];let x=*s;}",
        "fn f(s:[i32]){}fn result(){}",
        "fn f(s:&[&i32]){}fn result(){}",
        "struct S{s:&[i32];}fn result(){}",
    ] {
        rejects(
            text,
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn ownership_and_whole_base_exclusivity_are_independently_verified() {
    for text in [
        "fn bad()->&[i32]{let a=[1,2];return &a[..];}fn result(){}",
        "fn result()->i32{var a=[1,2];let s=&a[..];a[0]=4;return s[0];}",
        "fn result()->i32{var a=[1,2];let s=&mut a[0..1];let t=&mut a[1..2];return s[0]+t[0];}",
        "fn result()->i32{var a=[1,2];let s=&mut a[..];let t=s;return s[0]+t[0];}",
        "fn edge(p:&mut i32)->i32{*p=1;return 0;}fn result()->i32{var a=[1,2];let s=&a[edge(&mut a[0])..];return s[0];}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}\n{}",
            report.to_json()
        );
        assert!(
            matches!(report.phase, Phase::Analyze | Phase::Verify),
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics[0].code.starts_with("E-BORROW")
                || report.diagnostics[0].code.starts_with("E-VERIFY")
                || report.diagnostics[0].code.starts_with("E-ANALYZE")
                || report.diagnostics[0].code == "E-UNINITIALIZED",
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn suspended_parent_cannot_read_explicit_or_implicit_slice_metadata() {
    for access in ["s.len();", "&s[..];", "&s[0];"] {
        let text = format!(
            "fn result()->i32{{var a=[1,2];let s=&mut a[..];let child=&mut s[..];{access}child[0]=9;return child[0];}}"
        );
        rejects(&text, ResultCategory::SafeReject, Phase::Verify, "E-BORROW");
    }
    passes(
        "fn result()->i64{var a=[1,2];let s=&mut a[..];let child=&mut s[..];child[0]=9;let last=child[0];let n=s.len();let v=&s[..];let value=v[0];a[0]=3;return n+i64(last)+i64(value)+i64(a[0]);}",
    );
}

#[test]
fn foreign_abi_and_closure_windows_remain_closed() {
    rejects(
        "extern \"C\" fn bad(s:&[u8])contract{provides(noescape(s));effects(ffi);};fn result(){}",
        ResultCategory::SafeReject,
        Phase::Types,
        "E-FFI-DECLARATION",
    );
    for text in [
        "fn result()->i64{let a=[1,2];let s=&a[..];let f=()=>s.len();return f();}",
        "fn result()->i64{let a=[1,2];let s=&a[..];let f=move()=>s.len();return f();}",
        "fn result()->i64{let f=(s:&[i32])->i64=>s.len();return 0;}",
        "fn run<F:Fn(&[i32])->i64>(f:&F)->i64{return 0;}fn result(){}",
        "fn run<F:Fn()->i32>(f:&F,s:&[i32])->i32{return f();}fn result(){}",
        "fn run<F:Fn()->i32>(f:&F,s:&[i32])->&[i32]{return s;}fn result(){}",
    ] {
        rejects(
            text,
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn closure_local_windows_do_not_escape_the_environment_boundary() {
    for text in [
        "fn result()->i64{let f=()=>{let a=[1,2];let s=&a[..];return s.len();};return f();}",
        "fn result()->i64{let a=[1,2];let f=move()=>{let s=&a[..];return s.len();};return f();}",
        "fn result()->i64{let a=[1,2];let f=()=>{let s=&a[..];return s.len();};return f();}",
        "fn result()->i32{var a=[1,2];var f=move()=>{let s=&mut a[..];s[0]=s[0]+1;return s[0];};return f()+f();}",
        "fn result()->i32{var a=[1,2];var f=()=>{let s=&mut a[..];s[0]=s[0]+1;return s[0];};let last=f();return last+a[0];}",
    ] {
        passes(text);
    }
}

#[test]
fn scalar_ct_endpoints_work_while_ct_slice_execution_is_closed() {
    passes("fn result()->i64{let a=[1,2,3,4];let s=&a[#(1)..#(3)];return s.len();}");
    for text in [
        "fn size()->i64{let a=[1,2];let s=&a[..];return s.len();}fn result()->i64{return #(size());}",
        "fn result()->i64{let a=[1,2];let s=&a[..];return #(s.len());}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(
            matches!(report.phase, Phase::Ct | Phase::Verify),
            "{}",
            report.to_json()
        );
        assert!(
            matches!(
                report.status,
                ResultCategory::Unsupported | ResultCategory::SafeReject
            ),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("llvm"));
    }
}

fn limited(errors: Vec<Diagnostic>) {
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:#?}"
    );
}

#[test]
fn public_ct_preflight_bounds_slice_element_trees_before_copying() {
    let mut expanded = core("fn f(s:&[i32]){}fn result(){}");
    let mut element = Type::I32;
    for _ in 0..64 {
        element = Type::Tuple(vec![element]);
    }
    expanded.functions[0].parameters[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(element),
        }),
    };
    let limits = Limits {
        max_call_depth: 16,
        ..Limits::default()
    };
    limited(
        zil_ct::evaluate_source_constants_with_usage(&expanded, &TargetProfile::default(), &limits)
            .unwrap_err(),
    );
    limited(zil_types::infer_source_closures(&expanded, &limits).unwrap_err());
}

#[test]
fn range_endpoints_are_not_erased_before_name_checking() {
    for text in [
        "fn result(){let a=[1,2];let s=&a[missing..];}",
        "fn result(){let a=[1,2];let s=&a[..missing];}",
    ] {
        rejects(
            text,
            ResultCategory::SafeReject,
            Phase::Types,
            "E-UNKNOWN-NAME",
        );
    }
}
