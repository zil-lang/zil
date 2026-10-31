//! Alpha-renaming of already selected generated syntax. No runtime scope or
//! operation is inserted, and duplicate bindings keep their ordinary meaning.
use super::*;
use std::collections::BTreeMap;
#[path = "hygiene_types.rs"]
mod types;

struct Renamer<'a> {
    budget: &'a mut Budget,
    prefix: String,
    next: usize,
    scopes: Vec<BTreeMap<String, String>>,
    types: Option<types::TypeNames>,
}

impl<'a> Renamer<'a> {
    fn new(
        module: ModuleId,
        serial: u64,
        budget: &'a mut Budget,
        span: Span,
    ) -> CompileResult<Self> {
        budget.spend(1, 64, span)?;
        Ok(Self {
            budget,
            prefix: format!("$ctfor222${}${serial}$", module.0),
            next: 0,
            scopes: vec![BTreeMap::new()],
            types: None,
        })
    }
    fn bind(&mut self, name: &mut String, rename: bool, span: Span) -> CompileResult<()> {
        let scope = self.scopes.last_mut().expect("hygiene scope");
        self.budget.spend(
            1,
            name.len().saturating_mul(scope.len().saturating_add(2)),
            span,
        )?;
        if let Some(existing) = scope.get(name) {
            self.budget.text(existing, span)?;
            *name = existing.clone();
            return Ok(());
        }
        let bytes = name
            .len()
            .saturating_add(self.prefix.len())
            .saturating_add(24);
        self.budget
            .spend(1, bytes.saturating_mul(2).saturating_add(name.len()), span)?;
        let fresh = if rename {
            let ordinal = self.next;
            self.next = ordinal.checked_add(1).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "generated name budget exceeded",
                    Some(span),
                )]
            })?;
            format!("{}{ordinal}${name}", self.prefix)
        } else {
            name.clone()
        };
        scope.insert(name.clone(), fresh.clone());
        *name = fresh;
        Ok(())
    }
    fn reference(&mut self, name: &mut String, span: Span) -> CompileResult<()> {
        for scope in self.scopes.iter().rev() {
            self.budget.spend(
                0,
                name.len().saturating_mul(scope.len().saturating_add(1)),
                span,
            )?;
            if let Some(replacement) = scope.get(name) {
                self.budget.text(replacement, span)?;
                *name = replacement.clone();
                break;
            }
        }
        Ok(())
    }
    fn scope(&mut self, span: Span) -> CompileResult<()> {
        self.budget.node(span, self.scopes.len())?;
        self.scopes.push(BTreeMap::new());
        Ok(())
    }
    fn ty(&mut self, ty: &mut s::TypeRef, depth: usize) -> CompileResult<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            s::TypeKind::Named { path, args } | s::TypeKind::Dyn { path, args } => {
                if path.len() == 1 {
                    self.type_reference(&mut path[0], ty.span)?;
                }
                for argument in args {
                    self.argument(argument, depth + 1)?;
                }
            }
            s::TypeKind::Reference { inner, .. }
            | s::TypeKind::Group(inner)
            | s::TypeKind::Optional(inner)
            | s::TypeKind::ForLifetime { inner, .. } => self.ty(inner, depth + 1)?,
            s::TypeKind::Tuple(types) | s::TypeKind::Union(types) | s::TypeKind::Impl(types) => {
                for ty in types {
                    self.ty(ty, depth + 1)?;
                }
            }
            s::TypeKind::Array { element, length } => {
                self.ty(element, depth + 1)?;
                if let Some(length) = length {
                    self.expression(length, depth + 1)?;
                }
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                for parameter in params {
                    self.ty(parameter, depth + 1)?;
                }
                self.ty(result, depth + 1)?;
            }
            s::TypeKind::Error { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            s::TypeKind::MetaEval(value) => self.expression(value, depth + 1)?,
            s::TypeKind::MetaBlock(_) => return Err(self.unsupported(ty.span)),
        }
        Ok(())
    }
    fn argument(&mut self, argument: &mut s::GenericArg, depth: usize) -> CompileResult<()> {
        match argument {
            s::GenericArg::Type(ty) => self.ty(ty, depth),
            s::GenericArg::Const(value) => self.expression(value, depth),
        }
    }
    fn unsupported(&self, span: Span) -> Vec<Diagnostic> {
        vec![Diagnostic::unsupported(
            Phase::Expand,
            "#for generates ordinary functions, non-generic method-free structs, plain type aliases and non-generic Unit/Tuple enums; other Type/Syntax declarations are unsupported",
            Some(span),
        )]
    }
    fn pattern(
        &mut self,
        pattern: &mut s::Pattern,
        rename: bool,
        depth: usize,
    ) -> CompileResult<()> {
        self.budget.node(pattern.span, depth)?;
        match &mut pattern.kind {
            s::PatternKind::Name(name) => self.bind(name, rename, pattern.span)?,
            s::PatternKind::Reference { pattern, .. } => {
                self.pattern(pattern, rename, depth + 1)?
            }
            s::PatternKind::Tuple(patterns) => {
                for pattern in patterns {
                    self.pattern(pattern, rename, depth + 1)?;
                }
            }
            s::PatternKind::Constructor { path, arguments } => {
                if path.len() == 2 {
                    self.type_reference(&mut path[0], pattern.span)?;
                }
                for pattern in arguments {
                    self.pattern(pattern, rename, depth + 1)?;
                }
            }
            s::PatternKind::Wildcard | s::PatternKind::Literal(_) => {}
        }
        Ok(())
    }
    fn block(&mut self, block: &mut s::Block, depth: usize) -> CompileResult<()> {
        self.budget.node(block.span, depth)?;
        self.scope(block.span)?;
        let result = self.statements(&mut block.statements, false, depth + 1);
        self.scopes.pop();
        result
    }
    fn statements(
        &mut self,
        statements: &mut [s::Stmt],
        rename: bool,
        depth: usize,
    ) -> CompileResult<()> {
        for statement in statements {
            self.statement(statement, rename, depth)?;
        }
        Ok(())
    }
    fn branch(
        &mut self,
        condition: &mut s::Condition,
        body: &mut s::Block,
        depth: usize,
    ) -> CompileResult<()> {
        match condition {
            s::Condition::Expression(value) => {
                self.expression(value, depth)?;
                self.block(body, depth)
            }
            s::Condition::Let { pattern, value } => {
                self.expression(value, depth)?;
                self.scope(pattern.span)?;
                self.pattern(pattern, false, depth + 1)?;
                let result = self.block(body, depth + 1);
                self.scopes.pop();
                result
            }
        }
    }
    fn statement(
        &mut self,
        statement: &mut s::Stmt,
        rename: bool,
        depth: usize,
    ) -> CompileResult<()> {
        self.budget.node(statement.span, depth)?;
        match &mut statement.kind {
            s::StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(ty) = ty {
                    self.ty(ty, depth + 1)?;
                }
                self.expression(value, depth + 1)?;
                self.pattern(pattern, rename, depth + 1)?;
            }
            s::StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value, depth + 1)?;
                }
            }
            s::StmtKind::Yield(value) | s::StmtKind::Expression(value) => {
                self.expression(value, depth + 1)?
            }
            s::StmtKind::Break | s::StmtKind::Continue => {}
            s::StmtKind::Defer(body) | s::StmtKind::Loop(body) | s::StmtKind::Unsafe(body) => {
                self.block(body, depth + 1)?
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.branch(condition, then_body, depth + 1)?;
                match else_body {
                    Some(s::ElseBranch::Block(body)) => self.block(body, depth + 1)?,
                    Some(s::ElseBranch::If(statement)) => {
                        self.statement(statement, false, depth + 1)?
                    }
                    None => {}
                }
            }
            s::StmtKind::While { condition, body } => self.branch(condition, body, depth + 1)?,
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.expression(iterable, depth + 1)?;
                self.scope(statement.span)?;
                self.pattern(pattern, false, depth + 1)?;
                self.block(body, depth + 1)?;
                self.scopes.pop();
            }
            s::StmtKind::With { name, value, body } => {
                self.expression(value, depth + 1)?;
                self.scope(statement.span)?;
                self.bind(name, false, statement.span)?;
                self.block(body, depth + 1)?;
                self.scopes.pop();
            }
            s::StmtKind::Match { value, arms } => {
                self.expression(value, depth + 1)?;
                for arm in arms {
                    self.scope(arm.span)?;
                    self.pattern(&mut arm.pattern, false, depth + 1)?;
                    self.arrow(&mut arm.body, depth + 1)?;
                    self.scopes.pop();
                }
            }
            s::StmtKind::Assign { target, value, .. } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
            }
            s::StmtKind::Meta(_) => return Err(self.unsupported(statement.span)),
        }
        Ok(())
    }
    fn arrow(&mut self, body: &mut s::ArrowBody, depth: usize) -> CompileResult<()> {
        match body {
            s::ArrowBody::Expression(value) => self.expression(value, depth),
            s::ArrowBody::Block(block) => self.block(block, depth),
        }
    }
    fn record_type(&mut self, ty: &mut s::Expr, depth: usize) -> CompileResult<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            s::ExprKind::Name(name) => self.type_reference(name, ty.span)?,
            // A qualified constructor names its defining module. A local
            // generated type must never capture that qualifier.
            s::ExprKind::Member { object, .. } if self.types.is_none() => {
                self.record_type(object, depth + 1)?;
            }
            s::ExprKind::Member { .. } => {}
            s::ExprKind::Group(object) => {
                self.record_type(object, depth + 1)?;
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                self.record_type(callee, depth + 1)?;
                for argument in arguments {
                    self.argument(argument, depth + 1)?;
                }
            }
            _ => self.expression(ty, depth + 1)?,
        }
        Ok(())
    }
    fn expression(&mut self, expression: &mut s::Expr, depth: usize) -> CompileResult<()> {
        let span = expression.span;
        self.budget.node(span, depth)?;
        match &mut expression.kind {
            s::ExprKind::Name(name) => self.reference(name, span)?,
            s::ExprKind::Literal(_) | s::ExprKind::ReceiverMember(_) => {}
            s::ExprKind::Group(value)
            | s::ExprKind::Try(value)
            | s::ExprKind::MetaEval(value)
            | s::ExprKind::Unary { operand: value, .. } => self.expression(value, depth + 1)?,
            s::ExprKind::Tuple(values) | s::ExprKind::Array(values) => {
                for value in values {
                    self.expression(value, depth + 1)?;
                }
            }
            s::ExprKind::Binary { left, right, .. }
            | s::ExprKind::Pipeline {
                input: left,
                target: right,
            } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            s::ExprKind::Member { object, .. } => {
                if !self.type_namespace(object, depth + 1)? {
                    self.expression(object, depth + 1)?;
                }
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                self.expression(callee, depth + 1)?;
                for argument in arguments {
                    self.argument(argument, depth + 1)?;
                }
            }
            s::ExprKind::Call { callee, arguments } => {
                self.expression(callee, depth + 1)?;
                for argument in arguments {
                    self.expression(&mut argument.value, depth + 1)?;
                }
            }
            s::ExprKind::Index { object, index } => {
                self.expression(object, depth + 1)?;
                self.expression(index, depth + 1)?;
            }
            s::ExprKind::Slice { object, start, end } => {
                self.expression(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(value, depth + 1)?;
                }
            }
            s::ExprKind::Record { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.record_type(ty, depth + 1)?;
                }
                if let Some(base) = base {
                    self.expression(base, depth + 1)?;
                }
                for field in fields {
                    self.budget.node(field.span, depth + 1)?;
                    if let Some(value) = &mut field.value {
                        self.expression(value, depth + 1)?;
                    } else {
                        self.budget.spend(1, field.name.len(), field.span)?;
                        let mut value = s::Expr {
                            span: field.span,
                            kind: s::ExprKind::Name(field.name.clone()),
                        };
                        self.expression(&mut value, depth + 1)?;
                        field.value = Some(value);
                    }
                }
            }
            s::ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                self.scope(span)?;
                for parameter in params {
                    if let Some(ty) = &mut parameter.ty {
                        self.ty(ty, depth + 1)?;
                    }
                    self.bind(&mut parameter.name, false, parameter.span)?;
                }
                if let Some(ty) = return_type {
                    self.ty(ty, depth + 1)?;
                }
                self.arrow(body, depth + 1)?;
                self.scopes.pop();
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)?;
            }
            s::ExprKind::MetaBlock(_) | s::ExprKind::SyntaxIsland { .. } => {
                return Err(self.unsupported(span));
            }
        }
        Ok(())
    }
    fn function(&mut self, function: &mut s::Function, depth: usize) -> CompileResult<()> {
        self.budget.node(function.span, depth)?;
        self.scope(function.span)?;
        self.function_type_scope(function, depth + 1)?;
        for parameter in &mut function.generics {
            if matches!(parameter.kind, s::GenericParamKind::Const(_)) {
                self.bind(&mut parameter.name, false, parameter.span)?;
            }
        }
        for parameter in &mut function.params {
            self.ty(&mut parameter.ty, depth + 1)?;
            self.bind(&mut parameter.name, false, parameter.span)?;
        }
        if let Some(ty) = &mut function.return_type {
            self.ty(ty, depth + 1)?;
        }
        match &mut function.body {
            s::FunctionBody::Block(body) => self.block(body, depth + 1)?,
            s::FunctionBody::Expression(value) => self.expression(value, depth + 1)?,
            s::FunctionBody::Declaration => {}
        }
        self.scopes.pop();
        if let Some(types) = &mut self.types {
            types.parameters.clear();
        }
        Ok(())
    }
}

pub(super) fn statements(
    statements: &mut [s::Stmt],
    module: ModuleId,
    serial: u64,
    budget: &mut Budget,
) -> CompileResult<()> {
    let Some(first) = statements.first() else {
        return Ok(());
    };
    let mut renamer = Renamer::new(module, serial, budget, first.span)?;
    renamer.statements(statements, true, 0)
}

pub(super) fn items(
    items: &mut [s::Item],
    module: ModuleId,
    serial: u64,
    budget: &mut Budget,
    program: &CoreProgram,
    has_types: bool,
) -> CompileResult<()> {
    let Some(first) = items.first() else {
        return Ok(());
    };
    let mut renamer = Renamer::new(module, serial, budget, first.span)?;
    if has_types {
        // Only type declarations need module qualifiers. Charge each actual
        // lookup before inspecting it; the old function-only path does neither.
        let mut imports = &[][..];
        for candidate in &program.modules {
            renamer.budget.node(first.span, 0)?;
            if candidate.core.module_id == module {
                imports = candidate.imports.as_slice();
                break;
            }
        }
        return renamer.type_items(items, imports);
    }
    for item in &mut *items {
        let s::ItemKind::Function(function) = &mut item.kind else {
            return Err(renamer.unsupported(item.span));
        };
        // A duplicate within one emitted list must not become two fresh names.
        renamer.budget.spend(
            0,
            function
                .name
                .len()
                .saturating_mul(renamer.scopes[0].len().saturating_add(1)),
            item.span,
        )?;
        if renamer.scopes[0].contains_key(&function.name) {
            return Err(vec![Diagnostic::error(
                Phase::Expand,
                "E-NAME-DUPLICATE",
                Some(item.span),
                "duplicate function name in one generated iteration",
            )]);
        }
        // The original spelling has already paid its name traversal above.
        zil_expand::check_generated_function_name(&function.name, item.span)?;
        renamer.bind(&mut function.name, !item.public, item.span)?;
    }
    for item in items {
        let s::ItemKind::Function(function) = &mut item.kind else {
            unreachable!();
        };
        renamer.function(function, 0)?;
    }
    Ok(())
}

#[cfg(test)]
mod generated_enum_tests {
    use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
    use zil_ir::InstructionKind;

    #[test]
    fn impl230_generated_enum_raw_wire_reconstructs_and_rejects_forgery() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "raw230.zil",
            "#for(n in [42]){enum E{A(i32),B(i32)}pub fn result()->i32{let e=E.A(n);match e{E.A(x)=>{return x},E.B(x)=>{return x}}}}",
        );
        let limits = Limits::default();
        let target = TargetProfile::default();
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let selected = crate::ct_selection::select(
            std::slice::from_ref(&source),
            &[&ast],
            &[Vec::new()],
            source.module_id,
            source.fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let remaining = selected.receipt.remaining(&limits).unwrap();
        let core = zil_expand::expand_with_map_limits(&source, &selected.modules[0], &remaining)
            .unwrap()
            .core;
        let module = zil_types::check(&core, &target, &remaining).unwrap();
        let plan = zil_analyze::analyze(&module, &remaining).unwrap();
        let verified =
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &remaining).unwrap();
        assert_eq!(verified.module(), &module);
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &remaining).unwrap();
        assert_eq!(
            zil_ir::wire::decode_module_candidate(&bytes, &remaining).unwrap(),
            (module.clone(), plan.clone())
        );
        zil_verify::verify_module_bytes(&bytes, &target, &remaining).unwrap();
        let mut rejected = Vec::new();
        for alteration in 0..3 {
            let mut forged = module.clone();
            let mut changed = false;
            for instruction in forged
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
            {
                match &mut instruction.kind {
                    InstructionKind::MakeVariant { variant, .. } if alteration == 0 => {
                        *variant = 99;
                        changed = true;
                    }
                    InstructionKind::MakeVariant { fields, .. } if alteration == 1 => {
                        fields.clear();
                        changed = true;
                    }
                    InstructionKind::ReadVariant { index, .. } if alteration == 2 => {
                        *index = 99;
                        changed = true;
                    }
                    _ => {}
                }
                if changed {
                    break;
                }
            }
            assert!(
                changed,
                "mutation must touch a real generated-enum instruction"
            );
            let mut stale = plan.clone();
            stale.module_fingerprint = forged.fingerprint();
            let raw = zil_verify::verify_module(forged.clone(), stale.clone(), &target, &remaining)
                .unwrap_err();
            let bytes = zil_ir::wire::encode_module_candidate(&forged, &stale, &remaining).unwrap();
            let wire = zil_verify::verify_module_bytes(&bytes, &target, &remaining).unwrap_err();
            assert_eq!(raw[0].code, "E-VERIFY-ENUM", "{raw:?}");
            assert_eq!(wire[0].code, "E-VERIFY-ENUM", "{wire:?}");
            rejected.push((alteration, raw, wire));
        }
        if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            let path = std::path::PathBuf::from(root).join("raw-wire230");
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("source.zil"), &source.text).unwrap();
            std::fs::write(path.join("core.txt"), format!("{core:#?}")).unwrap();
            std::fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
            std::fs::write(path.join("candidate.txt"), format!("{plan:#?}")).unwrap();
            std::fs::write(path.join("candidate.wire"), bytes).unwrap();
            std::fs::write(path.join("rejected.txt"), format!("{rejected:#?}")).unwrap();
        }
    }
}
