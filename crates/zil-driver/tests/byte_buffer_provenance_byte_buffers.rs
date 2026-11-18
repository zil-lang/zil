//! Real source admission, candidate provenance and independently checked IR.
use std::collections::BTreeMap;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, ProjectSources, compile_project, compile_source};
use zil_ir::{HoldRoot, InstructionKind, Type};

fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "byte_buffer_provenance.zil", text)
}
fn checked(text: &str) -> CompileReport {
    let Some(root) = std::env::var_os("ZIL_IMPL094_RECORD") else {
        return compile_source(&input(text), &CompileOptions::default());
    };
    let dir = std::path::PathBuf::from(root).join(Fingerprint::of(text.as_bytes()).to_hex());
    std::fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("input.zil");
    std::fs::write(&source_path, text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), source_path.to_str().unwrap(), text);
    let report = compile_source(
        &source,
        &CompileOptions {
            emit_dir: Some(dir.join("stages")),
            ..CompileOptions::default()
        },
    );
    std::fs::write(dir.join("report.json"), report.to_json()).unwrap();
    report
}
fn passes(text: &str) {
    let report = checked(text);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{text}\n{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
}
fn rejects(text: &str, phase: Phase, code: &str) -> CompileReport {
    let report = checked(text);
    assert_eq!(
        report.status,
        if code == "E-UNSUPPORTED" {
            ResultCategory::Unsupported
        } else {
            ResultCategory::SafeReject
        },
        "{text}\n{}",
        report.to_json()
    );
    assert_eq!(report.phase, phase, "{text}\n{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        code,
        "{text}\n{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
    report
}

fn typed(text: &str) -> zil_ir::TypedModule {
    let source = input(text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default())
        .unwrap_or_else(|e| panic!("{text}: {e:#?}"))
}

#[test]
fn byte_storage_has_real_source_operations_and_sealed_single_definition() {
    let text = "fn main()->i32{let b=bytes_new();if !bytes_reserve(b,4){return 1;}if !bytes_push(b,7u8){return 2;}let p=bytes_view_mut(b);p[0]=9u8;return i32(bytes_len(b)+bytes_capacity(b))+i32(bytes_view(b)[0]);}";
    passes(text);
    let module = typed(text);
    assert_eq!(
        module
            .resources
            .iter()
            .filter(|r| r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME)
            .count(),
        1
    );
    let resource = module
        .resources
        .iter()
        .find(|r| r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME)
        .unwrap();
    assert_eq!(resource.payload, Type::ByteBuffer);
    assert_eq!(resource.teardown, None);
    let instructions = module.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::ByteBufferNew))
    );
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::ByteBufferReserve { .. }))
    );
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::ByteBufferPush { .. }))
    );
    assert!(instructions.iter().any(|i| matches!(
        i.kind,
        InstructionKind::ByteBufferBorrow { mutable: true, .. }
    )));
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
    );
}

#[test]
fn byte_view_operations_keep_the_original_mode_and_exact_manager() {
    let limits = Limits::default();
    for (primitive, mutable) in [("bytes_view", false), ("bytes_view_mut", true)] {
        let text = format!(
            "fn main(){{let selected=bytes_new();let sibling=bytes_new();bytes_len(sibling);{primitive}(selected);}}"
        );
        let source = input(&text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let target = TargetProfile::default();
        let expanded = zil_expand::expand_with_map_semantics(
            &source,
            &syntax,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap();
        let pristine = zil_types::check(&expanded.core, &target, &limits).unwrap();
        let candidate = zil_analyze::analyze(&pristine, &limits).unwrap();
        zil_verify::verify_module(
            pristine.clone(),
            candidate.clone(),
            &pristine.target,
            &limits,
        )
        .unwrap();
        let function = &pristine.functions[0];
        let replacement = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find_map(|instruction| match instruction.kind {
                InstructionKind::ByteBufferLen { buffer } => Some(buffer),
                _ => None,
            })
            .unwrap();
        for replace_manager in [false, true] {
            let mut module = pristine.clone();
            let function = &mut module.functions[0];
            let borrow = function
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
                .find(|instruction| {
                    matches!(instruction.kind, InstructionKind::ByteBufferBorrow { .. })
                })
                .unwrap();
            let InstructionKind::ByteBufferBorrow {
                buffer,
                mutable: actual,
            } = &mut borrow.kind
            else {
                unreachable!()
            };
            if replace_manager {
                *buffer = replacement;
            } else {
                *actual = !mutable;
                let result = borrow.result.unwrap();
                let Type::Address { mutable: mode, .. } = &mut function.values[result.0 as usize]
                else {
                    unreachable!()
                };
                *mode = !mutable;
            }
            let diagnostics = zil_analyze::analyze(&module, &limits).unwrap_err();
            let expected = if replace_manager {
                "selects another original parent place"
            } else {
                "replaces its independently resolved original opcode"
            };
            assert!(
                diagnostics.iter().any(|d| {
                    d.category == ResultCategory::SafeReject && d.message.contains(expected)
                }),
                "{diagnostics:?}"
            );
            let mut forged = candidate.clone();
            forged.module_fingerprint = module.fingerprint();
            assert!(
                zil_verify::verify_module(module.clone(), forged.clone(), &module.target, &limits)
                    .is_err()
            );
            let wire = zil_ir::wire::encode_module_candidate(&module, &forged, &limits).unwrap();
            assert!(zil_verify::verify_module_bytes(&wire, &module.target, &limits).is_err());
        }
    }
}

#[test]
fn returned_package_byte_view_keeps_the_actual_parameter_and_field() {
    let source = input(
        "package Buffers { manages first:ByteBuffer; manages second:ByteBuffer }
         fn view<life a,life b>(left:&'a Buffers,right:&'b Buffers)->&'a [u8] {
             return bytes_view(left.first)
         }
         fn main()->i32 {
             let left=Buffers{first:bytes_new(),second:bytes_new()}
             let right=Buffers{first:bytes_new(),second:bytes_new()}
             return i32(view(&left,&right).len())
         }",
    );
    let limits = Limits::default();
    let target = TargetProfile::default();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map_semantics(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let module = zil_types::check(&expanded.core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    for (replace_parameter, analyze_message, verify_message) in [
        (
            false,
            "changes its original field path",
            "actual field index differs",
        ),
        (
            true,
            "selects another original parent place",
            "another original parameter",
        ),
    ] {
        let mut changed = module.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "view")
            .unwrap();
        let [first, second] = function.parameters.as_slice() else {
            panic!("expected the two original parameter slots");
        };
        let mut instructions = function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions);
        if replace_parameter {
            let row = instructions
                .find(|row| row.kind == InstructionKind::Load(*first))
                .unwrap();
            row.kind = InstructionKind::Load(*second);
        } else {
            let row = instructions
                .find(|row| matches!(row.kind, InstructionKind::AddressField { index: 0, .. }))
                .unwrap();
            let InstructionKind::AddressField { index, .. } = &mut row.kind else {
                unreachable!();
            };
            *index = 1;
        }
        let diagnostics = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert!(
            diagnostics.iter().any(|d| {
                d.category == ResultCategory::SafeReject && d.message.contains(analyze_message)
            }),
            "{diagnostics:?}"
        );

        let mut forged = candidate.clone();
        forged.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &forged, &limits).unwrap();
        for diagnostics in [
            zil_verify::verify_module(changed, forged, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                diagnostics.iter().any(|d| {
                    d.category == ResultCategory::SafeReject && d.message.contains(verify_message)
                }),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn stored_byte_views_follow_short_circuit_loop_predecessors() {
    let source = r#"
package Buffer { manages data:ByteBuffer }
fn view(value:&Buffer)->&[u8] {return bytes_view(value.data)}
fn compare(left:&Buffer,right:&Buffer)->i32 {
    let a=view(left)
    let b=view(right)
    var at:i64=0
    while at<a.len() && at<b.len() {
        if a[at]<b[at] {return -1}
        if a[at]>b[at] {return 1}
        at+=1
    }
    return 0
}
fn make(byte:u8)->ByteBuffer contract {result(create)} {
    let buffer=bytes_new()
    if !bytes_reserve(buffer,1) || !bytes_push(buffer,byte) {return bytes_new()}
    return give buffer
}
fn main()->i32 {
    let a=Buffer{data:make(42u8)}
    let b=Buffer{data:make(73u8)}
    return compare(&a,&b)+1
}
"#;
    passes(source);
    let overlap = source
        .replace("left:&Buffer", "left:&mut Buffer")
        .replace("let b=view(right)", "let b=view(right)\n    bytes_reserve(left.data,2)")
        .replace("let a=Buffer", "var a=Buffer")
        .replace("compare(&a,&b)", "compare(&mut a,&b)");
    rejects(&overlap, Phase::Analyze, "E-BORROW-CONFLICT");
}

#[test]
fn create_give_and_description_parameter_window_returns_compose() {
    passes(
        "fn fresh()->ByteBuffer contract{result(create);}{return bytes_new();}fn relay(give b:ByteBuffer)->ByteBuffer contract{result(give);}{return give b;}fn fill(b:ByteBuffer)->bool{return bytes_reserve(b,3)&&bytes_push(b,8u8);}fn view(b:ByteBuffer)->&[u8]{return bytes_view(b);}fn consume(give b:ByteBuffer)->i32{return i32(bytes_len(b));}fn main()->i32{let a=fresh();let b=relay(give a);if !fill(b){return 5;}let s=view(b);let value=i32(s[0]);return value+consume(give b);}",
    );
}

#[test]
fn inferred_descriptor_and_written_slice_follow_actual_parameter_order() {
    let report = checked(r#"
        fn first(buffer:ByteBuffer, bytes:&[u8])->i64 { return bytes_len(buffer)+bytes.len() }
        fn last(bytes:&[u8], buffer:ByteBuffer)->i64 { return bytes.len()+bytes_len(buffer) }
        fn result()->i64 {
            let b=bytes_new()
            if !bytes_reserve(b,1) || !bytes_push(b,77u8) { return -1 }
            let a=[12u8,34u8]
            return first(b,&a[..])+last(&a[..],b)
        }
    "#);
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn byte_descriptions_and_borrowed_windows_preserve_actual_roots() {
    let module = typed(
        "fn window(b:ByteBuffer)->&[u8]{return bytes_view(b);}fn result()->i64{let b=bytes_new();return bytes_view(b).len();}",
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for (function, plan) in module.functions.iter().zip(&plan.functions) {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if matches!(instruction.kind, InstructionKind::ByteBufferBorrow { .. }) {
                let roots = &plan.holds[instruction.result.unwrap().0 as usize].roots;
                if function.name == "window" {
                    assert_eq!(roots, &[HoldRoot::Parameter(0)]);
                } else {
                    assert!(matches!(roots.as_slice(), [HoldRoot::Local(_)]));
                }
            }
        }
    }
    zil_verify::verify_module(module, plan, &TargetProfile::default(), &Limits::default()).unwrap();
}

#[test]
fn ordered_alias_helpers_do_not_confuse_parameter_names_with_disjointness() {
    passes(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);let n=s.len();bytes_reserve(b,4);return n;}fn result()->i64{let b=bytes_new();return act(b,b);}",
    );
    rejects(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,4);return s.len();}fn result()->i64{let b=bytes_new();return act(b,b);}",
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    passes(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,4);return s.len();}fn result()->i64{let a=bytes_new();let b=bytes_new();return act(a,b);}",
    );
}

#[test]
fn active_windows_block_growth_push_move_and_conflicting_metadata() {
    for text in [
        "fn result()->i64{let b=bytes_new();let s=bytes_view(b);bytes_reserve(b,0);return s.len();}",
        "fn result()->i64{let b=bytes_new();let s=bytes_view(b);bytes_push(b,1u8);return s.len();}",
        "fn take(give b:ByteBuffer){}fn result()->i64{let b=bytes_new();let s=bytes_view(b);take(give b);return s.len();}",
        "fn result()->i64{let b=bytes_new();let s=bytes_view_mut(b);let n=bytes_len(b);return s.len()+n;}",
        "fn result()->i64{let b=bytes_new();let s=bytes_view_mut(b);let t=bytes_view(b);return s.len()+t.len();}",
    ] {
        rejects(text, Phase::Verify, "E-BORROW-CONFLICT");
    }
    passes(
        "fn result()->i64{let b=bytes_new();let s=bytes_view(b);let n=bytes_len(b);return s.len()+n;}",
    );
}

#[test]
fn local_or_consumed_carriers_cannot_export_unowned_windows() {
    for (text, phase, code, message, occurrence) in [
        (
            "fn bad()->&[u8]{let b=bytes_new();return bytes_view(b);}fn main(){}",
            Phase::Expand,
            "E-SOURCE-SEMANTICS",
            "elided function result has no original receiver or unique borrowed input lifetime",
            Some("&[u8]"),
        ),
        (
            "fn bad(give b:ByteBuffer)->&[u8]{return bytes_view(b);}fn main(){}",
            Phase::Expand,
            "E-SOURCE-SEMANTICS",
            "elided function result has no original receiver or unique borrowed input lifetime",
            Some("&[u8]"),
        ),
        (
            "fn take(give b:ByteBuffer){}fn result()->i64{let b=bytes_new();let a=b;take(give b);return bytes_len(a);}",
            Phase::Analyze,
            "E-ANALYZE-IR",
            "resource closure: head transition invalidates a description required later",
            None,
        ),
        (
            "fn take(give b:ByteBuffer){}fn main(){let b=bytes_new();take(give b);take(give b);}",
            Phase::Types,
            "E-SEMANTIC-ORIGINAL",
            "original give transfers an unavailable or borrowed value",
            Some("give b"),
        ),
    ] {
        let report = rejects(text, phase, code);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.message, message, "{}", report.to_json());
        if let Some(occurrence) = occurrence {
            let start = text.rfind(occurrence).unwrap();
            assert_eq!(
                diagnostic.span,
                Some(zil_base::Span::new(FileId(0), start, start + occurrence.len())),
                "{}",
                report.to_json(),
            );
        }
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn actual_window_return_cannot_select_a_local_or_consumed_byte_owner() {
    let text = r#"
fn release(give owner:ByteBuffer) {}
fn window(owner:ByteBuffer,give consumed:ByteBuffer)->&[u8] {
    let local=bytes_new();
    let before_local_drop=bytes_view(local);
    let before_consumption=bytes_view(consumed);
    before_local_drop.len();
    before_consumption.len();
    release(give consumed);
    return bytes_view(owner);
}
fn main()->i32 {
    let owner=bytes_new();
    let consumed=bytes_new();
    if !bytes_reserve(owner,1) || !bytes_push(owner,7u8) {return 1;}
    let result=window(owner,give consumed);
    return i32(result[0])-7;
}
"#;
    passes(text);
    let source = input(text);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map_semantics(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let module = zil_types::check(&expanded.core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let window = module
        .functions
        .iter()
        .find(|f| f.name == "window")
        .unwrap();
    let [owner, consumed] = window.parameters.as_slice() else {
        panic!("expected a described owner and a consumed owner");
    };
    let local = window
        .locals
        .iter()
        .find(|local| local.name == "local")
        .unwrap()
        .id;
    let exit = window
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, zil_ir::Terminator::Return(Some(_))))
        .unwrap();
    let windows: Vec<_> = exit
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.kind {
            InstructionKind::ByteBufferBorrow {
                buffer,
                mutable: false,
            } => Some((buffer, instruction.result.unwrap())),
            _ => None,
        })
        .collect();
    let [
        (local_buffer, local_window),
        (consumed_buffer, consumed_window),
        (owner_buffer, valid_window),
    ] = windows.as_slice()
    else {
        panic!("expected three actual shared windows");
    };
    for (buffer, producer) in [
        (*local_buffer, InstructionKind::DescribeLocal { local }),
        (
            *consumed_buffer,
            InstructionKind::DescribeLocal { local: *consumed },
        ),
        (*owner_buffer, InstructionKind::Load(*owner)),
    ] {
        assert!(
            exit.instructions
                .iter()
                .any(|instruction| instruction.result == Some(buffer)
                    && instruction.kind == producer)
        );
    }
    assert!(
        exit.instructions
            .iter()
            .any(|instruction| instruction.kind == InstructionKind::DropManaged { local })
    );
    assert!(
        exit.instructions
            .iter()
            .any(|instruction| instruction.kind == InstructionKind::TakeLocal { local: *consumed })
    );
    assert_eq!(
        exit.terminator,
        zil_ir::Terminator::Return(Some(*valid_window))
    );
    for replacement in [local_window, consumed_window] {
        assert_eq!(
            window.values[replacement.0 as usize],
            window.values[valid_window.0 as usize]
        );
        let mut changed = module.clone();
        let window = changed
            .functions
            .iter_mut()
            .find(|f| f.id == window.id)
            .unwrap();
        let exit = window
            .blocks
            .iter_mut()
            .find(|block| block.id == exit.id)
            .unwrap();
        exit.terminator = zil_ir::Terminator::Return(Some(*replacement));
        let analysis = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert_eq!(analysis[0].code, "E-ANALYZE-IR");
        assert_eq!(
            analysis[0].message,
            "semantic program: actual Return changes its complete original result class/source role"
        );
        let mut claim = candidate.clone();
        claim.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
        let raw = zil_verify::verify_module(changed, claim, &target, &limits).unwrap_err();
        let serialized = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert_eq!(raw[0].code, "E-VERIFY-SEMANTIC");
        assert_eq!(
            raw[0].message,
            "actual Return violates the original rigid semantic result"
        );
        assert_eq!(raw, serialized);
    }
}

#[test]
fn storage_heads_and_unsupported_escape_routes_fail_closed() {
    for (text, phase, code, message) in [
        (
            "fn main(){let b=ByteBuffer{};}",
            Phase::Types,
            "E-RECORD-TYPE",
            "record construction requires a declared product, actual type is ByteBuffer",
        ),
        (
            "fn main(){let b=bytes_new();b.len=2;}",
            Phase::Types,
            "E-FIELD-TYPE",
            "field len requires a tuple or declared product, actual operand is ByteBuffer",
        ),
        (
            "package P{data:ByteBuffer;}fn main(){}",
            Phase::Types,
            "E-UNSUPPORTED",
            "ByteBuffer fields require an explicit package manages declaration",
        ),
        (
            "struct P{data:ByteBuffer;}fn main(){}",
            Phase::Types,
            "E-UNSUPPORTED",
            "ByteBuffer fields require an explicit package manages declaration",
        ),
        (
            "extern \"C\" fn f(b:ByteBuffer);fn main(){}",
            Phase::Types,
            "E-FFI-DECLARATION",
            "C parameters do not accept source resource passing modes",
        ),
        (
            "fn main(){let n=#(bytes_new());}",
            Phase::Ct,
            "E-CT-FORBIDDEN",
            "static callback or dynamic byte storage cannot enter a compile-time query",
        ),
        (
            "fn main(){let b=bytes_new();bytes_reserve(b,true);}",
            Phase::Types,
            "E-TYPE-MISMATCH",
            "bytes_reserve capacity requires i64, found Bool",
        ),
        (
            "fn main(){let b=bytes_new();bytes_push(b,256);}",
            Phase::Types,
            "E-INTEGER-RANGE",
            "integer does not fit u8",
        ),
        (
            "fn main(){let b=bytes_new();let s=&b;}",
            Phase::Types,
            "E-UNSUPPORTED",
            "address targets require a complete value/slot domain or actual shared user-resource head; descriptors are not head positions",
        ),
        (
            "fn main(){let bytes_push=2;}",
            Phase::Ct,
            "E-BYTE-BUFFER-NAME",
            "ByteBuffer and bare byte storage primitives cannot be rebound",
        ),
    ] {
        let report = rejects(text, phase, code);
        assert_eq!(report.diagnostics[0].message, message, "{}", report.to_json());
        if code == "E-TYPE-MISMATCH" {
            let argument = text.find("true").unwrap();
            assert_eq!(
                report.diagnostics[0].span,
                Some(zil_base::Span::new(FileId(0), argument, argument + "true".len())),
                "{}",
                report.to_json(),
            );
        }
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn an_uncalled_local_byte_descriptor_capture_keeps_its_owner_in_scope() {
    // The original 094 input is unchanged. Slice187 now admits the local
    // descriptor profile; nonescaping cleanup is checked independently.
    passes("fn main(){let b=bytes_new();let d=b;let f=move()=>bytes_len(d);}");
}

#[test]
fn parametric_windows_and_assignment_enter_full_production_verification() {
    passes(
        "fn tail<T:Copy>(p:&[T])->&[T]{return &p[1..];}fn mutable_tail<T:Copy>(p:&mut[T])->&mut[T]{return &mut p[1..];}fn put<T:SyncDrop>(p:&mut[T],v:T){p[0]=v;}fn replace<T:SyncDrop>(p:&mut T,v:T){*p=v;}fn length<T>(p:&[T])->i64{return p.len();}fn result()->i64{var a=[1,2,3];put::<i32>(mutable_tail::<i32>(&mut a[..]),7);replace::<i32>(&mut a[0],8);return length(tail(&a[..]))+i64(a[0]+a[1]);}",
    );
    passes(
        "fn put<T:SyncDrop>(p:&mut[T],v:T){p[0]=v;}fn length<T>(p:&[T])->i64{return p.len();}fn work<T:SyncDrop>(p:&mut[T],v:T)->i64{put(p,v);return length(p)+p.len();}fn result()->i64{let b=bytes_new();bytes_reserve(b,2);bytes_push(b,5u8);let n=work(bytes_view_mut(b),9u8);return n+length(bytes_view(b));}",
    );
}

#[test]
fn reusable_storage_and_generic_sequence_modules_share_real_interfaces() {
    let sources = [
        (
            "root.zil",
            "import storage;import sequence;fn result()->i64{let b=storage.create();storage.fill(b);let p=storage.view_mut(b);sequence.put(sequence.tail_mut(p),9u8);let n=sequence.count(p);return n+sequence.count(storage.view(b));}",
        ),
        (
            "storage.zil",
            "pub fn create()->ByteBuffer contract{result(create);}{return bytes_new();}pub fn fill(b:ByteBuffer)->bool{return bytes_reserve(b,2)&&bytes_push(b,4u8)&&bytes_push(b,5u8);}pub fn view(b:ByteBuffer)->&[u8]{return bytes_view(b);}pub fn view_mut(b:ByteBuffer)->&mut[u8]{return bytes_view_mut(b);}",
        ),
        (
            "sequence.zil",
            "pub fn tail_mut<T:Copy>(p:&mut[T])->&mut[T]{return &mut p[1..];}pub fn put<T:SyncDrop>(p:&mut[T],v:T){p[0]=v;}pub fn count<T>(p:&[T])->i64{return p.len();}",
        ),
    ];
    let recorded = std::env::var_os("ZIL_IMPL094_RECORD")
        .map(|root| std::path::PathBuf::from(root).join("three-modules"));
    let modules = sources
        .iter()
        .enumerate()
        .map(|(index, (path, text))| {
            let path = if let Some(dir) = &recorded {
                std::fs::create_dir_all(dir).unwrap();
                let path = dir.join(path);
                std::fs::write(&path, text).unwrap();
                path.to_str().unwrap().to_owned()
            } else {
                (*path).into()
            };
            SourceUnit::new(FileId(index as u32), ModuleId(index as u32), path, *text)
        })
        .collect();
    let project = ProjectSources {
        root: ModuleId(0),
        modules,
        paths: BTreeMap::from([
            (vec!["storage".into()], ModuleId(1)),
            (vec!["sequence".into()], ModuleId(2)),
        ]),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            emit_dir: recorded.as_ref().map(|dir| dir.join("stages")),
            ..CompileOptions::default()
        },
    );
    if let Some(dir) = recorded {
        std::fs::write(dir.join("report.json"), report.to_json()).unwrap();
    }
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify);
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn dynamic_storage_never_executes_through_compile_time_helpers() {
    for text in [
        "fn effect()->i32{let b=bytes_new();return 0;}fn result()->i32{return #(effect());}",
        "fn effect()->i32{if false{let b=bytes_new();}return 0;}fn result()->i32{return #(effect());}",
    ] {
        rejects(text, Phase::Ct, "E-CT-FORBIDDEN");
    }
    passes(
        "fn storage(b:ByteBuffer)->i64{return bytes_len(b);}fn pure()->u8{return 12u8;}fn result()->u8{return #(pure());}",
    );
}

#[test]
fn new_language_paths_keep_resource_limits() {
    let source = input("fn main(){let b=bytes_new();bytes_reserve(b,10);}");
    for limits in [
        Limits {
            max_nodes: 0,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        },
    ] {
        let report = compile_source(
            &source,
            &CompileOptions {
                limits,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::ResourceLimit,
            "{}",
            report.to_json()
        );
    }
}


#[test]
fn live_returned_window_reports_the_creation_conflict_and_subsequent_read() {
    use zil_base::DiagnosticRole;

    let source = r#"
fn window(owner:ByteBuffer)->&mut[u8] {return bytes_view_mut(owner);}
fn relay(owner:ByteBuffer)->&mut[u8] {return window(owner);}
fn main()->i32 {
    let b=bytes_new();
    bytes_reserve(b,4);
    bytes_push(b,7u8);
    let slice=relay(b);
    bytes_reserve(b,32);
    return i32(slice[0]);
}
"#;
    let report = checked(source);
    assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
    let conflict = report.diagnostics.iter().find(|d| d.phase == Phase::Analyze
        && d.code == "E-BORROW-CONFLICT").unwrap();
    // Labels may select a subexpression. They must still refer to the actual
    // operation in the original file, including the use after the conflict.
    for (role, operation) in [(DiagnosticRole::BorrowCreated, "relay(b)"),
                               (DiagnosticRole::ConflictingAccess, "bytes_reserve(b,32)"),
                               (DiagnosticRole::LaterUse, "slice[0]")] {
        let start = source.find(operation).unwrap();
        assert!(conflict.related.iter().any(|row| row.role == role
            && row.span.is_some_and(|span| span.file == FileId(0) && span.start < span.end
                && span.start >= start && span.end <= start + operation.len())
            && row.ir.is_some_and(|ir| ir.block.is_some() && ir.instruction.is_some())),
            "{} role {role:?}", report.to_json());
    }
}
