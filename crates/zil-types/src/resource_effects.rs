//! Bounded source-only write effects used to select a local closure receiver.
//! Independent Verify reconstructs the effects from the actual typed bodies.
use super::*;
use std::collections::{BTreeSet, VecDeque};

type Origins = BTreeSet<usize>;
pub(super) type Writes = BTreeMap<FunctionId, Origins>;
type Tables = BTreeMap<zil_base::ModuleId, BTreeMap<String, Signature>>;
type CallInfo = (FunctionId, Vec<bool>);

enum MethodInfo {
    Call(CallInfo),
    Field,
    Unknown,
}

struct Input<'a> {
    function: &'a CoreFunction,
    signatures: &'a BTreeMap<String, Signature>,
    imports: &'a [CoreImport],
}

pub(super) fn single(
    core: &CoreModule,
    table: &BTreeMap<String, Signature>,
    limits: &Limits,
) -> R<Writes> {
    solve(
        core.functions
            .iter()
            .filter(|function| {
                function.callable_parameter.is_none()
                    && function
                        .parameters
                        .iter()
                        .any(|p| matches!(p.ty, Type::Desc(_)))
            })
            .map(|function| Input {
                function,
                signatures: table,
                imports: &[],
            })
            .collect(),
        None,
        limits,
    )
}

pub(super) fn program(program: &CoreProgram, tables: &Tables, limits: &Limits) -> R<Writes> {
    let mut inputs = Vec::new();
    for module in &program.modules {
        let table = &tables[&module.core.module_id];
        for function in &module.core.functions {
            if function.callable_parameter.is_some() {
                continue;
            }
            if !function
                .parameters
                .iter()
                .any(|p| matches!(p.ty, Type::Desc(_)))
            {
                continue;
            }
            inputs.push(Input {
                function,
                signatures: table,
                imports: &module.imports,
            });
        }
    }
    solve(inputs, Some(tables), limits)
}

fn solve(inputs: Vec<Input<'_>>, tables: Option<&Tables>, limits: &Limits) -> R<Writes> {
    // Keep the existing allocation-free scheduling path for small consumers.
    // This threshold is a performance choice, not a semantic boundary.
    if inputs.len() >= 16 {
        return solve_worklist(&inputs, tables, limits);
    }
    solve_sweeps(&inputs, tables, limits)
}

fn solve_sweeps(inputs: &[Input<'_>], tables: Option<&Tables>, limits: &Limits) -> R<Writes> {
    // Functions with no borrowed resource parameter cannot add a caller head
    // write effect. Ordinary Copy-only projects allocate no effect states.
    if inputs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut summaries = Writes::new();
    let mut work = limits.max_analysis_steps;
    loop {
        let mut changed = false;
        for input in inputs {
            let mut scan = Scan::<Untracked> {
                input,
                tables,
                summaries: &summaries,
                scopes: vec![BTreeMap::new()],
                writes: Origins::new(),
                callees: Untracked,
                work: &mut work,
                limits,
            };
            for (index, parameter) in input.function.parameters.iter().enumerate() {
                scan.spend(parameter.name.len().saturating_add(1), parameter.span)?;
                let origins = if matches!(parameter.ty, Type::Desc(_)) {
                    Origins::from([index])
                } else {
                    Origins::new()
                };
                scan.scopes[0].insert(parameter.name.clone(), origins);
            }
            if input.function.foreign.is_some() {
                // Such resource ABI declarations are currently rejected by
                // ordinary checking. Never infer a read-only effect for them.
                scan.writes.extend(
                    input
                        .function
                        .parameters
                        .iter()
                        .enumerate()
                        .filter_map(|(i, p)| matches!(p.ty, Type::Desc(_)).then_some(i)),
                );
            } else {
                scan.block(&input.function.body, 0)?;
            }
            let writes = scan.writes;
            let id = input.signatures[&input.function.name].id;
            let entry = summaries.entry(id).or_default();
            let before = entry.len();
            entry.extend(writes);
            changed |= before != entry.len();
        }
        if !changed {
            return Ok(summaries);
        }
    }
}

// Function dependencies, unlike parameter-fact edges, share a single edge
// across all the callee's parameter positions. The existing source transfer is
// reused verbatim. Verify still reconstructs its own effects independently.
#[inline(never)]
fn solve_worklist(inputs: &[Input<'_>], tables: Option<&Tables>, limits: &Limits) -> R<Writes> {
    let mut work = limits.max_analysis_steps;
    let span = inputs.first().map_or(Span::default(), |i| i.function.span);
    charge(&mut work, inputs.len().saturating_mul(5), span)?;
    let ids = inputs
        .iter()
        .map(|i| i.signatures[&i.function.name].id)
        .collect::<Vec<_>>();
    let positions = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect::<BTreeMap<_, _>>();
    let mut callers = vec![Vec::new(); inputs.len()];
    let mut summaries = Writes::new();
    let mut initially_changed = Vec::new();
    // Every body is visited once in its original declaration order. Dependency
    // extraction happens during this visit, never by a second syntax traversal.
    for (index, input) in inputs.iter().enumerate() {
        let (writes, dependencies) =
            scan_once::<Tracked>(input, tables, &summaries, &mut work, limits)?;
        for dependency in dependencies {
            charge(&mut work, 1, input.function.span)?;
            if let Some(&callee) = positions.get(&dependency) {
                callers[callee].push(index);
            }
        }
        if !writes.is_empty() {
            charge(&mut work, 1, input.function.span)?;
            initially_changed.push(index);
        }
        summaries.insert(ids[index], writes);
    }
    let mut queued = vec![false; inputs.len()];
    let mut queue = VecDeque::new();
    // A caller scanned after its callee has already observed that first result.
    // Only backward/self edges can be stale after the initial forward sweep.
    for callee in initially_changed {
        for &caller in &callers[callee] {
            charge(&mut work, 1, inputs[caller].function.span)?;
            if caller <= callee && !queued[caller] {
                charge(&mut work, 1, inputs[caller].function.span)?;
                queued[caller] = true;
                queue.push_back(caller);
            }
        }
    }
    while let Some(index) = queue.pop_front() {
        charge(&mut work, 1, inputs[index].function.span)?;
        queued[index] = false;
        let (writes, _) =
            scan_once::<Untracked>(&inputs[index], tables, &summaries, &mut work, limits)?;
        let entry = summaries
            .get_mut(&ids[index])
            .expect("initialized effect row");
        let before = entry.len();
        // Union keeps the original monotone transfer and its conservative
        // branch/loop treatment; this is scheduling, not a new safety rule.
        charge(&mut work, writes.len(), inputs[index].function.span)?;
        entry.extend(writes);
        if entry.len() != before {
            for &caller in &callers[index] {
                charge(&mut work, 1, inputs[caller].function.span)?;
                if !queued[caller] {
                    charge(&mut work, 1, inputs[caller].function.span)?;
                    queued[caller] = true;
                    queue.push_back(caller);
                }
            }
        }
    }
    Ok(summaries)
}

fn charge(work: &mut usize, amount: usize, span: Span) -> R<()> {
    *work = work.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "resource closure effect work budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

fn scan_once<T: Tracking>(
    input: &Input<'_>,
    tables: Option<&Tables>,
    summaries: &Writes,
    work: &mut usize,
    limits: &Limits,
) -> R<(Origins, BTreeSet<FunctionId>)> {
    let mut scan = Scan::<T> {
        input,
        tables,
        summaries,
        scopes: vec![BTreeMap::new()],
        writes: Origins::new(),
        callees: T::default(),
        work,
        limits,
    };
    for (index, parameter) in input.function.parameters.iter().enumerate() {
        scan.spend(parameter.name.len().saturating_add(1), parameter.span)?;
        let origins = if matches!(parameter.ty, Type::Desc(_)) {
            Origins::from([index])
        } else {
            Origins::new()
        };
        scan.scopes[0].insert(parameter.name.clone(), origins);
    }
    if input.function.foreign.is_some() {
        scan.writes.extend(
            input
                .function
                .parameters
                .iter()
                .enumerate()
                .filter_map(|(i, p)| matches!(p.ty, Type::Desc(_)).then_some(i)),
        );
    } else {
        scan.block(&input.function.body, 0)?;
    }
    Ok((scan.writes, scan.callees.finish()))
}

// The legacy small-input path retains its original Scan layout: collecting
// dependencies uses a type-level policy rather than an always-present set.
trait Tracking: Default {
    const ENABLED: bool;
    fn observe(&mut self, callee: FunctionId);
    fn finish(self) -> BTreeSet<FunctionId>;
}
#[derive(Default)]
struct Untracked;
impl Tracking for Untracked {
    const ENABLED: bool = false;
    fn observe(&mut self, _: FunctionId) {}
    fn finish(self) -> BTreeSet<FunctionId> {
        BTreeSet::new()
    }
}
#[derive(Default)]
struct Tracked(BTreeSet<FunctionId>);
impl Tracking for Tracked {
    const ENABLED: bool = true;
    fn observe(&mut self, callee: FunctionId) {
        self.0.insert(callee);
    }
    fn finish(self) -> BTreeSet<FunctionId> {
        self.0
    }
}

struct Scan<'a, 'b, T: Tracking> {
    input: &'a Input<'a>,
    tables: Option<&'a Tables>,
    summaries: &'a Writes,
    scopes: Vec<BTreeMap<String, Origins>>,
    writes: Origins,
    callees: T,
    work: &'b mut usize,
    limits: &'a Limits,
}
impl<T: Tracking> Scan<'_, '_, T> {
    fn spend(&mut self, amount: usize, span: Span) -> R<()> {
        *self.work = self.work.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "resource closure effect work budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    fn visit(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "resource closure effect depth budget exceeded",
                Some(span),
            ));
        }
        self.spend(1, span)
    }
    fn lookup(&self, name: &str) -> Option<&Origins> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
    fn record(&mut self, origins: Origins, span: Span) -> R<()> {
        self.spend(origins.len(), span)?;
        self.writes.extend(origins);
        Ok(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.visit(depth, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.visit(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name, initializer, ..
                } => {
                    let origins = initializer
                        .as_ref()
                        .map(|e| self.expr(e, depth + 1))
                        .transpose()?
                        .unwrap_or_default();
                    self.spend(name.len().saturating_add(origins.len()), statement.span)?;
                    self.scopes
                        .last_mut()
                        .expect("scope")
                        .insert(name.clone(), origins);
                }
                CoreStatementKind::Assign { target, value } => {
                    let target_origins = self.place(target, depth + 1)?;
                    let origins = self.expr(value, depth + 1)?;
                    if let CoreExprKind::Name(name) = &target.kind {
                        self.spend(origins.len(), target.span)?;
                        if let Some(slot) =
                            self.scopes.iter_mut().rev().find_map(|s| s.get_mut(name))
                        {
                            // Union is intentional: branch and loop choices
                            // cannot strengthen which resource a descriptor names.
                            slot.extend(origins);
                        }
                    } else {
                        self.record(target_origins, target.span)?;
                    }
                }
                CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.repeat(body, Some(condition), depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.scopes
                        .push(BTreeMap::from([(name.clone(), Origins::new())]));
                    self.repeat(body, None, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::IfLet {
                    value,
                    pattern,
                    then_body,
                    else_body,
                } => {
                    self.expr(value, depth + 1)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    self.scopes.pop();
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    value,
                    pattern,
                    body,
                } => {
                    self.expr(value, depth + 1)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, depth + 1)?;
                    self.repeat(body, Some(value), depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expr(value, depth + 1)?;
                    for arm in arms {
                        self.scopes.push(BTreeMap::new());
                        self.pattern(&arm.pattern, depth + 1)?;
                        self.block(&arm.body, depth + 1)?;
                        self.scopes.pop();
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
                _ => {}
            }
        }
        self.scopes.pop();
        Ok(())
    }
    fn repeat(&mut self, body: &CoreBlock, condition: Option<&CoreExpr>, depth: usize) -> R<()> {
        loop {
            let size = self
                .scopes
                .iter()
                .flat_map(|s| s.iter())
                .map(|(name, origins)| name.len().saturating_add(origins.len()).saturating_add(1))
                .fold(0usize, usize::saturating_add);
            self.spend(size.saturating_add(self.scopes.len()), body.span)?;
            let before = self.scopes.clone();
            if let Some(condition) = condition {
                self.expr(condition, depth + 1)?;
            }
            self.block(body, depth + 1)?;
            if before == self.scopes {
                return Ok(());
            }
        }
    }
    fn pattern(&mut self, pattern: &CorePattern, depth: usize) -> R<()> {
        self.visit(depth, pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.spend(name.len(), pattern.span)?;
                self.scopes
                    .last_mut()
                    .expect("scope")
                    .insert(name.clone(), Origins::new());
            }
            CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn callee(&self, expr: &CoreExpr) -> Option<&Signature> {
        match &expr.kind {
            CoreExprKind::Name(name) if self.lookup(name).is_none() => {
                self.input.signatures.get(name)
            }
            CoreExprKind::Member { object, member } => {
                let CoreExprKind::Name(name) = &object.kind else {
                    return None;
                };
                if self.lookup(name).is_some() {
                    return None;
                }
                self.input
                    .signatures
                    .get(&format!("{name}.{member}"))
                    .or_else(|| {
                        let import = self.input.imports.iter().find(|i| i.alias == *name)?;
                        self.tables?.get(&import.module)?.get(member)
                    })
            }
            _ => None,
        }
    }
    fn method_info(&mut self, origins: &Origins, member: &str, span: Span) -> R<MethodInfo> {
        // Every tracked origin names a real Desc parameter. Resolve the exact
        // nominal method from those parameter types, including local aliases.
        // An unknown/mixed shape remains a conservative write, never read-only.
        if origins.is_empty() {
            return Ok(MethodInfo::Unknown);
        }
        self.spend(
            origins.len().saturating_add(
                self.input.imports.iter().fold(0usize, |count, import| {
                    count.saturating_add(import.alias.len().saturating_add(1))
                }),
            ),
            span,
        )?;
        let imports = self.tables.map(|signatures| program::Imports {
            aliases: self
                .input
                .imports
                .iter()
                .map(|import| (import.alias.clone(), import.module))
                .collect(),
            signatures,
        });
        let mut chosen: Option<&Signature> = None;
        let mut fields = 0usize;
        for origin in origins {
            let Some(CoreParameter {
                ty: Type::Desc(payload),
                ..
            }) = self.input.function.parameters.get(*origin)
            else {
                return Ok(MethodInfo::Unknown);
            };
            if let Type::Record {
                fields: members, ..
            } = payload.as_ref()
            {
                self.spend(members.len(), span)?;
                if members.iter().any(|(name, _)| name == member) {
                    fields += 1;
                    continue;
                }
            }
            let Some(signature) = aggregates::resolve_type_method(
                payload,
                member,
                self.input.signatures,
                imports.as_ref(),
            ) else {
                return Ok(MethodInfo::Unknown);
            };
            if chosen.is_some_and(|previous| previous.id != signature.id) {
                return Ok(MethodInfo::Unknown);
            }
            chosen = Some(signature);
        }
        if fields == origins.len() {
            return Ok(MethodInfo::Field);
        }
        if fields != 0 {
            return Ok(MethodInfo::Unknown);
        }
        let Some(signature) = chosen else {
            return Ok(MethodInfo::Unknown);
        };
        let id = signature.id;
        let count = signature.parameters.len();
        self.spend(count, span)?;
        Ok(MethodInfo::Call((
            id,
            chosen
                .expect("chosen method")
                .parameters
                .iter()
                .map(|ty| matches!(ty, Type::Desc(_)))
                .collect(),
        )))
    }
    fn call_effect(
        &mut self,
        info: &Option<CallInfo>,
        index: usize,
        origins: Origins,
        span: Span,
    ) -> R<()> {
        if T::ENABLED
            && !origins.is_empty()
            && let Some((id, parameters)) = info
            && parameters.get(index) == Some(&true)
        {
            self.spend(1, span)?;
            self.callees.observe(*id);
        }
        let writes = match info {
            Some((id, parameters)) => {
                parameters.get(index) == Some(&true)
                    && self.summaries.get(id).is_some_and(|s| s.contains(&index))
            }
            None => true,
        };
        if writes {
            self.record(origins, span)?;
        }
        Ok(())
    }
    fn place(&mut self, expression: &CoreExpr, depth: usize) -> R<Origins> {
        self.visit(depth, expression.span)?;
        match &expression.kind {
            CoreExprKind::Member { object, member } => {
                self.spend(member.len(), expression.span)?;
                self.place(object, depth + 1)
            }
            CoreExprKind::Index { object, index } => {
                let origins = self.place(object, depth + 1)?;
                self.expr(index, depth + 1)?;
                Ok(origins)
            }
            CoreExprKind::Slice { object, start, end } => {
                let origins = self.place(object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(endpoint, depth + 1)?;
                }
                Ok(origins)
            }
            CoreExprKind::Ascribed { value, .. } => self.place(value, depth + 1),
            _ => self.expr(expression, depth + 1),
        }
    }
    fn expr(&mut self, expr: &CoreExpr, depth: usize) -> R<Origins> {
        self.visit(depth, expr.span)?;
        match &expr.kind {
            CoreExprKind::Name(name) => {
                self.spend(
                    name.len()
                        .saturating_add(self.lookup(name).map_or(0, BTreeSet::len)),
                    expr.span,
                )?;
                return Ok(self.lookup(name).cloned().unwrap_or_default());
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Try(value)
            | CoreExprKind::Unary { operand: value, .. } => return self.expr(value, depth + 1),
            CoreExprKind::Member { object, member } => {
                self.spend(member.len(), expr.span)?;
                self.expr(object, depth + 1)?;
                // A resource payload contains only Copy fields in this slice.
                // Reading one produces a snapshot; it does not turn a later
                // mutation of that Copy local into a write through the Desc.
            }
            CoreExprKind::Index { object, index } => {
                self.expr(object, depth + 1)?;
                self.expr(index, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(endpoint, depth + 1)?;
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                if let Some(name) = dynamic_store::primitive(callee) {
                    self.visit(depth + 1, callee.span)?;
                    for (index, argument) in arguments.iter().enumerate() {
                        let origins = self.expr(argument, depth + 1)?;
                        if index == 0 && matches!(name, "store_reserve" | "store_push" | "store_pop") {
                            self.record(origins, argument.span)?;
                        }
                    }
                    return Ok(Origins::new());
                }
                if let Some(name) = byte_buffer::primitive(callee) {
                    self.visit(depth + 1, callee.span)?;
                    for (index, argument) in arguments.iter().enumerate() {
                        let origins = self.expr(argument, depth + 1)?;
                        if index == 0
                            && matches!(
                                name,
                                "bytes_reserve"
                                    | "bytes_truncate"
                                    | "bytes_push"
                                    | "bytes_view_mut"
                            )
                        {
                            self.record(origins, argument.span)?;
                        }
                    }
                    return Ok(Origins::new());
                }
                if self.callee(callee).is_none()
                    && let CoreExprKind::Member { object, member } = &callee.kind
                {
                    self.visit(depth + 1, callee.span)?;
                    self.spend(member.len(), callee.span)?;
                    let receiver = self.expr(object, depth + 2)?;
                    let (info, field) = match self.method_info(&receiver, member, callee.span)? {
                        MethodInfo::Call(info) => (Some(info), false),
                        MethodInfo::Field => (None, true),
                        MethodInfo::Unknown => (None, false),
                    };
                    if !field {
                        self.call_effect(&info, 0, receiver, object.span)?;
                    }
                    for (index, argument) in arguments.iter().enumerate() {
                        let origins = self.expr(argument, depth + 1)?;
                        self.call_effect(&info, index + 1, origins, argument.span)?;
                    }
                    return Ok(Origins::new());
                }
                // Copy only signatures cannot accept a resource descriptor.
                // Unknown callable shapes are conservatively writes until
                // ordinary checking either resolves or rejects the call.
                self.spend(self.input.imports.len(), callee.span)?;
                let count = self.callee(callee).map_or(0, |s| s.parameters.len());
                self.spend(count, callee.span)?;
                let info = self.callee(callee).map(|s| {
                    (
                        s.id,
                        s.parameters
                            .iter()
                            .map(|p| matches!(p, Type::Desc(_)))
                            .collect::<Vec<_>>(),
                    )
                });
                self.expr(callee, depth + 1)?;
                for (index, argument) in arguments.iter().enumerate() {
                    let origins = self.expr(argument, depth + 1)?;
                    // Local origins do not depend on the effect summaries.
                    // Record potential dependencies even while a callee is
                    // read-only; otherwise a later write could never wake us.
                    if T::ENABLED
                        && !origins.is_empty()
                        && let Some((id, parameters)) = &info
                        && parameters.get(index) == Some(&true)
                    {
                        self.spend(1, argument.span)?;
                        self.callees.observe(*id);
                    }
                    let writes = match &info {
                        Some((id, parameters)) => {
                            parameters.get(index) == Some(&true)
                                && self.summaries.get(id).is_some_and(|s| s.contains(&index))
                        }
                        None => true,
                    };
                    if writes {
                        self.record(origins, argument.span)?;
                    }
                }
            }
            CoreExprKind::Binary { left, right, .. } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for field in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, field) in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expr(base, depth + 1)?;
                for (_, field) in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                let mut origins = self.expr(then_value, depth + 1)?;
                let other = self.expr(else_value, depth + 1)?;
                self.spend(other.len(), expr.span)?;
                origins.extend(other);
                return Ok(origins);
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.scopes.push(BTreeMap::new());
                for (name, value) in bindings {
                    let origins = self.expr(value, depth + 1)?;
                    self.spend(name.len().saturating_add(origins.len()), expr.span)?;
                    self.scopes
                        .last_mut()
                        .expect("scope")
                        .insert(name.clone(), origins);
                }
                let origins = self.expr(value, depth + 1)?;
                self.scopes.pop();
                return Ok(origins);
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                // A local body may be called through a later binding. Counting
                // its visible head writes is conservative even if uncalled.
                self.scopes.push(BTreeMap::new());
                for parameter in parameters {
                    self.spend(parameter.name.len(), parameter.span)?;
                    self.scopes
                        .last_mut()
                        .expect("scope")
                        .insert(parameter.name.clone(), Origins::new());
                }
                self.block(body, depth + 1)?;
                self.scopes.pop();
            }
            _ => {}
        }
        Ok(Origins::new())
    }
}

#[cfg(test)]
#[path = "resource_effects_worklist_tests.rs"]
mod worklist_tests;
