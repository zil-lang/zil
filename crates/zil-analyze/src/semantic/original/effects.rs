//! Universal original ABI effects. Closed static edges use declared fixed
//! kinds; unresolved edges remain obligations for the original type solver.
//! Closure creation and a worker reference never execute the referenced body.
use super::sealed::{self, Resolution};
use super::*;

pub(super) struct Pending {
    calls: Vec<(SemanticDefinitionId, NodeId)>,
}
struct Walk<'a, 'i> {
    inventory: &'i Inventory<'a>,
    owner: SemanticDefinitionId,
    kind: CallKind,
    pending: Pending,
    unsafe_depth: usize,
}
impl Walk<'_, '_> {
    fn step(&self, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("original effect graph exceeds its finite depth"));
        }
        Ok(())
    }
    fn copy(&self, names: &BTreeSet<String>, b: &mut Budget) -> CompileResult<BTreeSet<String>> {
        b.spend(
            names
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(names.len()),
            WORK,
        )?;
        Ok(names.clone())
    }
    fn bind(&self, names: &mut BTreeSet<String>, name: &str, b: &mut Budget) -> CompileResult<()> {
        b.spend(names.len().saturating_add(name.len() + 1), WORK)?;
        names.insert(name.to_owned());
        Ok(())
    }
    fn pattern(
        &self,
        names: &mut BTreeSet<String>,
        pattern: &SemanticOriginalPattern,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => self.bind(names, name, b)?,
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.pattern(names, field, b, depth + 1)?;
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.pattern(names, field, b, depth + 1)?;
                }
            }
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => {
                self.pattern(names, pattern, b, depth + 1)?
            }
            _ => {}
        }
        Ok(())
    }
    fn required(&self, kind: CallKind) -> CompileResult<()> {
        if kind == CallKind::Suspend && self.kind == CallKind::Direct {
            return Err(fail(
                "original Direct body executes a Suspend operation or callee",
            ));
        }
        Ok(())
    }
    fn call(
        &mut self,
        names: &BTreeSet<String>,
        callee: &SemanticOriginalExpr,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let resolution = if let Some(path) = sealed::path(callee, b, 0)? {
            sealed::resolve(self.inventory, self.owner, &path, names, b)?
        } else {
            Resolution::Deferred
        };
        match resolution {
            Resolution::Definition(id) => {
                let definition = self.inventory.definition(id, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    signature, body, ..
                } = &definition.body
                else {
                    Solver::push(&mut self.pending.calls, (self.owner, node), b)?;
                    return Ok(());
                };
                if matches!(body, SemanticOriginalFunctionBody::External) && self.unsafe_depth == 0
                {
                    return Err(fail(
                        "original foreign invocation requires its real unsafe lexical region",
                    ));
                }
                self.required(signature.call_kind)
            }
            Resolution::Intrinsic(id) => self.required(id.call_kind()),
            Resolution::Deferred => Solver::push(&mut self.pending.calls, (self.owner, node), b),
        }
    }
    fn expr(
        &mut self,
        names: &mut BTreeSet<String>,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match &value.kind {
            CoreExprKind::Call { callee, arguments } => {
                self.expr(names, callee, b, depth + 1)?;
                for argument in arguments {
                    self.expr(names, argument, b, depth + 1)?;
                }
                self.call(names, callee, value.id, b)?;
            }
            CoreExprKind::Pipeline { input, target } => {
                self.expr(names, input, b, depth + 1)?;
                self.expr(names, target, b, depth + 1)?;
                self.call(names, target, value.id, b)?;
            }
            CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expr(names, callee, b, depth + 1)?;
                for argument in arguments {
                    self.expr(names, argument, b, depth + 1)?;
                }
                Solver::push(&mut self.pending.calls, (self.owner, value.id), b)?;
            }
            CoreExprKind::Closure { .. } => {}
            CoreExprKind::CompileTime(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::Splice(_)
            | CoreExprKind::SyntaxQuote { .. }
            | CoreExprKind::SyntaxIsland { .. } => {}
            CoreExprKind::MakeDynBorrow { source, .. }
            | CoreExprKind::MakeDynOwned { source, .. }
            | CoreExprKind::Ascribed { value: source, .. }
            | CoreExprKind::NumericConvert { value: source, .. }
            | CoreExprKind::IntegerConvert { value: source, .. }
            | CoreExprKind::Unary {
                operand: source, ..
            }
            | CoreExprKind::Group(source)
            | CoreExprKind::Try(source)
            | CoreExprKind::Spread(source)
            | CoreExprKind::TypeApply { callee: source, .. }
            | CoreExprKind::GenericApply { callee: source, .. }
            | CoreExprKind::Member { object: source, .. } => {
                self.expr(names, source, b, depth + 1)?
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Range {
                start: left,
                end: right,
            }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                self.expr(names, left, b, depth + 1)?;
                self.expr(names, right, b, depth + 1)?;
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for field in fields {
                    self.expr(names, field, b, depth + 1)?;
                }
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(names, object, b, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(names, endpoint, b, depth + 1)?;
                }
            }
            CoreExprKind::Record { fields, .. }
            | CoreExprKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.expr(names, field, b, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expr(names, base, b, depth + 1)?;
                for (_, field) in fields {
                    self.expr(names, field, b, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut local = self.copy(names, b)?;
                for (name, value) in bindings {
                    self.expr(&mut local, value, b, depth + 1)?;
                    self.bind(&mut local, name, b)?;
                }
                self.expr(&mut local, value, b, depth + 1)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(names, condition, b, depth + 1)?;
                self.expr(names, then_value, b, depth + 1)?;
                self.expr(names, else_value, b, depth + 1)?;
            }
            CoreExprKind::Unsupported(_) => {
                return Err(unknown("original effect operation has no complete meaning"));
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Character(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::ReceiverMember(_) => {}
        }
        Ok(())
    }
    fn scoped(
        &mut self,
        names: &BTreeSet<String>,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let mut local = self.copy(names, b)?;
        self.block(&mut local, body, b, depth + 1)
    }
    fn stmt(
        &mut self,
        names: &mut BTreeSet<String>,
        statement: &SemanticOriginalStatement,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match &statement.kind {
            CoreStatementKind::Binding {
                name, initializer, ..
            } => {
                if let Some(value) = initializer {
                    self.expr(names, value, b, depth + 1)?;
                }
                self.bind(names, name, b)?;
            }
            CoreStatementKind::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.expr(names, initializer, b, depth + 1)?;
                self.pattern(names, pattern, b, depth + 1)?;
            }
            CoreStatementKind::Assign { target, value }
            | CoreStatementKind::CompoundAssign { target, value, .. } => {
                self.expr(names, target, b, depth + 1)?;
                self.expr(names, value, b, depth + 1)?;
            }
            CoreStatementKind::Expression(value) => self.expr(names, value, b, depth + 1)?,
            CoreStatementKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(names, value, b, depth + 1)?;
                }
            }
            CoreStatementKind::Yield(value) => {
                self.expr(names, value, b, depth + 1)?;
                self.required(CallKind::Suspend)?;
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(names, condition, b, depth + 1)?;
                self.scoped(names, then_body, b, depth + 1)?;
                if let Some(body) = else_body {
                    self.scoped(names, body, b, depth + 1)?;
                }
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expr(names, value, b, depth + 1)?;
                let mut local = self.copy(names, b)?;
                self.pattern(&mut local, pattern, b, depth + 1)?;
                self.block(&mut local, then_body, b, depth + 1)?;
                if let Some(body) = else_body {
                    self.scoped(names, body, b, depth + 1)?;
                }
            }
            CoreStatementKind::While { condition, body } => {
                self.expr(names, condition, b, depth + 1)?;
                self.scoped(names, body, b, depth + 1)?;
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            }
            | CoreStatementKind::ForEach {
                pattern,
                iterator: value,
                body,
            } => {
                self.expr(names, value, b, depth + 1)?;
                let mut local = self.copy(names, b)?;
                self.pattern(&mut local, pattern, b, depth + 1)?;
                self.block(&mut local, body, b, depth + 1)?;
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expr(names, start, b, depth + 1)?;
                self.expr(names, end, b, depth + 1)?;
                let mut local = self.copy(names, b)?;
                self.bind(&mut local, name, b)?;
                self.block(&mut local, body, b, depth + 1)?;
            }
            CoreStatementKind::Match { value, arms } => {
                self.expr(names, value, b, depth + 1)?;
                for arm in arms {
                    let mut local = self.copy(names, b)?;
                    self.pattern(&mut local, &arm.pattern, b, depth + 1)?;
                    self.block(&mut local, &arm.body, b, depth + 1)?;
                }
            }
            CoreStatementKind::Block(body)
            | CoreStatementKind::Defer(body)
            | CoreStatementKind::Loop(body) => self.scoped(names, body, b, depth + 1)?,
            CoreStatementKind::Unsafe(body) => {
                self.unsafe_depth += 1;
                self.scoped(names, body, b, depth + 1)?;
                self.unsafe_depth -= 1;
            }
            CoreStatementKind::With { name, value, body } => {
                self.expr(names, value, b, depth + 1)?;
                let mut local = self.copy(names, b)?;
                self.bind(&mut local, name, b)?;
                self.block(&mut local, body, b, depth + 1)?;
            }
            CoreStatementKind::CompileTime(_) => {}
            CoreStatementKind::Break | CoreStatementKind::Continue => {}
            CoreStatementKind::Unsupported(_) => {
                return Err(unknown("original effect statement has no complete meaning"));
            }
        }
        Ok(())
    }
    fn block(
        &mut self,
        names: &mut BTreeSet<String>,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        for statement in &body.statements {
            self.stmt(names, statement, b, depth + 1)?;
        }
        Ok(())
    }
}

pub(super) fn collect(
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Pending> {
    let mut pending = Pending { calls: Vec::new() };
    for definition in &program.definitions {
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body,
            ..
        } = &definition.body
        else {
            continue;
        };
        let mut walk = Walk {
            inventory,
            owner: definition.id,
            kind: signature.call_kind,
            pending: Pending { calls: Vec::new() },
            unsafe_depth: 0,
        };
        let mut names = BTreeSet::new();
        for parameter in parameters {
            walk.bind(&mut names, &parameter.name, b)?;
        }
        // Real captured runtime names shadow core operations too. Type-only
        // ghost environment slots do not invent a runtime binding.
        for environment in &definition.environment {
            if let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                source: SemanticOriginalTypeSource::NameAt { name, .. },
                ..
            }) = &environment.source
            {
                walk.bind(&mut names, name, b)?;
            }
        }
        if let SemanticOriginalFunctionBody::Defined(body) = body {
            walk.block(&mut names, body, b, 0)?;
        }
        for call in walk.pending.calls {
            Solver::push(&mut pending.calls, call, b)?;
        }
    }
    Ok(pending)
}
impl Pending {
    pub(super) fn finish(
        self,
        checked: &Checked,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        for (owner, node) in self.calls {
            b.spend(checked.calls.len().saturating_add(1), WORK)?;
            let caller = inventory.definition(owner, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature: caller, ..
            } = &caller.body
            else {
                return Err(fail("original effect edge has no caller signature"));
            };
            b.lookup(checked.abstract_calls.len(), WORK)?;
            if let Some(call) = checked.abstract_calls.get(&(owner, node)) {
                if caller.call_kind == CallKind::Direct
                    && call.signature.call_kind == CallKind::Suspend
                {
                    return Err(fail(
                        "original Direct body invokes a Suspend abstract callable",
                    ));
                }
                continue;
            }
            b.lookup(checked.enum_constructors.len(), WORK)?;
            if checked.enum_constructors.contains_key(&(owner, node)) {
                continue;
            }
            let call = checked.calls.get(&(owner, node)).ok_or_else(|| {
                unknown("original call effect edge has no complete declaration-domain resolution")
            })?;
            let definition = inventory.definition(call.target, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature: callee, ..
            } = &definition.body
            else {
                return Err(fail("original effect edge selects a nonfunction target"));
            };
            let caller = inventory.definition(owner, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature: caller, ..
            } = &caller.body
            else {
                return Err(fail("original effect edge has no caller signature"));
            };
            if caller.call_kind == CallKind::Direct && callee.call_kind == CallKind::Suspend {
                return Err(fail("original Direct body invokes a Suspend callable"));
            }
        }
        Ok(())
    }
}
