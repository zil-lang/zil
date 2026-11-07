//! Per-witness effective equations and separate overridden-default checks.
//! Only the ordinary CT compiler/verifier/VM produces scalar values.
use super::*;
use zil_expand::associated_constants::{Equation, Plan, Source};
use zil_expand::ct_selection::Visitor;

fn unsupported(message: &str, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Ct, message, Some(span))]
}

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
    selected: Limits,
}
impl Rewrite<'_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.reserve_analysis_work(1)?;
        if depth > self.selected.max_call_depth {
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
            _ => Err(unsupported(
                "Self-dependent equation syntax needs a later scalar CT substitution consumer",
                value.span,
            )),
        }
    }
    fn limits(&self) -> Limits {
        let mut limits = self.selected.clone();
        limits.max_call_depth = limits
            .max_call_depth
            .min(self.budget.remaining().max_call_depth as usize);
        limits.max_nodes = limits
            .max_nodes
            .min(self.budget.remaining().max_generated_nodes as usize);
        limits.max_analysis_steps = limits
            .max_analysis_steps
            .min(self.budget.remaining().max_analysis_work as usize);
        limits
    }
    fn condition(&mut self, c: &mut s::Condition, depth: usize) -> CompileResult<()> {
        match c {
            s::Condition::Expression(e) => self.expression(e, depth),
            s::Condition::Let { .. } => Err(unsupported(
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
                    return Err(unsupported(
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
            _ => Err(unsupported(
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
    pub(super) fn temporary_mark(&self) -> (u64, usize) {
        (
            self.environment.budget.live_temporary_bytes(),
            self.environment.budget.usage().queries.capacity(),
        )
    }
    pub(super) fn release_equation_temporary(&mut self, mark: (u64, usize), keep: u64) {
        let query_bytes = self
            .environment
            .budget
            .usage()
            .queries
            .capacity()
            .saturating_sub(mark.1) as u64
            * std::mem::size_of::<CtQueryDependency>() as u64;
        let added = self
            .environment
            .budget
            .live_temporary_bytes()
            .saturating_sub(mark.0);
        self.environment
            .budget
            .release_temporary_bytes(added.saturating_sub(keep).saturating_sub(query_bytes));
    }
    fn equation_input(
        &mut self,
        module: ModuleId,
        fields: &[u64],
        span: Span,
    ) -> CompileResult<CtInputTable> {
        let bytes = fields
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(96))
            .ok_or_else(|| category("Self job identity size overflow", span))?;
        self.environment
            .budget
            .reserve_analysis_work(bytes as u64)?;
        self.environment
            .budget
            .reserve_temporary_bytes(bytes as u64)?;
        let mut identity = Vec::with_capacity(bytes);
        identity.extend_from_slice(b"zil/self-associated-equation/v1\0");
        identity.extend_from_slice(&self.environment.request_fingerprint().0);
        identity.extend_from_slice(&module.0.to_le_bytes());
        identity.extend_from_slice(&self.environment.target_width().to_le_bytes());
        for field in fields {
            identity.extend_from_slice(&field.to_le_bytes());
        }
        let invocation_fingerprint = Fingerprint::of(&identity);
        self.environment
            .budget
            .release_temporary_bytes(bytes as u64);
        Ok(CtInputTable {
            invocation_fingerprint,
            tokens: Vec::new(),
        })
    }
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
        if matches!(value, zil_ir::Constant::F32(_) | zil_ir::Constant::F64(_)) {
            return float_expression(&value, &mut self.environment.budget, span);
        }
        self.environment.budget.reserve_analysis_work(26)?;
        self.environment.budget.reserve_temporary_bytes(26)?;
        self.environment.budget.reserve_generated_nodes(1)?;
        let kind = match value {
            zil_ir::Constant::Bool(v) => s::ExprKind::Literal(s::Literal::Bool(v)),
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
        let mark = self.temporary_mark();
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
            self.environment.budget.reserve_temporary_bytes(
                (std::mem::size_of::<(String, s::Expr)>()
                    + std::mem::size_of::<(String, CtTy)>()
                    + 128) as u64,
            )?;
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
        let selected = self.environment.remaining_limits_for_input();
        Rewrite {
            replacements: &replacements,
            budget: &mut self.environment.budget,
            selected,
        }
        .expression(&mut expression, 0)?;
        let expected = associated_constants::scalar(
            &equation.receiving,
            &mut self.environment.budget,
            self.max_depth,
        )?
        .ok_or_else(|| category("Self receiving scalar absent", value.span))?;
        let owner = self.self_owner(module.id, equation.source, value.span)?;
        let input = self.equation_input(
            module.id,
            &[
                0,
                equation.trait_item as u64,
                equation.source.item as u64,
                equation.source.member as u64,
            ],
            value.span,
        )?;
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
        self.environment.budget.reserve_temporary_bytes(128)?;
        self.self_constant_sources
            .push((module.id, equation.source.item, equation.source.member));
        self.release_equation_temporary(mark, 128);
        Ok(())
    }
    fn actual_equation(
        &mut self,
        module: &s::Module,
        source: Source,
        receiving: &s::TypeRef,
        replacements: &[(String, s::Expr)],
        identity: &[u64],
    ) -> CompileResult<s::Expr> {
        let mark = self.temporary_mark();
        let value = constant(module, source)?
            .value
            .as_ref()
            .ok_or_else(|| category("effective equation has no value", module.span))?;
        let mut expression = self.equation_copy(value)?;
        let selected = self.environment.remaining_limits_for_input();
        Rewrite {
            replacements,
            budget: &mut self.environment.budget,
            selected,
        }
        .expression(&mut expression, 0)?;
        let expected =
            associated_constants::scalar(receiving, &mut self.environment.budget, self.max_depth)?
                .ok_or_else(|| category("effective equation scalar absent", value.span))?;
        let owner = self.self_owner(module.id, source, value.span)?;
        let input = self.equation_input(module.id, identity, value.span)?;
        self.environment
            .budget
            .reserve_analysis_work(self.sources.len() as u64 * 2)?;
        self.environment.budget.reserve_temporary_bytes(
            self.sources.len() as u64
                * (std::mem::size_of::<&s::Module>()
                    + std::mem::size_of::<compiler::SourceModule<'_>>()) as u64,
        )?;
        let result = self.environment.evaluate(
            Entry::Expression {
                module: module.id,
                expression: &expression,
                definition: owner.clone(),
                lexical_owner: Some(owner),
                expected: Some(expected),
                parameters: &[],
            },
            &input,
            None,
            &[],
            value.span,
        )?;
        let CtValue::Scalar(result) = result else {
            return Err(category(
                "effective equation result category differs",
                value.span,
            ));
        };
        let result = self.scalar_expression(result, value.span)?;
        self.release_equation_temporary(mark, 26);
        Ok(result)
    }
    fn reject_external_self_defaults(&mut self, module: &s::Module) -> CompileResult<()> {
        for item in &module.items {
            self.environment.budget.reserve_analysis_work(1)?;
            let s::ItemKind::Impl(implementation) = &item.kind else {
                continue;
            };
            let Some(mut application) = implementation.trait_type.as_ref() else {
                continue;
            };
            while let s::TypeKind::Group(inner) = &application.kind {
                self.environment.budget.reserve_analysis_work(1)?;
                application = inner;
            }
            let s::TypeKind::Named { path, .. } = &application.kind else {
                continue;
            };
            let [alias, trait_name] = path.as_slice() else {
                continue;
            };
            let mut target = None;
            for import_item in &module.items {
                self.environment.budget.reserve_analysis_work(1)?;
                let s::ItemKind::Import(import) = &import_item.kind else {
                    continue;
                };
                let Some(name) = import.alias.as_ref().or_else(|| import.path.last()) else {
                    continue;
                };
                self.environment
                    .budget
                    .reserve_analysis_work(alias.len().min(name.len()) as u64 + 1)?;
                if name != alias || import.syntax {
                    continue;
                }
                self.environment.budget.reserve_analysis_work(
                    (self.paths.len() + self.environment.scoped.map_or(0, |paths| paths.len()))
                        as u64
                        + import.path.iter().map(|p| p.len() as u64 + 1).sum::<u64>()
                        + 1,
                )?;
                target = self.environment.resolve_path(module.id, &import.path);
                break;
            }
            let Some(target) = target else { continue };
            self.environment
                .budget
                .reserve_analysis_work(self.parsed.len() as u64)?;
            let original = self
                .parsed
                .iter()
                .find(|p| p.module.id == target)
                .ok_or_else(|| category("external constant trait module is absent", item.span))?;
            for candidate in &original.module.items {
                self.environment.budget.reserve_analysis_work(1)?;
                let s::ItemKind::Trait(declaration) = &candidate.kind else {
                    continue;
                };
                self.environment.budget.reserve_analysis_work(
                    declaration.name.len().min(trait_name.len()) as u64 + 1,
                )?;
                if declaration.name != *trait_name {
                    continue;
                }
                for member in &declaration.members {
                    self.environment.budget.reserve_analysis_work(1)?;
                    let s::ItemKind::Const(constant) = &member.kind else {
                        continue;
                    };
                    let Some(value) = &constant.value else {
                        continue;
                    };
                    let mut dependencies = associated_constants::Dependencies {
                        parameters: &declaration.generics,
                        budget: &mut self.environment.budget,
                        symbolic: false,
                        self_dependent: false,
                    };
                    dependencies.expression(value, 0)?;
                    if dependencies.self_dependent {
                        return Err(unsupported(
                            "Self-dependent default with an impl in another module needs a complete cross-module constant witness bridge",
                            item.span,
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    fn plan_bytes(&mut self, plan: &Plan) -> CompileResult<u64> {
        self.environment
            .budget
            .reserve_analysis_work((plan.definitions.len() + plan.instances.len()) as u64)?;
        let mut bytes = plan
            .definitions
            .capacity()
            .saturating_mul(std::mem::size_of::<Equation>())
            .saturating_add(
                plan.instances
                    .capacity()
                    .saturating_mul(std::mem::size_of::<
                        zil_expand::associated_constants::Instance,
                    >()),
            );
        for equation in &plan.definitions {
            bytes = bytes.saturating_add(
                equation
                    .dependencies
                    .capacity()
                    .saturating_mul(std::mem::size_of::<usize>()),
            );
            let mut ty = &equation.receiving;
            loop {
                self.environment.budget.reserve_analysis_work(1)?;
                match &ty.kind {
                    s::TypeKind::Group(inner) => {
                        bytes = bytes.saturating_add(std::mem::size_of::<s::TypeRef>());
                        ty = inner;
                    }
                    s::TypeKind::Named { path, args } => {
                        bytes = bytes
                            .saturating_add(
                                path.capacity()
                                    .saturating_mul(std::mem::size_of::<String>()),
                            )
                            .saturating_add(
                                args.capacity()
                                    .saturating_mul(std::mem::size_of::<s::GenericArg>()),
                            );
                        for name in path {
                            self.environment.budget.reserve_analysis_work(1)?;
                            bytes = bytes.saturating_add(name.capacity());
                        }
                        break;
                    }
                    _ => {
                        return Err(category(
                            "plan receiving type is not its verified scalar shape",
                            ty.span,
                        ));
                    }
                }
            }
        }
        for instance in &plan.instances {
            bytes = bytes.saturating_add(
                instance
                    .effective
                    .capacity()
                    .saturating_add(instance.default_checks.capacity())
                    .saturating_mul(std::mem::size_of::<Source>()),
            );
        }
        Ok(bytes as u64)
    }
    pub(super) fn self_constants(&mut self, module: &mut s::Module) -> CompileResult<()> {
        // This prepass runs before any item deletion/generation. Its immutable
        // input and all Source ordinals remain fixed until every job completes.
        self.reject_external_self_defaults(module)?;
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
            return Err(unsupported(
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
        let temporary = self.temporary_mark();
        let sources_before = self.self_constant_sources.len();
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
        let plan_charge = before
            .max_source_bytes
            .saturating_sub(plan.remaining_limits.max_source_bytes)
            as u64;
        self.environment
            .budget
            .reserve_temporary_bytes(plan_charge)?;
        let live_plan = self.plan_bytes(&plan)?;
        if live_plan > plan_charge {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "associated plan allocation exceeds its prepaid bound",
                Some(module.span),
            )]);
        }
        self.environment
            .budget
            .release_temporary_bytes(plan_charge - live_plan);

        for equation in &plan.definitions {
            self.symbolic_equation(module, equation)?;
        }
        let mut updates = Vec::new();
        let mut output_bytes = 0u64;
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
                    values[current] = Some(self.actual_equation(
                        module,
                        source,
                        receiving,
                        &replacements,
                        &[
                            1,
                            instance.item as u64,
                            instance.trait_item as u64,
                            ordinals[current] as u64,
                            source.item as u64,
                            source.member as u64,
                        ],
                    )?);
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
                self.actual_equation(
                    module,
                    *source,
                    receiving,
                    &replacements,
                    &[
                        2,
                        instance.item as u64,
                        instance.trait_item as u64,
                        source.member as u64,
                        source.item as u64,
                        source.member as u64,
                    ],
                )?;
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
                if let Some(previous) = &c.value {
                    self.clear_expression_source_sites(previous)?;
                }
                c.value = Some(value);
                output_bytes = output_bytes.saturating_add(26);
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
                output_bytes = output_bytes.saturating_add(
                    (cost.work + cost.nodes * std::mem::size_of::<s::Item>()) as u64,
                );
                let mut emitted = original.clone();
                let s::ItemKind::Const(c) = &mut emitted.kind else {
                    unreachable!("trait constant")
                };
                c.value = Some(value);
                let s::ItemKind::Impl(i) = &mut module.items[implementation].kind else {
                    unreachable!("plan impl")
                };
                // A Vec reallocation moves inline nodes. Preserve only the
                // already authenticated mapping; the synthesized default has
                // no original expression authority.
                self.environment.budget.reserve_temporary_bytes(
                    i.members.len() as u64
                        * std::mem::size_of::<Vec<Option<compiler::source_sites::SourceSite>>>()
                            as u64,
                )?;
                let mut sites = Vec::with_capacity(i.members.len());
                for member in &i.members {
                    sites.push(self.take_item_sites(member)?);
                }
                let bytes = sites.capacity() as u64
                    * std::mem::size_of::<Vec<Option<compiler::source_sites::SourceSite>>>() as u64;
                i.members.push(emitted);
                for (member, sites) in i.members.iter().zip(sites) {
                    self.restore_item_sites(member, sites)?;
                }
                self.environment.budget.release_temporary_bytes(bytes);
            }
        }
        for instance in &plan.instances {
            self.environment
                .budget
                .reserve_analysis_work(self.self_constant_impl_sources.len() as u64 + 1)?;
            self.environment.budget.reserve_temporary_bytes(128)?;
            self.self_constant_impl_sources
                .push((module.id, instance.item));
        }
        self.release_equation_temporary(
            temporary,
            output_bytes
                .saturating_add(plan.instances.len() as u64 * 128)
                .saturating_add(
                    self.self_constant_sources
                        .len()
                        .saturating_sub(sources_before) as u64
                        * 128,
                ),
        );
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
