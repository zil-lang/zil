//! Source receiver classification, independent Analyze boundaries, and budgets.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::*;

const DEFINITIONS: &str = "resource R{n:i32} package P{manages a:R,manages b:R,n:i32} fn make()->P{return P{a:R{n:20},b:R{n:21},n:1}} fn consume(give r:R)->i32{return r.n} fn keep(give r:R,n:i32)->R contract{result(give)}{return give r} fn step(ok:bool)->bool!i32{if ok{return 0}return true} fn bump(r:R){r.n+=1}";

fn unit(body: &str) -> SourceUnit {
    SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "captures077.zil",
        format!("{DEFINITIONS} {body}"),
    )
}
fn core(body: &str) -> CoreModule {
    let source = unit(body);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &syntax).unwrap()
}
fn typed(body: &str) -> TypedModule {
    zil_types::check(&core(body), &TargetProfile::default(), &Limits::default()).unwrap()
}
fn checked(body: &str, kind: CallableKind) -> TypedModule {
    let module = typed(body);
    assert_eq!(module.closures.len(), 1, "{body}");
    assert_eq!(module.closures[0].kind, kind, "{body}");
    zil_analyze::analyze(&module, &Limits::default())
        .unwrap_or_else(|errors| panic!("{body}\n{errors:?}"));
    let report = compile_source(
        &unit(body),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{body}\n{}",
        report.to_json()
    );
    module
}
fn rejected(body: &str, phase: Phase) {
    let report = compile_source(
        &unit(body),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_ne!(report.status, ResultCategory::Success, "accepted {body}");
    assert!(
        report.diagnostics.iter().any(|error| error.phase == phase),
        "{body}\n{}",
        report.to_json()
    );
}

const READER: &str = "fn factory(p:P)->impl Fn()->i32{return move ()=>p.a.n+p.n}fn main()->i32{let f=factory(make());let moved=f;return moved()+moved()-42}";

#[test]
fn captures077_factory_owns_package_and_shared_calls_do_not_copy_it() {
    let module = checked(READER, CallableKind::Fn);
    let factory = module
        .functions
        .iter()
        .find(|f| f.name == "factory")
        .unwrap();
    assert_eq!(
        factory.callable_return_contract.as_ref().unwrap().cleanup,
        vec![0]
    );
    let entry = &module.functions[module.closures[0].function.0 as usize];
    let binding = &entry.blocks[entry.entry.0 as usize].instructions[1];
    let InstructionKind::BindPackageCapture { local, index: 0 } = binding.kind else {
        panic!("{binding:?}")
    };
    assert!(entry.blocks.iter().flat_map(|block| &block.instructions).any(|i| matches!(i.kind, InstructionKind::EndPackageCapture { local: ended, index: 0 } if ended == local)));
    assert!(!entry.blocks.iter().flat_map(|block| &block.instructions).any(|i| matches!(i.kind, InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) | InstructionKind::DropPackage { local: l } if l == local)));
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|block| &block.instructions)
            .any(|i| matches!(i.kind, InstructionKind::CallClosure { .. }))
    );
}

#[test]
fn captures077_copy_payload_and_descriptor_helper_writes_are_fnmut() {
    for body in [
        "fn main()->i32{var p=make();var f=move ()=>{p.n+=1;return p.n};return f()+f()-5}",
        "fn main()->i32{let p=make();var f=move ()=>{p.a.n+=1;return p.a.n};return f()+f()-43}",
        "fn main()->i32{let p=make();var f=move ()=>{let d=p.a;bump(d);return d.n};return f()+f()-43}",
        "fn main()->i32{let p=make();var f=move ()=>{bump(p.a);return p.a.n};return f()+f()-43}",
    ] {
        checked(body, CallableKind::FnMut);
    }
}

#[test]
fn captures077_refill_after_take_and_conditional_take_is_repeatable() {
    for body in [
        "fn main()->i32{var p=make();var f=move ()=>{p.a=give p.a;return p.a.n};return f()+f()-40}",
        "fn main()->i32{var p=make();var f=move ()=>{let old=give p.a;p.a=R{n:old.n+1};return p.a.n};return f()+f()-43}",
        "fn main()->i32{var p=make();var f=move (yes:bool)=>{if yes{let old=give p.a}p.a=R{n:21};return p.a.n};return f(true)+f(false)-42}",
        "fn main()->i32{var p=make();var f=move ()=>{let whole=p;p=whole;return p.a.n};return f()+f()-40}",
    ] {
        checked(body, CallableKind::FnMut);
    }
}

#[test]
fn captures077_loop_break_continue_and_lexical_shadowing_preserve_slots() {
    for body in [
        "fn main()->i32{var p=make();var f=move ()=>{for i in 0..3{let old=give p.a;p.a=R{n:old.n+1};if i==1{continue}}return p.a.n};return f()+f()-49}",
        "fn main()->i32{var p=make();var f=move ()=>{while true{let old=give p.a;p.a=R{n:old.n+1};break}return p.a.n};return f()+f()-43}",
    ] {
        checked(body, CallableKind::FnMut);
    }
    checked(
        "fn main()->i32{let p=make();let f=move ()=>{let n=p.a.n;if true{let p=make();let moved=p}return n};return f()+f()-40}",
        CallableKind::Fn,
    );
}

#[test]
fn captures077_error_edges_choose_once_only_when_responsibility_is_missing() {
    checked(
        "fn factory()->impl FnMut(bool)->bool!i32{var p=make();return move (ok)=>{p.a=R{n:step(ok)?};return p.a.n}}fn main()->bool!i32{var f=factory();let failed=f(false);return f(true)?}",
        CallableKind::FnMut,
    );
    checked(
        "fn factory()->impl FnOnce(bool)->bool!i32{var p=make();return move (ok)=>{p.a=keep(give p.a,step(ok)?);return p.a.n}}fn main()->bool!i32{let f=factory();return f(false)?}",
        CallableKind::FnOnce,
    );
    rejected(
        "fn factory()->impl FnMut(bool)->bool!i32{var p=make();return move (ok)=>{p.a=keep(give p.a,step(ok)?);return p.a.n}}fn main()->i32{return 0}",
        Phase::Types,
    );
}

#[test]
fn captures077_partial_whole_and_early_return_use_once() {
    for body in [
        "fn main()->i32{let p=make();let f=move ()=>{let old=give p.a;return old.n+p.b.n-41};return f()}",
        "fn main()->i32{let p=make();let f=move ()=>{let whole=p;return whole.n-1};return f()}",
        "fn main()->i32{var p=make();let f=move (yes:bool)=>{let old=give p.a;if yes{return old.n-20}p.a=give old;return p.a.n-20};return f(true)}",
        "fn main()->i32{var p=make();let f=move ()=>{while true{let old=give p.a;break}return p.b.n-21};return f()}",
    ] {
        checked(body, CallableKind::FnOnce);
    }
}

#[test]
fn captures077_nearest_source_rejections_do_not_weaken_other_modes() {
    rejected(
        "fn main()->i32{let p=make();let f=()=>p.a.n;return f()}",
        Phase::Types,
    );
    rejected(
        "fn main()->i32{var p=make();let f=move ()=>{p.n+=1;return p.n};return f()}",
        Phase::Types,
    );
    rejected(
        "fn main()->i32{let p=make();let old=give p.a;let f=move ()=>p.b.n;return f()}",
        Phase::Analyze,
    );
    rejected(
        "fn main()->i32{var p=make();var f=move ()=>{let old=p.a;p.a=R{n:22};return old.n};return f()}",
        Phase::Analyze,
    );
    rejected(
        "fn main()->i32{let p=make();let f=move ()=>{let old=give p.a;return old.n};let n=f();return f()+n}",
        Phase::Analyze,
    );
}

fn entry_index(module: &TypedModule) -> usize {
    module.closures[0].function.0 as usize
}
fn view(module: &TypedModule) -> LocalId {
    let entry = &module.functions[entry_index(module)];
    match entry.blocks[entry.entry.0 as usize].instructions[1].kind {
        InstructionKind::BindPackageCapture { local, .. } => local,
        _ => panic!("missing view"),
    }
}
fn raw_rejected(label: &str, mutation: impl FnOnce(&mut TypedModule)) {
    let mut module = typed(READER);
    mutation(&mut module);
    assert!(
        zil_analyze::analyze(&module, &Limits::default()).is_err(),
        "accepted {label}"
    );
}

#[test]
fn captures077_analyze_rebuilds_binding_and_activation_boundaries() {
    raw_rejected("duplicate bind", |module| {
        let entry = entry_index(module);
        let duplicate = module.functions[entry].blocks[0].instructions[1].clone();
        module.functions[entry].blocks[0]
            .instructions
            .insert(2, duplicate);
    });
    raw_rejected("missing bind", |module| {
        let entry = entry_index(module);
        module.functions[entry].blocks[0].instructions.remove(1);
    });
    raw_rejected("wrong type", |module| {
        let local = view(module);
        let entry = entry_index(module);
        module.functions[entry].locals[local.0 as usize].ty = Type::I32;
    });
    raw_rejected("invalid local", |module| {
        let entry = entry_index(module);
        module.functions[entry].blocks[0].instructions[1].kind =
            InstructionKind::BindPackageCapture {
                local: LocalId(u32::MAX),
                index: 0,
            };
    });
    raw_rejected("storage reset", |module| {
        let local = view(module);
        let entry = entry_index(module);
        let span = module.functions[entry].span;
        module.functions[entry].blocks[0].instructions.insert(
            2,
            Instruction {
                result: None,
                kind: InstructionKind::StorageLive(local),
                span,
            },
        );
    });
    raw_rejected("entry incoming edge", |module| {
        let entry = entry_index(module);
        let f = &mut module.functions[entry];
        f.blocks.push(BasicBlock {
            id: BlockId(f.blocks.len() as u32),
            instructions: vec![],
            terminator: Terminator::Jump(f.entry),
        });
    });
    raw_rejected("wrong end index", |module| {
        let entry = entry_index(module);
        for block in &mut module.functions[entry].blocks {
            for instruction in &mut block.instructions {
                if let InstructionKind::EndPackageCapture { index, .. } = &mut instruction.kind {
                    *index = 1;
                }
            }
        }
    });
    raw_rejected("missing end", |module| {
        let entry = entry_index(module);
        for block in &mut module.functions[entry].blocks {
            block
                .instructions
                .retain(|i| !matches!(i.kind, InstructionKind::EndPackageCapture { .. }));
        }
    });
}

#[test]
fn captures077_analyze_rejects_shared_alias_write_and_unrestored_fnmut() {
    raw_rejected("whole view disguised as Managed description", |module| {
        let local = view(module);
        let entry = entry_index(module);
        for block in &mut module.functions[entry].blocks {
            for instruction in &mut block.instructions {
                if matches!(
                    instruction.kind,
                    InstructionKind::DescribePackageField { .. }
                ) {
                    instruction.kind = InstructionKind::DescribeLocal { local };
                }
            }
        }
    });
    for body in [
        "fn main()->i32{var p=make();var f=move ()=>{p.n+=1;return p.n};return f()}",
        "fn main()->i32{let p=make();var f=move ()=>{let d=p.a;bump(d);return d.n};return f()}",
        "fn main()->i32{var p=make();var f=move ()=>{p.a=give p.a;return p.a.n};return f()}",
    ] {
        let mut module = typed(body);
        module.closures[0].kind = CallableKind::Fn;
        assert!(
            zil_analyze::analyze(&module, &Limits::default()).is_err(),
            "accepted Shared write {body}"
        );
    }
    let mut module = typed(
        "fn factory()->impl FnOnce(bool)->bool!i32{var p=make();return move (ok)=>{p.a=keep(give p.a,step(ok)?);return p.a.n}}fn main()->bool!i32{let f=factory();return f(false)?}",
    );
    module.closures[0].kind = CallableKind::FnMut;
    for function in &mut module.functions {
        if let Some(contract) = &mut function.callable_return_contract {
            contract.kind = CallableKind::FnMut;
        }
    }
    assert!(
        zil_analyze::analyze(&module, &Limits::default()).is_err(),
        "accepted missing Error-exit field"
    );
}

#[test]
fn captures077_type_and_analyze_work_limits_fail_closed_at_exact_boundary() {
    let body = "fn main()->i32{var p=make();var f=move ()=>{for i in 0..3{let old=give p.a;p.a=R{n:old.n+1}}return p.a.n};return f()+f()-49}";
    let core = core(body);
    for stage in ["types", "analyze"] {
        let module =
            zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
        let accepted = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if stage == "types" {
                zil_types::check(&core, &TargetProfile::default(), &limits).map(|_| ())
            } else {
                zil_analyze::analyze(&module, &limits).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert!(accepted(high).is_ok());
        while low < high {
            let middle = low + (high - low) / 2;
            if accepted(middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(low > 0 && accepted(low).is_ok());
        eprintln!(
            "captures077 {stage} work boundary: {low} Pass; {} ResourceLimit",
            low - 1
        );
        let errors = accepted(low - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.category == ResultCategory::ResourceLimit),
            "{stage}: {errors:?}"
        );
    }
}

#[test]
fn captures077_repeatable_restoration_is_a_return_obligation_not_a_trap_obligation() {
    let mut module = typed(READER);
    let mut entry = module.functions[entry_index(&module)].clone();
    let mut local = entry.locals[view(&module).0 as usize].clone();
    local.id = LocalId(0);
    entry.id = FunctionId(0);
    entry.return_type = Type::Unit;
    entry.values.clear();
    entry.locals = vec![local];
    let ins = |kind| Instruction {
        result: None,
        kind,
        span: entry.span,
    };
    entry.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            ins(InstructionKind::ScopeEnter { scope: ScopeId(0) }),
            ins(InstructionKind::BindPackageCapture {
                local: LocalId(0),
                index: 0,
            }),
            ins(InstructionKind::DropPackageField {
                local: LocalId(0),
                index: 0,
            }),
            ins(InstructionKind::EndPackageCapture {
                local: LocalId(0),
                index: 0,
            }),
            ins(InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Fallthrough,
            }),
        ],
        terminator: Terminator::Trap,
    }];
    entry.entry = BlockId(0);
    module.functions = vec![entry];
    module.closures[0].function = FunctionId(0);
    module.closures[0].kind = CallableKind::FnMut;
    module.closures[0].result = Type::Unit;
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    module.functions[0].blocks[0].terminator = Terminator::Return(None);
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
}
