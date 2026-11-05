//! Source/Typed witnesses for independent may-retire checking, not must-write.
use super::*;
use zil_ir::{CandidatePlan, TypedModule};

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn verify(module: &TypedModule, plan: &CandidatePlan, error: Option<&str>) {
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut claim = plan.clone();
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(module, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), claim, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        if let Some(message) = error {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == "E-VERIFY-RESOURCE" && d.message.contains(message)),
                "{errors:?}"
            );
        } else {
            result.unwrap();
        }
    }
}
fn positive(text: &str) -> (TypedModule, CandidatePlan) {
    let module = typed(text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    verify(&module, &plan, None);
    (module, plan)
}

/// Preserve the exact bad Source's SSA/CFG and ownership operations. A pure
/// descriptor read hoisted before the call supplies an admissible candidate,
/// independent of whether Analyze already rejects the unsafe original Source.
fn stale_source(text: &str, entry: &str, local_commit: bool) {
    stale_source_in(text, entry, "main", local_commit);
}
fn stale_source_in(text: &str, entry: &str, caller: &str, local_commit: bool) {
    let module = typed(text);
    let callee = module
        .functions
        .iter()
        .find(|f| f.name == entry)
        .unwrap()
        .id;
    let mut before = module.clone();
    let f = before
        .functions
        .iter_mut()
        .find(|f| f.name == caller)
        .unwrap();
    let (read_block, read) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    let InstructionKind::ReadDesc { desc, .. } = f.blocks[read_block].instructions[read].kind
    else {
        unreachable!()
    };
    let start = f.blocks[read_block]
        .instructions
        .iter()
        .position(|i| i.result == Some(desc))
        .unwrap();
    let moved: Vec<_> = f.blocks[read_block]
        .instructions
        .drain(start..=read)
        .collect();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions.iter().any(|i| {
                if local_commit {
                    matches!(i.kind, InstructionKind::CommitPlaceReplace { .. })
                } else {
                    matches!(i.kind,InstructionKind::Call{function,..} if function==callee)
                }
            })
        })
        .unwrap();
    let retire = block
        .instructions
        .iter()
        .position(|i| {
            if local_commit {
                matches!(i.kind, InstructionKind::CommitPlaceReplace { .. })
            } else {
                matches!(i.kind,InstructionKind::Call{function,..} if function==callee)
            }
        })
        .unwrap();
    let before_parent = block.instructions[..retire]
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { .. }))
        .unwrap_or(retire);
    block
        .instructions
        .splice(before_parent..before_parent, moved);
    let plan = zil_analyze::analyze(&before, &Limits::default()).unwrap();
    verify(&before, &plan, None);
    verify(&module, &plan, Some("invalidate"));
}

#[test]
fn g1361_verify_nonlocal_desc_alias_and_read_before_retirement() {
    let prefix = format!("{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}");
    let tail = "fn main()->i32{return 42}";
    let after = format!(
        "{prefix}fn relay(old:R,target:&mut P)->i32{{reset(target);return old.code}}{tail}"
    );
    stale_source_in(&after, "reset", "relay", false);
    let before = format!(
        "{prefix}fn relay(old:R,target:&mut P)->i32{{let seen=old.code;reset(target);return seen}}{tail}"
    );
    positive(&before);
    // These caller roots really are separate, but the independently checked
    // callee header supplies no resource-Desc/apart relation between inputs.
    // The safety fix therefore has a measured conservative acceptance delta.
    let apart = include_str!("helper-retirement-nonlocal-apart.zil");
    let module = typed(apart);
    let relay = module.functions.iter().find(|f| f.name == "relay").unwrap();
    eprintln!(
        "G1.3.6.1 apart relay header storage_life={:?} reference_return_bound={:?} callable_contract={:?}",
        relay.storage_life, relay.reference_return_bound, relay.callable_contract
    );
    stale_source_in(apart, "reset", "relay", false);
    let report = compile_source(&source(apart), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    eprintln!(
        "G1.3.6.1 apart caller Source actual new rejection: {}",
        report.to_json()
    );
}

#[test]
fn g1361_verify_live_desc_closure_and_address_closure_carriers() {
    for borrowed in [false, true] {
        let save = if borrowed {
            "let saved=()->i32=>old.code;let borrowed=&saved;let seen=borrowed();"
        } else {
            "let saved=()->i32=>old.code;let seen=saved();"
        };
        let text = format!(
            "{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};let old=packet.owner;{save}reset(&mut packet);return seen}}"
        );
        let (mut module, plan) = positive(&text);
        let reset = module
            .functions
            .iter()
            .find(|f| f.name == "reset")
            .unwrap()
            .id;
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let block = &mut f.blocks[0];
        let call = block
            .instructions
            .iter()
            .position(|i| {
                matches!(
                    i.kind,
                    InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureBorrowed { .. }
                )
            })
            .unwrap();
        let output = block.instructions[call].result.unwrap();
        let store = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind,InstructionKind::Store{value,..} if value==output))
            .unwrap();
        let moved: Vec<_> = block.instructions.drain(call..=store).collect();
        let retire = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind,InstructionKind::Call{function,..} if function==reset))
            .unwrap();
        block.instructions.splice(retire + 1..retire + 1, moved);
        verify(&module, &plan, Some("invalidate"));
    }
}

#[test]
fn g1361_verify_real_helper_and_inline_retire_the_same_old_desc() {
    stale_source(
        include_str!("helper-retirement-old-desc.zil"),
        "reset",
        false,
    );
    stale_source(
        include_str!("helper-retirement-local-counterpart.zil"),
        "reset",
        true,
    );
}

const PREFIX: &str = "resource R{code:i32}package P{manages owner:R;n:i32}";

#[test]
fn g1361_verify_readonly_and_scalar_neighbor_helpers_do_not_retire() {
    for body in ["let n=target.n;", "target.n=7;"] {
        positive(&format!(
            "{PREFIX}fn reset(target:&mut P){{{body}}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};let old=packet.owner;reset(&mut packet);return old.code}}"
        ));
    }
    positive(&format!(
        "{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:42}};let route=&mut packet;reset(route);return route.n}}"
    ));
    let (module, _) = positive(&format!(
        "{PREFIX}fn observe(input:R)->i32{{return input.code}}fn marker(head:&R)->i32{{return observe(head)}}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:42}};let route=&mut packet;reset(route);return route.n}}"
    ));
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::DescribeAddress { .. }))
    );
}

#[test]
fn g1361_verify_current_raw_wire_and_source_limits() {
    let text = include_str!("helper-retirement-old-desc.zil")
        .replace(
            "    reset(&mut packet);",
            "    let seen=old.code;\n    reset(&mut packet);",
        )
        .replace("    return old.code;", "    return seen;");
    let (module, plan) = positive(&text);
    let defaults = Limits::default();
    let target = TargetProfile::default();
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
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert_eq!(low, 20_866, "Verify current request work changed");
        let fp = run(low).unwrap();
        for _ in 0..2 {
            assert!(run(low - 1).is_none());
            assert_eq!(run(low), Some(fp));
        }
        eprintln!("G1.3.6.1 Verify wire={wire} work N={low} N-1 ResourceLimit fresh/retry Pass");
    }
    let input = source(&text);
    for (kind, max) in [
        ("work", defaults.max_analysis_steps),
        ("nodes", defaults.max_nodes),
        ("bytes", defaults.max_source_bytes),
        ("depth", defaults.max_call_depth),
    ] {
        let options = |n| {
            let mut limits = defaults.clone();
            match kind {
                "work" => limits.max_analysis_steps = n,
                "nodes" => limits.max_nodes = n,
                "bytes" => limits.max_source_bytes = n,
                _ => limits.max_call_depth = n,
            }
            CompileOptions {
                stop_after: Stage::Verify,
                limits,
                ..CompileOptions::default()
            }
        };
        let (mut low, mut high) = (1, max);
        while low < high {
            let mid = low + (high - low) / 2;
            match compile_source(&input, &options(mid)).status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                status => panic!("{kind} {status:?}"),
            }
        }
        assert_eq!(
            low,
            match kind {
                "work" => 20_866,
                "nodes" => 173,
                "bytes" => 348,
                _ => 10,
            },
            "Source {kind} current boundary changed"
        );
        let mut warm = zil_driver::CompilationSession::default();
        for (n, status) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                compile_source(&input, &options(n)),
                warm.compile_source(&input, &options(n)),
            ] {
                assert_eq!(report.status, status, "{kind}: {}", report.to_json());
                if status == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("G1.3.6.1 Source {kind} N={low} N-1 ResourceLimit fresh/warm/retry Pass");
    }
}

#[test]
fn g1361_verify_no_commit_failure_and_may_origin_remain_distinct() {
    let none = format!(
        "{PREFIX}enum F{{Bad}}fn reset(target:&mut P)->F!i32{{return F.Bad()}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};let old=packet.owner;match reset(&mut packet){{.ok(n)=>{{return old.code+n}},.err(e)=>{{return old.code}}}}}}"
    );
    positive(&none);
    // The actual IR has an Abort on the failed prepare edge and a possible
    // Commit on the success edge. No exit-sensitive postcondition is added.
    let possible = format!(
        "{PREFIX}enum F{{Bad}}fn fail()->F!i32{{return F.Bad()}}fn reset(target:&mut P)->F!i32{{target.owner=R{{code:fail()?}};return 0}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};let old=packet.owner;match reset(&mut packet){{.ok(n)=>{{return n}},.err(e)=>{{return old.code}}}}}}"
    );
    stale_source(&possible, "reset", false);

    let given = format!(
        "{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn make(input:P)->R contract{{result(give)}}{{var packet=input;reset(&mut packet);return give packet.owner}}fn main()->i32{{let r=make(P{{owner:R{{code:65}},n:0}});return r.code}}"
    );
    let (mut module, plan) = positive(&given);
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "make")
        .unwrap()
        .result_mode = zil_ir::PassingMode::Create;
    verify(&module, &plan, Some("fresh"));
}

#[test]
fn g1361_verify_nested_conditional_and_parameter_ordinal_are_real() {
    for (helpers, call) in [
        (
            "fn reset(target:&mut P){target.owner=R{code:66};}fn relay(spare:&mut P,target:&mut P){reset(target);}",
            "relay(&mut other,&mut packet)",
        ),
        (
            "fn reset(flag:bool,target:&mut P){if flag{target.owner=R{code:66};}}",
            "reset(false,&mut packet)",
        ),
        (
            "fn reset(target:&mut P){target.owner=R{code:66};}fn relay(target:&mut P){reset(target);}fn outer(target:&mut P){relay(target);}",
            "outer(&mut packet)",
        ),
    ] {
        let entry = if call.starts_with("outer") {
            "outer"
        } else if call.starts_with("relay") {
            "relay"
        } else {
            "reset"
        };
        let text = format!(
            "{PREFIX}{helpers}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};var other=P{{owner:R{{code:67}},n:1}};let old=packet.owner;{call};return old.code}}"
        );
        stale_source(&text, entry, false);
    }
    // Input1, not Input0, is retired through this forwarding helper.
    positive(&format!(
        "{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn relay(spare:&mut P,target:&mut P){{reset(target);}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};var other=P{{owner:R{{code:67}},n:1}};let old=packet.owner;relay(&mut packet,&mut other);return old.code}}"
    ));
}

#[test]
fn g1361_verify_actual_other_root_and_sibling_owner_are_not_retired() {
    positive(
        "resource R{code:i32}package P{manages first:R;manages second:R;n:i32}fn reset(target:&mut P){target.first=R{code:67};}fn main()->i32{var packet=P{first:R{code:65},second:R{code:66},n:0};let old=packet.second;reset(&mut packet);return old.code}",
    );
    let text = format!(
        "{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};var other=P{{owner:R{{code:67}},n:1}};let old=packet.owner;let left=&mut packet;let right=&mut other;reset(right);return old.code}}"
    );
    let (mut module, plan) = positive(&text);
    let reset = module
        .functions
        .iter()
        .find(|f| f.name == "reset")
        .unwrap()
        .id;
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let packet = f.locals.iter().find(|l| l.name == "packet").unwrap().id;
    let other = f.locals.iter().find(|l| l.name == "other").unwrap().id;
    let borrow = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { local } if local == other))
        .unwrap();
    borrow.kind = InstructionKind::BorrowLocalMut { local: packet };
    assert!(
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::Call{function,..} if function==reset))
    );
    let mut claim = plan;
    // Every actual address in this Source rooted at `other` comes from this
    // one borrow. Preserve honest coarse Holds along its stored/reborrow chain.
    for row in &mut claim.functions[f.id.0 as usize].holds {
        for root in &mut row.roots {
            if *root == zil_ir::HoldRoot::Local(other) {
                *root = zil_ir::HoldRoot::Local(packet);
            }
        }
        row.roots.sort();
        row.roots.dedup();
    }
    verify(&module, &claim, Some("invalidate"));
}
