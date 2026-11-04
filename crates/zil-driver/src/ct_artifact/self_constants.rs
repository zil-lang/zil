//! Per-witness effective equations and separate overridden-default checks.
//! Only the ordinary CT compiler/verifier/VM produces scalar values.
use super::*;
use zil_expand::associated_constants::{Equation, Plan, Source};
use zil_expand::ct_selection::Visitor;

fn members(module: &s::Module, item: usize) -> CompileResult<&[s::Item]> {
    match module.items.get(item).map(|i| &i.kind) {
        Some(s::ItemKind::Trait(t)) => Ok(&t.members),
        Some(s::ItemKind::Impl(i)) => Ok(&i.members),
        _ => Err(category(
            "associated equation owner ordinal is absent",
            module.span,
        )),
    }
}
fn constant(module: &s::Module, source: Source) -> CompileResult<&s::ConstDecl> {
    match members(module, source.item)?
        .get(source.member)
        .map(|m| &m.kind)
    {
        Some(s::ItemKind::Const(c)) => Ok(c),
        _ => Err(category(
            "associated equation member ordinal is absent",
            module.span,
        )),
    }
}
fn self_member(value: &s::Expr) -> Option<&str> {
    let s::ExprKind::Member {
        object,
        member: s::MemberName::Name(name),
    } = &value.kind
    else {
        return None;
    };
    let mut object = object.as_ref();
    while let s::ExprKind::Group(inner) = &object.kind {
        object = inner;
    }
    matches!(&object.kind,s::ExprKind::Name(n) if n=="Self").then_some(name.as_str())
}
struct Rewrite<'a, 'b> {
    replacements: &'a [(String, s::Expr)],
    budget: &'b mut zil_ct::CtExecutionBudget,
    max_depth: usize,
}
impl Rewrite<'_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.reserve_analysis_work(1)?;
        if depth > self.max_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "Self equation substitution depth exhausted",
                Some(span),
            )]);
        }
        Ok(())
    }
    fn expression(&mut self, value: &mut s::Expr, depth: usize) -> CompileResult<()> {
        self.node(value.span, depth)?;
        if let Some(name) = self_member(value) {
            for (member, replacement) in self.replacements {
                self.budget
                    .reserve_analysis_work(name.len().min(member.len()) as u64 + 1)?;
                if member == name {
                    let cost =
                        zil_expand::ct_selection::expression_cost(replacement, &self.limits())?;
                    self.budget.reserve_analysis_work(cost.work as u64)?;
                    self.budget.reserve_generated_nodes(cost.nodes as u64)?;
                    self.budget.reserve_temporary_bytes(
                        (cost.work + cost.nodes * std::mem::size_of::<s::Expr>()) as u64,
                    )?;
                    let span = value.span;
                    *value = replacement.clone();
                    value.span = span;
                    return Ok(());
                }
            }
            return Err(category(
                "Self dependency has no completed effective value",
                value.span,
            ));
        }
        match &mut value.kind {
            s::ExprKind::Literal(_) | s::ExprKind::Name(_) => Ok(()),
            s::ExprKind::Group(v)
            | s::ExprKind::Unary { operand: v, .. }
            | s::ExprKind::MetaEval(v) => self.expression(v, depth + 1),
            s::ExprKind::Binary { left, right, .. } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)
            }
            s::ExprKind::Member { object, .. } => self.expression(object, depth + 1),
            s::ExprKind::Ascribed { value, .. } => self.expression(value, depth + 1),
            s::ExprKind::TypeApply { callee, arguments } => {
                self.expression(callee, depth + 1)?;
                for arg in arguments {
                    self.node(value.span, depth + 1)?;
                    if let s::GenericArg::Const(e) = arg {
                        self.expression(e, depth + 1)?;
                    }
                }
                Ok(())
            }
            s::ExprKind::Call { callee, arguments } => {
                self.expression(callee, depth + 1)?;
                for arg in arguments {
                    self.expression(&mut arg.value, depth + 1)?;
                }
                Ok(())
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)
            }
            s::ExprKind::MetaBlock(block) => self.block(block, depth + 1),
            _ => Err(runtime(
                "Self-dependent equation syntax needs a later scalar CT substitution consumer",
                value.span,
            )),
        }
    }
    fn limits(&self) -> Limits {
        Limits {
            max_call_depth: self.max_depth,
            max_nodes: self.budget.remaining().max_generated_nodes as usize,
            max_analysis_steps: self.budget.remaining().max_analysis_work as usize,
            ..Limits::default()
        }
    }
    fn condition(&mut self, c: &mut s::Condition, depth: usize) -> CompileResult<()> {
        match c {
            s::Condition::Expression(e) => self.expression(e, depth),
            s::Condition::Let { .. } => Err(runtime(
                "Self equation patterns need a later substitution consumer",
                Span::default(),
            )),
        }
    }
    fn statement(&mut self, stmt: &mut s::Stmt, depth: usize) -> CompileResult<()> {
        self.node(stmt.span, depth)?;
        match &mut stmt.kind {
            s::StmtKind::Binding { pattern, value, .. } => {
                if !matches!(&pattern.kind,s::PatternKind::Name(n) if n!="Self") {
                    return Err(runtime(
                        "Self equation bindings require unshadowed scalar names",
                        stmt.span,
                    ));
                }
                self.expression(value, depth + 1)
            }
            s::StmtKind::Return(Some(e)) | s::StmtKind::Expression(e) => {
                self.expression(e, depth + 1)
            }
            s::StmtKind::Return(None) | s::StmtKind::Break | s::StmtKind::Continue => Ok(()),
            s::StmtKind::Assign { target, value, .. } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)
            }
            s::StmtKind::While { condition, body } => {
                self.condition(condition, depth + 1)?;
                self.block(body, depth + 1)
            }
            s::StmtKind::Loop(body) => self.block(body, depth + 1),
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition, depth + 1)?;
                self.block(then_body, depth + 1)?;
                match else_body {
                    Some(s::ElseBranch::Block(b)) => self.block(b, depth + 1),
                    Some(s::ElseBranch::If(i)) => self.statement(i, depth + 1),
                    None => Ok(()),
                }
            }
            _ => Err(runtime(
                "Self equation statement is outside the pure scalar substitution profile",
                stmt.span,
            )),
        }
    }
    fn block(&mut self, block: &mut s::Block, depth: usize) -> CompileResult<()> {
        self.node(block.span, depth)?;
        for statement in &mut block.statements {
            self.statement(statement, depth + 1)?;
        }
        Ok(())
    }
}

impl Materializer<'_, '_> {
    fn self_owner(
        &mut self,
        module: ModuleId,
        source: Source,
        span: Span,
    ) -> CompileResult<CtSymbolId> {
        self.environment
            .budget
            .reserve_analysis_work(self.parsed.len() as u64)?;
        let parsed = self
            .parsed
            .iter()
            .find(|p| p.module.id == module)
            .ok_or_else(|| category("Self equation original module absent", span))?;
        let parent = zil_base::NodeId(
            u32::try_from(source.item)
                .map_err(|_| category("Self equation owner ordinal overflow", span))?,
        );
        let declaration = compiler::source_scope::original_member(
            &parsed.module,
            parent,
            source.member,
            self.max_depth,
            |work| self.environment.budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| category("Self equation original member ordinal absent", span))?;
        Ok(CtSymbolId {
            module,
            declaration,
            generation_path: None,
        })
    }
    fn equation_copy(&mut self, value: &s::Expr) -> CompileResult<s::Expr> {
        let cost = zil_expand::ct_selection::expression_cost(
            value,
            &self.environment.remaining_limits_for_input(),
        )?;
        self.environment
            .budget
            .reserve_analysis_work(cost.work as u64)?;
        self.environment
            .budget
            .reserve_generated_nodes(cost.nodes as u64)?;
        self.environment.budget.reserve_temporary_bytes(
            (cost.work + cost.nodes * std::mem::size_of::<s::Expr>()) as u64,
        )?;
        Ok(value.clone())
    }
    fn equation<'a>(
        &mut self,
        plan: &'a Plan,
        source: Source,
    ) -> CompileResult<Option<&'a Equation>> {
        for equation in &plan.definitions {
            self.environment.budget.reserve_analysis_work(1)?;
            if equation.source == source {
                return Ok(Some(equation));
            }
        }
        Ok(None)
    }
    fn scalar_expression(&mut self, value: zil_ir::Constant, span: Span) -> CompileResult<s::Expr> {
        self.environment.budget.reserve_analysis_work(26)?;
        self.environment.budget.reserve_temporary_bytes(26)?;
        self.environment.budget.reserve_generated_nodes(1)?;
        let kind = match value {
            zil_ir::Constant::Bool(v) => s::ExprKind::Literal(s::Literal::Bool(v)),
            zil_ir::Constant::F32(_) | zil_ir::Constant::F64(_) => {
                return float_expression(&value, &mut self.environment.budget, span);
            }
            zil_ir::Constant::U8(v) => s::ExprKind::Literal(s::Literal::Integer(format!("{v}u8"))),
            zil_ir::Constant::U16(v) => {
                s::ExprKind::Literal(s::Literal::Integer(format!("{v}u16")))
            }
            zil_ir::Constant::I32(v) => {
                s::ExprKind::Literal(s::Literal::Integer(format!("{v}i32")))
            }
            zil_ir::Constant::I64(v) => {
                s::ExprKind::Literal(s::Literal::Integer(format!("{v}i64")))
            }
            zil_ir::Constant::U64(v) => {
                s::ExprKind::Literal(s::Literal::Integer(format!("{v}u64")))
            }
            zil_ir::Constant::Usize(v) => {
                s::ExprKind::Literal(s::Literal::Integer(format!("{v}usize")))
            }
            _ => {
                return Err(runtime(
                    "Self equation did not produce an ordinary scalar",
                    span,
                ));
            }
        };
        Ok(s::Expr { span, kind })
    }
    fn symbolic_equation(&mut self, module: &s::Module, equation: &Equation) -> CompileResult<()> {
        let c = constant(module, equation.source)?;
        let value = c
            .value
            .as_ref()
            .ok_or_else(|| category("Self equation value absent", module.span))?;
        let mut expression = self.equation_copy(value)?;
        let mut replacements = Vec::new();
        let mut parameters = Vec::new();
        for ordinal in &equation.dependencies {
            self.environment.budget.reserve_analysis_work(32)?;
            self.environment.budget.reserve_temporary_bytes(128)?;
            let dependency = constant(
                module,
                Source {
                    item: equation.trait_item,
                    member: *ordinal,
                },
            )?;
            let ty = dependency
                .ty
                .as_ref()
                .ok_or_else(|| category("Self dependency type absent", value.span))?;
            let expected =
                associated_constants::scalar(ty, &mut self.environment.budget, self.max_depth)?
                    .ok_or_else(|| {
                        category("Self dependency is not an existing scalar", value.span)
                    })?;
            let name = format!("$self_constant_{ordinal}");
            self.environment
                .budget
                .reserve_analysis_work(dependency.name.len() as u64)?;
            self.environment
                .budget
                .reserve_temporary_bytes(dependency.name.len() as u64)?;
            replacements.push((
                dependency.name.clone(),
                s::Expr {
                    span: value.span,
                    kind: s::ExprKind::Name(name.clone()),
                },
            ));
            parameters.push((name, expected));
        }
        Rewrite {
            replacements: &replacements,
            budget: &mut self.environment.budget,
            max_depth: self.max_depth,
        }
        .expression(&mut expression, 0)?;
        let expected = associated_constants::scalar(
            &equation.receiving,
            &mut self.environment.budget,
            self.max_depth,
        )?
        .ok_or_else(|| category("Self receiving scalar absent", value.span))?;
        let owner = self.self_owner(module.id, equation.source, value.span)?;
        let input = self.environment.empty_input(value.span)?;
        self.environment.verify_expression(
            Entry::Expression {
                module: module.id,
                expression: &expression,
                definition: owner.clone(),
                lexical_owner: Some(owner),
                expected: Some(expected),
                parameters: &parameters,
            },
            &input,
        )?;
        self.environment
            .budget
            .reserve_analysis_work(self.self_constant_sources.len() as u64 + 1)?;
        self.environment
            .budget
            .reserve_temporary_bytes(std::mem::size_of::<(ModuleId, usize, usize)>() as u64)?;
        self.self_constant_sources.insert((
            module.id,
            equation.source.item,
            equation.source.member,
        ));
        Ok(())
    }
    fn actual_equation(
        &mut self,
        module: &s::Module,
        source: Source,
        receiving: &s::TypeRef,
        replacements: &[(String, s::Expr)],
    ) -> CompileResult<s::Expr> {
        let value = constant(module, source)?
            .value
            .as_ref()
            .ok_or_else(|| category("effective equation has no value", module.span))?;
        let mut expression = self.equation_copy(value)?;
        Rewrite {
            replacements,
            budget: &mut self.environment.budget,
            max_depth: self.max_depth,
        }
        .expression(&mut expression, 0)?;
        let expected =
            associated_constants::scalar(receiving, &mut self.environment.budget, self.max_depth)?
                .ok_or_else(|| category("effective equation scalar absent", value.span))?;
        let owner = self.self_owner(module.id, source, value.span)?;
        let result = self.expression_value(module.id, &owner, &expression, Some(expected))?;
        let CtValue::Scalar(result) = result else {
            return Err(category(
                "effective equation result category differs",
                value.span,
            ));
        };
        self.scalar_expression(result, value.span)
    }
    pub(super) fn self_constants(&mut self, module: &mut s::Module) -> CompileResult<()> {
        // This prepass runs before any item deletion/generation. Its immutable
        // input and all Source ordinals remain fixed until every job completes.
        let mut found = false;
        for item in &module.items {
            self.environment.budget.reserve_analysis_work(1)?;
            let (members, parameters) = match &item.kind {
                s::ItemKind::Trait(t) => (&t.members, &t.generics),
                s::ItemKind::Impl(i) => (&i.members, &i.generics),
                _ => continue,
            };
            for member in members {
                self.environment.budget.reserve_analysis_work(1)?;
                if let s::ItemKind::Const(c) = &member.kind
                    && let Some(value) = &c.value
                {
                    let mut deps = associated_constants::Dependencies {
                        parameters,
                        budget: &mut self.environment.budget,
                        symbolic: false,
                        self_dependent: false,
                    };
                    deps.expression(value, 0)?;
                    found |= deps.self_dependent;
                }
            }
        }
        if !found {
            return Ok(());
        }
        self.environment
            .budget
            .reserve_analysis_work(self.parsed.len() as u64)?;
        let original = self
            .parsed
            .iter()
            .find(|p| p.module.id == module.id)
            .ok_or_else(|| category("original equation module absent", module.span))?;
        let cost = zil_expand::ct_selection::module_cost(
            module,
            &self.environment.remaining_limits_for_input(),
        )?;
        self.environment
            .budget
            .reserve_analysis_work(cost.work.saturating_mul(2) as u64)?;
        if original.module != *module {
            return Err(runtime(
                "Self-dependent constants require aligned original module ordinals",
                module.span,
            ));
        }
        for (ordinal, item) in module.items.iter_mut().enumerate() {
            self.environment.budget.reserve_analysis_work(1)?;
            let owner = CtSymbolId {
                module: module.id,
                declaration: zil_base::NodeId(
                    u32::try_from(ordinal)
                        .map_err(|_| category("original owner ordinal overflow", item.span))?,
                ),
                generation_path: None,
            };
            match &mut item.kind {
                s::ItemKind::Trait(t) => {
                    self.associated_constants(module.id, &mut t.members, &t.generics, &owner, true)?
                }
                s::ItemKind::Impl(i) => {
                    self.associated_constants(module.id, &mut i.members, &i.generics, &owner, true)?
                }
                _ => {}
            }
        }
        let before = self.environment.remaining_limits_for_input();
        let plan = zil_expand::associated_constants::prepare(
            module,
            self.environment.target_width(),
            &before,
        )?;
        self.environment.budget.reserve_analysis_work(
            before
                .max_analysis_steps
                .saturating_sub(plan.remaining_limits.max_analysis_steps) as u64,
        )?;
        self.environment.budget.reserve_generated_nodes(
            before
                .max_nodes
                .saturating_sub(plan.remaining_limits.max_nodes) as u64,
        )?;
        for equation in &plan.definitions {
            self.symbolic_equation(module, equation)?;
        }
        let mut updates = Vec::new();
        for instance in &plan.instances {
            let declarations = members(module, instance.trait_item)?;
            let mut ordinals = Vec::new();
            for (ordinal, item) in declarations.iter().enumerate() {
                self.environment.budget.reserve_analysis_work(1)?;
                if matches!(item.kind, s::ItemKind::Const(_)) {
                    self.environment
                        .budget
                        .reserve_temporary_bytes(std::mem::size_of::<usize>() as u64)?;
                    ordinals.push(ordinal);
                }
            }
            self.environment
                .budget
                .reserve_analysis_work(instance.effective.len() as u64 * 3)?;
            self.environment.budget.reserve_temporary_bytes(
                (instance.effective.len() * (std::mem::size_of::<Option<s::Expr>>() + 1)) as u64,
            )?;
            let mut values = vec![None; instance.effective.len()];
            let mut states = vec![0u8; instance.effective.len()];
            for root in 0..instance.effective.len() {
                self.environment.budget.reserve_analysis_work(1)?;
                if states[root] == 2 {
                    continue;
                }
                let mut stack = vec![(root, 0usize)];
                self.environment
                    .budget
                    .reserve_temporary_bytes(std::mem::size_of::<(usize, usize)>() as u64)?;
                while let Some(&(current, next)) = stack.last() {
                    self.environment.budget.reserve_analysis_work(1)?;
                    states[current] = 1;
                    let source = instance.effective[current];
                    let dependencies = self
                        .equation(&plan, source)?
                        .map(|e| e.dependencies.as_slice())
                        .unwrap_or_default();
                    if let Some(dependency) = dependencies.get(next) {
                        self.environment
                            .budget
                            .reserve_analysis_work(ordinals.len() as u64)?;
                        let position =
                            ordinals
                                .iter()
                                .position(|o| o == dependency)
                                .ok_or_else(|| {
                                    category("effective dependency ordinal absent", module.span)
                                })?;
                        stack.last_mut().expect("nonempty").1 += 1;
                        if states[position] == 1 {
                            return Err(vec![Diagnostic::error(
                                Phase::Ct,
                                "E-CT-ASSOCIATED-CYCLE",
                                Some(module.span),
                                "active effective associated constant equation cycle",
                            )]);
                        }
                        if states[position] == 0 {
                            if stack.len() >= self.max_depth {
                                return Err(vec![Diagnostic::limit(
                                    Phase::Ct,
                                    "associated constant dependency depth exhausted",
                                    Some(module.span),
                                )]);
                            }
                            self.environment.budget.reserve_temporary_bytes(
                                std::mem::size_of::<(usize, usize)>() as u64,
                            )?;
                            stack.push((position, 0));
                        }
                        continue;
                    }
                    let replacements = self.effective_replacements(
                        module,
                        instance.trait_item,
                        &ordinals,
                        &values,
                    )?;
                    let receiving = constant(
                        module,
                        Source {
                            item: instance.trait_item,
                            member: ordinals[current],
                        },
                    )?
                    .ty
                    .as_ref()
                    .ok_or_else(|| category("effective receiving type absent", module.span))?;
                    values[current] =
                        Some(self.actual_equation(module, source, receiving, &replacements)?);
                    states[current] = 2;
                    stack.pop();
                }
            }
            let replacements =
                self.effective_replacements(module, instance.trait_item, &ordinals, &values)?;
            for source in &instance.default_checks {
                self.environment.budget.reserve_analysis_work(1)?;
                let receiving = constant(module, *source)?
                    .ty
                    .as_ref()
                    .ok_or_else(|| category("default check receiving type absent", module.span))?;
                self.actual_equation(module, *source, receiving, &replacements)?;
            }
            for (index, source) in instance.effective.iter().enumerate() {
                self.environment.budget.reserve_analysis_work(1)?;
                self.environment
                    .budget
                    .reserve_temporary_bytes(std::mem::size_of::<(
                        usize,
                        usize,
                        Option<usize>,
                        s::Expr,
                    )>() as u64)?;
                updates.push((
                    instance.item,
                    ordinals[index],
                    (source.item == instance.item).then_some(source.member),
                    values[index].take().expect("completed effective scalar"),
                ));
            }
        }
        // Publishing happens only after all instances and DefaultCheck jobs.
        for (implementation, declaration, override_member, value) in updates {
            self.environment.budget.reserve_analysis_work(1)?;
            if let Some(ordinal) = override_member {
                let s::ItemKind::Impl(i) = &mut module.items[implementation].kind else {
                    unreachable!("plan impl")
                };
                let s::ItemKind::Const(c) = &mut i.members[ordinal].kind else {
                    unreachable!("plan constant")
                };
                c.value = Some(value);
            } else {
                // Find the exact already committed instance, not a same name.
                self.environment
                    .budget
                    .reserve_analysis_work(plan.instances.len() as u64)?;
                let instance = plan
                    .instances
                    .iter()
                    .find(|i| i.item == implementation)
                    .expect("planned instance");
                let original = &members(module, instance.trait_item)?[declaration];
                let cost = zil_expand::ct_selection::item_cost(
                    original,
                    &self.environment.remaining_limits_for_input(),
                )?;
                self.environment
                    .budget
                    .reserve_analysis_work(cost.work as u64)?;
                self.environment
                    .budget
                    .reserve_generated_nodes(cost.nodes as u64)?;
                self.environment.budget.reserve_temporary_bytes(
                    (cost.work + cost.nodes * std::mem::size_of::<s::Item>()) as u64,
                )?;
                let mut emitted = original.clone();
                let s::ItemKind::Const(c) = &mut emitted.kind else {
                    unreachable!("trait constant")
                };
                c.value = Some(value);
                let s::ItemKind::Impl(i) = &mut module.items[implementation].kind else {
                    unreachable!("plan impl")
                };
                i.members.push(emitted);
            }
        }
        Ok(())
    }
    fn effective_replacements(
        &mut self,
        module: &s::Module,
        trait_item: usize,
        ordinals: &[usize],
        values: &[Option<s::Expr>],
    ) -> CompileResult<Vec<(String, s::Expr)>> {
        let mut replacements = Vec::new();
        for (ordinal, value) in ordinals.iter().zip(values) {
            self.environment.budget.reserve_analysis_work(1)?;
            if let Some(value) = value {
                let c = constant(
                    module,
                    Source {
                        item: trait_item,
                        member: *ordinal,
                    },
                )?;
                self.environment
                    .budget
                    .reserve_analysis_work(c.name.len() as u64)?;
                self.environment.budget.reserve_temporary_bytes(
                    (c.name.len() + std::mem::size_of::<(String, s::Expr)>()) as u64,
                )?;
                replacements.push((c.name.clone(), self.equation_copy(value)?));
            }
        }
        Ok(replacements)
    }
}
