//! Preserve request-local Core references through a verified CT rewrite.
//! This is structural relocation, never class, source, provider or borrow proof.
use super::*;
use zil_base::{ModuleId, NodeId};
type Key = (NodeId, NodeId, u8);
type Creator = Option<(ModuleId, NodeId)>;
struct Inventory {
    module: ModuleId,
    wanted: BTreeMap<Key, u8>,
    phase: u8,
    temporary: usize,
}
fn invalid(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-SEMANTIC-RELOCATION",
        None,
        message,
    )]
}
impl Inventory {
    fn want(&mut self, key: Key, budget: &mut Walker<'_>) -> CompileResult<()> {
        budget.charge(0, Span::default())?;
        let search = self.wanted.len().checked_ilog2().unwrap_or(0) as usize + 1;
        budget.work_left = budget.work_left.checked_sub(search).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "CT semantic reference lookup budget exhausted",
                None,
            )]
        })?;
        if !self.wanted.contains_key(&key) {
            self.temporary = self
                .temporary
                .saturating_add(12 * std::mem::size_of::<(Key, u8)>());
        }
        if self.temporary > budget.limits.max_source_bytes {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT semantic reference inventory exceeds temporary bytes",
                None,
            )]);
        }
        self.wanted.entry(key).or_insert(0);
        Ok(())
    }
    fn enter(
        &mut self,
        key: Key,
        budget: &mut Walker<'_>,
        span: Span,
        depth: usize,
    ) -> CompileResult<()> {
        budget.charge(depth, span)?;
        let search = self.wanted.len().checked_ilog2().unwrap_or(0) as usize + 1;
        budget.work_left = budget.work_left.checked_sub(search).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "CT semantic relocation work exhausted",
                Some(span),
            )]
        })?;
        if let Some(seen) = self.wanted.get_mut(&key) {
            if *seen & self.phase != 0 {
                return Err(invalid(
                    "CT semantic emission identity is duplicated in its actual creator",
                ));
            }
            *seen |= self.phase;
        }
        Ok(())
    }
    fn function(
        &mut self,
        target: &SemanticFunctionTarget,
        budget: &mut Walker<'_>,
    ) -> CompileResult<Creator> {
        budget.charge(0, Span::default())?;
        match *target {
            SemanticFunctionTarget::Core { module, function } => {
                if module == self.module {
                    self.want((function, function, 2), budget)?;
                }
                Ok(Some((module, function)))
            }
            SemanticFunctionTarget::CoreClosure {
                module,
                creator,
                expression,
                occurrence: 0,
            } => {
                self.closure(
                    &SemanticClosureTarget::Core {
                        module,
                        creator,
                        expression,
                        occurrence: 0,
                    },
                    budget,
                )?;
                Ok(Some((module, creator)))
            }
            _ => Err(invalid(
                "CT Core semantic target has no exact Core emission occurrence",
            )),
        }
    }
    fn closure(
        &mut self,
        target: &SemanticClosureTarget,
        budget: &mut Walker<'_>,
    ) -> CompileResult<()> {
        budget.charge(0, Span::default())?;
        match *target {
            SemanticClosureTarget::Core {
                module,
                creator,
                expression,
                occurrence: 0,
            } => {
                if module == self.module {
                    self.want((creator, creator, 2), budget)?;
                    self.want((creator, expression, 0), budget)?;
                }
                Ok(())
            }
            _ => Err(invalid(
                "CT Core closure target lacks its actual Core emission",
            )),
        }
    }
    fn operand(
        &mut self,
        creator: Creator,
        value: &SemanticOperand,
        budget: &mut Walker<'_>,
    ) -> CompileResult<()> {
        budget.charge(0, Span::default())?;
        match value {
            SemanticOperand::CoreExpression {
                expression,
                occurrence: 0,
            } => {
                let (module, creator) = creator
                    .ok_or_else(|| invalid("CT Core operand has no actual function owner"))?;
                if module == self.module {
                    self.want((creator, *expression, 0), budget)?;
                }
                Ok(())
            }
            SemanticOperand::CoreExpression { .. } => Err(invalid(
                "CT Core operand occurrence is not an actual unique emission",
            )),
            _ => Ok(()),
        }
    }
    fn class(
        &mut self,
        creator: Creator,
        value: &SemanticClassExpr,
        budget: &mut Walker<'_>,
    ) -> CompileResult<()> {
        budget.charge(0, Span::default())?;
        match value {
            SemanticClassExpr::OuterOccurrence { operand, .. }
            | SemanticClassExpr::IntrinsicClass { operand, .. } => {
                self.operand(creator, operand, budget)
            }
            _ => Ok(()),
        }
    }
    fn ty(
        &mut self,
        creator: Creator,
        value: &SemanticTypeValue,
        budget: &mut Walker<'_>,
    ) -> CompileResult<()> {
        budget.charge(0, Span::default())?;
        match value {
            SemanticTypeValue::Resolved { classes, .. } => {
                for class in classes {
                    self.class(creator, class, budget)?;
                }
            }
            SemanticTypeValue::Core {
                source:
                    SemanticCoreTypeSource::Expression {
                        module,
                        function,
                        expression,
                        occurrence,
                        ..
                    }
                    | SemanticCoreTypeSource::OriginalExpression {
                        module,
                        function,
                        expression,
                        occurrence,
                        ..
                    },
                ..
            } => {
                if *occurrence != 0 {
                    return Err(invalid(
                        "CT deferred type occurrence is not an actual Core emission",
                    ));
                }
                if *module == self.module {
                    self.want((*function, *function, 2), budget)?;
                    self.want((*function, *expression, 0), budget)?;
                }
            }
            SemanticTypeValue::Core {
                source: SemanticCoreTypeSource::OriginalRecipe(_),
                ..
            } => {}
        }
        Ok(())
    }
    fn actual(
        &mut self,
        creator: Creator,
        value: &SemanticActual,
        budget: &mut Walker<'_>,
    ) -> CompileResult<()> {
        budget.charge(0, Span::default())?;
        match value {
            SemanticActual::Type(value) | SemanticActual::Const { ty: value, .. } => {
                self.ty(creator, value, budget)
            }
            SemanticActual::Life(value) => self.class(creator, value, budget),
            SemanticActual::CoreRecipe { .. } | SemanticActual::CoreInferred { .. } => Ok(()),
        }
    }
    fn expression(
        &mut self,
        creator: NodeId,
        e: &CoreExpr,
        budget: &mut Walker<'_>,
        depth: usize,
    ) -> CompileResult<()> {
        self.enter((creator, e.id, 0), budget, e.span, depth)?;
        match &e.kind {
            CoreExprKind::MakeDynBorrow { source, .. }
            | CoreExprKind::MakeDynOwned { source, .. } => {
                self.expression(creator, source, budget, depth + 1)?
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Try(value) => self.expression(creator, value, budget, depth + 1)?,
            CoreExprKind::Unary { operand, .. } => {
                self.expression(creator, operand, budget, depth + 1)?
            }
            CoreExprKind::Binary { left, right, .. } => {
                self.expression(creator, left, budget, depth + 1)?;
                self.expression(creator, right, budget, depth + 1)?;
            }
            CoreExprKind::TypeApply { callee, .. } => {
                self.expression(creator, callee, budget, depth + 1)?
            }
            CoreExprKind::Call { callee, arguments }
            | CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expression(creator, callee, budget, depth + 1)?;
                for arg in arguments {
                    self.expression(creator, arg, budget, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    self.expression(creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.expression(creator, object, budget, depth + 1)?;
                self.expression(creator, index, budget, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expression(creator, object, budget, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, .. } => {
                self.expression(creator, object, budget, depth + 1)?
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.expression(creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expression(creator, base, budget, depth + 1)?;
                for (_, value) in fields {
                    self.expression(creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, value) in bindings {
                    self.expression(creator, value, budget, depth + 1)?;
                }
                self.expression(creator, value, budget, depth + 1)?;
            }
            CoreExprKind::Closure { body, .. } => self.block(creator, body, budget, depth + 1)?,
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(creator, condition, budget, depth + 1)?;
                self.expression(creator, then_value, budget, depth + 1)?;
                self.expression(creator, else_value, budget, depth + 1)?;
            }
            CoreExprKind::Spread(_)
            | CoreExprKind::Group(_)
            | CoreExprKind::Character(_)
            | CoreExprKind::Range { .. }
            | CoreExprKind::Pipeline { .. }
            | CoreExprKind::RecordConstructor { .. }
            | CoreExprKind::Splice(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::SyntaxQuote { .. }
            | CoreExprKind::SyntaxIsland { .. }
            | CoreExprKind::GenericApply { .. } => {
                return Err(invalid(
                    "CT semantic relocation requires actual elaborated Core operations",
                ));
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::ReceiverMember(_)
            | CoreExprKind::Unsupported(_) => {}
        }
        Ok(())
    }
    fn block(
        &mut self,
        creator: NodeId,
        body: &CoreBlock,
        budget: &mut Walker<'_>,
        depth: usize,
    ) -> CompileResult<()> {
        budget.charge(depth, body.span)?;
        for s in &body.statements {
            self.enter((creator, s.id, 1), budget, s.span, depth + 1)?;
            match &s.kind {
                CoreStatementKind::Binding { initializer, .. } => {
                    if let Some(value) = initializer {
                        self.expression(creator, value, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::TupleBinding { initializer, .. } => {
                    self.expression(creator, initializer, budget, depth + 1)?
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expression(creator, target, budget, depth + 1)?;
                    self.expression(creator, value, budget, depth + 1)?;
                }
                CoreStatementKind::Expression(value) => {
                    self.expression(creator, value, budget, depth + 1)?
                }
                CoreStatementKind::Return(value) => {
                    if let Some(value) = value {
                        self.expression(creator, value, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expression(creator, condition, budget, depth + 1)?;
                    self.block(creator, then_body, budget, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(creator, body, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::IfLet {
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.expression(creator, value, budget, depth + 1)?;
                    self.block(creator, then_body, budget, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(creator, body, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expression(creator, condition, budget, depth + 1)?;
                    self.block(creator, body, budget, depth + 1)?;
                }
                CoreStatementKind::WhileLet { value, body, .. } => {
                    self.expression(creator, value, budget, depth + 1)?;
                    self.block(creator, body, budget, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    self.expression(creator, start, budget, depth + 1)?;
                    self.expression(creator, end, budget, depth + 1)?;
                    self.block(creator, body, budget, depth + 1)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expression(creator, value, budget, depth + 1)?;
                    for arm in arms {
                        self.block(creator, &arm.body, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Defer(body)
                | CoreStatementKind::Unsafe(body) => {
                    self.block(creator, body, budget, depth + 1)?
                }
                CoreStatementKind::ForEach { .. }
                | CoreStatementKind::CompoundAssign { .. }
                | CoreStatementKind::Yield(_)
                | CoreStatementKind::Loop(_)
                | CoreStatementKind::With { .. }
                | CoreStatementKind::CompileTime(_) => {
                    return Err(invalid(
                        "CT semantic relocation requires actual elaborated Core statements",
                    ));
                }
                CoreStatementKind::Break
                | CoreStatementKind::Continue
                | CoreStatementKind::Unsupported(_) => {}
            }
        }
        Ok(())
    }
}
pub(super) fn preserve(walker: &mut Walker<'_>, functions: &[CoreFunction]) -> CompileResult<()> {
    let core = walker.core;
    let Some(program) = &core.semantic else {
        return Ok(());
    };
    let mut inventory = Inventory {
        module: core.module_id,
        wanted: BTreeMap::new(),
        phase: 1,
        temporary: 0,
    };
    let mut owners = BTreeMap::new();
    for instance in &program.instances {
        walker.charge(0, Span::default())?;
        let owner = match &instance.target {
            SemanticInstanceTarget::Function(target) => inventory.function(target, walker)?,
            SemanticInstanceTarget::Scope => None,
        };
        inventory.temporary = inventory
            .temporary
            .saturating_add(12 * std::mem::size_of::<(SemanticInstanceId, Creator)>());
        if inventory.temporary > walker.limits.max_source_bytes {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT semantic owner inventory exceeds temporary bytes",
                None,
            )]);
        }
        if owners.insert(instance.id, owner).is_some() {
            return Err(invalid("CT semantic instance identity is duplicated"));
        }
    }
    for instance in &program.instances {
        walker.charge(0, Span::default())?;
        let search = owners.len().checked_ilog2().unwrap_or(0) as usize + 1;
        walker.work_left = walker
            .work_left
            .checked_sub(search.saturating_mul(2))
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT semantic owner lookup work exhausted",
                    None,
                )]
            })?;
        let creator = owners[&instance.id];
        for actual in &instance.actuals {
            inventory.actual(creator, actual, walker)?;
        }
        for binding in &instance.header_embedding {
            inventory.class(creator, &binding.actual, walker)?;
        }
        if let Some(value) = &instance.self_type {
            inventory.ty(creator, value, walker)?;
        }
        if let Some(environment) = &instance.environment {
            inventory.closure(&environment.closure, walker)?;
            let parent = instance
                .parent
                .and_then(|id| owners.get(&id).copied())
                .flatten();
            for actual in &environment.actuals {
                inventory.actual(parent, actual, walker)?;
            }
        }
    }
    for site in &program.sites {
        walker.charge(0, Span::default())?;
        let search = owners.len().checked_ilog2().unwrap_or(0) as usize + 1;
        walker.work_left = walker.work_left.checked_sub(search).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "CT semantic site owner lookup work exhausted",
                None,
            )]
        })?;
        let creator = *owners
            .get(&site.instance)
            .ok_or_else(|| invalid("CT semantic site owner is absent"))?;
        let (module, function) =
            creator.ok_or_else(|| invalid("CT semantic operation has no actual Core function"))?;
        let SemanticSiteAddress::Core {
            expression,
            occurrence: 0,
            role,
        } = site.at
        else {
            return Err(invalid(
                "CT semantic operation does not name a unique Core emission",
            ));
        };
        if module == core.module_id {
            inventory.want(
                (
                    function,
                    expression,
                    u8::from(role == SemanticEmissionRole::Binding),
                ),
                walker,
            )?;
        }
        match &site.action {
            SemanticSiteAction::Construct { result, value } => {
                inventory.operand(creator, result, walker)?;
                inventory.ty(creator, value, walker)?;
            }
            SemanticSiteAction::CoreBodyCall => {}
            SemanticSiteAction::BodyCall { actuals } => {
                for actual in actuals {
                    inventory.actual(creator, actual, walker)?;
                }
            }
            SemanticSiteAction::Call { target, actuals } => {
                inventory.function(target, walker)?;
                for actual in actuals {
                    inventory.actual(creator, actual, walker)?;
                }
            }
            SemanticSiteAction::CallableCall { callee } => {
                inventory.operand(creator, callee, walker)?
            }
            SemanticSiteAction::TypeUse { subject, expected } => {
                inventory.operand(creator, subject, walker)?;
                inventory.ty(creator, expected, walker)?;
            }
            SemanticSiteAction::DeclareLocal { local, expected } => {
                let SemanticLocalTarget::Core {
                    statement,
                    occurrence: 0,
                } = local
                else {
                    return Err(invalid(
                        "CT local does not name its actual Core declaration",
                    ));
                };
                if module == core.module_id {
                    inventory.want((function, *statement, 1), walker)?;
                }
                inventory.ty(creator, expected, walker)?;
            }
            SemanticSiteAction::Closure {
                result,
                closure,
                environment,
            } => {
                inventory.operand(creator, result, walker)?;
                inventory.closure(closure, walker)?;
                for actual in environment {
                    inventory.actual(creator, actual, walker)?;
                }
            }
        }
    }
    for (phase, definitions) in [(1, core.functions.as_slice()), (2, functions)] {
        inventory.phase = phase;
        for function in definitions {
            inventory.enter((function.id, function.id, 2), walker, function.span, 0)?;
            inventory.block(function.id, &function.body, walker, 0)?;
        }
    }
    if inventory.wanted.values().any(|seen| *seen != 3) {
        return Err(invalid(
            "verified CT rewrite retired an original semantic occurrence still required by its program",
        ));
    }
    Ok(())
}
