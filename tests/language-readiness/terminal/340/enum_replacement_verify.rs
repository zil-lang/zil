//! Real Source/Typed replacement witnesses; candidate mutations remain raw/Wire.
use super::*;
use zil_ir::{CandidatePlan, PassingMode, TypedModule};

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

fn candidate(text: &str) -> (TypedModule, CandidatePlan) {
    let typed = typed(text);
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    (typed, plan)
}

fn verify_both(module: &TypedModule, plan: &CandidatePlan) {
    let target = TargetProfile::default();
    let limits = Limits::default();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(module, plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

fn reject_both(module: &TypedModule, plan: &CandidatePlan, code: &str, message: &str) {
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut claim = plan.clone();
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(module, &claim, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(module.clone(), claim, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.code == code && e.message.contains(message)),
            "{errors:?}"
        );
    }
}

#[test]
fn g135_enum_verify_real_runtime_arm_tickets() {
    let text = include_str!("replace-enum-terminal-pending.zil");
    let (module, plan) = candidate(text);
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
        matches!(&i.kind, InstructionKind::ReservePlaceAddress { path, .. } if path.is_empty())
    }));
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
        matches!(i.kind, InstructionKind::CommitPlaceReplace { value, .. }
            if matches!(main.values[value.0 as usize], Type::Enum { .. }))
    }));
    assert!(
        !main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| { matches!(i.kind, InstructionKind::WriteAddress { .. }) })
    );
    verify_both(&module, &plan);
}

#[test]
fn g135_enum_verify_real_empty_and_scalar_arms_in_both_directions() {
    for scalar in [false, true] {
        let (arm, make, pattern, result) = if scalar {
            ("Vacant(i32)", "E.Vacant(77)", "E.Vacant(n)", "n")
        } else {
            ("Vacant", "E.Vacant()", "E.Vacant()", "77")
        };
        let text = format!(
            "resource R{{code:i32}}enum E{{Own(manages R,i32),{arm}}}fn work(old:bool,new:bool)->i32{{var packet=if old{{E.Own(R{{code:65}},1)}}else{{{make}}};let target=&mut packet;*target=if new{{E.Own(R{{code:66}},2)}}else{{{make}}};match packet{{E.Own(r,n)=>{{return r.code+n}},{pattern}=>{{return {result}}}}}}}fn main()->i32{{return work(false,true)}}"
        );
        let (module, plan) = candidate(&text);
        verify_both(&module, &plan);
    }
}

const ORIGIN_HEADER: &str = "resource R{code:i32}enum E{Left(manages R,i32),Right(manages R,i32)}";

#[test]
fn g135_enum_verify_actual_rhs_origin_in_both_directions() {
    // Runtime choice keeps both actual RHS arms, so the ordinary match does
    // not rely on an unreachable-arm precision shortcut.
    let fresh = format!(
        "{ORIGIN_HEADER}fn fresh(flag:bool,input:E)->R contract{{result(create)}}{{var packet=input;let target=&mut packet;*target=if flag{{E.Left(R{{code:42}},1)}}else{{E.Right(R{{code:42}},2)}};match packet{{E.Left(r,n)=>{{return give r}},E.Right(r,n)=>{{return give r}}}}}}fn main()->i32{{let r=fresh(false,E.Left(R{{code:65}},0));return r.code}}"
    );
    let (module, plan) = candidate(&fresh);
    verify_both(&module, &plan);
    let mut wrong_tag = module.clone();
    let f = wrong_tag
        .functions
        .iter_mut()
        .find(|f| f.name == "fresh")
        .unwrap();
    let unpack = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| {
            if let InstructionKind::UnpackVariant { variant, .. } = &mut i.kind {
                Some(variant)
            } else {
                None
            }
        })
        .unwrap();
    *unpack ^= 1;
    // Both declared arms have exactly equal payload types; only the actual
    // successful tag edge can justify this Unpack after the replacement.
    reject_both(&wrong_tag, &plan, "E-VERIFY-SUM", "selected tag");

    let incoming = format!(
        "{ORIGIN_HEADER}fn incoming(flag:bool,give input:R)->R contract{{result(give)}}{{var packet=E.Left(R{{code:65}},0);let target=&mut packet;*target=if flag{{E.Left(give input,1)}}else{{E.Right(give input,2)}};match packet{{E.Left(r,n)=>{{return give r}},E.Right(r,n)=>{{return give r}}}}}}fn main()->i32{{let r=incoming(false,R{{code:42}});return r.code}}"
    );
    let (mut module, plan) = candidate(&incoming);
    verify_both(&module, &plan);
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "incoming")
        .unwrap()
        .result_mode = PassingMode::Create;
    reject_both(&module, &plan, "E-VERIFY-RESOURCE", "fresh");
}

#[test]
fn g135_enum_verify_actual_selector_requires_single_root() {
    for same in [true, false] {
        let other = if same { "x" } else { "y" };
        let text = format!(
            "{ORIGIN_HEADER}fn choose(flag:bool)->i32{{var x=E.Left(R{{code:65}},1);var y=E.Right(R{{code:66}},2);let target=if flag{{&mut x}}else{{&mut {other}}};*target=E.Right(R{{code:67}},3);return 42}}fn main()->i32{{return choose(false)}}"
        );
        let module = typed(&text);
        if same {
            let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
            verify_both(&module, &plan);
            continue;
        }
        assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
        // Same actual CFG and honest multi-root Holds; a direct-x reserve
        // supplies a legal candidate without asking Analyze to authorize the
        // address-selected raw candidate this independent kernel must reject.
        let mut direct = module.clone();
        let f = direct
            .functions
            .iter_mut()
            .find(|f| f.name == "choose")
            .unwrap();
        let x = f.locals.iter().find(|l| l.name == "x").unwrap().id;
        let reserve = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
            .unwrap();
        let InstructionKind::ReservePlaceAddress { reservation, .. } = reserve.kind else {
            unreachable!()
        };
        reserve.kind = InstructionKind::ReservePlaceLocal {
            reservation,
            local: x,
            path: vec![],
        };
        let plan = zil_analyze::analyze(&direct, &Limits::default()).unwrap();
        verify_both(&direct, &plan);
        reject_both(
            &module,
            &plan,
            "E-UNSUPPORTED",
            "one independently reconstructed actual target",
        );
    }
}

#[test]
fn g135_enum_verify_commit_retires_actual_old_tag_in_both_routes() {
    let text = "resource R{code:i32}enum E{Left(manages R),Right(manages R)}fn choose(old:bool,new:bool)->i32{var packet=if old{E.Left(R{code:65})}else{E.Right(R{code:66})};let target=&mut packet;*target=if new{E.Left(R{code:67})}else{E.Right(R{code:68})};match packet{E.Left(ref r)=>{return r.code},E.Right(ref r)=>{return r.code}}}fn main()->i32{return choose(false,true)}";
    let (module, plan) = candidate(text);
    verify_both(&module, &plan);
    for variant_route in [false, true] {
        let mut raw = module.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "choose")
            .unwrap();
        let root = f.locals.iter().find(|l| l.name == "packet").unwrap().id;
        let (block, position) = f.blocks.iter().enumerate().find_map(|(b, block)| {
            block.instructions.iter().position(|i| {
                matches!(&i.kind, InstructionKind::PlaceTagLocal { local, path } if *local == root && path.is_empty())
            }).map(|i| (b, i))
        }).expect("real ref-pattern uses actual generic tag observation");
        if variant_route {
            f.blocks[block].instructions[position].kind = InstructionKind::VariantTagLocal {
                local: root,
                path: vec![],
            };
        }
        // Equivalent specialized raw route is a separate layer; the original
        // generic route above is produced by this actual Source.
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        verify_both(&raw, &claim);
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "choose")
            .unwrap();
        let tag = f.blocks[block].instructions.remove(position);
        let (before, index) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block.instructions.iter().position(|i| {
                matches!(i.kind, InstructionKind::BorrowLocalMut { local } if local == root)
            }).map(|i| (b, i))
            })
            .unwrap();
        // Both old and new tags depend on independent runtime booleans. Move
        // only the real i32 observation before preparation, leaving its use
        // after Commit: layout-equal payloads do not restore the old epoch.
        f.blocks[before].instructions.insert(index, tag);
        reject_both(&raw, &plan, "E-VERIFY-SUM", "current");
    }
}

#[test]
fn g135_enum_verify_old_root_cannot_be_consumed_while_prepared() {
    let text = format!(
        "{ORIGIN_HEADER}fn pass(value:E)->E{{return value}}fn main()->i32{{var packet=E.Left(R{{code:65}},1);let spare=E.Right(R{{code:66}},2);let target=&mut packet;*target=pass(spare);return 42}}"
    );
    let (mut module, plan) = candidate(&text);
    verify_both(&module, &plan);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let root = f.locals.iter().find(|l| l.name == "packet").unwrap().id;
    let spare = f.locals.iter().find(|l| l.name == "spare").unwrap().id;
    let take = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::TakeLocal { local } if local == spare))
        .unwrap();
    take.kind = InstructionKind::TakeLocal { local: root };
    // The actual retained pin rejects before Commit's complete-old check.
    reject_both(&module, &plan, "E-BORROW-REPLACEMENT-PIN", "");

    let leaf = format!(
        "{ORIGIN_HEADER}fn main()->i32{{var packet=E.Left(R{{code:65}},1);let target=&mut packet;*target=E.Right(R{{code:67}},3);return 42}}"
    );
    let (mut module, plan) = candidate(&leaf);
    verify_both(&module, &plan);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let root = f.locals.iter().find(|l| l.name == "packet").unwrap().id;
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
        })
        .unwrap();
    let reserve = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
        .unwrap();
    let leaf = block.instructions[reserve + 1..]
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
        .unwrap();
    leaf.kind = InstructionKind::TakeVariantField {
        local: root,
        path: vec![zil_ir::PlaceProjection::Variant(
            zil_ir::VariantProjection {
                variant: 0,
                index: 0,
            },
        )],
    };
    reject_both(&module, &plan, "E-BORROW-REPLACEMENT-PIN", "");
}

#[test]
fn g135_enum_verify_known_arm_create_boundary_is_also_ordinary() {
    // Existing ordinary Sum flow visits the CFG's impossible Left arm and
    // supplies Unknown for its absent payload. Do not call this a new RHS
    // freshness failure or weaken freshness to turn the probe into a positive.
    let ordinary = format!(
        "{ORIGIN_HEADER}fn fresh()->R contract{{result(create)}}{{let value=E.Right(R{{code:42}},2);match value{{E.Left(r,n)=>{{return give r}},E.Right(r,n)=>{{return give r}}}}}}fn main()->i32{{let r=fresh();return r.code}}"
    );
    let (module, plan) = candidate(&ordinary);
    reject_both(&module, &plan, "E-VERIFY-RESOURCE", "fresh");
}

#[test]
fn g135_enum_verify_raw_weak_payload_desc_route_is_closed() {
    let text = "resource R{code:i32}enum E{Left(manages R),Right(manages R)}fn choose(flag:bool)->i32{var packet=if flag{E.Left(R{code:65})}else{E.Right(R{code:66})};match packet{E.Left(ref r)=>{let seen=r.code;let target=&mut packet;*target=E.Right(R{code:67});return seen},E.Right(ref r)=>{let seen=r.code;let target=&mut packet;*target=E.Left(R{code:68});return seen}}}fn main()->i32{return choose(false)}";
    let (mut module, mut plan) = candidate(text);
    verify_both(&module, &plan);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "choose")
        .unwrap();
    let mut readers = Vec::new();
    for block in &mut f.blocks {
        let Some(read) = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadAddress { .. }))
        else {
            continue;
        };
        let InstructionKind::ReadAddress { address: field } = block.instructions[read].kind else {
            unreachable!()
        };
        let head = block
            .instructions
            .iter()
            .find_map(|i| {
                if i.result == Some(field)
                    && let InstructionKind::AddressField { address, index: 0 } = i.kind
                {
                    return Some(address);
                }
                None
            })
            .unwrap();
        let Type::Address { inner, .. } = &f.values[head.0 as usize] else {
            unreachable!()
        };
        let Type::Managed(payload) = inner.as_ref() else {
            unreachable!()
        };
        let desc = zil_ir::ValueId(f.values.len() as u32);
        let desc_ty = Type::Desc(payload.clone());
        f.values.push(desc_ty);
        let span = block.instructions[read].span;
        block.instructions[read].kind = InstructionKind::ReadDesc { desc, index: 0 };
        block.instructions.insert(
            read,
            zil_ir::Instruction {
                result: Some(desc),
                kind: InstructionKind::DescribeAddress { address: head },
                span,
            },
        );
        let holds = &mut plan.functions[f.id.0 as usize].holds;
        let roots = holds[head.0 as usize].roots.clone();
        holds.push(zil_ir::ValueHold { value: desc, roots });
        readers.push(block.id);
    }
    assert_eq!(readers.len(), 2);
    plan.module_fingerprint = module.fingerprint();
    // A raw weak descriptor route is not produced by the original ref-source.
    // This route is already ungrounded before Commit in the resource kernel;
    // it is not evidence that the later retirement check was reached.
    reject_both(
        &module,
        &plan,
        "E-VERIFY-RESOURCE",
        "no independently grounded source",
    );
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "choose")
        .unwrap();
    for id in readers {
        let block = &mut f.blocks[id.0 as usize];
        let read = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
            .unwrap();
        let value = block.instructions[read].result.unwrap();
        let store = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::Store { value: v, .. } if v == value))
            .unwrap();
        let moved: Vec<_> = block.instructions.drain(read..=store).collect();
        let commit = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            .unwrap();
        block.instructions.splice(commit + 1..commit + 1, moved);
    }
    // With the later consumer this raw descriptor retains an actual address
    // dependency. Its active window rejects earlier than resource retirement.
    reject_both(&module, &plan, "E-BORROW-CONFLICT", "active address window");
}
#[test]
fn g135_enum_verify_actual_abort_keeps_old_and_cleans_prepared_enum() {
    let (module, plan) = candidate(include_str!("replace-enum-terminal-failure.zil"));
    verify_both(&module, &plan);
    let work = module.functions.iter().find(|f| f.name == "work").unwrap();
    let root = work.locals.iter().find(|l| l.name == "value").unwrap().id;
    let error = work
        .blocks
        .iter()
        .position(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }))
        })
        .expect("actual failed-prepare edge aborts its reservation");
    let cleanup: Vec<_> = work.blocks[error]
        .instructions
        .iter()
        .filter_map(|i| {
            if let InstructionKind::DropEnum { local } = i.kind {
                Some(local)
            } else {
                None
            }
        })
        .collect();
    assert!(
        cleanup.len() >= 2 && cleanup[0] != root && cleanup.contains(&root),
        "{cleanup:?}"
    );
    let mut raw = module.clone();
    let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
    f.blocks[error]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }));
    reject_both(&raw, &plan, "E-VERIFY-PLACE-REPLACE", "leaks");
    let mut raw = module;
    let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
    let prepared = cleanup[0];
    f.blocks[error]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropEnum { local } if local == prepared));
    reject_both(&raw, &plan, "E-VERIFY-RESOURCE", "");
}

#[test]
fn g135_enum_verify_current_raw_wire_and_source_request_boundaries() {
    let text = include_str!("replace-enum-terminal-pending.zil");
    let (module, plan) = candidate(text);
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &defaults).unwrap();
    for wire in [false, true] {
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..defaults.clone()
            };
            let result = if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
            };
            match result {
                Ok(verified) => Some(verified.fingerprint()),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    None
                }
            }
        };
        let (mut low, mut high) = (1, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if run(mid).is_some() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert_eq!(low, 152_448, "independent Verify current work changed");
        let fingerprint = run(low).unwrap();
        for _ in 0..2 {
            assert!(run(low - 1).is_none());
            assert_eq!(run(low), Some(fingerprint));
        }
        eprintln!("G1.3.5.1 Verify wire={wire} work N={low} N-1 ResourceLimit, fresh/retry Pass");
    }

    let input = source(text);
    for (kind, maximum) in [
        ("work", defaults.max_analysis_steps),
        ("nodes", defaults.max_nodes),
        ("source_bytes", defaults.max_source_bytes),
        ("depth", defaults.max_call_depth),
    ] {
        let options = |n| {
            let mut limits = defaults.clone();
            match kind {
                "work" => limits.max_analysis_steps = n,
                "nodes" => limits.max_nodes = n,
                "source_bytes" => limits.max_source_bytes = n,
                _ => limits.max_call_depth = n,
            }
            CompileOptions {
                stop_after: Stage::Verify,
                limits,
                ..CompileOptions::default()
            }
        };
        let (mut low, mut high) = (1, maximum);
        while low < high {
            let mid = low + (high - low) / 2;
            match compile_source(&input, &options(mid)).status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                status => panic!("unexpected Source {kind} status {status:?}"),
            }
        }
        assert_eq!(
            low,
            match kind {
                "work" => 152_448,
                "nodes" => 349,
                "source_bytes" => 669,
                _ => 11,
            },
            "whole Source {kind} boundary changed"
        );
        let mut warm = zil_driver::CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                compile_source(&input, &options(n)),
                warm.compile_source(&input, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{kind}: {}", report.to_json());
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("G1.3.5.1 whole Source {kind} N={low} N-1 ResourceLimit fresh/warm/retry Pass");
    }

    let analyze_options = |work| CompileOptions {
        stop_after: Stage::Analyze,
        limits: Limits {
            max_analysis_steps: work,
            ..defaults.clone()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    while low < high {
        let mid = low + (high - low) / 2;
        let report = compile_source(&input, &analyze_options(mid));
        match report.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    // G1.3.6: unchanged Source/IR, previous111234 plus the15-step paid
    // producer exclusion inventory; no detailed nonlocal graph is built here.
    assert_eq!(low, 111_249, "current whole Enum Analyze work changed");
    let mut warm = zil_driver::CompilationSession::default();
    for (work, status) in [
        (111_249, ResultCategory::Success),
        (111_248, ResultCategory::ResourceLimit),
        (111_249, ResultCategory::Success),
    ] {
        for report in [
            compile_source(&input, &analyze_options(work)),
            warm.compile_source(&input, &analyze_options(work)),
        ] {
            assert_eq!(report.status, status, "{}", report.to_json());
            if status == ResultCategory::ResourceLimit {
                assert_eq!(report.phase, Phase::Analyze);
            }
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    eprintln!("G1.3.6 current whole Enum Analyze N={low} N-1/fresh/warm/retry Pass");
}
