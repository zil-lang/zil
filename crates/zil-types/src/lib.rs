#![forbid(unsafe_code)]
//! Name/type checking and explicit Copy value/closure CFG construction.
//! Default scalar captures retain local addresses; other capabilities fail closed.
use std::collections::BTreeMap;
use zil_base::{CompileResult, Diagnostic, Limits, Phase, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;
mod addresses;
mod aggregates;
mod byte_buffer;
mod callable_fields;
mod closures;
mod conditions;
mod dynamic_store;
mod enums;
mod errors;
mod foreign;
mod functions;
mod helpers;
mod inference;
mod lending;
mod managed_slots;
mod names;
mod packages;
mod program;
mod resource_effects;
mod resources;
mod retained;
mod state_origins;
mod store_templates;
pub use inference::{infer_program_closures, infer_source_closures};
pub use program::{check_program, resolve_program_shapes};
type R<T> = Result<T, Diagnostic>;
#[derive(Clone)]
struct Signature {
    id: Option<FunctionId>,
    // Effective type bindings; state_parameter keeps their distinct owning source.
    callable: Vec<CoreCallableParameter>,
    state_parameter: Option<CoreStateParameter>,
    callable_return: Option<CoreCallableReturn>,
    type_parameter: Option<CoreTypeParameter>,
    template: Option<(zil_base::ModuleId, zil_base::NodeId)>,
    foreign: bool,
    cleanup: bool,
    parameters: Vec<Type>,
    result: Type,
    public: bool,
    receiver: Option<Type>,
}
impl Signature {
    fn concrete_id(&self, span: Span) -> R<FunctionId> {
        self.id.ok_or_else(|| {
            error(
                "E-TYPE-APPLICATION",
                span,
                "a template has no concrete function identity before instantiation",
            )
        })
    }
}
fn error(code: &str, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(Phase::Types, code, Some(span), message)
}
fn unsupported(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, Some(span))
}

fn signatures(
    core: &CoreModule,
    limits: &Limits,
    offset: usize,
    exports: Option<&std::collections::BTreeSet<zil_base::NodeId>>,
) -> CompileResult<BTreeMap<String, Signature>> {
    let mut errors = Vec::new();
    if core.schema_version != SCHEMA_VERSION {
        return Err(vec![Diagnostic::error(
            Phase::Types,
            "E-SCHEMA-VERSION",
            None,
            "unknown Core schema",
        )]);
    }
    if !core.unsupported.is_empty() {
        return Err(core
            .unsupported
            .iter()
            .map(|f| Diagnostic::unsupported(Phase::Types, &f.feature, f.span))
            .collect());
    }
    if core.functions.len() > limits.max_functions {
        return Err(vec![Diagnostic::limit(
            Phase::Types,
            "function budget exceeded",
            None,
        )]);
    }
    let mut signatures = BTreeMap::new();
    let mut foreign_work = limits.max_analysis_steps;
    let mut concrete_index = 0usize;
    for f in &core.functions {
        let i = concrete_index;
        if f.callable_parameters.is_empty()
            && f.type_parameters.is_empty()
            && f.state_parameters.is_empty()
        {
            concrete_index += 1;
        }
        if let Err(diagnostic) = retained::validate_factory(f) {
            errors.push(diagnostic);
        }
        if let Err(diagnostic) = helpers::validate_template(f) {
            errors.push(diagnostic);
        }
        if let Err(diagnostic) = store_templates::validate_header(f) {
            errors.push(diagnostic);
        }
        if let Err(diagnostic) = foreign::check_declaration(
            f,
            exports.is_some_and(|ids| ids.contains(&f.id)),
            &mut foreign_work,
        ) {
            errors.push(diagnostic);
        }
        if signatures
            .insert(
                f.name.clone(),
                Signature {
                    callable: state_origins::bounds(f).into_owned(),
                    state_parameter: f.state_parameters.first().cloned(),
                    callable_return: f.callable_return.clone(),
                    type_parameter: f.type_parameters.first().cloned(),
                    template: (!(f.callable_parameters.is_empty()
                        && f.type_parameters.is_empty()
                        && f.state_parameters.is_empty()))
                    .then_some((core.module_id, f.id)),
                    foreign: f.foreign.is_some(),
                    cleanup: core
                        .aggregates
                        .iter()
                        .any(|a| a.teardown.as_ref() == Some(&f.name)),
                    id: (f.type_parameters.is_empty() && f.state_parameters.is_empty()).then_some(
                        FunctionId(u32::try_from(offset + i).map_err(|_| {
                            vec![Diagnostic::limit(
                                Phase::Types,
                                "function identity overflow",
                                Some(f.span),
                            )]
                        })?),
                    ),
                    parameters: f.parameters.iter().map(|p| p.ty.clone()).collect(),
                    result: f.return_type.clone(),
                    public: exports.is_none_or(|ids| ids.contains(&f.id)),
                    receiver: f.receiver.as_ref().map(|r| r.ty.clone()),
                },
            )
            .is_some()
        {
            errors.push(error(
                "E-NAME-DUPLICATE",
                f.span,
                format!("function {} is declared twice", f.name),
            ))
        }
        if !f.type_parameters.is_empty() {
            continue;
        }
        signature_errors(f, &mut errors);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(signatures)
}

fn signature_errors(f: &CoreFunction, errors: &mut Vec<Diagnostic>) {
    if f.return_type.is_package_address() {
        errors.push(unsupported(f.span, "Store element windows are synchronous inputs; address results derived from element windows are not implemented"));
    }
    if f.return_type.is_shared_resource_head_address()
        || (matches!(f.return_type, Type::Address { .. })
            && (f
                .parameters
                .iter()
                .any(|p| p.ty.is_shared_resource_head_address())
                || f.receiver
                    .as_ref()
                    .is_some_and(|r| r.ty.is_shared_resource_head_address())))
    {
        errors.push(unsupported(
            f.span,
            "resource-head windows are synchronous inputs; returning a head or a window derived from a head input is not implemented",
        ));
    }
    if !(f.callable_return.is_some()
        || callable_fields::template(f)
        || f.return_type.is_copy_value()
        || f.return_type.is_scalar_or_address()
        || packages::supported(&f.return_type)
        || errors::supported(&f.return_type)
        || packages::owned_array_supported(&f.return_type)
        || matches!(&f.return_type, Type::Managed(_) if resources::supported(&f.return_type)))
    {
        errors.push(unsupported(
            f.span,
            format!("return type {}", f.return_type.name()),
        ))
    }
    if matches!(f.result_mode, PassingMode::Create | PassingMode::Give)
        && !matches!(f.return_type, Type::Managed(_))
        && !errors::managed_success(&f.return_type)
    {
        errors.push(error(
            "E-RESOURCE-RESULT-MODE",
            f.span,
            "create/give result requires a managed resource or direct owned success payload",
        ));
    }
    if f.receiver.as_ref().is_some_and(|r| {
        !(r.ty.is_copy_value()
            || r.ty.is_scalar_or_address()
            || resources::supported(&r.ty)
            || packages::supported(&r.ty)
            || errors::supported(&r.ty))
    }) {
        errors.push(unsupported(f.span, "receiver capability typing"))
    }
    for (index, p) in f.parameters.iter().enumerate() {
        if f.state_parameters
            .iter()
            .any(|state| state.index as usize == index)
            || f.callable_parameters
                .iter()
                .any(|bound| bound.index as usize == index)
        {
            continue;
        }
        if !(p.ty.is_copy_value()
            || p.ty.is_scalar_or_address()
            || resources::supported(&p.ty)
            || packages::supported(&p.ty)
            || errors::supported(&p.ty)
            || packages::owned_array_supported(&p.ty))
        {
            errors.push(unsupported(
                p.span,
                format!("parameter type {}", p.ty.name()),
            ))
        }
        if matches!(
            p.mode,
            PassingMode::Give
                | PassingMode::Create
                | PassingMode::Describe
                | PassingMode::Borrow
                | PassingMode::BorrowMut
        ) && !(p.mode == PassingMode::Borrow
            && (matches!(&p.ty, Type::Address { mutable: false, inner } if inner.is_address_pointee())
                || p.ty.is_shared_resource_head_address()
                || p.ty.is_shared_package_address()))
            && !(p.mode == PassingMode::BorrowMut
                && matches!(&p.ty, Type::Address { mutable: true, inner } if inner.is_address_pointee() || p.ty.is_package_address()))
            && !(p.mode == PassingMode::Give
                && (matches!(p.ty, Type::Managed(_)) || errors::managed_success(&p.ty)))
            && !(p.mode == PassingMode::Describe && matches!(p.ty, Type::Desc(_)))
        {
            errors.push(unsupported(p.span, "resource/address parameter mode"))
        }
    }
}

pub fn check(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedModule> {
    // The bounded raw traversal precedes signature Type cloning. It preserves
    // lexical receiver diagnostics before unsupported signature admission.
    let inferred = aggregates::resolve(infer_source_closures(core, limits)?, limits)?;
    let core = &inferred;
    let mut signatures = signatures(core, limits, 0, None)?;
    let mut resource_definitions =
        resources::definitions(core, &signatures).map_err(|e| vec![e])?;
    let enum_types = enums::definitions(core);
    let mut errors = Vec::new();
    let mut functions = Vec::new();
    let concrete_count = core
        .functions
        .iter()
        .filter(|f| {
            f.callable_parameters.is_empty()
                && f.type_parameters.is_empty()
                && f.state_parameters.is_empty()
        })
        .count();
    let mut closure_state = closures::State::new(concrete_count, limits);
    closure_state.resource_writes =
        resource_effects::single(core, &signatures, limits).map_err(|e| vec![e])?;
    helpers::install_single(core, &signatures, &enum_types, &mut closure_state, limits)
        .map_err(|e| vec![e])?;
    // Ground each opaque result before its dependents, independent of source
    // declaration order. Concrete environments still use ordinary direct calls.
    let mut work = limits.max_analysis_steps;
    let mut graph = BTreeMap::new();
    let mut factory_functions = BTreeMap::new();
    for f in core
        .functions
        .iter()
        .filter(|f| f.callable_return.is_some())
    {
        let id = signatures[&f.name]
            .concrete_id(f.span)
            .map_err(|e| vec![e])?;
        let dependencies =
            retained::factory_dependencies(f, &signatures, None, &closure_state, &mut work, limits)
                .map_err(|e| vec![e])?;
        graph.insert(id, (f.span, dependencies));
        factory_functions.insert(id, f);
    }
    let factory_order = retained::order(graph, &mut work).map_err(|e| vec![e])?;
    let ordered = factory_order
        .into_iter()
        .map(|id| factory_functions[&id])
        .chain(core.functions.iter().filter(|f| {
            (f.callable_parameters.is_empty()
                && f.type_parameters.is_empty()
                && f.state_parameters.is_empty())
                && f.callable_return.is_none()
        }));
    for f in ordered {
        match Builder::new(
            f,
            signatures[&f.name]
                .concrete_id(f.span)
                .map_err(|e| vec![e])?,
            &signatures,
            limits,
            &mut closure_state,
        )
        .and_then(|mut b| {
            b.enums = Some(&enum_types);
            b.finish(&f.body)
        }) {
            Ok(typed) => {
                if f.callable_return.is_some() {
                    signatures
                        .get_mut(&f.name)
                        .expect("declared factory")
                        .result = typed.return_type.clone();
                    helpers::resolve_factory_result(
                        &mut closure_state,
                        core.module_id,
                        &f.name,
                        &typed.return_type,
                        f.span,
                    )
                    .map_err(|e| vec![e])?;
                }
                functions.push(typed);
            }
            Err(e) => errors.push(e),
        }
        if !errors.is_empty() {
            return Err(errors);
        }
    }
    let bound_elements =
        store_templates::bound_elements(&mut closure_state).map_err(|e| vec![e])?;
    let (closures, synthesized) = closure_state.finish().map_err(|e| vec![e])?;
    functions.extend(synthesized);
    functions.sort_by_key(|f| f.id);
    byte_buffer::add_definition(&functions, &mut resource_definitions, limits)
        .map_err(|e| vec![e])?;
    dynamic_store::add_definitions(
        &functions,
        &mut resource_definitions,
        &closures,
        &bound_elements,
        limits,
    )
    .map_err(|e| vec![e])?;
    let mut module = TypedModule {
        resources: resource_definitions,
        schema_version: SCHEMA_VERSION,
        module_id: core.module_id,
        source_fingerprint: core.source_fingerprint,
        target: target.clone(),
        functions,
        closures,
        unsupported: vec![],
    };
    lending::populate(&mut module, limits).map_err(|e| vec![e])?;
    Ok(module)
}
struct Builder<'a, 'c> {
    function: TypedFunction,
    helper_bounds: Vec<CoreCallableParameter>,
    // Owning State specialization removes symbolic parameters, but a direct
    // field invocation still uses the mode promised by that static interface.
    state_callable_mode: Option<(ClosureId, CallableKind)>,
    state_uses: Option<state_origins::Uses>,
    factory_bound: Option<CoreCallableReturn>,
    signatures: &'a BTreeMap<String, Signature>,
    imports: Option<&'a program::Imports<'a>>,
    enums: Option<&'a BTreeMap<String, Type>>,
    limits: &'a Limits,
    scopes: Vec<BTreeMap<String, LocalId>>,
    scope_ids: Vec<ScopeId>,
    scope_entries: BTreeMap<ScopeId, BlockId>,
    next_scope: u32,
    current: Option<BlockId>,
    loops: Vec<LoopContext>,
    instructions: usize,
    match_work_left: usize,
    closures: &'c mut closures::State,
    first_closure: usize,
    borrowed_captures: BTreeMap<LocalId, Type>,
    written_captures: BTreeMap<LocalId, u32>,
    stored_copy_captures: BTreeMap<LocalId, u32>,
    address_written_captures: BTreeMap<LocalId, u32>,
    managed_captures: BTreeMap<LocalId, (u32, Type)>,
    closure_captures: BTreeMap<LocalId, (u32, Type)>,
    closure_slots: BTreeMap<LocalId, ClosureId>,
    package_captures: BTreeMap<LocalId, u32>,
    temporary_locals: std::collections::BTreeSet<LocalId>,
    unsafe_regions: Vec<u32>,
    next_unsafe_region: u32,
    resource_temps: Vec<LocalId>,
    sequenced_values: std::collections::BTreeSet<LocalId>,
    managed_slots: std::collections::BTreeSet<LocalId>,
    managed_slot_work: usize,
    managed_rebindings: std::collections::BTreeSet<managed_slots::BindingSite>,
}
struct LoopContext {
    continue_target: BlockId,
    break_target: BlockId,
    has_break: bool,
    has_continue: bool,
    scope_depth: usize,
    unsafe_depth: usize,
}
impl<'a, 'c> Builder<'a, 'c> {
    fn new(
        f: &CoreFunction,
        id: FunctionId,
        signatures: &'a BTreeMap<String, Signature>,
        limits: &'a Limits,
        closures: &'c mut closures::State,
    ) -> R<Self> {
        closures.block(f.span)?;
        let first_closure = closures.pending.len();
        let mut b = Self {
            function: TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                result_mode: f.result_mode,
                foreign: f.foreign.clone(),
                id,
                name: f.name.clone(),
                environment: None,
                parameters: vec![],
                return_type: f.return_type.clone(),
                locals: vec![],
                values: vec![],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Unreachable,
                }],
                entry: BlockId(0),
                span: f.span,
            },
            helper_bounds: f.callable_parameters.clone(),
            state_callable_mode: None,
            state_uses: None,
            factory_bound: f.callable_return.clone(),
            signatures,
            imports: None,
            enums: None,
            limits,
            scopes: vec![BTreeMap::new()],
            scope_ids: vec![ScopeId(0)],
            scope_entries: BTreeMap::from([(ScopeId(0), BlockId(0))]),
            next_scope: 1,
            current: Some(BlockId(0)),
            loops: vec![],
            instructions: 0,
            match_work_left: limits.max_analysis_steps,
            closures,
            first_closure,
            borrowed_captures: BTreeMap::new(),
            written_captures: BTreeMap::new(),
            stored_copy_captures: BTreeMap::new(),
            address_written_captures: BTreeMap::new(),
            managed_captures: BTreeMap::new(),
            closure_captures: BTreeMap::new(),
            closure_slots: BTreeMap::new(),
            package_captures: BTreeMap::new(),
            temporary_locals: std::collections::BTreeSet::new(),
            unsafe_regions: Vec::new(),
            next_unsafe_region: 0,
            resource_temps: Vec::new(),
            sequenced_values: std::collections::BTreeSet::new(),
            managed_slots: std::collections::BTreeSet::new(),
            managed_slot_work: limits.max_analysis_steps,
            managed_rebindings: managed_slots::reassigned_bindings(&f.body, limits)?,
        };
        if f.foreign.is_none() {
            b.emit(
                InstructionKind::ScopeEnter { scope: ScopeId(0) },
                None,
                f.span,
            )?;
        }
        for p in &f.parameters {
            let local = b.declare_local(&p.name, p.ty.clone(), false, p.span, false)?;
            b.function.parameters.push(local)
        }
        Ok(b)
    }
    fn resolve_callee(&self, callee: &CoreExpr) -> R<&Signature> {
        match &callee.kind {
            CoreExprKind::Name(name) => {
                if self.lookup(name).is_some() {
                    return Err(unsupported(callee.span, "calling a stored callable"));
                }
                self.signatures.get(name).ok_or_else(|| {
                    error(
                        "E-UNKNOWN-NAME",
                        callee.span,
                        format!("unknown function {name}"),
                    )
                })
            }
            CoreExprKind::Member { object, member } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return Err(unsupported(callee.span, "indirect callable dispatch"));
                };
                if self.lookup(alias).is_some() {
                    return Err(unsupported(callee.span, "calling a stored member"));
                }
                if let Some(signature) = self.signatures.get(&format!("{alias}.{member}")) {
                    return Ok(signature);
                }
                let imports = self.imports.ok_or_else(|| {
                    unsupported(callee.span, "module-qualified call requires a project")
                })?;
                let module = imports.aliases.get(alias).ok_or_else(|| {
                    error(
                        "E-UNKNOWN-NAME",
                        object.span,
                        format!("unknown module alias {alias}"),
                    )
                })?;
                let signature = imports.signatures[module].get(member).ok_or_else(|| {
                    error(
                        "E-MODULE-MEMBER",
                        callee.span,
                        format!("module {alias} has no function {member}"),
                    )
                })?;
                if !signature.public {
                    return Err(error(
                        "E-MODULE-PRIVATE",
                        callee.span,
                        format!("function {alias}.{member} is private"),
                    ));
                }
                Ok(signature)
            }
            _ => Err(unsupported(callee.span, "indirect callable dispatch")),
        }
    }
    fn finish(mut self, body: &CoreBlock) -> R<TypedFunction> {
        if self.function.foreign.is_some() {
            if !body.statements.is_empty() {
                return Err(unsupported(
                    body.span,
                    "foreign declarations cannot carry a body",
                ));
            }
            self.function.blocks.clear();
            return Ok(self.function);
        }
        self.block(body)?;
        if self.current.is_some() {
            if self.function.return_type == Type::Unit {
                self.end_scopes_kind(0, CleanupExitKind::Return, body.span)?;
                self.terminate(Terminator::Return(None));
            } else {
                return Err(error(
                    "E-MISSING-RETURN",
                    body.span,
                    "not all paths return a value",
                ));
            }
        }
        if self.factory_bound.is_some() && self.function.callable_return_contract.is_none() {
            return Err(unsupported(
                body.span,
                "opaque callable factory must establish one returned hidden closure type",
            ));
        }
        if matches!(self.function.return_type, Type::Address { .. }) {
            self.closures.snapshot(
                self.function
                    .locals
                    .len()
                    .saturating_add(self.function.values.len()),
                body.span,
            )?;
            if self
                .function
                .locals
                .iter()
                .map(|local| &local.ty)
                .chain(&self.function.values)
                .any(Type::is_package_address)
            {
                return Err(unsupported(
                    body.span,
                    "Store element windows are synchronous inputs; address results derived from element windows are not implemented",
                ));
            }
        }
        for i in self.first_closure..self.closures.pending.len() {
            self.materialize_closure(ClosureId(i as u32))?;
        }
        Ok(self.function)
    }
    fn local(&mut self, name: &str, ty: Type, mutable: bool, span: Span) -> R<LocalId> {
        self.declare_local(name, ty, mutable, span, true)
    }
    fn declare_local(
        &mut self,
        name: &str,
        ty: Type,
        mutable: bool,
        span: Span,
        start: bool,
    ) -> R<LocalId> {
        if self.function.locals.len() >= self.limits.max_locals {
            return Err(Diagnostic::limit(
                Phase::Types,
                "local budget exceeded",
                Some(span),
            ));
        }
        self.closures.local(span)?;
        let scope = self.scopes.last_mut().expect("scope exists");
        if scope.contains_key(name) {
            return Err(error(
                "E-NAME-DUPLICATE",
                span,
                format!("duplicate local {name}"),
            ));
        }
        let id = LocalId(self.function.locals.len() as u32);
        self.function.locals.push(LocalDecl {
            id,
            name: name.into(),
            ty,
            mutable,
            span,
        });
        scope.insert(name.into(), id);
        if start {
            self.emit(InstructionKind::StorageLive(id), None, span)?;
        }
        Ok(id)
    }
    fn lookup(&self, name: &str) -> Option<LocalId> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }
    fn temp(&mut self, ty: Type, span: Span) -> R<LocalId> {
        let name = format!("$temporary{}", self.function.locals.len());
        let local = self.declare_local(&name, ty, true, span, false)?;
        self.temporary_locals.insert(local);
        // Compiler slots are reserved at their lexical entry so every branch
        // has the same storage set. Initialization/Drop remain at evaluation
        // and full-expression boundaries; no carrier exists before Store.
        self.emit(InstructionKind::StorageLive(local), None, span)?;
        let current = self.current.expect("live block");
        let instruction = self.function.blocks[current.0 as usize]
            .instructions
            .pop()
            .expect("storage live");
        let scope = *self.scope_ids.last().expect("scope");
        let entry = self.scope_entries[&scope];
        let instructions = &mut self.function.blocks[entry.0 as usize].instructions;
        self.closures.snapshot(instructions.len(), span)?;
        let mut position = instructions
            .iter()
            .position(
                |i| matches!(i.kind, InstructionKind::ScopeEnter { scope: id } if id == scope),
            )
            .expect("scope entry");
        while instructions.get(position + 1).is_some_and(|instruction| {
            matches!(instruction.kind, InstructionKind::BindPackageCapture { .. })
        }) {
            position += 1;
        }
        instructions.insert(position + 1, instruction);
        // Earlier exits already emitted in this scope also see the reserved
        // vacant slot. Close its storage there without inventing a cleanup.
        for block in &mut self.function.blocks {
            self.closures.snapshot(block.instructions.len(), span)?;
            if let Some(position) = block.instructions.iter().position(
                |i| matches!(i.kind, InstructionKind::ScopeExit { scope: id, .. } if id == scope),
            ) {
                self.closures.instruction(false, span)?;
                self.instructions += 1;
                if self.instructions > self.limits.max_instructions {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "instruction budget exceeded",
                        Some(span),
                    ));
                }
                block.instructions.insert(
                    position,
                    Instruction {
                        result: None,
                        kind: InstructionKind::StorageDead(local),
                        span,
                    },
                );
            }
        }
        if self.has_cleanup(&self.function.locals[local.0 as usize].ty) {
            self.resource_temps.push(local);
        }
        Ok(local)
    }
    fn block_id(&mut self) -> R<BlockId> {
        if self.function.blocks.len() >= self.limits.max_blocks {
            return Err(Diagnostic::limit(
                Phase::Types,
                "block budget exceeded",
                None,
            ));
        }
        self.closures.block(self.function.span)?;
        let id = BlockId(self.function.blocks.len() as u32);
        self.function.blocks.push(BasicBlock {
            id,
            instructions: vec![],
            terminator: Terminator::Unreachable,
        });
        Ok(id)
    }
    fn terminate(&mut self, t: Terminator) {
        let current = self.current.take().expect("live block");
        self.function.blocks[current.0 as usize].terminator = t;
    }
    fn emit(&mut self, kind: InstructionKind, ty: Option<Type>, span: Span) -> R<Option<ValueId>> {
        self.closures.instruction(ty.is_some(), span)?;
        self.instructions += 1;
        if self.instructions > self.limits.max_instructions {
            return Err(Diagnostic::limit(
                Phase::Types,
                "instruction budget exceeded",
                Some(span),
            ));
        }
        let result = if let Some(ty) = ty {
            if self.function.values.len() >= self.limits.max_values {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "value budget exceeded",
                    Some(span),
                ));
            }
            let id = ValueId(self.function.values.len() as u32);
            self.function.values.push(ty);
            Some(id)
        } else {
            None
        };
        self.function.blocks[self.current.expect("live block").0 as usize]
            .instructions
            .push(Instruction { result, kind, span });
        Ok(result)
    }
    fn value(&mut self, kind: InstructionKind, ty: Type, span: Span) -> R<ValueId> {
        Ok(self.emit(kind, Some(ty), span)?.expect("typed value"))
    }
    fn constant(&mut self, c: Constant, span: Span) -> R<(ValueId, Type)> {
        let t = c.ty();
        Ok((
            self.value(InstructionKind::Constant(c), t.clone(), span)?,
            t,
        ))
    }
    fn store(&mut self, local: LocalId, value: ValueId, span: Span) -> R<()> {
        self.emit(InstructionKind::Store { local, value }, None, span)?;
        if let Some(index) = self.written_captures.get(&local).copied() {
            self.emit(InstructionKind::WriteCapture { index, value }, None, span)?;
        }
        Ok(())
    }
    fn push_scope(&mut self, span: Span) -> R<()> {
        let scope = ScopeId(self.next_scope);
        self.next_scope = self.next_scope.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "scope identity overflow", Some(span))
        })?;
        self.scope_entries
            .insert(scope, self.current.expect("live block"));
        self.emit(InstructionKind::ScopeEnter { scope }, None, span)?;
        self.scopes.push(BTreeMap::new());
        self.scope_ids.push(scope);
        Ok(())
    }
    fn pop_scope(&mut self) {
        let locals = self.scopes.pop().expect("scope");
        self.resource_temps
            .retain(|local| !locals.values().any(|l| l == local));
        self.scope_ids.pop();
    }
    fn end_scopes(&mut self, depth: usize, span: Span) -> R<()> {
        self.end_scopes_kind(depth, CleanupExitKind::Fallthrough, span)
    }
    fn end_scopes_kind(&mut self, depth: usize, kind: CleanupExitKind, span: Span) -> R<()> {
        for index in (depth..self.scopes.len()).rev() {
            let mut locals = self.scopes[index].values().copied().collect::<Vec<_>>();
            locals.sort_unstable_by(|a, b| b.cmp(a));
            for local in locals {
                if let Some(index) = self.package_captures.get(&local).copied() {
                    self.emit(
                        InstructionKind::EndPackageCapture { local, index },
                        None,
                        span,
                    )?;
                    continue;
                }
                if !self.temporary_locals.contains(&local) || self.resource_temps.contains(&local) {
                    self.drop_owned_local(local, span)?;
                }
                if let Some(capture) = self.address_written_captures.get(&local).copied() {
                    // Copy captures have activation locals. Mutations through
                    // &mut (ordinary or foreign calls, projections or aliases)
                    // bypass store(), so publish the final local before it dies.
                    // The same path handles normal, early and Error exits.
                    let ty = self.function.locals[local.0 as usize].ty.clone();
                    let value = self.value(InstructionKind::Load(local), ty, span)?;
                    self.emit(
                        InstructionKind::WriteCapture {
                            index: capture,
                            value,
                        },
                        None,
                        span,
                    )?;
                }
                self.emit(InstructionKind::StorageDead(local), None, span)?;
            }
            self.emit(
                InstructionKind::ScopeExit {
                    scope: self.scope_ids[index],
                    kind,
                },
                None,
                span,
            )?;
        }
        Ok(())
    }
    fn block(&mut self, block: &CoreBlock) -> R<()> {
        self.push_scope(block.span)?;
        for s in &block.statements {
            if self.current.is_none() {
                return Err(unsupported(s.span, "unreachable-statement type checking"));
            }
            self.stmt(s)?;
        }
        if self.current.is_some() {
            self.end_scopes(self.scopes.len() - 1, block.span)?;
        }
        self.pop_scope();
        Ok(())
    }
    fn stmt(&mut self, s: &CoreStatement) -> R<()> {
        let rewritten = self.rewrite_managed_statement(s)?;
        self.stmt_inner(rewritten.as_ref().unwrap_or(s))?;
        if self.current.is_some() {
            self.finish_resource_temps(s.span)?;
        }
        Ok(())
    }
    fn stmt_inner(&mut self, s: &CoreStatement) -> R<()> {
        match &s.kind {
            CoreStatementKind::Binding {
                name,
                mutable,
                ty,
                initializer,
            } => {
                let Some(e) = initializer else {
                    return Err(unsupported(s.span, "uninitialized source binding"));
                };
                // A resource annotation names the nominal type; create/give
                // determines whether this binding holds the carrier or a description.
                let store_constructor = matches!(&e.kind, CoreExprKind::Call { callee, .. } if dynamic_store::primitive(callee) == Some("store_new"));
                let wanted = ty
                    .as_ref()
                    .filter(|t| !matches!(t, Type::Desc(_)) || store_constructor);
                let (v, t) = self.transfer_expr(e, wanted)?;
                if let Some(wanted) = ty
                    && !matches!((&t, wanted), (Type::Managed(a), Type::Desc(b)) if a == b)
                {
                    self.same(&t, wanted, s.span)?;
                }
                if self.has_cleanup(&t) {
                    // The initializer owns its completed result in SSA. End
                    // its temporary scope before committing a newer lexical
                    // owner; the binding is not visible during initialization.
                    self.finish_resource_temps(s.span)?;
                }
                if *mutable
                    && matches!(t, Type::Managed(_))
                    && self
                        .managed_rebindings
                        .contains(&(s.id, s.span, name.clone()))
                {
                    return self.managed_slot(name, t, v, s.span);
                }
                if *mutable
                    && self.owned_closure(&t)
                    && self
                        .managed_rebindings
                        .contains(&(s.id, s.span, name.clone()))
                    && let Type::Closure(id) = t
                {
                    return self.closure_slot(name, id, v, s.span);
                }
                let local = self.local(name, t, *mutable, s.span)?;
                self.store(local, v, s.span)?;
            }
            CoreStatementKind::Assign { target, value } => {
                if self.package_assign(target, value)? {
                    return Ok(());
                }
                if self.resource_assign(target, value)? {
                    return Ok(());
                }
                if self.indirect_place(target) {
                    return self.assign_address(target, value);
                }
                let CoreExprKind::Name(name) = &target.kind else {
                    return self.assign_field(target, value);
                };
                let l = self.lookup(name).ok_or_else(|| {
                    error(
                        "E-UNKNOWN-NAME",
                        target.span,
                        format!("unknown variable {name}"),
                    )
                })?;
                let d = self.function.locals[l.0 as usize].clone();
                if self.closure_slots.contains_key(&l) {
                    return self.assign_closure_slot(l, value, s.span);
                }
                if matches!(d.ty, Type::Managed(_)) {
                    return Err(unsupported(
                        s.span,
                        "resource/package replacement is not implemented",
                    ));
                }
                if !d.mutable {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        target.span,
                        format!("{name} is not mutable"),
                    ));
                }
                if errors::owned(&d.ty) {
                    return Err(unsupported(
                        s.span,
                        "owned error-union whole-local reassignment requires a retained slot",
                    ));
                }
                let (v, t) = self.transfer_expr(value, Some(&d.ty))?;
                self.same(&t, &d.ty, value.span)?;
                self.store(l, v, s.span)?;
            }
            CoreStatementKind::Expression(e) => {
                let (value, ty) = if self.known_type(e).is_some_and(|ty| self.owned_closure(&ty)) {
                    self.transfer_expr(e, None)?
                } else {
                    self.expr(e, None)?
                };
                self.discard_resource(value, ty, e.span)?;
            }
            CoreStatementKind::Return(e) => {
                if self.factory_bound.is_some() {
                    return self.factory_return(e.as_ref(), s.span);
                }
                let wanted = self.function.return_type.clone();
                let value = if let Some(e) = e {
                    let (v, t) = self.transfer_expr(e, Some(&wanted))?;
                    self.same(&t, &wanted, e.span)?;
                    if t == Type::Unit { None } else { Some(v) }
                } else if matches!(&wanted, Type::ErrorUnion { .. }) {
                    let (v, ty) = self.constant(Constant::Unit, s.span)?;
                    Some(self.inject_error_union(v, ty, &wanted, s.span)?.0)
                } else {
                    self.same(&Type::Unit, &wanted, s.span)?;
                    None
                };
                self.return_value(value, s.span)?;
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let (c, t) = self.expr(condition, Some(&Type::Bool))?;
                self.same(&t, &Type::Bool, condition.span)?;
                self.finish_resource_temps(condition.span)?;
                let tb = self.block_id()?;
                let eb = self.block_id()?;
                self.terminate(Terminator::Branch {
                    condition: c,
                    then_block: tb,
                    else_block: eb,
                });
                self.current = Some(tb);
                self.block(then_body)?;
                let then_end = self.current.take();
                self.current = Some(eb);
                if let Some(body) = else_body {
                    self.block(body)?
                }
                let else_end = self.current.take();
                if then_end.is_some() || else_end.is_some() {
                    let join = self.block_id()?;
                    for end in [then_end, else_end].into_iter().flatten() {
                        self.function.blocks[end.0 as usize].terminator = Terminator::Jump(join)
                    }
                    self.current = Some(join)
                }
            }
            CoreStatementKind::While { condition, body } => {
                let head = self.block_id()?;
                let loop_body = self.block_id()?;
                let after = self.block_id()?;
                self.terminate(Terminator::Jump(head));
                self.current = Some(head);
                let (c, t) = self.expr(condition, Some(&Type::Bool))?;
                self.same(&t, &Type::Bool, condition.span)?;
                self.finish_resource_temps(condition.span)?;
                let unconditional = matches!(condition.kind, CoreExprKind::Bool(true));
                self.terminate(if unconditional {
                    Terminator::Jump(loop_body)
                } else {
                    Terminator::Branch {
                        condition: c,
                        then_block: loop_body,
                        else_block: after,
                    }
                });
                self.loops.push(LoopContext {
                    continue_target: head,
                    break_target: after,
                    has_break: false,
                    has_continue: false,
                    scope_depth: self.scopes.len(),
                    unsafe_depth: self.unsafe_regions.len(),
                });
                self.current = Some(loop_body);
                self.block(body)?;
                if self.current.is_some() {
                    self.terminate(Terminator::Jump(head))
                }
                let context = self.loops.pop().expect("loop context");
                self.current = if unconditional && !context.has_break {
                    None
                } else {
                    Some(after)
                };
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.if_let(pattern, value, then_body, else_body.as_ref(), s.span)?;
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                self.while_let(pattern, value, body, s.span)?;
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.for_range(name, start, end, body, s.span)?;
            }
            CoreStatementKind::Match { value, arms } => {
                self.match_statement(value, arms, s.span)?
            }
            CoreStatementKind::Block(b) => self.block(b)?,
            CoreStatementKind::Unsafe(b) => self.unsafe_block(b)?,
            CoreStatementKind::Break | CoreStatementKind::Continue => {
                let Some(context) = self.loops.last_mut() else {
                    return Err(error(
                        "E-LOOP-CONTROL",
                        s.span,
                        "loop control outside a loop",
                    ));
                };
                let target = if matches!(s.kind, CoreStatementKind::Break) {
                    context.has_break = true;
                    context.break_target
                } else {
                    context.has_continue = true;
                    context.continue_target
                };
                let depth = context.scope_depth;
                let unsafe_depth = context.unsafe_depth;
                let kind = if matches!(s.kind, CoreStatementKind::Break) {
                    CleanupExitKind::Break
                } else {
                    CleanupExitKind::Continue
                };
                self.end_unsafe_regions(unsafe_depth, s.span)?;
                self.end_scopes_kind(depth, kind, s.span)?;
                self.terminate(Terminator::Jump(target));
            }
            CoreStatementKind::Unsupported(feature) => return Err(unsupported(s.span, feature)),
        }
        Ok(())
    }
    fn for_range(
        &mut self,
        name: &str,
        start: &CoreExpr,
        end: &CoreExpr,
        body: &CoreBlock,
        span: Span,
    ) -> R<()> {
        // Type lookup is pure. Emit each endpoint once, in the outer scope and
        // in source order, before making the iteration name visible.
        let hint = self
            .range_hint(start, 0)
            .or_else(|| self.range_hint(end, 0));
        let (start_value, ty) = self.expr(start, hint.as_ref())?;
        if !ty.is_integer() {
            return Err(error(
                "E-OPERATOR-TYPE",
                start.span,
                "integer range endpoints must have type u8, u16, i32 or i64",
            ));
        }
        let (end_value, end_ty) = self.expr(end, Some(&ty))?;
        self.same(&end_ty, &ty, end.span)?;
        self.finish_resource_temps(span)?;

        self.push_scope(span)?;
        let counter = self.temp(ty.clone(), span)?;
        let iteration = self.local(name, ty.clone(), false, span)?;
        self.store(counter, start_value, span)?;
        let head = self.block_id()?;
        let loop_body = self.block_id()?;
        let step = self.block_id()?;
        let after = self.block_id()?;
        self.terminate(Terminator::Jump(head));
        self.current = Some(head);
        let current = self.value(InstructionKind::Load(counter), ty.clone(), span)?;
        let condition = self.value(
            InstructionKind::Binary {
                op: BinaryOp::Less,
                left: current,
                right: end_value,
            },
            Type::Bool,
            span,
        )?;
        self.terminate(Terminator::Branch {
            condition,
            then_block: loop_body,
            else_block: after,
        });
        self.loops.push(LoopContext {
            continue_target: step,
            break_target: after,
            has_break: false,
            has_continue: false,
            scope_depth: self.scopes.len(),
            unsafe_depth: self.unsafe_regions.len(),
        });
        self.current = Some(loop_body);
        self.store(iteration, current, span)?;
        self.block(body)?;
        let falls_through = self.current.is_some();
        if falls_through {
            self.terminate(Terminator::Jump(step));
        }
        let context = self.loops.pop().expect("loop context");
        if falls_through || context.has_continue {
            self.current = Some(step);
            let one = match ty {
                Type::U8 => Constant::U8(1),
                Type::U16 => Constant::U16(1),
                Type::I32 => Constant::I32(1),
                Type::I64 => Constant::I64(1),
                _ => unreachable!("integer range type checked above"),
            };
            let (one, _) = self.constant(one, span)?;
            // The body is entered only when counter < end, so counter + 1
            // cannot exceed end, including when end is the integer maximum.
            let next = self.value(
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: current,
                    right: one,
                },
                ty,
                span,
            )?;
            self.store(counter, next, span)?;
            self.terminate(Terminator::Jump(head));
        }
        // A body that always exits has no step edge. Its unused step block
        // stays empty, without SSA uses that would lack a reachable dominator.
        self.current = Some(after);
        self.end_scopes(self.scopes.len() - 1, span)?;
        self.pop_scope();
        Ok(())
    }
    fn range_hint(&self, e: &CoreExpr, depth: usize) -> Option<Type> {
        if depth > self.limits.max_call_depth {
            return None;
        }
        match &e.kind {
            CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Some(Type::Bool),
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::Dereference,
                ..
            } => self.known_type(e),
            CoreExprKind::Unary { operand, .. } => self.range_hint(operand, depth + 1),
            CoreExprKind::Sequence { value, .. } => self.range_hint(value, depth + 1),
            CoreExprKind::Binary {
                op:
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or,
                ..
            } => Some(Type::Bool),
            CoreExprKind::Binary { left, right, .. } => self
                .range_hint(left, depth + 1)
                .or_else(|| self.range_hint(right, depth + 1)),
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => self
                .range_hint(then_value, depth + 1)
                .or_else(|| self.range_hint(else_value, depth + 1)),
            _ => self.known_type(e),
        }
    }
    fn same(&self, actual: &Type, expected: &Type, span: Span) -> R<()> {
        if actual == expected {
            Ok(())
        } else if matches!(actual, Type::Closure(_)) && !matches!(expected, Type::Closure(_)) {
            Err(unsupported(
                span,
                "closure values cannot escape through ordinary scalar signatures or scalar storage",
            ))
        } else {
            Err(error(
                "E-TYPE-MISMATCH",
                span,
                format!("expected {}, found {}", expected.name(), actual.name()),
            ))
        }
    }
    fn expr(&mut self, e: &CoreExpr, expected: Option<&Type>) -> R<(ValueId, Type)> {
        if let Some(union @ Type::ErrorUnion { .. }) = expected
            && !matches!(
                &e.kind,
                CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            )
        {
            // A sealed Store constructor has no operands from which to infer
            // its element. A direct owned-success signature is an explicit
            // expected resource shape, just like a direct create signature.
            let payload_expected = match (&e.kind, union) {
                (CoreExprKind::Call { callee, .. }, Type::ErrorUnion { result, .. })
                    if dynamic_store::primitive(callee) == Some("store_new")
                        && matches!(result.as_ref(), Type::Managed(payload)
                            if matches!(payload.as_ref(), Type::OwnedStore { .. })) =>
                {
                    Some(result.as_ref())
                }
                _ => None,
            };
            let (value, actual) = self.expr_raw(e, payload_expected)?;
            return self.inject_error_union(value, actual, union, e.span);
        }
        self.expr_raw(e, expected)
    }
    fn expr_raw(&mut self, e: &CoreExpr, expected: Option<&Type>) -> R<(ValueId, Type)> {
        let result = match &e.kind {
            CoreExprKind::Literal(value) => {
                foreign::check_literal(value, e.span)?;
                self.constant(value.clone(), e.span)?
            }
            CoreExprKind::Ascribed { ty, value } => self.expr(value, Some(ty))?,
            CoreExprKind::IntegerConvert { value, target } => {
                if !target.is_integer() {
                    return Err(error(
                        "E-INTEGER-CONVERSION",
                        e.span,
                        "conversion target must be u8, u16, i32 or i64",
                    ));
                }
                // The conversion target never changes the operand's type.
                let (value, source) = self.expr(value, None)?;
                if !source.is_integer() {
                    return Err(error(
                        "E-INTEGER-CONVERSION",
                        e.span,
                        "conversion operand must be an integer",
                    ));
                }
                let result = self.value(
                    InstructionKind::ConvertInteger {
                        value,
                        target: target.clone(),
                    },
                    target.clone(),
                    e.span,
                )?;
                (result, target.clone())
            }
            CoreExprKind::Try(operand) => self.try_expr(operand, e.span)?,
            CoreExprKind::Integer(text) => {
                self.constant(integer(text, expected, false, e.span)?, e.span)?
            }
            CoreExprKind::Bool(v) => self.constant(Constant::Bool(*v), e.span)?,
            CoreExprKind::Unit => self.constant(Constant::Unit, e.span)?,
            CoreExprKind::Name(name) => {
                if let Some(local) = self.lookup(name) {
                    if self.closure_slots.contains_key(&local) {
                        return self.take_closure_slot(local, e.span);
                    }
                    let ty = self.function.locals[local.0 as usize].ty.clone();
                    if let Type::Managed(payload) = ty {
                        let desc = Type::Desc(payload);
                        return Ok((
                            self.value(
                                InstructionKind::DescribeLocal { local },
                                desc.clone(),
                                e.span,
                            )?,
                            desc,
                        ));
                    }
                    let kind = if let Some(index) = self.stored_copy_captures.get(&local) {
                        InstructionKind::ReadCapture { index: *index }
                    } else if matches!(ty, Type::Package { .. })
                        || packages::owned_array_supported(&ty)
                        || errors::owned(&ty)
                    {
                        InstructionKind::TakeLocal { local }
                    } else {
                        InstructionKind::Load(local)
                    };
                    let value = self.value(kind, ty.clone(), e.span)?;
                    if let Some(inner) = self.borrowed_captures.get(&local).cloned() {
                        (
                            self.value(
                                InstructionKind::ReadAddress { address: value },
                                inner.clone(),
                                e.span,
                            )?,
                            inner,
                        )
                    } else {
                        (value, ty)
                    }
                } else {
                    self.function_reference(e)?
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => self.resource_give(operand, e.span)?,
            CoreExprKind::Unary {
                op: UnaryOp::Borrow,
                operand,
            } => self.borrow_place(operand)?,
            CoreExprKind::Unary {
                op: UnaryOp::BorrowMut,
                operand,
            } => self.borrow_place_mode(operand, true)?,
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let (address, ty) = self.expr(operand, None)?;
                let Type::Address { inner, .. } = ty else {
                    return Err(error(
                        "E-OPERATOR-TYPE",
                        e.span,
                        "dereference requires an address",
                    ));
                };
                if !inner.is_copy_value() {
                    return Err(unsupported(
                        e.span,
                        "dereference requires a finite Copy pointee",
                    ));
                }
                let inner = *inner;
                (
                    self.value(
                        InstructionKind::ReadAddress { address },
                        inner.clone(),
                        e.span,
                    )?,
                    inner,
                )
            }
            CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if matches!(operand.kind, CoreExprKind::Integer(_)) => {
                let CoreExprKind::Integer(text) = &operand.kind else {
                    unreachable!()
                };
                self.constant(integer(text, expected, true, e.span)?, e.span)?
            }
            CoreExprKind::Unary { op, operand } => {
                if !matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot) {
                    return Err(unsupported(e.span, "address/resource unary operation"));
                }
                let want = if *op == UnaryOp::Not {
                    Some(&Type::Bool)
                } else {
                    expected
                };
                let (v, t) = self.expr(operand, want)?;
                if (*op == UnaryOp::Not && t != Type::Bool)
                    || (*op != UnaryOp::Not && !t.is_integer())
                {
                    return Err(error(
                        "E-OPERATOR-TYPE",
                        e.span,
                        "unary operator operand type",
                    ));
                }
                (
                    self.value(
                        InstructionKind::Unary {
                            op: *op,
                            operand: v,
                        },
                        t.clone(),
                        e.span,
                    )?,
                    t,
                )
            }
            CoreExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => return self.short_circuit(left, right, false, e.span),
            CoreExprKind::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => return self.short_circuit(left, right, true, e.span),
            CoreExprKind::Binary { op, left, right } => {
                let comparison = matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                );
                let operand_hint = self.known_type(left).or_else(|| self.known_type(right));
                let numeric_expected = if comparison {
                    operand_hint.as_ref()
                } else {
                    expected.or(operand_hint.as_ref())
                };
                let (l, lt) = self.expr(left, numeric_expected)?;
                let (r, rt) = self.expr(right, Some(&lt))?;
                self.same(&rt, &lt, e.span)?;
                let equality = matches!(op, BinaryOp::Equal | BinaryOp::NotEqual);
                if !lt.is_integer() && !(equality && matches!(lt, Type::Bool | Type::ForeignHandle))
                {
                    return Err(error(
                        "E-OPERATOR-TYPE",
                        e.span,
                        "binary operator operand type",
                    ));
                }
                let t = if comparison { Type::Bool } else { lt };
                (
                    self.value(
                        InstructionKind::Binary {
                            op: *op,
                            left: l,
                            right: r,
                        },
                        t.clone(),
                        e.span,
                    )?,
                    t,
                )
            }
            CoreExprKind::Call { callee, arguments } => {
                if let CoreExprKind::TypeApply {
                    callee: target,
                    type_arguments,
                } = &callee.kind
                {
                    return self.call_type_template(
                        target,
                        type_arguments,
                        arguments,
                        expected,
                        e.span,
                    );
                }
                if let Some(name) = dynamic_store::primitive(callee) {
                    return self.dynamic_store_call(name, arguments, expected, e.span);
                }
                if let Some(name) = byte_buffer::primitive(callee) {
                    return self.byte_buffer_call(name, arguments, expected, e.span);
                }
                if let CoreExprKind::Member { object, member } = &callee.kind
                    && member == "len"
                    && self
                        .known_type(object)
                        .is_some_and(|ty| ty.is_slice_address())
                {
                    if !arguments.is_empty() {
                        return Err(error(
                            "E-CALL-ARITY",
                            e.span,
                            "slice len requires no arguments",
                        ));
                    }
                    let (address, ty) = self.expr(object, None)?;
                    if !ty.is_slice_address() {
                        return Err(error(
                            "E-SLICE-TYPE",
                            object.span,
                            "slice len requires a borrowed slice",
                        ));
                    }
                    let value =
                        self.value(InstructionKind::SliceLength { address }, Type::I64, e.span)?;
                    if let Some(expected) = expected {
                        self.same(&Type::I64, expected, e.span)?;
                    }
                    return Ok((value, Type::I64));
                }
                if foreign::callback_constructor(callee) {
                    return self.foreign_callback(arguments, e.span);
                }
                if let Some((ty, variant)) = self.enum_constructor(callee)? {
                    return self.make_variant(&ty, variant, arguments, expected, e.span);
                }
                if let CoreExprKind::Member { object, member } = &callee.kind
                    && let Some(signature) = self.method_signature(object, member).cloned()
                {
                    return self.method_call(object, arguments, &signature, expected, e.span);
                }
                let direct = match &callee.kind {
                    CoreExprKind::Name(name) => self.lookup(name).is_none(),
                    CoreExprKind::Member { object, .. } => matches!(
                        &object.kind,
                        CoreExprKind::Name(name) if self.lookup(name).is_none()
                    ),
                    _ => false,
                };
                if !direct {
                    return self.call_closure(callee, arguments, expected, e.span);
                }
                let signature = self.resolve_callee(callee)?.clone();
                if signature.type_parameter.is_some() {
                    return self
                        .call_inferred_type_template(signature, arguments, expected, e.span);
                }
                if signature.callable_return.is_some()
                    && matches!(signature.result, Type::Opaque { .. })
                {
                    return Err(unsupported(
                        e.span,
                        "callable factory result identity has not been resolved",
                    ));
                }
                if !signature.callable.is_empty() {
                    return self.call_helper(&signature, arguments, expected, e.span);
                }
                if arguments.len() != signature.parameters.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        e.span,
                        format!(
                            "{} arguments required, {} supplied",
                            signature.parameters.len(),
                            arguments.len()
                        ),
                    ));
                }
                let mut values = vec![];
                for (a, t) in arguments.iter().zip(&signature.parameters) {
                    let (v, actual) = self.argument_expr(a, t)?;
                    self.same(&actual, t, a.span)?;
                    values.push(self.stage_argument(v, &actual, a.span)?)
                }
                let values = self.commit_arguments(values, e.span)?;
                let call = self.direct_call(&signature, values, e.span)?;
                if signature.result == Type::Unit {
                    self.emit(call, None, e.span)?;
                    self.constant(Constant::Unit, e.span)?
                } else {
                    let id = self.value(call, signature.result.clone(), e.span)?;
                    (id, signature.result)
                }
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition_temps = self.resource_temps.len();
                let (c, t) = self.expr(condition, Some(&Type::Bool))?;
                self.same(&t, &Type::Bool, condition.span)?;
                // The condition has its own temporary scope. A preceding
                // argument of the enclosing call must remain alive until that
                // call completes.
                self.finish_resource_temps_from(condition_temps, condition.span)?;
                let tb = self.block_id()?;
                let eb = self.block_id()?;
                let join = self.block_id()?;
                self.terminate(Terminator::Branch {
                    condition: c,
                    then_block: tb,
                    else_block: eb,
                });
                self.current = Some(tb);
                let (tv, tt) = self.transfer_expr(then_value, expected)?;
                let result = self.temp(tt.clone(), e.span)?;
                self.store(result, tv, e.span)?;
                self.terminate(Terminator::Jump(join));
                self.current = Some(eb);
                let (ev, et) = self.transfer_expr(else_value, Some(&tt))?;
                self.same(&et, &tt, else_value.span)?;
                self.store(result, ev, e.span)?;
                self.terminate(Terminator::Jump(join));
                self.current = Some(join);
                (
                    self.value(
                        if self.affine_type(&tt) || matches!(tt, Type::Managed(_)) {
                            InstructionKind::TakeLocal { local: result }
                        } else {
                            InstructionKind::Load(result)
                        },
                        tt.clone(),
                        e.span,
                    )?,
                    tt,
                )
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.push_scope(e.span)?;
                let mut hints = self.sequence_hints(value);
                for (name, v) in bindings {
                    if hints.is_empty() {
                        hints = self.sequence_hints(value);
                    }
                    let (id, ty) = if hints
                        .get(name)
                        .is_some_and(|ty| matches!(ty, Type::Closure(_)))
                    {
                        self.owned_array_input(v, hints.get(name))?
                    } else {
                        self.expr(v, hints.get(name))?
                    };
                    let l = self.local(name, ty, false, v.span)?;
                    self.sequenced_values.insert(l);
                    self.store(l, id, v.span)?
                }
                let result = self.expr(value, expected)?;
                self.end_scopes(self.scopes.len() - 1, e.span)?;
                self.pop_scope();
                result
            }
            CoreExprKind::Closure {
                move_capture,
                parameters,
                return_type,
                body,
            } => {
                let input = closures::Input {
                    move_capture: *move_capture,
                    parameters,
                    result: return_type.as_ref(),
                    body,
                    span: e.span,
                };
                self.closure_expression(input, expected)?
            }
            CoreExprKind::Member { .. } if self.enum_constructor(e)?.is_some() => {
                let (ty, variant) = self.enum_constructor(e)?.expect("checked constructor");
                return self.make_variant(&ty, variant, &[], expected, e.span);
            }
            CoreExprKind::Member { object, .. } if matches!(&object.kind, CoreExprKind::Name(name) if self.lookup(name).is_none()) => {
                self.function_reference(e)?
            }
            CoreExprKind::Tuple(_)
            | CoreExprKind::Array(_)
            | CoreExprKind::Index { .. }
            | CoreExprKind::Record { .. }
            | CoreExprKind::RecordUpdate { .. }
            | CoreExprKind::Member { .. } => self.aggregate(e, expected)?,
            CoreExprKind::Unsupported(feature) => return Err(unsupported(e.span, feature)),
            _ => {
                return Err(unsupported(
                    e.span,
                    "aggregate, closure, string or projection typing",
                ));
            }
        };
        if let Some(expected) = expected {
            self.same(&result.1, expected, e.span)?
        }
        Ok(result)
    }
    /// A known operand constrains an unsuffixed integer literal; this does not
    /// insert numeric conversions or change the type of a bound runtime value.
    fn known_type(&self, e: &CoreExpr) -> Option<Type> {
        if let CoreExprKind::Call { callee, arguments } = &e.kind
            && let Some(name) = dynamic_store::primitive(callee)
            && let Some(first) = arguments.first()
            && let Some(ty) = self.known_type(first)
            && let Some(element) = dynamic_store::element(&ty)
        {
            return Some(dynamic_store::result_type(name, element));
        }
        if let CoreExprKind::Call { callee, .. } = &e.kind
            && let Some(name) = byte_buffer::primitive(callee)
        {
            return byte_buffer::result_type(name);
        }
        if let Ok(Some((ty, _))) = self.enum_constructor(e) {
            return Some(ty);
        }
        if let CoreExprKind::Call { callee, .. } = &e.kind
            && let Ok(Some((ty, _))) = self.enum_constructor(callee)
        {
            return Some(ty);
        }
        match &e.kind {
            CoreExprKind::Try(operand) => match self.known_type(operand)? {
                Type::ErrorUnion { result, .. } => Some(*result),
                _ => None,
            },
            CoreExprKind::Literal(value) => Some(value.ty()),
            CoreExprKind::Ascribed { ty, .. } => Some(ty.clone()),
            CoreExprKind::IntegerConvert { target, .. } => Some(target.clone()),
            CoreExprKind::Name(name) => self
                .lookup(name)
                .map(|id| {
                    self.borrowed_captures
                        .get(&id)
                        .cloned()
                        .or_else(|| self.closure_slots.get(&id).copied().map(Type::Closure))
                        .unwrap_or_else(|| self.function.locals[id.0 as usize].ty.clone())
                })
                .or_else(|| self.signatures.get(name).and_then(Signature::pointer_type)),
            CoreExprKind::Index { object, .. } => match self.known_type(object)? {
                Type::Array { element, .. } => Some(*element),
                Type::Address { inner, .. } => match *inner {
                    Type::Array { element, .. } | Type::Slice { element } => Some(*element),
                    _ => None,
                },
                _ => None,
            },
            CoreExprKind::Slice { object, .. } => {
                let base = self.known_type(object)?;
                let inner = match base {
                    Type::Address { inner, .. } => *inner,
                    other => other,
                };
                match inner {
                    Type::Array { element, .. } | Type::Slice { element }
                        if element.is_copy_value() =>
                    {
                        Some(Type::Slice { element })
                    }
                    _ => None,
                }
            }
            CoreExprKind::Member { object, member } => {
                if matches!(&object.kind, CoreExprKind::Name(name) if self.lookup(name).is_none()) {
                    return self
                        .resolve_callee(e)
                        .ok()
                        .and_then(Signature::pointer_type);
                }
                let ty = self.known_type(object)?;
                let ty = match &ty {
                    Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => {
                        inner.as_ref()
                    }
                    _ => &ty,
                };
                aggregates::field(ty, member, e.span).ok().map(|(_, t)| {
                    if matches!(ty, Type::Package { .. }) {
                        packages::projection_type(t)
                    } else {
                        t
                    }
                })
            }
            CoreExprKind::Record { ty, .. } => Some(ty.clone()),
            CoreExprKind::RecordUpdate { ty, base, .. } => {
                ty.clone().or_else(|| self.known_type(base))
            }
            CoreExprKind::Sequence { value, .. } => self.known_type(value),
            CoreExprKind::Tuple(values) => values
                .iter()
                .map(|v| self.known_type(v))
                .collect::<Option<Vec<_>>>()
                .map(Type::Tuple),
            CoreExprKind::Bool(_) => Some(Type::Bool),
            CoreExprKind::Unit => Some(Type::Unit),
            CoreExprKind::Integer(n) if n.ends_with("u8") => Some(Type::U8),
            CoreExprKind::Integer(n) if n.ends_with("u16") => Some(Type::U16),
            CoreExprKind::Integer(n) if n.ends_with("i64") => Some(Type::I64),
            CoreExprKind::Integer(n) if n.ends_with("i32") => Some(Type::I32),
            CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Some(Type::Bool),
            CoreExprKind::Unary {
                op: op @ (UnaryOp::Borrow | UnaryOp::BorrowMut),
                operand,
            } => self
                .known_type(operand)
                .filter(|ty| ty.is_address_pointee()
                    || (*op == UnaryOp::Borrow && matches!(ty, Type::Managed(payload)
                        if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())))
                .map(|inner| Type::Address {
                    mutable: *op == UnaryOp::BorrowMut,
                    inner: Box::new(inner),
                }),
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => match self.known_type(operand)? {
                Type::Address { inner, .. } if inner.is_address_pointee() => Some(*inner),
                _ => None,
            },
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => self
                .package_place(operand)
                .ok()
                .flatten()
                .map(|(_, _, ty)| ty)
                .or_else(|| self.known_type(operand)),
            CoreExprKind::Unary { operand, .. } => self.known_type(operand),
            CoreExprKind::Call { callee, .. } => {
                if let CoreExprKind::Member { object, member } = &callee.kind
                    && member == "len"
                    && self
                        .known_type(object)
                        .is_some_and(|ty| ty.is_slice_address())
                {
                    return Some(Type::I64);
                }
                if let CoreExprKind::Member { object, member } = &callee.kind
                    && let Some(signature) = self.method_signature(object, member)
                {
                    return Some(signature.result.clone());
                }
                if let Some(Type::Closure(id)) = self.known_type(callee) {
                    return self.closures.pending[id.0 as usize]
                        .inference
                        .result()
                        .ok()
                        .flatten();
                }
                if let Some(Type::FunctionPointer { result, .. }) = self.known_type(callee) {
                    return Some(*result);
                }
                self.resolve_callee(callee).ok().map(|s| s.result.clone())
            }
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => self
                .known_type(then_value)
                .or_else(|| self.known_type(else_value)),
            _ => None,
        }
    }
    fn closure_expression(
        &mut self,
        input: closures::Input<'_>,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let mut outer = BTreeMap::new();
        for scope in &self.scopes {
            for (name, local) in scope {
                let work = name.len().checked_add(1).ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "closure lookup work overflow",
                        Some(input.span),
                    )
                })?;
                self.closures.snapshot(work, input.span)?;
                outer.insert(
                    name.clone(),
                    (
                        *local,
                        self.borrowed_captures
                            .get(local)
                            .cloned()
                            .or_else(|| self.managed_captures.get(local).map(|(_, ty)| ty.clone()))
                            .or_else(|| self.closure_captures.get(local).map(|(_, ty)| ty.clone()))
                            .or_else(|| self.closure_slots.get(local).copied().map(Type::Closure))
                            .unwrap_or_else(|| self.function.locals[local.0 as usize].ty.clone()),
                    ),
                );
            }
        }
        let span = input.span;
        let id = self
            .closures
            .create(input, &outer, self.signatures, self.imports, self.enums)?;
        if let Some(Type::FunctionPointer { parameters, result }) = expected {
            let pending = &mut self.closures.pending[id.0 as usize];
            if parameters.len() != pending.parameters.len() {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "closure and expected function pointer have different arity",
                ));
            }
            for (index, ty) in parameters.iter().enumerate() {
                pending.inference.argument(index, ty, span)?;
            }
            pending.inference.expect_result(result, span)?;
            let (slot, mut pending) = self.closures.take_thin(id, span)?;
            let (parameters, result) = pending.inference.resolve(false, span)?;
            let function_id = pending.function;
            let function = CoreFunction {
                callable_return: None,
                state_parameters: Vec::new(),
                type_parameters: Vec::new(),
                callable_parameters: Vec::new(),
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: zil_base::NodeId(0),
                name: format!("$function{}", function_id.0),
                parameters: pending
                    .parameters
                    .into_iter()
                    .zip(parameters)
                    .map(|(p, ty)| CoreParameter {
                        name: p.name,
                        ty,
                        mode: p.mode,
                        span: p.span,
                    })
                    .collect(),
                return_type: result,
                body: pending.body,
                receiver: None,
                span,
            };
            let mut builder = Builder::new(
                &function,
                function_id,
                self.signatures,
                self.limits,
                self.closures,
            )?;
            builder.imports = self.imports;
            builder.enums = self.enums;
            let typed = builder.finish(&function.body)?;
            self.closures.thin[slot] = Some(typed);
            let ty = expected.expect("function pointer context").clone();
            let value = self.value(
                InstructionKind::FunctionRef {
                    function: function_id,
                },
                ty.clone(),
                span,
            )?;
            return Ok((value, ty));
        }
        // Capture loads occur at this exact creation point. Delayed type
        // inference never reloads these outer locals or re-resolves their names.
        let captures = self.closures.pending[id.0 as usize].captures.clone();
        let mut values = Vec::new();
        for capture in captures {
            let stored = capture.stored_type();
            let value = if self.closure_slots.contains_key(&capture.source) {
                if capture.moved {
                    self.take_closure_slot(capture.source, capture.span)?.0
                } else {
                    self.borrow_closure_slot(capture.source, capture.written, capture.span)?
                        .0
                }
            } else if let Some((index, _)) = self.closure_captures.get(&capture.source).cloned() {
                let instruction = if capture.moved {
                    InstructionKind::TakeCapture { index }
                } else {
                    InstructionKind::BorrowCapture {
                        index,
                        mutable: capture.written,
                    }
                };
                self.value(instruction, stored.clone(), capture.span)?
            } else if matches!(capture.ty, Type::Managed(_)) {
                let kind = if let Some((index, _)) = self.managed_captures.get(&capture.source) {
                    if matches!(stored, Type::Managed(_)) {
                        InstructionKind::TakeCapture { index: *index }
                    } else {
                        InstructionKind::DescribeCapture { index: *index }
                    }
                } else if matches!(stored, Type::Managed(_)) {
                    InstructionKind::TakeLocal {
                        local: capture.source,
                    }
                } else {
                    InstructionKind::DescribeLocal {
                        local: capture.source,
                    }
                };
                self.value(kind, stored, capture.span)?
            } else if capture.moved && self.borrowed_captures.contains_key(&capture.source) {
                let expression = CoreExpr {
                    id: zil_base::NodeId(0),
                    kind: CoreExprKind::Name(capture.name.clone()),
                    span: capture.span,
                };
                self.expr(&expression, Some(&stored))?.0
            } else if capture.borrowed {
                let expression = CoreExpr {
                    id: zil_base::NodeId(0),
                    kind: CoreExprKind::Name(capture.name.clone()),
                    span: capture.span,
                };
                self.borrow_place_mode(&expression, capture.written)?.0
            } else if !capture.moved && matches!(capture.ty, Type::Address { .. }) {
                let address = self.value(
                    InstructionKind::Load(capture.source),
                    capture.ty.clone(),
                    capture.span,
                )?;
                self.value(
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: matches!(stored, Type::Address { mutable: true, .. }),
                    },
                    stored.clone(),
                    capture.span,
                )?
            } else {
                let kind = if self.affine_type(&capture.ty) {
                    InstructionKind::TakeLocal {
                        local: capture.source,
                    }
                } else {
                    InstructionKind::Load(capture.source)
                };
                self.value(kind, stored, capture.span)?
            };
            values.push(value);
        }
        let ty = Type::Closure(id);
        let value = self.value(
            InstructionKind::MakeClosure {
                closure: id,
                captures: values,
            },
            ty.clone(),
            span,
        )?;
        if self.closures.pending[id.0 as usize].inference.ready()? {
            self.materialize_closure(id)?;
        }
        Ok((value, ty))
    }
    fn materialize_closure(&mut self, id: ClosureId) -> R<()> {
        let pending = &mut self.closures.pending[id.0 as usize];
        if pending.typed.is_some() {
            return Ok(());
        }
        let (parameters, result) = pending.inference.resolve(true, pending.span)?;
        if pending.owns_leaf_store {
            let span = pending.span;
            let cost = inference::type_snapshot_work(&result, span, self.limits)?;
            self.closures.snapshot(cost, span)?;
            if !result.is_copy_value() {
                return Err(unsupported(
                    span,
                    "owned Store capture environments require Copy results",
                ));
            }
        }
        let pending = &self.closures.pending[id.0 as usize];
        let captures = pending.captures.clone();
        let function_id = pending.function;
        let function = CoreFunction {
            callable_return: None,
            state_parameters: Vec::new(),
            type_parameters: Vec::new(),
            callable_parameters: Vec::new(),
            result_mode: zil_ir::PassingMode::Infer,
            foreign: None,
            id: zil_base::NodeId(0),
            name: format!("$closure{}", id.0),
            parameters: pending
                .parameters
                .iter()
                .zip(parameters)
                .map(|(p, ty)| CoreParameter {
                    name: p.name.clone(),
                    ty,
                    mode: p.mode,
                    span: p.span,
                })
                .collect(),
            return_type: result,
            body: pending.body.clone(),
            receiver: None,
            span: pending.span,
        };
        let mut builder = Builder::new(
            &function,
            function_id,
            self.signatures,
            self.limits,
            self.closures,
        )?;
        builder.imports = self.imports;
        builder.enums = self.enums;
        builder.function.environment = Some(id);
        let lends = lending::supported_result(&function.return_type);
        for (index, capture) in captures.iter().enumerate() {
            let stored = capture.stored_type();
            if matches!(stored, Type::Package { .. }) {
                let local = builder.declare_local(
                    &capture.name,
                    stored,
                    capture.written,
                    capture.span,
                    false,
                )?;
                builder.emit(
                    InstructionKind::BindPackageCapture {
                        local,
                        index: index as u32,
                    },
                    None,
                    capture.span,
                )?;
                builder.package_captures.insert(local, index as u32);
            }
        }
        for (index, capture) in captures.into_iter().enumerate() {
            let stored = capture.stored_type();
            if matches!(stored, Type::Package { .. }) {
                continue;
            }
            if let Type::Closure(child) = &stored {
                let mutable = capture.written
                    && builder.closures.pending[child.0 as usize].callable_kind()
                        != CallableKind::FnOnce;
                let address = Type::Address {
                    mutable,
                    inner: Box::new(stored.clone()),
                };
                let local = builder.local(&capture.name, address.clone(), false, capture.span)?;
                let value = builder.value(
                    InstructionKind::BorrowCapture {
                        index: index as u32,
                        mutable,
                    },
                    address,
                    capture.span,
                )?;
                builder.store(local, value, capture.span)?;
                builder
                    .closure_captures
                    .insert(local, (index as u32, stored));
                continue;
            }
            if let Type::Managed(payload) = &stored {
                // The hidden environment keeps the carrier. Each activation
                // receives a descriptor into that live field, never a copy or
                // an eager move of the carrier on a Shared/Exclusive call.
                let desc = Type::Desc(payload.clone());
                let local = builder.local(&capture.name, desc.clone(), false, capture.span)?;
                let value = builder.value(
                    InstructionKind::DescribeCapture {
                        index: index as u32,
                    },
                    desc,
                    capture.span,
                )?;
                builder.store(local, value, capture.span)?;
                builder
                    .managed_captures
                    .insert(local, (index as u32, stored));
                continue;
            }
            let inline_copy = stored.is_copy_value();
            let local =
                builder.local(&capture.name, stored.clone(), capture.written, capture.span)?;
            if capture.borrowed {
                builder.borrowed_captures.insert(local, capture.ty.clone());
            }
            let value = builder.value(
                InstructionKind::ReadCapture {
                    index: index as u32,
                },
                stored,
                capture.span,
            )?;
            builder.store(local, value, capture.span)?;
            if lends && inline_copy {
                builder.stored_copy_captures.insert(local, index as u32);
            }
            if capture.written && !capture.borrowed && capture.ty.is_copy_value() {
                builder.written_captures.insert(local, index as u32);
                if capture.address_written && !lends {
                    builder.address_written_captures.insert(local, index as u32);
                }
            }
        }
        let typed = builder.finish(&function.body)?;
        self.closures.pending[id.0 as usize].typed = Some(typed);
        Ok(())
    }
    fn call_closure(
        &mut self,
        callee: &CoreExpr,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        // Evaluate the callee once, before any argument, even for a conditional
        // or hygienic Sequence expression that produces a closure value.
        let mut existing_place = match &callee.kind {
            CoreExprKind::Name(name) => self.lookup(name),
            _ => None,
        };
        let mut borrowed_callee = None;
        let field_callee = self.package_callable_callee(callee)?;
        // An owning environment is not a Copy callee value. Shared and
        // Exclusive invocation borrow the actual local; Consume transfers it.
        let captured_once = existing_place.and_then(|local| self.closure_captures.get(&local).cloned())
            .filter(|(_, ty)| matches!(ty, Type::Closure(id) if self.closures.pending[id.0 as usize].callable_kind() == CallableKind::FnOnce));
        let (mut callee_value, ty) = if let Some((value, id, kind)) = field_callee {
            if kind == CallableKind::FnOnce {
                (Some(value), Type::Closure(id))
            } else {
                borrowed_callee = Some(value);
                (None, Type::Closure(id))
            }
        } else if let Some((index, ty)) = captured_once {
            existing_place = None;
            (
                Some(self.value(InstructionKind::TakeCapture { index }, ty.clone(), span)?),
                ty,
            )
        } else if let Some(local) = existing_place
            && let Some(id) = self.closure_slots.get(&local).copied()
        {
            existing_place = None;
            let kind = self
                .declared_state_kind(callee, None, id)?
                .unwrap_or_else(|| self.closures.pending[id.0 as usize].callable_kind());
            if kind == CallableKind::FnOnce {
                let (value, ty) = self.take_closure_slot(local, span)?;
                (Some(value), ty)
            } else {
                borrowed_callee = Some(
                    self.borrow_closure_slot(local, kind == CallableKind::FnMut, span)?
                        .0,
                );
                (None, Type::Closure(id))
            }
        } else if let Some(local) = existing_place
            && (self.owned_closure(&self.function.locals[local.0 as usize].ty)
                || self.lending_closure(&self.function.locals[local.0 as usize].ty)?
                || matches!(&self.function.locals[local.0 as usize].ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_))))
        {
            (None, self.function.locals[local.0 as usize].ty.clone())
        } else {
            let (value, ty) = self.expr(callee, None)?;
            (Some(value), ty)
        };
        if let Type::FunctionPointer { parameters, result } = ty {
            return self.call_indirect(
                callee_value.expect("function value"),
                &parameters,
                *result,
                arguments,
                expected,
                span,
            );
        }
        if matches!(&ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)))
        {
            return self.call_borrowed_helper_parameter(callee, arguments, expected, span);
        }
        let Type::Closure(id) = ty else {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "called value is not a function pointer or concrete closure",
            ));
        };
        let declared_kind = self.declared_state_kind(callee, None, id)?;
        let kind = declared_kind.unwrap_or_else(|| {
            field_callee.map_or_else(
                || {
                    self.helper_bounds
                        .iter()
                        .find(|bound| {
                            existing_place
                                == self.function.parameters.get(bound.index as usize).copied()
                        })
                        .map_or_else(
                            || self.closures.pending[id.0 as usize].callable_kind(),
                            |bound| bound.kind,
                        )
                },
                |(_, _, kind)| kind,
            )
        });
        let owns_resources = self.closures.pending[id.0 as usize].owns_resources();
        let lends = self.lending_closure(&Type::Closure(id))?;
        if lends
            && (kind == CallableKind::FnOnce
                || (existing_place.is_none() && borrowed_callee.is_none()))
        {
            return Err(unsupported(
                span,
                "environment lending requires a non-consuming call on an actual stored closure position",
            ));
        }
        if kind == CallableKind::FnOnce
            && let Some(local) = existing_place
        {
            callee_value = Some(self.value(
                InstructionKind::TakeLocal { local },
                Type::Closure(id),
                span,
            )?);
        }
        let call_place = if borrowed_callee.is_none()
            && (kind == CallableKind::FnMut
                || (kind == CallableKind::Fn && (owns_resources || lends)))
        {
            let local = if let Some(local) = existing_place {
                if kind == CallableKind::FnMut && !self.function.locals[local.0 as usize].mutable {
                    return Err(error(
                        "E-CLOSURE-MUTABILITY",
                        span,
                        "FnMut call requires a mutable closure position; bind it with var",
                    ));
                }
                local
            } else {
                let local = self.temp(Type::Closure(id), span)?;
                self.store(local, callee_value.expect("temporary closure value"), span)?;
                local
            };
            Some(local)
        } else {
            None
        };
        // A consumed callee has not entered its body while arguments are
        // being evaluated. Park its ownership so a later ? can release it.
        let staged_callee = if kind == CallableKind::FnOnce {
            Some(self.stage_argument(
                callee_value.expect("consumed environment"),
                &Type::Closure(id),
                span,
            )?)
        } else {
            None
        };
        let pending = &mut self.closures.pending[id.0 as usize];
        if arguments.len() != pending.inference.arity() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                format!(
                    "{} arguments required, {} supplied",
                    pending.inference.arity(),
                    arguments.len()
                ),
            ));
        }
        if let Some(expected) = expected {
            pending.inference.expect_result(expected, span)?;
        }
        // Type constraints have no runtime effects. Gather the known argument
        // types before defaulting any integer literal, so f(1, known_i64)
        // constrains both unified parameters to i64 without reordering values.
        for (index, argument) in arguments.iter().enumerate() {
            self.closures.snapshot(1, argument.span)?;
            if let Some(hint) = self.range_hint(argument, 0)
                && !matches!(
                    self.closures.pending[id.0 as usize]
                        .inference
                        .parameter(index)?,
                    Some(Type::Address { .. })
                )
            {
                self.closures.pending[id.0 as usize].inference.argument(
                    index,
                    &hint,
                    argument.span,
                )?;
            }
        }
        let mut values = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            self.closures.snapshot(1, argument.span)?;
            let hint = self.closures.pending[id.0 as usize]
                .inference
                .parameter(index)?;
            let (value, actual) = if let Some(parameter) = hint.as_ref() {
                self.argument_expr(argument, parameter)?
            } else {
                self.expr(argument, None)?
            };
            self.closures.pending[id.0 as usize].inference.argument(
                index,
                &actual,
                argument.span,
            )?;
            values.push(self.stage_argument(value, &actual, argument.span)?);
        }
        let values = self.commit_arguments(values, span)?;
        if let Some(staged) = staged_callee {
            callee_value = Some(self.commit_arguments(vec![staged], span)?[0]);
        }
        self.materialize_closure(id)?;
        let result = self.closures.pending[id.0 as usize]
            .inference
            .result()?
            .expect("resolved closure");
        let instruction = if let Some(callee) = borrowed_callee {
            InstructionKind::CallClosureBorrowed {
                callee,
                kind,
                arguments: values,
            }
        } else {
            match kind {
                CallableKind::FnOnce => InstructionKind::CallClosureOnce {
                    callee: callee_value.expect("consumed environment"),
                    arguments: values,
                },
                CallableKind::FnMut => InstructionKind::CallClosureMut {
                    callee: call_place.expect("exclusive environment"),
                    arguments: values,
                },
                CallableKind::Fn if owns_resources || lends => InstructionKind::CallClosureShared {
                    callee: call_place.expect("shared environment"),
                    arguments: values,
                },
                _ => InstructionKind::CallClosure {
                    callee: callee_value.expect("Copy environment"),
                    arguments: values,
                },
            }
        };
        if result == Type::Unit {
            self.emit(instruction, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            let value = self.value(instruction, result.clone(), span)?;
            Ok((value, result))
        }
    }
    fn short_circuit(
        &mut self,
        left: &CoreExpr,
        right: &CoreExpr,
        short: bool,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let (l, t) = self.expr(left, Some(&Type::Bool))?;
        self.same(&t, &Type::Bool, left.span)?;
        let result = self.temp(Type::Bool, span)?;
        let sb = self.block_id()?;
        let rb = self.block_id()?;
        let join = self.block_id()?;
        self.terminate(Terminator::Branch {
            condition: l,
            then_block: if short { sb } else { rb },
            else_block: if short { rb } else { sb },
        });
        self.current = Some(sb);
        let (v, _) = self.constant(Constant::Bool(short), span)?;
        self.store(result, v, span)?;
        self.terminate(Terminator::Jump(join));
        self.current = Some(rb);
        let (v, t) = self.expr(right, Some(&Type::Bool))?;
        self.same(&t, &Type::Bool, right.span)?;
        self.store(result, v, span)?;
        self.terminate(Terminator::Jump(join));
        self.current = Some(join);
        Ok((
            self.value(InstructionKind::Load(result), Type::Bool, span)?,
            Type::Bool,
        ))
    }
}
fn integer(text: &str, expected: Option<&Type>, negative: bool, span: Span) -> R<Constant> {
    let clean = text.replace('_', "");
    let (mut digits, mut ty) = (clean.as_str(), None);
    for (suffix, t) in [
        ("u8", Type::U8),
        ("u16", Type::U16),
        ("i32", Type::I32),
        ("i64", Type::I64),
    ] {
        if let Some(v) = clean.strip_suffix(suffix) {
            digits = v;
            ty = Some(t);
            break;
        }
    }
    let ty = ty.unwrap_or(match expected {
        Some(Type::U8) => Type::U8,
        Some(Type::U16) => Type::U16,
        Some(Type::I64) => Type::I64,
        _ => Type::I32,
    });
    let (radix, digits) = if let Some(s) = digits.strip_prefix("0x") {
        (16, s)
    } else if let Some(s) = digits.strip_prefix("0b") {
        (2, s)
    } else if let Some(s) = digits.strip_prefix("0o") {
        (8, s)
    } else {
        (10, digits)
    };
    let value = i128::from_str_radix(digits, radix)
        .map_err(|_| error("E-INTEGER-LITERAL", span, "invalid or too large integer"))?;
    let value = if negative { -value } else { value };
    match ty {
        Type::U8 => u8::try_from(value)
            .map(Constant::U8)
            .map_err(|_| error("E-INTEGER-RANGE", span, "integer does not fit u8")),
        Type::U16 => u16::try_from(value)
            .map(Constant::U16)
            .map_err(|_| error("E-INTEGER-RANGE", span, "integer does not fit u16")),
        Type::I32 => i32::try_from(value)
            .map(Constant::I32)
            .map_err(|_| error("E-INTEGER-RANGE", span, "integer does not fit i32")),
        Type::I64 => i64::try_from(value)
            .map(Constant::I64)
            .map_err(|_| error("E-INTEGER-RANGE", span, "integer does not fit i64")),
        _ => unreachable!(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, NodeId};
    fn span() -> Span {
        Span::new(FileId(0), 0, 1)
    }
    fn expr(kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: NodeId(0),
            kind,
            span: span(),
        }
    }
    fn module(value: CoreExpr) -> CoreModule {
        CoreModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            functions: vec![CoreFunction {
                callable_return: None,
                state_parameters: Vec::new(),
                type_parameters: Vec::new(),
                callable_parameters: Vec::new(),
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: NodeId(0),
                name: "main".into(),
                parameters: vec![],
                return_type: Type::I32,
                body: CoreBlock {
                    statements: vec![CoreStatement {
                        id: NodeId(0),
                        kind: CoreStatementKind::Return(Some(value)),
                        span: span(),
                    }],
                    span: span(),
                },
                receiver: None,
                span: span(),
            }],
            aggregates: vec![],
            enums: vec![],
            unsupported: vec![],
        }
    }
    #[test]
    fn scalar_arithmetic_builds_cfg() {
        let m = module(expr(CoreExprKind::Binary {
            op: BinaryOp::Add,
            left: Box::new(expr(CoreExprKind::Integer("20".into()))),
            right: Box::new(expr(CoreExprKind::Integer("22".into()))),
        }));
        let t = check(&m, &TargetProfile::default(), &Limits::default()).unwrap();
        assert_eq!(t.functions[0].values.len(), 3);
        assert!(matches!(
            t.functions[0].blocks[0].terminator,
            Terminator::Return(Some(_))
        ));
    }
    #[test]
    fn wrong_return_type_rejected() {
        let m = module(expr(CoreExprKind::Bool(true)));
        assert_eq!(
            check(&m, &TargetProfile::default(), &Limits::default()).unwrap_err()[0].code,
            "E-TYPE-MISMATCH"
        )
    }
    #[test]
    fn minimum_signed_integer_supported() {
        assert_eq!(
            integer("2147483648", Some(&Type::I32), true, span()).unwrap(),
            Constant::I32(i32::MIN)
        );
        assert!(integer("2147483648", Some(&Type::I32), false, span()).is_err());
    }
    #[test]
    fn unknown_name_rejected() {
        assert_eq!(
            check(
                &module(expr(CoreExprKind::Name("absent".into()))),
                &TargetProfile::default(),
                &Limits::default()
            )
            .unwrap_err()[0]
                .code,
            "E-UNKNOWN-NAME"
        )
    }
}
