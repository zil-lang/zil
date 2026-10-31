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
    typed_weight: usize,
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
    functions: Vec<Vec<FunctionMemo>>,
    typed_weight: Vec<usize>,
    weight: usize,
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
        if program.modules.len() > MAX_MODULES {
            return None;
        }
        let mut weight = size_of::<Environment>() + size_of::<ProgramMemo>();
        for value in [
            &target.id,
            &target.triple,
            &target.backend,
            &target.rule_version,
        ] {
            text(&mut weight, value)?;
        }
        let mut cacheable = Vec::with_capacity(program.modules.len());
        for module in &program.modules {
            cacheable.push(source_weight(module, tables, &mut weight)?);
        }
        if !cacheable.iter().any(|enabled| *enabled) {
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
        let mut reuse = Vec::with_capacity(program.modules.len());
        let mut typed_weight = Vec::with_capacity(program.modules.len());
        for (index, module) in program.modules.iter().enumerate() {
            let old = previous
                .modules
                .get(index)
                .and_then(Option::as_ref)
                .filter(|old| {
                    cacheable[index]
                        && environment_matches
                        && old.core == module.core
                        && old.functions.len() == module.core.functions.len()
                });
            let old_weight = old.map_or(0, |old| old.typed_weight);
            add(&mut weight, old_weight)?;
            reuse.push(old.is_some());
            typed_weight.push(old_weight);
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
            typed_weight,
            weight,
            functions: (0..program.modules.len()).map(|_| Vec::new()).collect(),
            stats: ProgramMemoStats {
                eligible: true,
                ..ProgramMemoStats::default()
            },
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
            let size = typed_weight(typed, false)?;
            add(&mut self.weight, size)?;
            self.typed_weight[module] += size;
            self.functions[module].push(FunctionMemo {
                typed: typed.clone(),
                usage,
            });
        }
        Some(())
    }

    pub(super) fn finish(mut self) -> ProgramMemo {
        let environment = if self.environment_matches {
            self.previous.environment.clone()
        } else {
            Some(Arc::new(Environment {
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
        let modules = self
            .program
            .modules
            .iter()
            .enumerate()
            .map(|(index, module)| {
                if !self.cacheable[index] {
                    None
                } else if self.reuse[index] {
                    self.previous.modules[index].clone()
                } else {
                    Some(Arc::new(ModuleMemo {
                        core: module.core.clone(),
                        functions: std::mem::take(&mut self.functions[index]),
                        typed_weight: self.typed_weight[index],
                    }))
                }
            })
            .collect::<Vec<_>>();
        self.stats.retained_modules = modules.iter().filter(|m| m.is_some()).count();
        self.stats.retained_weight = self.weight;
        ProgramMemo {
            environment,
            modules,
            stats: self.stats,
        }
    }
}

fn add(weight: &mut usize, bytes: usize) -> Option<()> {
    *weight = weight.checked_add(bytes).filter(|n| *n <= MAX_WEIGHT)?;
    Some(())
}
fn array<T>(weight: &mut usize, length: usize) -> Option<()> {
    // Include allocation bookkeeping as structural slack, without claiming to
    // predict a particular allocator's usable size or resident pages.
    add(weight, length.checked_mul(size_of::<T>())?.checked_add(64)?)
}
fn text(weight: &mut usize, value: &str) -> Option<()> {
    add(weight, value.len().checked_add(64)?)
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
    add(weight, size_of::<ModuleMemo>() + 256)?;
    array::<CoreFunction>(weight, core.functions.len())?;
    array::<CoreImport>(weight, module.imports.len())?;
    for import in &module.imports {
        text(weight, &import.alias)?;
    }
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
            text(weight, &foreign.abi)?;
            text(weight, &foreign.symbol)?;
            array::<u32>(weight, foreign.noescape_parameters.len())?;
        }
        array::<PassingMode>(weight, f.parameters.len())?;
        add(weight, size_of::<DeclarationIdentity>())?;
        // Function/table String keys and all signature values are retained too.
        add(
            weight,
            size_of::<FunctionMemo>() + size_of::<Signature>() + 256,
        )?;
        text(weight, &f.name)?;
        text(weight, &f.name)?;
        text(weight, &f.name)?;
        array::<CoreParameter>(weight, f.parameters.len())?;
        array::<Type>(weight, f.parameters.len())?;
        for p in &f.parameters {
            text(weight, &p.name)?;
        }
        let mut pending = vec![Input::Block(&f.body)];
        while let Some(input) = pending.pop() {
            match input {
                Input::Block(block) => {
                    array::<CoreStatement>(weight, block.statements.len())?;
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
                                text(weight, name)?;
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
                                text(weight, name)?;
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
                    // A fixed node allowance bounds both boxed and inline children
                    // and the temporary traversal stack before growing it.
                    add(weight, size_of::<CoreExpr>() + 64)?;
                    use CoreExprKind as E;
                    match &e.kind {
                        E::Literal(value) if value.ty().is_b0_scalar() => {}
                        E::Integer(s) | E::Name(s) => text(weight, s)?,
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
                            array::<CoreExpr>(weight, arguments.len())?;
                            pending.push(Input::Expr(callee));
                            pending.extend(arguments.iter().map(Input::Expr));
                        }
                        E::Member { object, member } if matches!(object.kind, E::Name(_)) => {
                            text(weight, member)?;
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
    let mut weight = size_of::<FunctionMemo>();
    text(&mut weight, &function.name)?;
    array::<LocalId>(&mut weight, function.parameters.len())?;
    array::<LocalDecl>(&mut weight, function.locals.len())?;
    array::<Type>(&mut weight, function.values.len())?;
    array::<BasicBlock>(&mut weight, function.blocks.len())?;
    for local in &function.locals {
        text(&mut weight, &local.name)?;
    }
    for block in &function.blocks {
        array::<Instruction>(&mut weight, block.instructions.len())?;
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
                I::Call { arguments, .. } => array::<ValueId>(&mut weight, arguments.len())?,
                I::ForeignCall { arguments, .. } if allow_foreign => {
                    array::<ValueId>(&mut weight, arguments.len())?
                }
                I::UnsafeEnter { .. } | I::UnsafeExit { .. } if allow_foreign => {}
                _ => return None,
            }
        }
    }
    Some(weight)
}
