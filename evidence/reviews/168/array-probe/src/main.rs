use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{Instruction, InstructionKind, LocalDecl, LocalId, Type};

fn main() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "independent167.zil",
        "resource R{n:i32}impl Drop for R{fn drop(self:Self){}}fn take(give r:R){}fn main()->i32{return 0}");
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let original = zil_types::check(&core, &target, &Limits::default()).unwrap();
    let claims = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    zil_verify::verify_module(original.clone(), claims.clone(), &target, &Limits::default()).unwrap();
    let length = 128u32;
    let width = 2048usize;
    let leaf = Type::Record { name: "WideCopyLeaf".into(), fields: (0..width).map(|i| (format!("f{i}"), Type::I32)).collect() };
    let package = Type::Package { name: "BudgetPackage".into(), fields: vec![
        ("owned".into(), Type::Managed(Box::new(original.resources[0].payload.clone()))),
        ("wide".into(), leaf),
    ] };
    let mut module = original;
    let mut plan = claims;
    let main = module.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let local = LocalId(main.locals.len() as u32);
    main.locals.push(LocalDecl { id: local, name: "uninitialized_wide_package_array".into(), ty: Type::Array { element: Box::new(package), length }, mutable: true, span: main.span });
    let at = main.blocks[0].instructions.iter().position(|i| matches!(i.kind, InstructionKind::ScopeEnter { .. })).unwrap() + 1;
    let operations = [InstructionKind::StorageLive(local), InstructionKind::DropOwnedArray { local }, InstructionKind::StorageDead(local)]
        .map(|kind| Instruction { result: None, kind, span: main.span });
    main.blocks[0].instructions.splice(at..at, operations);
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits { max_analysis_steps: 100_000, ..Limits::default() };
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    std::fs::write("wide-package-array.bin", &bytes).unwrap();
    for (route, result) in [
        ("raw", zil_verify::verify_module(module, plan, &target, &limits)),
        ("wire", zil_verify::verify_module_bytes(&bytes, &target, &limits)),
    ] {
        match result {
            Ok(_) => println!("{route}: Accepted; at least {} Copy-field scalar tokens copied into per-element teardown GEPs > declared analysis budget={}", width * length as usize, limits.max_analysis_steps),
            Err(errors) => println!("{route}: {errors:?}"),
        }
    }
}
