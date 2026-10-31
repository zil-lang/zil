//! Bounded, process-local memoization of successful ordinary scalar builders.
//! Declaration checking and every later analysis/verification stage stay live.
//! Entries are private raw values: there is no decoder or public insertion API.
use super::*;
use std::mem::size_of;
use std::sync::Arc;
use zil_base::{Fingerprint, ModuleId};

type Tables = BTreeMap<ModuleId, BTreeMap<String, Signature>>;
const MAX_WEIGHT: usize = 64 * 1024 * 1024;
const MAX_MODULES: usize = 16_384;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgramMemoStats {
    pub eligible: bool,
    pub reused_functions: usize,
    pub checked_functions: usize,
    pub retained_modules: usize,
    /// Bounded structural storage accounting, not a measurement of allocator RSS.
    pub retained_weight: usize,
    pub source_modules_reused: usize,
    pub source_modules_checked: usize,
    pub capacity_skipped_modules: usize,
    /// New cache snapshots copied after complete module selection, not output clones.
    pub snapshot_cloned_functions: usize,
}

/// One successful project epoch, at most 16,384 modules / 64 MiB structural
/// weight. The caller commits a returned epoch only after its request succeeds.
/// Unchanged modules share immutable storage across adjacent epochs.
#[derive(Default)]
pub struct ProgramMemo {
    environment: Option<Arc<Environment>>,
    modules: Vec<Option<Arc<ModuleMemo>>>,
    stats: ProgramMemoStats,
}
impl ProgramMemo {
    pub fn stats(&self) -> ProgramMemoStats {
        self.stats
    }
}

struct Environment {
    // Derived entirely from the complete identity below, including the lengths
    // used for bounded per-function working receipt reservations.
    weight: usize,
    root: ModuleId,
    target: TargetProfile,
    limits: Limits,
    context: Fingerprint,
    order: Vec<ModuleId>,
    imports: Vec<Vec<CoreImport>>,
    declarations: Vec<Vec<DeclarationIdentity>>,
    // Exact complete tables include negative name lookup, visibility, concrete
    // FunctionId assignments and every field the ordinary Builder can consume.
    tables: Tables,
}
#[derive(Clone, PartialEq, Eq)]
struct DeclarationIdentity {
    node: zil_base::NodeId,
    result_mode: PassingMode,
    parameter_modes: Vec<PassingMode>,
    foreign: Option<ForeignDeclaration>,
}
impl DeclarationIdentity {
    fn new(function: &CoreFunction) -> Self {
        Self {
            node: function.id,
            result_mode: function.result_mode,
            parameter_modes: function.parameters.iter().map(|p| p.mode).collect(),
            foreign: function.foreign.clone(),
        }
    }
    fn matches(&self, function: &CoreFunction) -> bool {
        self.node == function.id
            && self.result_mode == function.result_mode
            && self.foreign == function.foreign
            && self.parameter_modes.len() == function.parameters.len()
            && self
                .parameter_modes
                .iter()
                .zip(&function.parameters)
                .all(|(mode, p)| *mode == p.mode)
    }
}
struct FunctionMemo {
    typed: TypedFunction,
    usage: closures::MemoUsage,
}
struct ModuleMemo {
    // Full equality binds raw callers too, including bodies, NodeId and spans;
    // a caller-supplied source_fingerprint alone is never sufficient.
    core: CoreModule,
    functions: Vec<FunctionMemo>,
    source_weight: usize,
    typed_weight: usize,
}

/// Only small receipts are retained while the actual typed output is built.
/// No speculative TypedFunction snapshot is copied by record().
struct FunctionReceipt {
    id: FunctionId,
    usage: closures::MemoUsage,
}

pub(super) struct Request<'a> {
    program: &'a CoreProgram,
    tables: &'a Tables,
    target: &'a TargetProfile,
    limits: &'a Limits,
    context: Fingerprint,
    previous: &'a ProgramMemo,
    environment_matches: bool,
    reuse: Vec<bool>,
    cacheable: Vec<bool>,
    functions: Vec<Vec<FunctionReceipt>>,
    source_weight: Vec<usize>,
    typed_weight: Vec<usize>,
    weight: usize,
    capacity: usize,
    stats: ProgramMemoStats,
}
impl<'a> Request<'a> {
    pub(super) fn new(
        program: &'a CoreProgram,
        tables: &'a Tables,
        target: &'a TargetProfile,
        limits: &'a Limits,
        context: Fingerprint,
        previous: &'a ProgramMemo,
    ) -> Option<Self> {
        Self::with_capacity(
            program, tables, target, limits, context, previous, MAX_WEIGHT,
        )
    }

    // Production always uses MAX_WEIGHT. Small internal controls exercise exact
    // boundaries without changing language Limits or the public driver/schema.
    fn with_capacity(
        program: &'a CoreProgram,
        tables: &'a Tables,
        target: &'a TargetProfile,
        limits: &'a Limits,
        context: Fingerprint,
        previous: &'a ProgramMemo,
        capacity: usize,
    ) -> Option<Self> {
        if program.modules.len() > MAX_MODULES || capacity > MAX_WEIGHT {
            return None;
        }
        let environment_matches = previous.environment.as_ref().is_some_and(|old| {
            old.root == program.root
                && old.target == *target
                && old.limits == *limits
                && old.context == context
                && old.tables == *tables
                && old.declarations.len() == program.modules.len()
                && old
                    .declarations
                    .iter()
                    .zip(&program.modules)
                    .all(|(headers, module)| {
                        headers.len() == module.core.functions.len()
                            && headers
                                .iter()
                                .zip(&module.core.functions)
                                .all(|(old, current)| old.matches(current))
                    })
                && old.order.len() == program.modules.len()
                && old
                    .order
                    .iter()
                    .zip(&program.modules)
                    .all(|(id, m)| *id == m.core.module_id)
                && old
                    .imports
                    .iter()
                    .zip(&program.modules)
                    .all(|(imports, m)| imports == &m.imports)
        });
        let weight = if environment_matches {
            previous
                .environment
                .as_ref()
                .expect("matched environment")
                .weight
        } else {
            environment_weight(program, target)?
        };
        if weight > capacity {
            // The complete environment is mandatory even for a partial epoch.
            return None;
        }
        let mut reuse = Vec::with_capacity(program.modules.len());
        let mut cacheable = Vec::with_capacity(program.modules.len());
        let mut source_weights = Vec::with_capacity(program.modules.len());
        let mut typed_weights = Vec::with_capacity(program.modules.len());
        let mut stats = ProgramMemoStats {
            eligible: true,
            ..ProgramMemoStats::default()
        };
        for (index, module) in program.modules.iter().enumerate() {
            let old = previous
                .modules
                .get(index)
                .and_then(Option::as_ref)
                .filter(|old| {
                    environment_matches
                        && old.core == module.core
                        && old.functions.len() == module.core.functions.len()
                });
            if let Some(old) = old {
                // Full Core equality proves the previous whitelist/weight
                // receipt still applies; a source fingerprint is insufficient.
                stats.source_modules_reused += 1;
                cacheable.push(true);
                source_weights.push(old.source_weight);
                typed_weights.push(old.typed_weight);
            } else {
                let mut source = 0;
                cacheable.push(source_weight(module, tables, &mut source)?);
                stats.source_modules_checked += 1;
                source_weights.push(source);
                typed_weights.push(0);
            }
            reuse.push(old.is_some());
        }
        if !cacheable.iter().any(|enabled| *enabled) {
            return None;
        }
        Some(Self {
            program,
            tables,
            target,
            limits,
            context,
            previous,
            environment_matches,
            reuse,
            cacheable,
            source_weight: source_weights,
            typed_weight: typed_weights,
            weight,
            capacity,
            functions: (0..program.modules.len()).map(|_| Vec::new()).collect(),
            stats,
        })
    }

    pub(super) fn load(
        &mut self,
        module: usize,
        function: usize,
        state: &mut closures::State,
    ) -> Option<TypedFunction> {
        if self.reuse[module] {
            let old = &self.previous.modules[module]
                .as_ref()
                .expect("classified reusable module")
                .functions[function];
            if state.replay_memo(old.usage) {
                self.stats.reused_functions += 1;
                return Some(old.typed.clone());
            }
        }
        self.stats.checked_functions += 1;
        None
    }

    pub(super) fn record(
        &mut self,
        module: usize,
        typed: &TypedFunction,
        usage: closures::MemoUsage,
    ) -> Option<()> {
        if !self.cacheable[module] {
            // Scalar FFI/unsafe modules always run the original Builder. Their
            // complete foreign contracts are part of the environment identity.
            typed_weight(typed, true)?;
        } else if !self.reuse[module] {
            add(&mut self.typed_weight[module], typed_weight(typed, false)?);
            if self.source_weight[module].saturating_add(self.typed_weight[module])
                <= self.capacity - self.weight
            {
                if self.functions[module].is_empty() {
                    // This exact-length receipt reservation was accounted for
                    // before the request grew any per-function metadata.
                    self.functions[module]
                        .reserve_exact(self.program.modules[module].core.functions.len());
                }
                self.functions[module].push(FunctionReceipt {
                    id: typed.id,
                    usage,
                });
            } else {
                // An oversized module is optional storage, not a language error.
                // Continue output validation; only its small receipts are dropped.
                self.functions[module].clear();
            }
        }
        Some(())
    }

    /// Called before lending mutates any function, after ordinary functions
    /// have been sorted by FunctionId. No generated identities qualify here.
    pub(super) fn finish(mut self, output: &[TypedFunction]) -> ProgramMemo {
        let mut weight = self.weight;
        let mut modules = Vec::with_capacity(self.program.modules.len());
        for (index, module) in self.program.modules.iter().enumerate() {
            if !self.cacheable[index] {
                modules.push(None);
                continue;
            }
            let size = self.source_weight[index].saturating_add(self.typed_weight[index]);
            let Some(next_weight) = weight.checked_add(size).filter(|n| *n <= self.capacity) else {
                self.stats.capacity_skipped_modules += 1;
                modules.push(None);
                continue;
            };
            if self.reuse[index] {
                modules.push(self.previous.modules[index].clone());
            } else {
                let receipts = &self.functions[index];
                if receipts.len() != module.core.functions.len()
                    || !receipts
                        .iter()
                        .all(|r| output.get(r.id.0 as usize).is_some_and(|f| f.id == r.id))
                {
                    // Never publish an incomplete snapshot if an internal
                    // construction assumption fails. The actual output survives.
                    return ProgramMemo::default();
                }
                let functions = receipts
                    .iter()
                    .zip(&module.core.functions)
                    .map(|(receipt, core)| {
                        let mut typed = output[receipt.id.0 as usize].clone();
                        // Project labels are assigned after record(). Restore the
                        // exact original Builder name expected by a later load().
                        typed.name.clone_from(&core.name);
                        self.stats.snapshot_cloned_functions += 1;
                        FunctionMemo {
                            typed,
                            usage: receipt.usage,
                        }
                    })
                    .collect();
                modules.push(Some(Arc::new(ModuleMemo {
                    core: module.core.clone(),
                    functions,
                    source_weight: self.source_weight[index],
                    typed_weight: self.typed_weight[index],
                })));
            }
            weight = next_weight;
            self.stats.retained_modules += 1;
        }
        if self.stats.retained_modules == 0 {
            // Do not copy the complete environment for an empty selection.
            return ProgramMemo {
                stats: self.stats,
                ..ProgramMemo::default()
            };
        }
        let environment = if self.environment_matches {
            self.previous.environment.clone()
        } else {
            Some(Arc::new(Environment {
                weight: self.weight,
                root: self.program.root,
                target: self.target.clone(),
                limits: self.limits.clone(),
                context: self.context,
                order: self
                    .program
                    .modules
                    .iter()
                    .map(|m| m.core.module_id)
                    .collect(),
                imports: self
                    .program
                    .modules
                    .iter()
                    .map(|m| m.imports.clone())
                    .collect(),
                declarations: self
                    .program
                    .modules
                    .iter()
                    .map(|m| {
                        m.core
                            .functions
                            .iter()
                            .map(DeclarationIdentity::new)
                            .collect()
                    })
                    .collect(),
                tables: self.tables.clone(),
            }))
        };
        self.stats.retained_weight = weight;
        ProgramMemo {
            environment,
            modules,
            stats: self.stats,
        }
    }
}

// Capacity overflow is a saturating size, never an eligibility failure. The
// whitelist must still examine the rest of an oversized live module.
fn add(weight: &mut usize, bytes: usize) {
    *weight = weight.saturating_add(bytes).min(MAX_WEIGHT + 1);
}
fn array<T>(weight: &mut usize, length: usize) {
    add(
        weight,
        length.saturating_mul(size_of::<T>()).saturating_add(64),
    );
}
fn text(weight: &mut usize, value: &str) {
    add(weight, value.len().saturating_add(64));
}

fn scalar_header(f: &CoreFunction) -> bool {
    f.type_parameters.is_empty()
        && f.state_parameters.is_empty()
        && f.callable_parameters.is_empty()
        && f.callable_return.is_none()
        && f.receiver.is_none()
        && matches!(f.result_mode, PassingMode::Copy | PassingMode::Infer)
        && f.return_type.is_b0_scalar()
        && f.parameters.iter().all(|p| {
            matches!(p.mode, PassingMode::Copy | PassingMode::Infer) && p.ty.is_b0_scalar()
        })
}

fn environment_weight(program: &CoreProgram, target: &TargetProfile) -> Option<usize> {
    let mut weight = size_of::<Environment>() + size_of::<ProgramMemo>();
    for value in [
        &target.id,
        &target.triple,
        &target.backend,
        &target.rule_version,
    ] {
        text(&mut weight, value);
    }
    // Include all environment storage and conservative working receipt/vector
    // reservations, even for modules eventually omitted from the epoch.
    array::<ModuleId>(&mut weight, program.modules.len());
    array::<Vec<CoreImport>>(&mut weight, program.modules.len());
    array::<Vec<DeclarationIdentity>>(&mut weight, program.modules.len());
    array::<Option<Arc<ModuleMemo>>>(&mut weight, program.modules.len());
    array::<Vec<FunctionReceipt>>(&mut weight, program.modules.len());
    array::<[usize; 4]>(&mut weight, program.modules.len());
    for module in &program.modules {
        let core = &module.core;
        if !core.aggregates.is_empty() || !core.enums.is_empty() || !core.unsupported.is_empty() {
            return None;
        }
        add(&mut weight, 256); // module table node / allocation slack
        array::<CoreImport>(&mut weight, module.imports.len());
        for import in &module.imports {
            text(&mut weight, &import.alias);
        }
        array::<DeclarationIdentity>(&mut weight, core.functions.len());
        array::<FunctionReceipt>(&mut weight, core.functions.len());
        for f in &core.functions {
            if !scalar_header(f) {
                return None;
            }
            add(&mut weight, size_of::<Signature>() + 256);
            text(&mut weight, &f.name);
            array::<PassingMode>(&mut weight, f.parameters.len());
            array::<Type>(&mut weight, f.parameters.len());
            if let Some(foreign) = &f.foreign {
                text(&mut weight, &foreign.abi);
                text(&mut weight, &foreign.symbol);
                array::<u32>(&mut weight, foreign.noescape_parameters.len());
            }
        }
        if weight > MAX_WEIGHT {
            return Some(weight);
        }
    }
    Some(weight)
}

/// This conservative whitelist is an optional optimization classifier. It runs
/// only after the existing complete bounded shape/declaration preflight and
/// never supplies a diagnostic or changes the language's logical charges.
fn source_weight(module: &ProgramModule, tables: &Tables, weight: &mut usize) -> Option<bool> {
    let core = &module.core;
    let mut cacheable = true;
    if !core.aggregates.is_empty() || !core.enums.is_empty() || !core.unsupported.is_empty() {
        return None;
    }
    add(weight, size_of::<ModuleMemo>() + 256);
    array::<CoreFunction>(weight, core.functions.len());
    array::<FunctionMemo>(weight, core.functions.len());
    for f in &core.functions {
        if !f.type_parameters.is_empty()
            || !f.state_parameters.is_empty()
            || !f.callable_parameters.is_empty()
            || f.callable_return.is_some()
            || f.receiver.is_some()
            || !matches!(f.result_mode, PassingMode::Copy | PassingMode::Infer)
            || !f.return_type.is_b0_scalar()
            || f.parameters.iter().any(|p| {
                !matches!(p.mode, PassingMode::Copy | PassingMode::Infer) || !p.ty.is_b0_scalar()
            })
        {
            return None;
        }
        if let Some(foreign) = &f.foreign {
            cacheable = false;
            text(weight, &foreign.abi);
            text(weight, &foreign.symbol);
            array::<u32>(weight, foreign.noescape_parameters.len());
        }
        text(weight, &f.name);
        array::<CoreParameter>(weight, f.parameters.len());
        for p in &f.parameters {
            text(weight, &p.name);
        }
        let mut pending = vec![Input::Block(&f.body)];
        while let Some(input) = pending.pop() {
            match input {
                Input::Block(block) => {
                    array::<CoreStatement>(weight, block.statements.len());
                    for statement in &block.statements {
                        use CoreStatementKind as S;
                        match &statement.kind {
                            S::Binding {
                                name,
                                ty,
                                initializer,
                                ..
                            } => {
                                if ty.as_ref().is_some_and(|t| !t.is_b0_scalar()) {
                                    return None;
                                }
                                text(weight, name);
                                pending.extend(initializer.iter().map(Input::Expr));
                            }
                            S::Assign { target, value } => {
                                pending.push(Input::Expr(target));
                                pending.push(Input::Expr(value));
                            }
                            S::Expression(e) => pending.push(Input::Expr(e)),
                            S::Return(e) => pending.extend(e.iter().map(Input::Expr)),
                            S::If {
                                condition,
                                then_body,
                                else_body,
                            } => {
                                pending.push(Input::Expr(condition));
                                pending.push(Input::Block(then_body));
                                pending.extend(else_body.iter().map(Input::Block));
                            }
                            S::While { condition, body } => {
                                pending.push(Input::Expr(condition));
                                pending.push(Input::Block(body));
                            }
                            S::ForRange {
                                name,
                                start,
                                end,
                                body,
                            } => {
                                text(weight, name);
                                pending.push(Input::Expr(start));
                                pending.push(Input::Expr(end));
                                pending.push(Input::Block(body));
                            }
                            S::Block(body) => pending.push(Input::Block(body)),
                            S::Unsafe(body) => {
                                cacheable = false;
                                pending.push(Input::Block(body));
                            }
                            S::Break | S::Continue => {}
                            _ => return None,
                        }
                    }
                }
                Input::Expr(e) => {
                    // Existing shape/name preflight bounds this traversal by
                    // language Limits. Size saturation must not skip its tail.
                    add(weight, size_of::<CoreExpr>() + 64);
                    use CoreExprKind as E;
                    match &e.kind {
                        E::Literal(value) if value.ty().is_b0_scalar() => {}
                        E::Integer(s) | E::Name(s) => text(weight, s),
                        E::Bool(_) | E::Unit => {}
                        E::Ascribed { ty, value } | E::IntegerConvert { target: ty, value }
                            if ty.is_b0_scalar() =>
                        {
                            pending.push(Input::Expr(value))
                        }
                        E::Unary {
                            op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                            operand,
                        } => pending.push(Input::Expr(operand)),
                        E::Binary { left, right, .. } => {
                            pending.push(Input::Expr(left));
                            pending.push(Input::Expr(right));
                        }
                        E::Call { callee, arguments } => {
                            // Only declared scalar functions can be called here;
                            // resource/callback constructors are not declarations.
                            let declared = match &callee.kind {
                                E::Name(name) => tables[&core.module_id].contains_key(name),
                                E::Member { object, member } => match &object.kind {
                                    E::Name(alias) => module
                                        .imports
                                        .iter()
                                        .find(|i| &i.alias == alias)
                                        .and_then(|i| tables.get(&i.module))
                                        .is_some_and(|t| t.contains_key(member)),
                                    _ => false,
                                },
                                _ => false,
                            };
                            if !declared {
                                return None;
                            }
                            array::<CoreExpr>(weight, arguments.len());
                            pending.push(Input::Expr(callee));
                            pending.extend(arguments.iter().map(Input::Expr));
                        }
                        E::Member { object, member } if matches!(object.kind, E::Name(_)) => {
                            text(weight, member);
                            pending.push(Input::Expr(object));
                        }
                        E::If {
                            condition,
                            then_value,
                            else_value,
                        } => {
                            pending.push(Input::Expr(condition));
                            pending.push(Input::Expr(then_value));
                            pending.push(Input::Expr(else_value));
                        }
                        _ => return None,
                    }
                }
            }
        }
    }
    Some(cacheable)
}
enum Input<'a> {
    Block(&'a CoreBlock),
    Expr(&'a CoreExpr),
}

fn typed_weight(function: &TypedFunction, allow_foreign: bool) -> Option<usize> {
    if function.environment.is_some()
        || (function.foreign.is_some() && !allow_foreign)
        || function.callable_return_contract.is_some()
        || function.callable_contract.is_some()
        || !function.return_type.is_b0_scalar()
        || function.locals.iter().any(|l| !l.ty.is_b0_scalar())
        || function.values.iter().any(|t| !t.is_b0_scalar())
    {
        return None;
    }
    let mut weight = 0;
    text(&mut weight, &function.name);
    array::<LocalId>(&mut weight, function.parameters.len());
    array::<LocalDecl>(&mut weight, function.locals.len());
    array::<Type>(&mut weight, function.values.len());
    array::<BasicBlock>(&mut weight, function.blocks.len());
    for local in &function.locals {
        text(&mut weight, &local.name);
    }
    for block in &function.blocks {
        array::<Instruction>(&mut weight, block.instructions.len());
        for instruction in &block.instructions {
            use InstructionKind as I;
            match &instruction.kind {
                I::ScopeEnter { .. }
                | I::ScopeExit { .. }
                | I::Load(_)
                | I::Store { .. }
                | I::StorageLive(_)
                | I::StorageDead(_)
                | I::Unary { .. }
                | I::Binary { .. } => {}
                I::Constant(value) if value.ty().is_b0_scalar() => {}
                I::ConvertInteger { target, .. } if target.is_b0_scalar() => {}
                I::Call { arguments, .. } => array::<ValueId>(&mut weight, arguments.len()),
                I::ForeignCall { arguments, .. } if allow_foreign => {
                    array::<ValueId>(&mut weight, arguments.len())
                }
                I::UnsafeEnter { .. } | I::UnsafeExit { .. } if allow_foreign => {}
                _ => return None,
            }
        }
    }
    Some(weight)
}

#[cfg(test)]
#[path = "cp32_program_memo_tests.rs"]
mod cp32_tests;
