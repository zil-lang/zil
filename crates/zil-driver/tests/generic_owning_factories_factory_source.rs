//! Analyze controls derived from actual generic owning-factory source. Raw
//! mutations below preserve the original source label but are not source cases.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

const BASE: &str = "resource Owner{n:i32} fn base(give owner:Owner)->impl Fn(i32)->i32{return move(x:i32)=>owner.n+x}";
fn source(kind: &str) -> String {
    let binding = if kind == "FnMut" {
        "var inner=f;"
    } else {
        "let inner=f;"
    };
    format!(
        "{BASE} fn wrap<F:{kind}(i32)->i32>(f:F,bias:i32)->impl {kind}(i32)->i32{{{binding}return move(x:i32)=>inner(x)+bias}} fn main()->i32{{var result=wrap(base(Owner{{n:2}}),3);return result(4)}}"
    )
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "factory220-source.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let root = PathBuf::from(root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join(format!("source-{}.zil", module.source_fingerprint)),
            text,
        )
        .unwrap();
    }
    module
}
fn factory(module: &TypedModule) -> usize {
    module
        .functions
        .iter()
        .position(|f| {
            f.callable_return_contract.is_some()
                && f.parameters
                    .iter()
                    .any(|p| matches!(f.locals[p.0 as usize].ty, Type::Closure(_)))
        })
        .unwrap()
}
fn record(module: &TypedModule, bytes: &[u8], result: &str) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") else {
        return;
    };
    let path = PathBuf::from(root).join(format!(
        "analyze220-source-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("candidate.wire"), bytes).unwrap();
    fs::write(path.join("result.txt"), result).unwrap();
}
fn both(module: &TypedModule, rejection: Option<&str>) {
    let limits = Limits::default();
    // A fresh public candidate means semantic rejection cannot be explained
    // by an intentionally stale module fingerprint after a mutation.
    let bytes =
        wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits).unwrap();
    let (decoded, _) = wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(module, &decoded);
    for input in [module, &decoded] {
        let result = zil_analyze::analyze(input, &limits);
        record(input, &bytes, &format!("{result:#?}"));
        if let Some(message) = rejection {
            let errors = result
                .expect_err("source-derived malformed ownership must be rejected independently");
            assert!(
                errors
                    .iter()
                    .all(|d| d.phase == Phase::Analyze
                        && d.category != ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
            assert!(
                errors.iter().any(|d| d.message.contains(message)),
                "wanted {message:?}: {errors:#?}"
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn source_owned_modes_pass_the_independent_analyze_boundary() {
    for kind in ["Fn", "FnMut", "FnOnce"] {
        let module = typed(&source(kind));
        let f = &module.functions[factory(&module)];
        assert!(f.callable_contract.is_none());
        assert_ne!(f.locals[f.parameters[0].0 as usize].ty, f.return_type);
        both(&module, None);
    }
}

#[test]
fn source_factory_return_claims_are_not_trusted() {
    let good = typed(&source("FnOnce"));
    let i = factory(&good);
    let mut missing = good.clone();
    missing.functions[i].callable_return_contract = None;
    both(&missing, Some("no public callable return contract"));
    let mut cleanup = good.clone();
    cleanup.functions[i]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    both(&cleanup, Some("cleanup differs"));
    let mut borrowed = good.clone();
    let p = borrowed.functions[i].parameters[0].0 as usize;
    borrowed.functions[i].locals[p].ty = Type::Address {
        mutable: false,
        inner: Box::new(borrowed.functions[i].locals[p].ty.clone()),
    };
    both(&borrowed, Some("ordinary self-contained factory"));
    let mut fake_helper = good;
    fake_helper.functions[i].callable_contract = Some(CallableParameterContract {
        roles: vec![],
        traces: CallableTraces::Finite(vec![]),
    });
    both(&fake_helper, Some("ordinary self-contained factory"));
}

#[test]
fn source_transfer_cannot_be_repeated_or_replaced_with_a_copy() {
    let good = typed(&source("FnOnce"));
    let index = factory(&good);
    let mut twice = good.clone();
    let f = &mut twice.functions[index];
    let parameter = f.parameters[0];
    let new = ValueId(f.values.len() as u32);
    f.values.push(f.locals[parameter.0 as usize].ty.clone());
    let (block, position) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(
                    |i| matches!(i.kind,InstructionKind::TakeLocal{local} if local==parameter),
                )
                .map(|p| (b, p))
        })
        .unwrap();
    let mut duplicate = f.blocks[block].instructions[position].clone();
    duplicate.result = Some(new);
    f.blocks[block].instructions.insert(position + 1, duplicate);
    both(&twice, Some("not initialized"));

    let mut copied = good;
    let closure = copied.functions[index]
        .callable_return_contract
        .as_ref()
        .unwrap()
        .closure;
    let entry = copied.closures[closure.0 as usize].function.0 as usize;
    let instruction = copied.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
        .unwrap();
    let InstructionKind::TakeCapture { index } = instruction.kind else {
        unreachable!()
    };
    instruction.kind = InstructionKind::ReadCapture { index };
    both(&copied, Some("ReadCapture cannot copy"));
}

#[test]
fn source_input_capture_graph_must_remain_self_contained() {
    let mut module = typed(&source("FnOnce"));
    let f = &module.functions[factory(&module)];
    let Type::Closure(id) = f.locals[f.parameters[0].0 as usize].ty else {
        unreachable!()
    };
    let capture = module.closures[id.0 as usize]
        .captures
        .iter_mut()
        .find(|ty| matches!(ty, Type::Managed(_)))
        .unwrap();
    let Type::Managed(payload) = capture else {
        unreachable!()
    };
    *capture = Type::Desc(payload.clone());
    // The source also contains base's ordinary public factory contract. Its
    // earlier check correctly rejects the now-external capture before wrap's
    // transitive graph check; the synthetic control isolates that later check.
    both(&module, Some("captures an external address or description"));
}

#[test]
fn source_owning_factory_has_an_exact_replayable_analysis_limit() {
    let module = typed(&source("FnOnce"));
    let mut lo = 0;
    let mut hi = Limits::default().max_analysis_steps;
    both(&module, None);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: mid,
                ..Limits::default()
            },
        ) {
            Ok(_) => hi = mid,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                lo = mid + 1;
            }
        }
    }
    let n = lo;
    assert!(n > 0);
    for steps in [n, n - 1, n] {
        let result = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        );
        if steps == n {
            result.unwrap();
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit)
            );
        }
    }
    eprintln!("source-derived Analyze220 N={n}");
}
