use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{HoldRoot, InstructionKind, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn evidence(name: &str) -> Option<PathBuf> {
    std::env::var_os("ZIL_TEST_REPORT_DIR").map(|root| {
        let p = PathBuf::from(root).join(format!(
            "{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    })
}

const PRE: &str = "package P{manages b:ByteBuffer;manages c:ByteBuffer}fn make()->P{return P{b:bytes_new(),c:bytes_new()}}";

fn check(body: &str) -> zil_driver::CompileReport {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "field120.zil",
        format!("{PRE}{body}"),
    );
    let directory = evidence("source");
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: directory.as_ref().map(|p| p.join("phases")),
            ..CompileOptions::default()
        },
    );
    if let Some(directory) = directory {
        fs::write(directory.join("source.zil"), &source.text).unwrap();
        fs::write(directory.join("report.json"), report.to_json()).unwrap();
    }
    report
}
fn success(body: &str) {
    let report = check(body);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{body}\n{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn rejected(body: &str, phase: &str, code: &str) {
    let report = check(body);
    assert_eq!(report.phase.as_str(), phase, "{body}\n{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        code,
        "{body}\n{}",
        report.to_json()
    );
    assert_eq!(report.status, ResultCategory::SafeReject);
}
fn typed(body: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw120.zil", format!("{PRE}{body}"));
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn independent_field_take_preserves_a_live_sibling_window() {
    let report = check(
        "fn value()->i64{let p=make();let s=bytes_view(p.b);let old=give p.c;return s.len()+bytes_len(old)}",
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn independent_mutation_refill_and_two_exclusive_windows() {
    for body in [
        "fn value()->i64{var p=make();let s=bytes_view(p.b);let old=give p.c;p.c=bytes_new();return s.len()+bytes_len(old)+bytes_len(p.c)}",
        "fn value()->i64{var p=make();let s=bytes_view(p.b);p.c=bytes_new();return s.len()+bytes_len(p.c)}",
        "fn value()->i64{let p=make();let s=bytes_view(p.b);bytes_reserve(p.c,8);bytes_push(p.c,9u8);return s.len()+bytes_len(p.c)}",
        "fn value()->i64{let p=make();let s=bytes_view_mut(p.b);let t=bytes_view_mut(p.c);return s.len()+t.len()}",
        "fn value()->i64{let p=make();bytes_reserve(p.b,2);bytes_push(p.b,1u8);bytes_reserve(p.c,2);bytes_push(p.c,2u8);let s=bytes_view_mut(p.b);let t=bytes_view_mut(p.c);s[0]=7u8;t[0]=8u8;return i64(s[0])+i64(t[0])}",
    ] {
        success(body);
    }
}

#[test]
fn helper_returns_and_required_apart_substitute_actual_fields() {
    for body in [
        "fn view(b:ByteBuffer)->&[u8]{return bytes_view(b)}fn value()->i64{let p=make();let s=view(p.b);let c=give p.c;return s.len()+bytes_len(c)}",
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,8);return s.len()}fn value()->i64{let p=make();return act(p.b,p.c)}",
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,8);return s.len()}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn value()->i64{let p=make();return relay(p.b,p.c)}",
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);let n=s.len();bytes_reserve(b,8);return n}fn value()->i64{let p=make();return act(p.b,p.b)}",
    ] {
        success(body);
    }
    for body in [
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,8);return s.len()}fn value()->i64{let p=make();return act(p.b,p.b)}",
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,8);return s.len()}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn value()->i64{let p=make();return relay(p.c,p.c)}",
    ] {
        rejected(body, "verify", "E-BORROW-CONFLICT");
    }
}

#[test]
fn owning_fnmut_and_fnonce_keep_precise_fields() {
    for body in [
        "fn factory()->impl FnMut()->i64{var p=make();return move()=>{let s=bytes_view(p.b);p.c=bytes_new();bytes_reserve(p.c,4);return s.len()+bytes_len(p.c)}}fn value()->i64{var f=factory();return f()+f()}",
        "fn factory()->impl FnOnce()->i64{let p=make();return move()=>{let s=bytes_view(p.b);let c=give p.c;return s.len()+bytes_len(c)}}fn value()->i64{let f=factory();return f()}",
        "enum Fault{Stop}fn fail()->Fault!i64{return Fault.Stop}fn factory()->impl FnOnce()->Fault!i64{let p=make();return move()=>{let s=bytes_view(p.b);let c=give p.c;let n=s.len();return fail()?+n+bytes_len(c)}}fn value()->Fault!i64{let f=factory();return f()}",
    ] {
        success(body);
    }
}

#[test]
fn same_field_ancestor_and_replacement_do_not_revive_old_views() {
    for body in [
        "fn value()->i64{let p=make();let s=bytes_view(p.b);let c=give p.b;return s.len()+bytes_len(c)}",
        "fn value()->i64{var p=make();let s=bytes_view(p.b);p.b=bytes_new();return s.len()}",
        "fn value()->i64{let p=make();let s=bytes_view(p.b);bytes_reserve(p.b,8);return s.len()}",
        "fn value()->i64{let p=make();let s=bytes_view(p.b);bytes_push(p.b,9u8);return s.len()}",
        "fn value()->i64{let p=make();let s=bytes_view(p.b);let q=p;return s.len()+bytes_len(q.c)}",
        "fn value()->i64{let p=make();let s=bytes_view_mut(p.b);let t=bytes_view_mut(p.b);return s.len()+t.len()}",
        "fn value()->i64{let p=make();let s=bytes_view(p.b);let f=move()=>bytes_len(p.c);return s.len()+f()}",
        "fn factory()->impl FnOnce()->i64{let p=make();return move()=>{let s=bytes_view(p.b);let old=give p.b;return s.len()+bytes_len(old)}}fn value()->i64{let f=factory();return f()}",
    ] {
        rejected(body, "verify", "E-BORROW-CONFLICT");
    }
}

#[test]
fn branch_origins_preserve_all_possible_fields_and_finite_resource_siblings_are_apart() {
    rejected(
        "fn work(flag:bool)->i64{var p=make();let s=if flag{bytes_view(p.b)}else{bytes_view(p.c)};p.c=bytes_new();return s.len()}fn value()->i64{return work(true)}",
        "verify",
        "E-BORROW-CONFLICT",
    );
    success(
        "fn work(flag:bool)->i64{var p=make();let s=if flag{bytes_view(p.b)}else{bytes_view(p.b)};p.c=bytes_new();return s.len()}fn value()->i64{return work(true)}",
    );
    // 192 validates this nominal finite Copy payload independently and tracks
    // its actual field. Replacing r cannot invalidate the distinct b window.
    success(
        "resource R{x:i32}impl Drop for R{fn drop(self:Self){}}package Q{manages b:ByteBuffer;manages r:R}fn value()->i64{var p=Q{b:bytes_new(),r:R{x:0}};let s=bytes_view(p.b);p.r=R{x:1};return s.len()}",
    );
}

#[test]
fn public_hold_remains_coarse_and_raw_wire_cannot_forge_apartness() {
    let body = "fn value()->i64{let p=make();let s=bytes_view(p.b);let old=give p.c;return s.len()+bytes_len(old)}";
    let module = typed(body);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut field_claims = 0;
    for (function, facts) in module.functions.iter().zip(&plan.functions) {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let InstructionKind::DescribePackageField { local, .. } = instruction.kind {
                let claim = &facts.holds[instruction.result.unwrap().0 as usize];
                assert_eq!(claim.roots, vec![HoldRoot::Local(local)]);
                field_claims += 1;
            }
        }
    }
    assert!(field_claims > 0);
    let verify_pair = |module: TypedModule,
                       mut plan: zil_ir::CandidatePlan,
                       expected: Option<&str>,
                       name: &str| {
        // An attacker can recompute an untrusted candidate identity; check
        // the semantic gate too, rather than stopping at stale fingerprints.
        plan.module_fingerprint = module.fingerprint();
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
        if let Some(dir) = evidence(name) {
            fs::write(dir.join("candidate.bin"), &bytes).unwrap();
        }
        for (module, plan) in [(module, plan), decoded] {
            let result = zil_verify::verify_module(
                module,
                plan,
                &TargetProfile::default(),
                &Limits::default(),
            );
            if let Some(code) = expected {
                assert_eq!(result.unwrap_err()[0].code, code);
            } else {
                result.unwrap();
            }
        }
    };
    verify_pair(module.clone(), plan.clone(), None, "valid-raw-wire");
    let mut false_claim = plan.clone();
    let claim = false_claim
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.holds)
        .find(|h| !h.roots.is_empty())
        .unwrap();
    claim.roots.clear();
    verify_pair(
        module.clone(),
        false_claim,
        Some("E-VERIFY-HOLD"),
        "missing-hold",
    );
    let mut false_claim = plan.clone();
    let claim = false_claim
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.holds)
        .find(|h| !h.roots.is_empty())
        .unwrap();
    claim.roots = vec![HoldRoot::Parameter(0)];
    verify_pair(
        module.clone(),
        false_claim,
        Some("E-VERIFY-HOLD"),
        "fake-parameter-hold",
    );
    let mut changed = module.clone();
    for instruction in changed
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::TakePackageField { index, .. } = &mut instruction.kind {
            *index = 0;
        }
    }
    verify_pair(
        changed,
        plan.clone(),
        Some("E-BORROW-CONFLICT"),
        "take-live-field",
    );
    let mut changed = module;
    for instruction in changed
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::DescribePackageField { index, .. } = &mut instruction.kind {
            *index = 1;
        }
    }
    verify_pair(
        changed,
        plan,
        Some("E-BORROW-CONFLICT"),
        "describe-taken-field",
    );
}

#[test]
fn changed_root_work_remains_bounded_at_exact_analysis_and_verify_limits() {
    let module =
        typed("fn value()->i64{var p=make();let s=bytes_view(p.b);p.c=bytes_new();return s.len()}");
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut boundaries = Vec::new();
    for analyze in [true, false] {
        let works = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if analyze {
                zil_analyze::analyze(&module, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category)
            } else {
                zil_verify::verify_module(
                    module.clone(),
                    plan.clone(),
                    &TargetProfile::default(),
                    &limits,
                )
                .map(|_| ())
                .map_err(|e| e[0].category)
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert!(works(high).is_ok());
        while low + 1 < high {
            let mid = (low + high) / 2;
            if works(mid).is_ok() {
                high = mid
            } else {
                low = mid
            }
        }
        assert_eq!(works(low), Err(ResultCategory::ResourceLimit));
        assert!(works(high).is_ok());
        boundaries.push((analyze, high, low));
    }
    if let Some(dir) = evidence("budget-boundaries") {
        fs::write(dir.join("boundaries.txt"), format!("{boundaries:?}\n")).unwrap();
    }
}
