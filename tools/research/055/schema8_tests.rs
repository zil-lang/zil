//! Integration-only schema8 checks. The schema7 performance freeze keeps its
//! original code/input identity; none of these executions are timing samples.
use super::*;
use zil_base::Fingerprint;
use zil_ir::{CoreImport, CoreProgram, InstructionKind};

fn project() -> TypedModule {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/mainline/error_cleanup");
    let names = ["main", "failure", "token", "io"];
    let sources = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let path = root.join(format!("{name}.zil"));
            SourceUnit::new(
                FileId(i as u32),
                ModuleId(i as u32),
                path.to_string_lossy(),
                fs::read_to_string(&path).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let parsed = sources
        .iter()
        .map(|source| zil_syntax::parse(source).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = parsed
        .iter()
        .map(|module| {
            module
                .items
                .iter()
                .filter_map(|item| {
                    let zil_syntax::ast::ItemKind::Import(import) = &item.kind else {
                        return None;
                    };
                    assert!(!item.public && !import.syntax && import.path.len() == 1);
                    let id = names
                        .iter()
                        .position(|name| *name == import.path[0])
                        .expect("actual source dependency is supplied");
                    Some(CoreImport {
                        alias: import
                            .alias
                            .clone()
                            .unwrap_or_else(|| import.path[0].clone()),
                        module: ModuleId(id as u32),
                        span: item.span,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let limits = Limits::default();
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &parsed.iter().collect::<Vec<_>>(),
        &imports,
        &limits,
    )
    .unwrap()
    .into_iter()
    .map(|(module, _)| module)
    .collect();
    let bytes = sources
        .iter()
        .flat_map(|source| source.fingerprint().0)
        .collect::<Vec<_>>();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::of(&bytes),
        modules,
    };
    zil_types::check_program(&program, &TargetProfile::host_llvm(), &limits)
        .unwrap()
        .module
}

#[test]
fn schema8_actual_error_project_preserves_supported_graphs_and_facts() {
    assert_eq!(zil_base::SCHEMA_VERSION, 8);
    let module = project();
    production(module.clone()).unwrap();
    let mut checked = 0;
    let mut points = 0;
    let mut error_exits = 0;
    let mut error_operations = 0;
    let mut projections = 0;
    let mut terminators = 0;
    let mut full_output = String::new();
    for function in &module.functions {
        if function.foreign.is_some() {
            continue;
        }
        let graph = graph::extract(&module, function)
            .unwrap_or_else(|reason| panic!("{}: {reason}", function.name));
        oracle::audit_extraction(&module, function, &graph).unwrap();
        let rows = oracle_rows(&module, function, &graph).unwrap();
        let flat = graph::plan(&graph, false).unwrap();
        let cut = graph::plan(&graph, true).unwrap();
        let expected = snapshot(&graph, &flat, &rows).unwrap();
        full_output.push_str(&expected);
        for (plan, actual) in [
            (&flat, graph::solve(&graph, &flat).unwrap().0),
            (&cut, graph::solve(&graph, &cut).unwrap().0),
            (&flat, block::solve(&graph, function).unwrap().0),
        ] {
            assert_eq!(actual, rows, "{}", function.name);
            assert_eq!(snapshot(&graph, plan, &actual).unwrap(), expected);
        }
        checked += 1;
        points += graph.nodes.len();
        terminators += function.blocks.len();
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if matches!(
                instruction.kind,
                InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::ErrorTag { .. }
                    | InstructionKind::ReadErrorUnion { .. }
            ) {
                error_operations += 1;
            }
            if matches!(instruction.kind, InstructionKind::ReadErrorUnion { .. }) {
                projections += 1;
            }
        }
        error_exits += function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(
                    instruction.kind,
                    InstructionKind::ScopeExit {
                        kind: zil_ir::CleanupExitKind::Error,
                        ..
                    }
                )
            })
            .count();
    }
    assert!(checked > 0 && points > 0 && error_exits > 0);
    if let Some(output) = std::env::var_os("ZIL055_SCHEMA8_REPORT_DIR") {
        let output = PathBuf::from(output);
        fs::create_dir(&output).unwrap();
        fs::write(output.join("error_cleanup.typed"), format!("{module:#?}")).unwrap();
        fs::write(output.join("error_cleanup.expected"), full_output).unwrap();
        fs::write(output.join("counts.json"), format!("{{\"schema\":8,\"typed_fingerprint\":{},\"source_modules\":4,\"checked_functions\":{checked},\"points\":{points},\"error_scope_exits\":{error_exits},\"error_operations\":{error_operations},\"error_projections\":{projections},\"terminators\":{terminators},\"unsupported_functions\":0,\"production_verified\":true,\"full_shadow_equal\":true}}\n", json_string(&module.fingerprint().to_hex()))).unwrap();
    }
    // Managed returns are supported. A Desc/borrowed-closure return would
    // explicitly fail this whole-project check rather than silently skip it.
    println!(
        "schema8-shadow modules=4 checked_functions={checked} points={points} error_scope_exits={error_exits} unsupported_functions=0; whole-module production Verify passed; no timing"
    );
}

#[test]
fn schema8_error_projection_failures_and_scope_events_cannot_be_omitted() {
    let module = typed(
        "error-shadow.zil",
        "fn get()->bool!i32{return true} fn main()->bool!i32{return get()?}",
    )
    .unwrap();
    production(module.clone()).unwrap();
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap();
    let graph = graph::extract(&module, function).unwrap();
    oracle::audit_extraction(&module, function, &graph).unwrap();
    let mut bad = graph.clone();
    let exit = bad
        .nodes
        .iter_mut()
        .find(|node| node.label.starts_with("ScopeExit") && node.label.contains("Error"))
        .unwrap();
    exit.observable = false;
    assert!(oracle::audit_extraction(&module, function, &bad).is_err());
    let mut bad = graph.clone();
    let projection = bad
        .nodes
        .iter_mut()
        .find(|node| node.label.starts_with("ReadErrorUnion"))
        .unwrap();
    assert_eq!(projection.edges.len(), 2);
    projection.edges.pop();
    assert!(oracle::audit_extraction(&module, function, &bad).is_err());
    let mut bad = graph.clone();
    let injected = bad
        .nodes
        .iter_mut()
        .find(|node| node.label.starts_with("MakeErrorUnion") && node.label.contains("true"))
        .unwrap();
    injected.label = injected.label.replace("true", "false");
    assert!(oracle::audit_extraction(&module, function, &bad).is_err());
    let mut bad = graph.clone();
    let exit = bad
        .nodes
        .iter_mut()
        .find(|node| node.label.starts_with("ScopeExit") && node.label.contains("Error"))
        .unwrap();
    exit.label = exit.label.replace("Error", "Return");
    assert!(oracle::audit_extraction(&module, function, &bad).is_err());
    let mut bad = graph.clone();
    let branch = bad
        .nodes
        .iter_mut()
        .find(|node| node.label.starts_with("Branch"))
        .unwrap();
    assert_eq!(branch.edges.len(), 2);
    let yes = branch.edges[0].to;
    branch.edges[0].to = branch.edges[1].to;
    branch.edges[1].to = yes;
    assert!(oracle::audit_extraction(&module, function, &bad).is_err());
}
