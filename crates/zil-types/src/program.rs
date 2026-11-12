use super::*;
use std::collections::BTreeSet;
use zil_base::{Fingerprint, ModuleId};

pub(super) struct Imports<'a> {
    pub aliases: BTreeMap<String, ModuleId>,
    pub signatures: &'a BTreeMap<ModuleId, BTreeMap<String, Signature>>,
}

/// Resolve finite source shape names within their declaring module. The result
/// remains untrusted Core; it does not authorize execution or bypass checking.
pub fn resolve_program_shapes(
    program: &CoreProgram,
    limits: &Limits,
) -> CompileResult<CoreProgram> {
    // Raw callers receive the same bounded preflight as the source driver;
    // no recursive Type clone in signatures may precede it.
    let inferred = infer_program_closures(program, limits)?;
    aggregates::resolve_program(&inferred, limits)
}

pub fn check_program(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedProgram> {
    let inferred = resolve_program_shapes(program, limits)?;
    check_resolved_program(&inferred, target, limits)
}

pub(super) fn check_resolved_program(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedProgram> {
    check_resolved_program_with_reuse(program, target, limits, None)
}

pub(super) fn check_resolved_program_with_reuse(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
    mut reuse: Option<&mut incremental::Reuse<'_>>,
) -> CompileResult<TypedProgram> {
    let context_started = reuse.as_ref().and_then(|reuse| reuse.start_cost());
    let reject = |code: &str, text: &str| vec![Diagnostic::error(Phase::Types, code, None, text)];
    let limit = || {
        vec![Diagnostic::limit(
            Phase::Types,
            "project input budget exceeded",
            None,
        )]
    };
    if program.modules.len() > limits.max_nodes {
        return Err(limit());
    }
    let mut header_inputs = Vec::new();
    let mut module_ids = BTreeSet::new();
    let mut count = 0usize;
    let mut declaration_count = 0usize;
    let mut work = limits.max_analysis_steps;
    for module in &program.modules {
        let core = &module.core;
        declaration_count = declaration_count
            .checked_add(core.functions.len())
            .filter(|n| *n <= limits.max_functions)
            .ok_or_else(limit)?;
        count = count
            .checked_add(
                core.functions
                    .iter()
                    .filter(|f| {
                        f.callable_parameters.is_empty()
                            && f.type_parameters.is_empty()
                            && f.state_parameters.is_empty()
                    })
                    .count(),
            )
            .filter(|n| *n <= limits.max_functions)
            .ok_or_else(limit)?;
        work = work
            .checked_sub(module.imports.len())
            .and_then(|n| n.checked_sub(module.exports.len()))
            .and_then(|n| n.checked_sub(core.functions.len()))
            .ok_or_else(limit)?;
        for import in &module.imports {
            work = work.checked_sub(import.alias.len()).ok_or_else(limit)?;
        }
        for function in &core.functions {
            work = work
                .checked_sub(function.name.len())
                .and_then(|n| n.checked_sub(function.parameters.len()))
                .ok_or_else(limit)?;
        }
        let ids: BTreeSet<_> = core.functions.iter().map(|f| f.id).collect();
        let exports: BTreeSet<_> = module.exports.iter().copied().collect();
        if ids.len() != core.functions.len()
            || exports.len() != module.exports.len()
            || !exports.is_subset(&ids)
        {
            return Err(reject(
                "E-MODULE-EXPORT",
                "function identities and exports must refer to unique declarations",
            ));
        }
        if !module_ids.insert(core.module_id) {
            return Err(reject("E-MODULE-ID", "duplicate module identity"));
        }
        header_inputs.push((
            core,
            exports,
            count
                - core
                    .functions
                    .iter()
                    .filter(|f| {
                        f.callable_parameters.is_empty()
                            && f.type_parameters.is_empty()
                            && f.state_parameters.is_empty()
                    })
                    .count(),
        ));
    }
    if !module_ids.contains(&program.root) {
        return Err(reject(
            "E-MODULE-ROOT",
            "project root module does not exist",
        ));
    }
    // Preserve the single-module entry's lexical diagnostic precedence across
    // the entire bounded input graph. Unsupported aggregate/trait headers must
    // not conceal a receiver error in this module or a later dependency.
    names::check_modules(program.modules.iter().map(|m| m.core.as_ref()), limits)
        .map_err(|e| vec![e])?;
    let mut closure_state = closures::State::new(count, limits);
    let mut life_declarations = Vec::new();
    let mut life_instances = Vec::new();
    for source in &program.modules {
        let (declarations, instances) = closure_state
            .copy_stored_inventory(&source.core)
            .map_err(|e| vec![e])?;
        life_declarations.extend(declarations);
        life_instances.extend(instances);
    }
    let mut signatures_by_module = BTreeMap::new();
    let mut has_source_contexts = false;
    for (core, exports, offset) in header_inputs {
        has_source_contexts |= !core.closure_life_contexts.is_empty();
        let table = signatures(
            core,
            limits,
            offset,
            Some(&exports),
            &mut closure_state.array_billing,
        )?;
        signatures_by_module.insert(core.module_id, table);
    }
    let (dyn_declarations, dyn_interfaces, dyn_vtables) =
        dyn_dispatch::inventory_program(program, &signatures_by_module, limits)
            .map_err(|e| vec![e])?;
    // Reserve the complete ordinary-function range before any local closure
    // gets an identity. Generated environment entries never enter interfaces.
    if has_source_contexts {
        closure_state
            .snapshot(program.modules.len(), Span::default())
            .map_err(|e| vec![e])?;
        for module in &program.modules {
            closure_state
                .install_source_contexts(
                    &module.core,
                    &signatures_by_module[&module.core.module_id],
                )
                .map_err(|e| vec![e])?;
        }
    }
    if program
        .modules
        .iter()
        .any(|m| !m.core.aggregate_applications.is_empty())
    {
        closure_state
            .snapshot(program.modules.len(), Span::default())
            .map_err(|e| vec![e])?;
        let modules = program
            .modules
            .iter()
            .map(|m| m.core.as_ref())
            .collect::<Vec<_>>();
        let mut product_work = limits.max_analysis_steps;
        closure_state.products = aggregate_templates::origins(&modules, limits, &mut product_work)
            .map_err(|e| vec![e])?;
        closure_state
            .snapshot(limits.max_analysis_steps - product_work, Span::default())
            .map_err(|e| vec![e])?;
    }
    closure_state.resource_writes =
        resource_effects::program(program, &signatures_by_module, limits).map_err(|e| vec![e])?;
    let mut enum_work = limits.max_analysis_steps;
    let mut visibility = aggregates::Visibility::new(program, limits)?;
    if program.modules.iter().any(|m| {
        m.core.functions.iter().any(|f| {
            !(f.callable_parameters.is_empty()
                && f.type_parameters.is_empty()
                && f.state_parameters.is_empty())
        })
    }) {
        let mut enum_budget = limits.max_analysis_steps;
        for table in signatures_by_module.values() {
            helpers::reserve_signatures(table, &mut closure_state, limits, Span::default())
                .map_err(|e| vec![e])?;
        }
        closure_state.helper_tables = std::rc::Rc::new(signatures_by_module.clone());
        for module in &program.modules {
            let core = &module.core;
            if core.functions.iter().all(|f| {
                f.callable_parameters.is_empty()
                    && f.type_parameters.is_empty()
                    && f.state_parameters.is_empty()
            }) {
                continue;
            }
            let context = std::rc::Rc::new(helpers::Context {
                module: core.module_id,
                imports: module
                    .imports
                    .iter()
                    .map(|i| (i.alias.clone(), i.module))
                    .collect(),
                enums: enums::project_definitions(program, core, &mut enum_budget, limits)?,
            });
            helpers::install(core, context, &mut closure_state, limits).map_err(|e| vec![e])?;
        }
    }
    let mut dependency_work = limits.max_analysis_steps;
    let mut factory_graph = BTreeMap::new();
    let mut factory_sources = BTreeMap::new();
    for module in &program.modules {
        let table = &signatures_by_module[&module.core.module_id];
        let imports = imports_for(module, &signatures_by_module)?;
        for f in module.core.functions.iter().filter(|f| {
            (f.callable_return.is_some() && f.callable_parameters.is_empty())
                || task_factories::factory(f)
        }) {
            let id = table[&f.name].concrete_id(f.span).map_err(|e| vec![e])?;
            let dependencies = retained::factory_dependencies(
                f,
                table,
                Some(&imports),
                &closure_state,
                &mut dependency_work,
                limits,
            )
            .map_err(|e| vec![e])?;
            factory_graph.insert(id, (f.span, dependencies));
            factory_sources.insert(id, (module, f));
        }
    }
    let factory_order =
        retained::order(factory_graph, &mut dependency_work).map_err(|e| vec![e])?;
    let mut factories = BTreeMap::new();
    for id in factory_order {
        let (module, f) = factory_sources[&id];
        let core = &module.core;
        let enum_types = enums::project_definitions(program, core, &mut enum_work, limits)?;
        let table = &signatures_by_module[&core.module_id];
        let imports = imports_for(module, &signatures_by_module)?;
        let resolved_task;
        let f = if task_factories::factory(f) {
            resolved_task =
                task_factories::prepare(f, table, Some(&imports), &mut closure_state, limits)
                    .map_err(|e| vec![e])?;
            &resolved_task
        } else {
            f
        };
        let pending_start = closure_state.pending.len();
        let thin_start = closure_state.thin.len();
        let mut builder =
            Builder::new(f, id, table, limits, &mut closure_state).map_err(|e| vec![e])?;
        builder.imports = Some(&imports);
        builder.enums = Some(&enum_types);
        builder.dyn_declarations = &dyn_declarations;
        builder.dyn_interfaces = &dyn_interfaces;
        builder.dyn_vtables = &dyn_vtables;
        let typed = builder.finish(&f.body).map_err(|e| vec![e])?;
        visibility.check(
            &typed,
            core.module_id,
            &closure_state.internal_product_fields,
        )?;
        for closure in &closure_state.pending[pending_start..] {
            if let Some(function) = &closure.typed {
                visibility.check(
                    function,
                    core.module_id,
                    &closure_state.internal_product_fields,
                )?;
            }
        }
        for function in closure_state.thin[thin_start..].iter().flatten() {
            visibility.check(
                function,
                core.module_id,
                &closure_state.internal_product_fields,
            )?;
        }
        signatures_by_module
            .get_mut(&core.module_id)
            .expect("module table")
            .get_mut(&f.name)
            .expect("factory signature")
            .result = typed.return_type.clone();
        if task_factories::factory(f) {
            let signature = signatures_by_module
                .get_mut(&core.module_id)
                .expect("module table")
                .get_mut(&f.name)
                .expect("Task factory signature");
            signature.storage_life = typed.storage_life.clone();
            if typed.storage_life.is_some() {
                signature.life = None;
            }
        }
        helpers::resolve_factory_result(
            &mut closure_state,
            core.module_id,
            &f.name,
            &typed.return_type,
            typed.storage_life.as_ref(),
            f.span,
        )
        .map_err(|e| vec![e])?;
        factories.insert(id, typed);
    }
    if let Some(reuse) = &mut reuse {
        reuse.context_constructed(context_started);
        reuse.prepare(program, &signatures_by_module, target, limits);
    }
    let mut functions = Vec::new();
    let mut interfaces = Vec::new();
    let mut task_interfaces = Vec::new();
    let mut resources = Vec::new();
    let mut root_functions = Vec::new();
    for module in &program.modules {
        let core = &module.core;
        let table = &signatures_by_module[&core.module_id];
        resources.extend(resources::definitions(core, table).map_err(|e| vec![e])?);
        let enum_types = enums::project_definitions(program, core, &mut enum_work, limits)?;
        let imports = imports_for(module, &signatures_by_module)?;
        let mut exports = Vec::new();
        let mut task_exports = Vec::new();
        for f in core.functions.iter().filter(|f| {
            f.callable_parameters.is_empty()
                && f.type_parameters.is_empty()
                && f.state_parameters.is_empty()
        }) {
            let signature = &table[&f.name];
            let id = signature.concrete_id(f.span).map_err(|e| vec![e])?;
            let mut typed = if let Some(typed) = factories.remove(&id) {
                typed
            } else if let Some(typed) = match reuse.as_mut() {
                Some(reuse) => reuse
                    .get(core.module_id, f, &mut closure_state, limits)
                    .map_err(|e| vec![e])?,
                None => None,
            } {
                visibility.check(
                    &typed,
                    core.module_id,
                    &closure_state.internal_product_fields,
                )?;
                typed
            } else {
                let accounting_before = closure_state.accounting();
                let cfg_started = reuse.as_ref().and_then(|reuse| reuse.start_cost());
                let result: CompileResult<TypedFunction> = (|| {
                    let pending_start = closure_state.pending.len();
                    let thin_start = closure_state.thin.len();
                    let mut builder = Builder::new(f, id, table, limits, &mut closure_state)
                        .map_err(|e| vec![e])?;
                    builder.imports = Some(&imports);
                    builder.enums = Some(&enum_types);
                    builder.dyn_declarations = &dyn_declarations;
                    builder.dyn_interfaces = &dyn_interfaces;
                    builder.dyn_vtables = &dyn_vtables;
                    let typed = builder.finish(&f.body).map_err(|e| vec![e])?;
                    visibility.check(
                        &typed,
                        core.module_id,
                        &closure_state.internal_product_fields,
                    )?;
                    for closure in &closure_state.pending[pending_start..] {
                        if let Some(function) = &closure.typed {
                            visibility.check(
                                function,
                                core.module_id,
                                &closure_state.internal_product_fields,
                            )?;
                        }
                    }
                    for function in closure_state.thin[thin_start..].iter().flatten() {
                        visibility.check(
                            function,
                            core.module_id,
                            &closure_state.internal_product_fields,
                        )?;
                    }
                    Ok(typed)
                })();
                if let Some(reuse) = &mut reuse {
                    reuse.cfg_executed(cfg_started);
                }
                let typed = result?;
                if let Some(reuse) = &mut reuse {
                    reuse.save(
                        core.module_id,
                        f,
                        &typed,
                        accounting_before,
                        closure_state.accounting(),
                        closure_state.array_billing,
                    );
                }
                typed
            };
            // This is a diagnostic label only; all resolution and calls use module/function IDs.
            if core.module_id != program.root {
                typed.name = format!("@module{}:function{}", core.module_id.0, id.0);
            }
            if let Some(contract) = &typed.storage_life {
                // Export and root-function/interface snapshots below preserve
                // this complete relation, never an independently flat claim.
                borrowed_fp::charge_contract(
                    contract,
                    |work| closure_state.snapshot(work, f.span),
                    3,
                )
                .map_err(|e| vec![e])?;
            }
            closure_state
                .array_snapshot(&typed.return_type, 1, f.span)
                .map_err(|e| vec![e])?;
            for parameter in &f.parameters {
                closure_state
                    .array_snapshot(&parameter.ty, 1, parameter.span)
                    .map_err(|e| vec![e])?;
            }
            let export = ScalarFunction {
                storage_life: typed.storage_life.clone(),
                reference_return_bound: typed.reference_return_bound.clone(),
                result_mode: f.result_mode,
                name: f.name.clone(),
                function: id,
                parameters: f
                    .parameters
                    .iter()
                    .map(|p| (p.mode, p.ty.clone()))
                    .collect(),
                result: typed.return_type.clone(),
            };
            if core.module_id == program.root && f.foreign.is_none() {
                closure_state
                    .array_snapshot(&export.result, 1, f.span)
                    .map_err(|e| vec![e])?;
                for (_, ty) in &export.parameters {
                    closure_state
                        .array_snapshot(ty, 1, f.span)
                        .map_err(|e| vec![e])?;
                }
                root_functions.push(export.clone());
            }
            if signature.public {
                if tasks::suspends(&typed) {
                    if !signature.scalar_thread_worker()
                        || signature.parameters.as_slice() != [Type::I32]
                    {
                        return Err(vec![error(
                            "E-TASK-WORKER",
                            f.span,
                            "public scalar task requires exact ordinary fn(i32)->i32",
                        )]);
                    }
                    closure_state
                        .snapshot(f.name.len().saturating_add(1), f.span)
                        .map_err(|e| vec![e])?;
                    task_exports.push(TaskExport {
                        name: f.name.clone(),
                        worker: id,
                        parameter: export.parameters[0].clone(),
                        result: (export.result_mode, export.result.clone()),
                        abi_version: 1,
                    });
                } else {
                    exports.push(export);
                }
            }
            if let Some(reuse) = &mut reuse {
                reuse.observe(core.module_id, f, &typed);
            }
            functions.push(typed);
        }
        exports.sort_by(|a, b| a.name.cmp(&b.name));
        task_exports.sort_by(|a, b| a.name.cmp(&b.name));
        if !task_exports.is_empty() {
            task_interfaces.push(TaskInterface {
                module: core.module_id,
                exports: task_exports,
            });
        }
        interfaces.push(ScalarInterface {
            callable_returns: None,
            callable_contracts: None,
            module: core.module_id,
            exports,
            // Concrete helper exports are appended after every source body has
            // been checked. Their hidden layouts are resolved below as well.
            signature_fingerprint: Fingerprint::ZERO,
            implementation_fingerprint: core.source_fingerprint,
            result_origins: None,
            contract_fingerprint: None,
        });
    }
    for (function, export) in closure_state
        .helper_functions
        .iter()
        .zip(&closure_state.helper_exports)
    {
        visibility.check(
            function,
            export.module,
            &closure_state.internal_product_fields,
        )?;
        if export.public {
            let interface = interfaces
                .iter_mut()
                .find(|i| i.module == export.module)
                .expect("definition module interface");
            interface.exports.push(export.signature.clone());
        }
    }
    let bound_elements =
        store_templates::bound_elements(&mut closure_state).map_err(|e| vec![e])?;
    let thread_transfers = std::mem::take(&mut closure_state.thread_transfers);
    let thread_entries = std::mem::take(&mut closure_state.thread_entries);
    let (closures, synthesized, mut remaining) = closure_state.finish().map_err(|e| vec![e])?;
    functions.extend(synthesized);
    functions.sort_by_key(|f| f.id);
    tasks::add_definition(&functions, &mut resources, &mut remaining, limits)
        .map_err(|e| vec![e])?;
    registration::add_definitions(&functions, &mut resources, &mut remaining)
        .map_err(|e| vec![e])?;
    byte_buffer::add_definition(&functions, &mut resources, limits).map_err(|e| vec![e])?;
    dynamic_store::add_definitions(
        &functions,
        &mut resources,
        &closures,
        &bound_elements,
        limits,
        &mut remaining,
    )
    .map_err(|e| vec![e])?;
    let mut module = TypedModule {
        life_declarations,
        life_instances,
        dyn_declarations,
        dyn_interfaces,
        dyn_vtables,
        resources,
        schema_version: SCHEMA_VERSION,
        module_id: program.root,
        source_fingerprint: program.source_fingerprint,
        target: target.clone(),
        functions,
        closures,
        unsupported: Vec::new(),
    };
    lending::populate(&mut module, &mut remaining).map_err(|e| vec![e])?;
    registration::threaded::check(&module, &thread_transfers, &thread_entries, &mut remaining)
        .map_err(|e| vec![e])?;
    for interface in &mut interfaces {
        interface.exports.sort_by(|a, b| a.name.cmp(&b.name));
        interface.signature_fingerprint =
            interface_fingerprint(&interface.exports, target, &module, &mut remaining, limits)?;
    }
    Ok(TypedProgram {
        task_interfaces,
        module,
        interfaces,
        root_functions,
    })
}

fn imports_for<'a>(
    module: &ProgramModule,
    signatures: &'a BTreeMap<ModuleId, BTreeMap<String, Signature>>,
) -> CompileResult<Imports<'a>> {
    let table = &signatures[&module.core.module_id];
    let mut imports = Imports {
        aliases: BTreeMap::new(),
        signatures,
    };
    for import in &module.imports {
        if !signatures.contains_key(&import.module) {
            return Err(vec![error(
                "E-MODULE-NOT-FOUND",
                import.span,
                "import refers to an absent module",
            )]);
        }
        if table.contains_key(&import.alias)
            || imports
                .aliases
                .insert(import.alias.clone(), import.module)
                .is_some()
        {
            return Err(vec![error(
                "E-MODULE-ALIAS",
                import.span,
                "module alias conflicts with another declaration",
            )]);
        }
    }
    Ok(imports)
}

fn interface_fingerprint(
    exports: &[ScalarFunction],
    target: &TargetProfile,
    module: &TypedModule,
    remaining: &mut usize,
    limits: &Limits,
) -> CompileResult<Fingerprint> {
    let closures = &module.closures;
    let functions = &module.functions;
    let stored = !module.life_declarations.is_empty() || !module.life_instances.is_empty();
    if stored {
        let work = inference::life_table_snapshot_work(
            &module.life_declarations,
            &module.life_instances,
            &Limits {
                max_analysis_steps: *remaining,
                ..limits.clone()
            },
        )
        .map_err(|e| vec![e])?;
        *remaining = remaining
            .checked_sub(work.saturating_mul(64).saturating_add(128))
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Types,
                    "stored interface context identity budget exceeded",
                    None,
                )]
            })?;
    }
    let mut bytes = b"ZIL-SCALAR-INTERFACE-3\0".to_vec();
    bytes.extend_from_slice(&target.fingerprint().0);
    bytes.extend_from_slice(&(exports.len() as u64).to_le_bytes());
    for export in exports {
        let cost = export
            .name
            .len()
            .saturating_mul(2)
            .saturating_add(export.parameters.len().saturating_mul(8))
            .saturating_add(
                export
                    .reference_return_bound
                    .as_ref()
                    .map_or(0, |b| b.len().saturating_mul(8)),
            )
            .saturating_add(64);
        *remaining = remaining.checked_sub(cost).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Types,
                "export header identity budget exceeded",
                None,
            )]
        })?;
        bytes.extend_from_slice(&(export.name.len() as u64).to_le_bytes());
        bytes.extend_from_slice(export.name.as_bytes());
        bytes.extend_from_slice(&(export.parameters.len() as u64).to_le_bytes());
        for (mode, ty) in &export.parameters {
            bytes.push(match mode {
                PassingMode::Copy => 0,
                PassingMode::Move => 1,
                PassingMode::Describe => 2,
                PassingMode::Create => 3,
                PassingMode::Give => 4,
                PassingMode::Borrow => 5,
                PassingMode::BorrowMut => 6,
                PassingMode::Infer => 7,
            });
            interface_identity::encode(
                ty,
                closures,
                &module.dyn_declarations,
                &module.dyn_interfaces,
                &mut bytes,
                limits,
                remaining,
            )
            .map_err(|e| vec![e])?;
        }
        bytes.push(match export.result_mode {
            PassingMode::Copy => 0,
            PassingMode::Move => 1,
            PassingMode::Describe => 2,
            PassingMode::Create => 3,
            PassingMode::Give => 4,
            PassingMode::Borrow => 5,
            PassingMode::BorrowMut => 6,
            PassingMode::Infer => 7,
        });
        interface_identity::encode(
            &export.result,
            closures,
            &module.dyn_declarations,
            &module.dyn_interfaces,
            &mut bytes,
            limits,
            remaining,
        )
        .map_err(|e| vec![e])?;
        bytes.push(u8::from(export.reference_return_bound.is_some()));
        if let Some(bound) = &export.reference_return_bound {
            bytes.extend_from_slice(&(bound.len() as u64).to_le_bytes());
            for index in bound {
                bytes.extend_from_slice(&index.to_le_bytes());
            }
        }
    }
    let exhausted = || {
        vec![Diagnostic::limit(
            Phase::Types,
            "export relation identity work exceeded",
            None,
        )]
    };
    if stored {
        bytes.extend_from_slice(b"STORED-CONTEXT-2\0");
        bytes.extend_from_slice(
            &life_inventory_fingerprint(&module.life_declarations, &module.life_instances).0,
        );
    }
    for export in exports {
        if let Some(contract) = &export.storage_life {
            *remaining = remaining
                .checked_sub(export.name.len().saturating_mul(2))
                .ok_or_else(exhausted)?;
            borrowed_fp::charge_contract(
                contract,
                |work| {
                    *remaining = remaining
                        .checked_sub(work)
                        .ok_or_else(|| exhausted().remove(0))?;
                    Ok(())
                },
                64,
            )
            .map_err(|e| vec![e])?;
            bytes.extend_from_slice(b"STORED-SIGNATURE-2\0");
            bytes.extend_from_slice(&(export.name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(export.name.as_bytes());
            bytes.extend_from_slice(&contract.fingerprint().0);
        }
    }
    *remaining = remaining.checked_sub(exports.len()).ok_or_else(exhausted)?;
    for export in exports {
        if let Some(contract) = &functions[export.function.0 as usize].protocol_contract {
            *remaining = remaining
                .checked_sub(export.name.len().saturating_mul(2).saturating_add(128))
                .ok_or_else(exhausted)?;
            registration::charge_contract(contract, |work| {
                *remaining = remaining
                    .checked_sub(work.saturating_mul(64))
                    .ok_or_else(|| exhausted().remove(0))?;
                Ok(())
            })
            .map_err(|e| vec![e])?;
            bytes.extend_from_slice(b"REGISTRATION-PHASE-1\0");
            bytes.extend_from_slice(&(export.name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(export.name.as_bytes());
            bytes.extend_from_slice(&protocol_contract_fingerprint(contract).0);
        }
        if let Some(contract) = &functions[export.function.0 as usize].callable_contract {
            *remaining = remaining
                .checked_sub(contract.roles.len())
                .ok_or_else(exhausted)?;
            for role in &contract.roles {
                if let Some(life) = &role.life {
                    let size = 128usize.saturating_add(export.name.len().saturating_mul(2));
                    *remaining = remaining.checked_sub(size).ok_or_else(exhausted)?;
                    borrowed_fp::charge_contract(
                        life,
                        |work| {
                            *remaining = remaining
                                .checked_sub(work)
                                .ok_or_else(|| exhausted().remove(0))?;
                            Ok(())
                        },
                        64,
                    )
                    .map_err(|e| vec![e])?;
                    bytes.extend_from_slice(b"HRTB-CALL-2\0");
                    bytes.extend_from_slice(&(export.name.len() as u64).to_le_bytes());
                    bytes.extend_from_slice(export.name.as_bytes());
                    bytes.extend_from_slice(&role.parameter.to_le_bytes());
                    bytes.extend_from_slice(&life.fingerprint().0);
                }
            }
        }
    }
    Ok(Fingerprint::of(&bytes))
}
