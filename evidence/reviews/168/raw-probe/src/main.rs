use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{ClosureId, FunctionId, Instruction, InstructionKind, LocalDecl, LocalId, Type};

fn main() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "independent168.zil",
        "fn make()->impl Fn()->i32{return ()=>42}fn main()->i32{let f=make();return f()-42}");
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let mut module = zil_types::check(&core, &target, &Limits::default()).unwrap();
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default()).unwrap();
    let prototype = module.closures[0].clone();
    let entry = module.functions[prototype.function.0 as usize].clone();
    let entry_plan = plan.functions[prototype.function.0 as usize].clone();
    let width = 512usize;
    let depth = 10usize;
    let leaf = Type::Record { name: "WideCopyLeaf".into(), fields: (0..width).map(|i| (format!("f{i}"), Type::I32)).collect() };
    for index in 1..=depth {
        let mut definition = prototype.clone();
        definition.id = ClosureId(index as u32);
        definition.function = FunctionId(module.functions.len() as u32);
        definition.captures = if index == 1 { vec![leaf.clone()] } else { vec![Type::Closure(ClosureId(index as u32 - 1)); 2] };
        let mut function = entry.clone();
        function.id = definition.function;
        function.name = format!("$independent168_{index}");
        function.environment = Some(definition.id);
        let mut claims = entry_plan.clone();
        claims.function = function.id;
        module.functions.push(function);
        plan.functions.push(claims);
        module.closures.push(definition);
    }
    let main = module.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let local = LocalId(main.locals.len() as u32);
    main.locals.push(LocalDecl { id: local, name: "uninitialized_wide_dag".into(), ty: Type::Closure(ClosureId(depth as u32)), mutable: true, span: main.span });
    let at = main.blocks[0].instructions.iter().position(|i| matches!(i.kind, InstructionKind::ScopeEnter { .. })).unwrap() + 1;
    let span = main.span;
    let operations = [InstructionKind::StorageLive(local), InstructionKind::DropClosure { local }, InstructionKind::StorageDead(local)]
        .map(|kind| Instruction { result: None, kind, span });
    main.blocks[0].instructions.splice(at..at, operations);
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits { max_analysis_steps: 100_000, ..Limits::default() };
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    std::fs::write("wide-leaf-dag.bin", &bytes).unwrap();
    for (route, result) in [
        ("raw", zil_verify::verify_module(module, plan, &target, &limits)),
        ("wire", zil_verify::verify_module_bytes(&bytes, &target, &limits)),
    ] {
        match result {
            Ok(_) => println!("{route}: Accepted; expanded alignment type visits={} > declared analysis budget={}", ((width + 3) << (depth - 1)) - 1, limits.max_analysis_steps),
            Err(errors) => println!("{route}: {errors:?}"),
        }
    }
}
