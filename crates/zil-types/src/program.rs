use super::*;
use std::borrow::Cow;
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
    resolve_program_shapes_borrowed(program, limits).map(Cow::into_owned)
}

/// Resolve and check every shape, preserving the original allocation when the
/// complete result is unchanged. Borrowing confers no verification authority.
pub fn resolve_program_shapes_borrowed<'a>(
    program: &'a CoreProgram,
    limits: &Limits,
) -> CompileResult<Cow<'a, CoreProgram>> {
    // Raw callers receive the same bounded preflight as the source driver;
    // no recursive Type clone in signatures may precede it.
    match infer_program_closures_borrowed(program, limits)? {
        Cow::Borrowed(inferred) => aggregates::resolve_program_borrowed(inferred, limits),
        Cow::Owned(inferred) => {
            let resolved = aggregates::resolve_program_borrowed(&inferred, limits)?;
            Ok(Cow::Owned(match resolved {
                Cow::Borrowed(_) => inferred,
                Cow::Owned(resolved) => resolved,
            }))
        }
    }
}

pub fn check_program(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedProgram> {
    check_program_inner(program, target, limits, None).map(|(program, _)| program)
}

/// Reuse only process-local successful scalar body construction. The returned
/// memo is a transaction candidate: drivers commit it only after the complete
/// request succeeds. All declarations and later semantic passes still execute.
pub fn check_program_with_memo(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
    context: Fingerprint,
    memo: &program_memo::ProgramMemo,
) -> CompileResult<(TypedProgram, program_memo::ProgramMemo)> {
    check_program_inner(program, target, limits, Some((memo, context)))
}

fn check_program_inner(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
    previous: Option<(&program_memo::ProgramMemo, Fingerprint)>,
) -> CompileResult<(TypedProgram, program_memo::ProgramMemo)> {
    let inferred = resolve_program_shapes_borrowed(program, limits)?;
    let program = &inferred;
    let project_enums = enums::ProjectDefinitions::new(program);
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
    names::check_modules(program.modules.iter().map(|m| &m.core), limits).map_err(|e| vec![e])?;
    let mut signatures_by_module = BTreeMap::new();
    for (core, exports, offset) in header_inputs {
        let table = signatures(core, limits, offset, Some(&exports))?;
        signatures_by_module.insert(core.module_id, table);
    }
    // Reserve the complete ordinary-function range before any local closure
    // gets an identity. Generated environment entries never enter interfaces.
    let mut closure_state = closures::State::new(count, limits);
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
                enums: project_enums.resolve(core, &mut enum_budget, limits)?,
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
        for f in module
            .core
            .functions
            .iter()
            .filter(|f| f.callable_return.is_some() && f.callable_parameters.is_empty())
        {
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
        let enum_types = project_enums.resolve(core, &mut enum_work, limits)?;
        let table = &signatures_by_module[&core.module_id];
        let imports = imports_for(module, &signatures_by_module)?;
        let pending_start = closure_state.pending.len();
        let thin_start = closure_state.thin.len();
        let mut builder =
            Builder::new(f, id, table, limits, &mut closure_state).map_err(|e| vec![e])?;
        builder.imports = Some(&imports);
        builder.enums = Some(&enum_types);
        let typed = builder.finish(&f.body).map_err(|e| vec![e])?;
        visibility.check(&typed, core.module_id)?;
        for closure in &closure_state.pending[pending_start..] {
            if let Some(function) = &closure.typed {
                visibility.check(function, core.module_id)?;
            }
        }
        for function in closure_state.thin[thin_start..].iter().flatten() {
            visibility.check(function, core.module_id)?;
        }
        signatures_by_module
            .get_mut(&core.module_id)
            .expect("module table")
            .get_mut(&f.name)
            .expect("factory signature")
            .result = typed.return_type.clone();
        helpers::resolve_factory_result(
            &mut closure_state,
            core.module_id,
            &f.name,
            &typed.return_type,
            f.span,
        )
        .map_err(|e| vec![e])?;
        factories.insert(id, typed);
    }
    let mut memo = previous.and_then(|(previous, context)| {
        program_memo::Request::new(
            program,
            &signatures_by_module,
            target,
            limits,
            context,
            previous,
        )
    });
    let mut functions = Vec::new();
    let mut interfaces = Vec::new();
    let mut resources = Vec::new();
    let mut root_functions = Vec::new();
    for (module_index, module) in program.modules.iter().enumerate() {
        let core = &module.core;
        let table = &signatures_by_module[&core.module_id];
        resources.extend(resources::definitions(core, table).map_err(|e| vec![e])?);
        let enum_types = project_enums.resolve(core, &mut enum_work, limits)?;
        let imports = imports_for(module, &signatures_by_module)?;
        let mut exports = Vec::new();
        for (function_index, f) in core
            .functions
            .iter()
            .filter(|f| {
                f.callable_parameters.is_empty()
                    && f.type_parameters.is_empty()
                    && f.state_parameters.is_empty()
            })
            .enumerate()
        {
            let signature = &table[&f.name];
            let id = signature.concrete_id(f.span).map_err(|e| vec![e])?;
            let before = memo.as_ref().map(|_| closure_state.memo_counters());
            let cached = memo
                .as_mut()
                .and_then(|memo| memo.load(module_index, function_index, &mut closure_state));
            let mut typed = if let Some(typed) = cached {
                visibility.check(&typed, core.module_id)?;
                typed
            } else if let Some(typed) = factories.remove(&id) {
                typed
            } else {
                let pending_start = closure_state.pending.len();
                let thin_start = closure_state.thin.len();
                let mut builder =
                    Builder::new(f, id, table, limits, &mut closure_state).map_err(|e| vec![e])?;
                builder.imports = Some(&imports);
                builder.enums = Some(&enum_types);
                let typed = builder.finish(&f.body).map_err(|e| vec![e])?;
                visibility.check(&typed, core.module_id)?;
                for closure in &closure_state.pending[pending_start..] {
                    if let Some(function) = &closure.typed {
                        visibility.check(function, core.module_id)?;
                    }
                }
                for function in closure_state.thin[thin_start..].iter().flatten() {
                    visibility.check(function, core.module_id)?;
                }
                typed
            };
            if memo.as_mut().is_some_and(|memo| {
                let usage = closure_state
                    .memo_usage(before.expect("active memo captured builder counters"));
                memo.record(module_index, &typed, usage).is_none()
            }) {
                // An ineligible output disables this optional epoch. Storage
                // capacity only omits modules; it cannot alter the typed result.
                memo = None;
            }
            // This is a diagnostic label only; all resolution and calls use module/function IDs.
            if core.module_id != program.root {
                typed.name = format!("@module{}:function{}", core.module_id.0, id.0);
            }
            let export = ScalarFunction {
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
                root_functions.push(export.clone());
            }
            if signature.public {
                exports.push(export);
            }
            functions.push(typed);
        }
        exports.sort_by(|a, b| a.name.cmp(&b.name));
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
        visibility.check(function, export.module)?;
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
    let (closures, synthesized) = closure_state.finish().map_err(|e| vec![e])?;
    functions.extend(synthesized);
    functions.sort_by_key(|f| f.id);
    byte_buffer::add_definition(&functions, &mut resources, limits).map_err(|e| vec![e])?;
    dynamic_store::add_definitions(
        &functions,
        &mut resources,
        &closures,
        &bound_elements,
        limits,
    )
    .map_err(|e| vec![e])?;
    let mut module = TypedModule {
        resources,
        schema_version: SCHEMA_VERSION,
        module_id: program.root,
        source_fingerprint: program.source_fingerprint,
        target: target.clone(),
        functions,
        closures,
        unsupported: Vec::new(),
    };
    // Memo snapshots describe the ordinary Builder output, before lending can
    // mutate it. Selection is complete before any new cache snapshot is copied.
    let next_memo = memo
        .map(|memo| memo.finish(&module.functions))
        .unwrap_or_default();
    lending::populate(&mut module, limits).map_err(|e| vec![e])?;
    for interface in &mut interfaces {
        interface.exports.sort_by(|a, b| a.name.cmp(&b.name));
        interface.signature_fingerprint =
            interface_fingerprint(&interface.exports, target, &module.closures);
    }
    Ok((
        TypedProgram {
            module,
            interfaces,
            root_functions,
        },
        next_memo,
    ))
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
    closures: &[ClosureDefinition],
) -> Fingerprint {
    let mut bytes = b"ZIL-SCALAR-INTERFACE-2\0".to_vec();
    bytes.extend_from_slice(&target.fingerprint().0);
    bytes.extend_from_slice(&(exports.len() as u64).to_le_bytes());
    for export in exports {
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
            type_identity(&mut bytes, ty, closures);
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
        type_identity(&mut bytes, &export.result, closures);
    }
    Fingerprint::of(&bytes)
}
fn type_identity(bytes: &mut Vec<u8>, ty: &Type, closures: &[ClosureDefinition]) {
    match ty {
        Type::Unit => bytes.push(0),
        Type::Bool => bytes.push(1),
        Type::U8 => bytes.push(15),
        Type::I32 => bytes.push(2),
        Type::I64 => bytes.push(3),
        Type::U16 => bytes.push(16),
        Type::ByteBuffer => bytes.push(21),
        Type::OwnedStore { element } => {
            bytes.push(22);
            type_identity(bytes, element, closures);
        }
        Type::ForeignHandle => bytes.push(17),
        Type::CWideString => bytes.push(18),
        Type::ForeignCallback { parameters, result } => {
            bytes.push(19);
            bytes.extend_from_slice(&(parameters.len() as u64).to_le_bytes());
            for parameter in parameters {
                type_identity(bytes, parameter, closures);
            }
            type_identity(bytes, result, closures);
        }
        Type::Address { mutable, inner } => {
            bytes.extend_from_slice(&[4, u8::from(*mutable)]);
            type_identity(bytes, inner, closures);
        }
        Type::Array { element, length } => {
            bytes.push(7);
            bytes.extend_from_slice(&length.to_le_bytes());
            type_identity(bytes, element, closures);
        }
        Type::Slice { element } => {
            bytes.push(20);
            type_identity(bytes, element, closures);
        }
        Type::FunctionPointer { parameters, result } => {
            bytes.push(8);
            bytes.extend_from_slice(&(parameters.len() as u64).to_le_bytes());
            for parameter in parameters {
                type_identity(bytes, parameter, closures);
            }
            type_identity(bytes, result, closures);
        }
        Type::Tuple(fields) => {
            bytes.push(5);
            bytes.extend_from_slice(&(fields.len() as u64).to_le_bytes());
            for ty in fields {
                type_identity(bytes, ty, closures);
            }
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            bytes.push(if matches!(ty, Type::Package { .. }) {
                13
            } else {
                6
            });
            bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&(fields.len() as u64).to_le_bytes());
            for (name, ty) in fields {
                bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
                bytes.extend_from_slice(name.as_bytes());
                type_identity(bytes, ty, closures);
            }
        }
        Type::Enum { name, variants } => {
            bytes.push(9);
            bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&(variants.len() as u64).to_le_bytes());
            for (name, fields) in variants {
                bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
                bytes.extend_from_slice(name.as_bytes());
                bytes.extend_from_slice(&(fields.len() as u64).to_le_bytes());
                for ty in fields {
                    type_identity(bytes, ty, closures);
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            bytes.push(12);
            type_identity(bytes, error, closures);
            type_identity(bytes, result, closures);
        }
        Type::Managed(payload) => {
            bytes.push(10);
            type_identity(bytes, payload, closures);
        }
        Type::Desc(payload) => {
            bytes.push(11);
            type_identity(bytes, payload, closures);
        }
        Type::Closure(id) => {
            // A concrete helper's ABI includes its actual environment layout.
            // An id alone would collide after a capture changed type while the
            // same source closure remained at the same ordinal.
            let definition = &closures[id.0 as usize];
            bytes.push(14);
            bytes.extend_from_slice(&id.0.to_le_bytes());
            bytes.push(match definition.kind {
                CallableKind::Fn => 0,
                CallableKind::FnMut => 1,
                CallableKind::FnOnce => 2,
                CallableKind::FunctionPointer => 3,
            });
            bytes.extend_from_slice(&(definition.captures.len() as u64).to_le_bytes());
            for capture in &definition.captures {
                type_identity(bytes, capture, closures);
            }
            bytes.extend_from_slice(&(definition.parameters.len() as u64).to_le_bytes());
            for parameter in &definition.parameters {
                type_identity(bytes, parameter, closures);
            }
            type_identity(bytes, &definition.result, closures);
        }
        _ => unreachable!("unsupported interface type rejected before hashing"),
    }
}
