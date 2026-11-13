//! Data relocation for one real compiler-created CT wrapper. This module
//! produces raw original/body/site operands; it grants no CT or type authority.
use super::*;

fn invalid(message: &str, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-SEMANTIC-RELOCATION",
        Some(span),
        message,
    )]
}

struct Builder<'a, 'p> {
    collector: &'a mut Collector<'p>,
    module: ModuleId,
    bytes_left: &'a mut usize,
}
impl Builder<'_, '_> {
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth > self.collector.limits.max_call_depth.min(128) {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT wrapper original body depth exceeded",
                None,
            )]);
        }
        self.collector.charge(depth)
    }
    fn reserve(&mut self, bytes: usize) -> CompileResult<()> {
        *self.bytes_left = self.bytes_left.checked_sub(bytes).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "CT semantic relocation storage exhausted",
                None,
            )]
        })?;
        Ok(())
    }
    fn rows<T>(&mut self, length: usize) -> CompileResult<()> {
        self.collector.text_len(length)?;
        self.reserve(
            length
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "CT semantic relocation vector size overflow",
                        None,
                    )]
                })?,
        )
    }
    fn text(&mut self, value: &str) -> CompileResult<String> {
        self.collector.text(value)?;
        self.reserve(value.len())?;
        Ok(value.to_owned())
    }
    fn constant(&mut self, value: &Constant) -> CompileResult<Constant> {
        if let Constant::WideString(units) = value {
            self.rows::<u16>(units.len())?;
        }
        Ok(value.clone())
    }
    fn annotation(&mut self, ty: &Type, depth: usize) -> CompileResult<SemanticTypeRecipe> {
        self.node(depth)?;
        if ty.is_b0_scalar() {
            return Ok(SemanticTypeRecipe::Atom(ty.clone()));
        }
        Ok(match ty {
            Type::Tuple(fields) => {
                self.rows::<SemanticTypeRecipe>(fields.len())?;
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    values.push(self.annotation(field, depth + 1)?);
                }
                SemanticTypeRecipe::Tuple(values)
            }
            Type::Array { element, length } => {
                self.reserve(
                    std::mem::size_of::<SemanticTypeRecipe>()
                        .saturating_add(std::mem::size_of::<SemanticConstRecipe>()),
                )?;
                SemanticTypeRecipe::Array {
                    element: Box::new(self.annotation(element, depth + 1)?),
                    length: Box::new(SemanticConstRecipe::Value(Constant::Usize(u64::from(
                        *length,
                    )))),
                }
            }
            Type::Record { name, .. } | Type::Enum { name, .. } | Type::Named(name) => {
                self.nominal(ty, name, depth + 1)?
            }
            Type::ErrorUnion { error, result } => {
                self.reserve(2 * std::mem::size_of::<SemanticTypeRecipe>())?;
                SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(self.annotation(error, depth + 1)?),
                    result: Box::new(self.annotation(result, depth + 1)?),
                }
            }
            // Address/opaque/qualified values require their full original
            // class directory. A bare machine Type cannot establish Known0.
            _ => {
                return Err(invalid(
                    "CT wrapper annotation needs its complete serializable semantic type",
                    Span::default(),
                ));
            }
        })
    }
    fn nominal(
        &mut self,
        ty: &Type,
        name: &str,
        depth: usize,
    ) -> CompileResult<SemanticTypeRecipe> {
        let program = self.collector.program;
        let mut application = None;
        for module in &program.modules {
            for instance in &module.core.life_instances {
                self.node(depth)?;
                self.collector.ty(&instance.ty, depth)?;
                if instance.ty == *ty {
                    if application.as_ref().is_some_and(|old| *old != instance) {
                        return Err(invalid(
                            "CT nominal annotation has conflicting complete actual applications",
                            Span::default(),
                        ));
                    }
                    application = Some(instance);
                }
            }
        }
        let mut declaration = application.map(|row| row.declaration);
        if declaration.is_none() {
            // This is an actual declaration-header match, including the real
            // module and node. A physical name alone never grants a class or
            // nominal actual; generic headers still need the full application.
            for module in &program.modules {
                for aggregate in &module.core.aggregates {
                    self.node(depth)?;
                    if named(module.core.module_id, &aggregate.name, self.module, name) {
                        let id = LifeDeclarationId {
                            module: module.core.module_id,
                            declaration: aggregate.id,
                            compiler: false,
                        };
                        if declaration.is_some_and(|old| old != id) {
                            return Err(invalid(
                                "CT nominal annotation selects multiple declarations",
                                aggregate.span,
                            ));
                        }
                        declaration = Some(id);
                    }
                }
                for enumeration in &module.core.enums {
                    self.node(depth)?;
                    if named(module.core.module_id, &enumeration.name, self.module, name) {
                        let id = LifeDeclarationId {
                            module: module.core.module_id,
                            declaration: enumeration.id,
                            compiler: false,
                        };
                        if declaration.is_some_and(|old| old != id) {
                            return Err(invalid(
                                "CT nominal annotation selects multiple declarations",
                                enumeration.span,
                            ));
                        }
                        declaration = Some(id);
                    }
                }
            }
        }
        let declaration = declaration.ok_or_else(|| {
            invalid(
                "CT nominal annotation has no complete original declaration",
                Span::default(),
            )
        })?;
        let mut definition = None;
        for module in &program.modules {
            if let Some(program) = &module.core.semantic {
                for candidate in &program.definitions {
                    self.node(depth)?;
                    if matches!(candidate.body, SemanticDefinitionBody::Nominal { declaration: actual, .. } if actual == declaration)
                    {
                        if definition.is_some() {
                            return Err(invalid(
                                "CT nominal annotation has multiple original owners",
                                Span::default(),
                            ));
                        }
                        definition = Some(candidate);
                    }
                }
            }
        }
        let definition = definition.ok_or_else(|| {
            invalid(
                "CT nominal annotation has no original semantic owner",
                Span::default(),
            )
        })?;
        let mut arguments = Vec::new();
        if let Some(application) = application {
            if application.arguments.len() != definition.parameters.len() {
                return Err(invalid(
                    "CT nominal application changes full mixed formal arity",
                    Span::default(),
                ));
            }
            self.rows::<SemanticActualRecipe>(application.arguments.len())?;
            for (formal, actual) in definition.parameters.iter().zip(&application.arguments) {
                let value = match (formal, actual) {
                    (SemanticFormal::Type { .. }, LifeNominalActual::Type(ty)) => {
                        SemanticActualRecipe::Type(self.annotation(ty, depth + 1)?)
                    }
                    (
                        SemanticFormal::Type { .. },
                        LifeNominalActual::QualifiedType { ty, life },
                    ) if life.binder_count == 0
                        && life.slots.is_empty()
                        && life.retained.is_empty()
                        && life.nominal_arguments.is_empty() =>
                    {
                        SemanticActualRecipe::Type(self.annotation(ty, depth + 1)?)
                    }
                    (SemanticFormal::Const { .. }, LifeNominalActual::Const(value)) => {
                        SemanticActualRecipe::Const {
                            carrier: self.annotation(&value.ty(), depth + 1)?,
                            value: SemanticConstRecipe::Value(self.constant(value)?),
                        }
                    }
                    _ => {
                        return Err(invalid(
                            "CT nominal actual needs its full intrinsic lifetime or formal-kind consumer",
                            Span::default(),
                        ));
                    }
                };
                arguments.push(value);
            }
        } else if !definition.parameters.is_empty() {
            return Err(invalid(
                "CT generic nominal annotation lacks its complete actual application",
                Span::default(),
            ));
        }
        Ok(SemanticTypeRecipe::Nominal {
            definition: definition.id,
            arguments,
        })
    }
}
fn named(owner: ModuleId, declaration: &str, current: ModuleId, actual: &str) -> bool {
    (owner == current && declaration == actual)
        || actual
            .strip_prefix("@module")
            .and_then(|tail| tail.split_once(':'))
            .is_some_and(|(module, name)| {
                module.parse::<u32>().ok() == Some(owner.0) && name == declaration
            })
}
// The operation constructors below follow the actual shared Core grammar.
// They change only annotation slots; every operation/field/NodeId/Span stays.
impl Builder<'_, '_> {
    fn core_closure_parameter(
        &mut self,
        value: &CoreClosureParameter,
        depth: usize,
    ) -> CompileResult<SemanticOriginalClosureParameter> {
        self.node(depth)?;
        let CoreClosureParameter {
            name,
            ty,
            mode,
            span,
        } = value;
        Ok(SemanticOriginalClosureParameter {
            name: self.text(name)?,
            ty: match ty {
                Some(value) => Some(self.annotation(value, depth + 1)?),
                None => None,
            },
            mode: (mode).clone(),
            span: (span).clone(),
        })
    }
    fn core_pattern(
        &mut self,
        value: &CorePattern,
        depth: usize,
    ) -> CompileResult<SemanticOriginalPattern> {
        self.node(depth)?;
        let CorePattern { kind, span } = value;
        Ok(SemanticOriginalPattern {
            kind: self.core_pattern_kind(kind, depth + 1)?,
            span: (span).clone(),
        })
    }
    fn core_pattern_kind(
        &mut self,
        value: &CorePatternKind,
        depth: usize,
    ) -> CompileResult<SemanticOriginalPatternKind> {
        self.node(depth)?;
        Ok(match value {
            CorePatternKind::Name(value_0) => {
                SemanticOriginalPatternKind::Name(self.text(value_0)?)
            }
            CorePatternKind::Wildcard => SemanticOriginalPatternKind::Wildcard,
            CorePatternKind::Tuple(value_0) => SemanticOriginalPatternKind::Tuple({
                self.rows::<SemanticOriginalPattern>(value_0.len())?;
                let mut result = Vec::with_capacity(value_0.len());
                for value in value_0 {
                    result.push(self.core_pattern(value, depth + 1)?);
                }
                result
            }),
            CorePatternKind::Record { ty, fields } => SemanticOriginalPatternKind::Record {
                ty: self.annotation(ty, depth + 1)?,
                fields: {
                    self.rows::<(String, SemanticOriginalPattern)>(fields.len())?;
                    let mut result = Vec::with_capacity(fields.len());
                    for value in fields {
                        result.push((
                            self.text(&(value).0)?,
                            self.core_pattern(&(value).1, depth + 1)?,
                        ));
                    }
                    result
                },
            },
            CorePatternKind::Reference { mutable, pattern } => {
                SemanticOriginalPatternKind::Reference {
                    mutable: (mutable).clone(),
                    pattern: {
                        self.reserve(std::mem::size_of::<SemanticOriginalPattern>())?;
                        Box::new(self.core_pattern((pattern).as_ref(), depth + 1)?)
                    },
                }
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                SemanticOriginalPatternKind::ErrorArm {
                    is_error: (is_error).clone(),
                    pattern: {
                        self.reserve(std::mem::size_of::<SemanticOriginalPattern>())?;
                        Box::new(self.core_pattern((pattern).as_ref(), depth + 1)?)
                    },
                }
            }
            CorePatternKind::Variant {
                ty,
                variant,
                fields,
            } => SemanticOriginalPatternKind::Variant {
                ty: self.annotation(ty, depth + 1)?,
                variant: self.text(variant)?,
                fields: {
                    self.rows::<SemanticOriginalPattern>(fields.len())?;
                    let mut result = Vec::with_capacity(fields.len());
                    for value in fields {
                        result.push(self.core_pattern(value, depth + 1)?);
                    }
                    result
                },
            },
            CorePatternKind::Integer(value_0) => {
                SemanticOriginalPatternKind::Integer(self.text(value_0)?)
            }
            CorePatternKind::Bool(value_0) => SemanticOriginalPatternKind::Bool((value_0).clone()),
            CorePatternKind::Constructor {
                definition,
                variant,
                fields,
            } => SemanticOriginalPatternKind::Constructor {
                definition: match definition {
                    Some(value) => Some((value).clone()),
                    None => None,
                },
                variant: self.text(variant)?,
                fields: {
                    self.rows::<SemanticOriginalPattern>(fields.len())?;
                    let mut result = Vec::with_capacity(fields.len());
                    for value in fields {
                        result.push(self.core_pattern(value, depth + 1)?);
                    }
                    result
                },
            },
            CorePatternKind::RecordConstructor { definition, fields } => {
                SemanticOriginalPatternKind::RecordConstructor {
                    definition: match definition {
                        Some(value) => Some((value).clone()),
                        None => None,
                    },
                    fields: {
                        self.rows::<(String, SemanticOriginalPattern)>(fields.len())?;
                        let mut result = Vec::with_capacity(fields.len());
                        for value in fields {
                            result.push((
                                self.text(&(value).0)?,
                                self.core_pattern(&(value).1, depth + 1)?,
                            ));
                        }
                        result
                    },
                }
            }
            CorePatternKind::Float(value_0) => {
                SemanticOriginalPatternKind::Float(self.text(value_0)?)
            }
            CorePatternKind::String(value_0) => {
                SemanticOriginalPatternKind::String(self.text(value_0)?)
            }
            CorePatternKind::Character(value_0) => {
                SemanticOriginalPatternKind::Character(self.text(value_0)?)
            }
        })
    }
    fn core_match_arm(
        &mut self,
        value: &CoreMatchArm,
        depth: usize,
    ) -> CompileResult<SemanticOriginalMatchArm> {
        self.node(depth)?;
        let CoreMatchArm {
            pattern,
            body,
            span,
        } = value;
        Ok(SemanticOriginalMatchArm {
            pattern: self.core_pattern(pattern, depth + 1)?,
            body: self.core_block(body, depth + 1)?,
            span: (span).clone(),
        })
    }
    fn core_block(
        &mut self,
        value: &CoreBlock,
        depth: usize,
    ) -> CompileResult<SemanticOriginalBlock> {
        self.node(depth)?;
        let CoreBlock { statements, span } = value;
        Ok(SemanticOriginalBlock {
            statements: {
                self.rows::<SemanticOriginalStatement>(statements.len())?;
                let mut result = Vec::with_capacity(statements.len());
                for value in statements {
                    result.push(self.core_statement(value, depth + 1)?);
                }
                result
            },
            span: (span).clone(),
        })
    }
    fn core_statement(
        &mut self,
        value: &CoreStatement,
        depth: usize,
    ) -> CompileResult<SemanticOriginalStatement> {
        self.node(depth)?;
        let CoreStatement { id, kind, span } = value;
        Ok(SemanticOriginalStatement {
            id: (id).clone(),
            kind: self.core_statement_kind(kind, depth + 1)?,
            span: (span).clone(),
        })
    }
    fn core_statement_kind(
        &mut self,
        value: &CoreStatementKind,
        depth: usize,
    ) -> CompileResult<SemanticOriginalStatementKind> {
        self.node(depth)?;
        Ok(match value {
            CoreStatementKind::Binding {
                name,
                mutable,
                ty,
                initializer,
            } => SemanticOriginalStatementKind::Binding {
                name: self.text(name)?,
                mutable: (mutable).clone(),
                ty: match ty {
                    Some(value) => Some(self.annotation(value, depth + 1)?),
                    None => None,
                },
                initializer: match initializer {
                    Some(value) => Some(self.core_expr(value, depth + 1)?),
                    None => None,
                },
            },
            CoreStatementKind::TupleBinding {
                pattern,
                mutable,
                ty,
                initializer,
            } => SemanticOriginalStatementKind::TupleBinding {
                pattern: self.core_pattern(pattern, depth + 1)?,
                mutable: (mutable).clone(),
                ty: match ty {
                    Some(value) => Some(self.annotation(value, depth + 1)?),
                    None => None,
                },
                initializer: self.core_expr(initializer, depth + 1)?,
            },
            CoreStatementKind::Assign { target, value } => SemanticOriginalStatementKind::Assign {
                target: self.core_expr(target, depth + 1)?,
                value: self.core_expr(value, depth + 1)?,
            },
            CoreStatementKind::Expression(value_0) => {
                SemanticOriginalStatementKind::Expression(self.core_expr(value_0, depth + 1)?)
            }
            CoreStatementKind::Return(value_0) => {
                SemanticOriginalStatementKind::Return(match value_0 {
                    Some(value) => Some(self.core_expr(value, depth + 1)?),
                    None => None,
                })
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => SemanticOriginalStatementKind::If {
                condition: self.core_expr(condition, depth + 1)?,
                then_body: self.core_block(then_body, depth + 1)?,
                else_body: match else_body {
                    Some(value) => Some(self.core_block(value, depth + 1)?),
                    None => None,
                },
            },
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => SemanticOriginalStatementKind::IfLet {
                pattern: self.core_pattern(pattern, depth + 1)?,
                value: self.core_expr(value, depth + 1)?,
                then_body: self.core_block(then_body, depth + 1)?,
                else_body: match else_body {
                    Some(value) => Some(self.core_block(value, depth + 1)?),
                    None => None,
                },
            },
            CoreStatementKind::While { condition, body } => SemanticOriginalStatementKind::While {
                condition: self.core_expr(condition, depth + 1)?,
                body: self.core_block(body, depth + 1)?,
            },
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => SemanticOriginalStatementKind::WhileLet {
                pattern: self.core_pattern(pattern, depth + 1)?,
                value: self.core_expr(value, depth + 1)?,
                body: self.core_block(body, depth + 1)?,
            },
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => SemanticOriginalStatementKind::ForRange {
                name: self.text(name)?,
                start: self.core_expr(start, depth + 1)?,
                end: self.core_expr(end, depth + 1)?,
                body: self.core_block(body, depth + 1)?,
            },
            CoreStatementKind::Match { value, arms } => SemanticOriginalStatementKind::Match {
                value: self.core_expr(value, depth + 1)?,
                arms: {
                    self.rows::<SemanticOriginalMatchArm>(arms.len())?;
                    let mut result = Vec::with_capacity(arms.len());
                    for value in arms {
                        result.push(self.core_match_arm(value, depth + 1)?);
                    }
                    result
                },
            },
            CoreStatementKind::Block(value_0) => {
                SemanticOriginalStatementKind::Block(self.core_block(value_0, depth + 1)?)
            }
            CoreStatementKind::Defer(value_0) => {
                SemanticOriginalStatementKind::Defer(self.core_block(value_0, depth + 1)?)
            }
            CoreStatementKind::Unsafe(value_0) => {
                SemanticOriginalStatementKind::Unsafe(self.core_block(value_0, depth + 1)?)
            }
            CoreStatementKind::Break => SemanticOriginalStatementKind::Break,
            CoreStatementKind::Continue => SemanticOriginalStatementKind::Continue,
            CoreStatementKind::Unsupported(value_0) => {
                SemanticOriginalStatementKind::Unsupported(self.text(value_0)?)
            }
            CoreStatementKind::ForEach {
                pattern,
                iterator,
                body,
            } => SemanticOriginalStatementKind::ForEach {
                pattern: self.core_pattern(pattern, depth + 1)?,
                iterator: self.core_expr(iterator, depth + 1)?,
                body: self.core_block(body, depth + 1)?,
            },
            CoreStatementKind::CompoundAssign { target, op, value } => {
                SemanticOriginalStatementKind::CompoundAssign {
                    target: self.core_expr(target, depth + 1)?,
                    op: (op).clone(),
                    value: self.core_expr(value, depth + 1)?,
                }
            }
            CoreStatementKind::Yield(value_0) => {
                SemanticOriginalStatementKind::Yield(self.core_expr(value_0, depth + 1)?)
            }
            CoreStatementKind::Loop(value_0) => {
                SemanticOriginalStatementKind::Loop(self.core_block(value_0, depth + 1)?)
            }
            CoreStatementKind::With { name, value, body } => SemanticOriginalStatementKind::With {
                name: self.text(name)?,
                value: self.core_expr(value, depth + 1)?,
                body: self.core_block(body, depth + 1)?,
            },
            CoreStatementKind::CompileTime(value_0) => {
                SemanticOriginalStatementKind::CompileTime({
                    self.reserve(std::mem::size_of::<SemanticOriginalStatement>())?;
                    Box::new(self.core_statement((value_0).as_ref(), depth + 1)?)
                })
            }
        })
    }
    fn core_expr(&mut self, value: &CoreExpr, depth: usize) -> CompileResult<SemanticOriginalExpr> {
        self.node(depth)?;
        let CoreExpr { id, kind, span } = value;
        Ok(SemanticOriginalExpr {
            id: (id).clone(),
            kind: self.core_expr_kind(kind, depth + 1)?,
            span: (span).clone(),
        })
    }
    fn core_expr_kind(
        &mut self,
        value: &CoreExprKind,
        depth: usize,
    ) -> CompileResult<SemanticOriginalExprKind> {
        self.node(depth)?;
        Ok(match value {
            CoreExprKind::MakeDynBorrow {
                interface,
                vtable,
                source,
                mutable,
            } => SemanticOriginalExprKind::MakeDynBorrow {
                interface: (interface).clone(),
                vtable: (vtable).clone(),
                source: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((source).as_ref(), depth + 1)?)
                },
                mutable: (mutable).clone(),
            },
            CoreExprKind::MakeDynOwned {
                interface,
                vtable,
                source,
                route,
            } => SemanticOriginalExprKind::MakeDynOwned {
                interface: (interface).clone(),
                vtable: (vtable).clone(),
                source: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((source).as_ref(), depth + 1)?)
                },
                route: (route).clone(),
            },
            CoreExprKind::CallDynSlot {
                callee,
                interface,
                slot,
                arguments,
            } => SemanticOriginalExprKind::CallDynSlot {
                callee: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((callee).as_ref(), depth + 1)?)
                },
                interface: (interface).clone(),
                slot: (slot).clone(),
                arguments: {
                    self.rows::<SemanticOriginalExpr>(arguments.len())?;
                    let mut result = Vec::with_capacity(arguments.len());
                    for value in arguments {
                        result.push(self.core_expr(value, depth + 1)?);
                    }
                    result
                },
            },
            CoreExprKind::Literal(value_0) => {
                SemanticOriginalExprKind::Literal(self.constant(value_0)?)
            }
            CoreExprKind::Ascribed { ty, value } => SemanticOriginalExprKind::Ascribed {
                ty: self.annotation(ty, depth + 1)?,
                value: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((value).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::Integer(value_0) => {
                SemanticOriginalExprKind::Integer(self.text(value_0)?)
            }
            CoreExprKind::Float(value_0) => SemanticOriginalExprKind::Float(self.text(value_0)?),
            CoreExprKind::NumericConvert { value, target } => {
                SemanticOriginalExprKind::NumericConvert {
                    value: {
                        self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                        Box::new(self.core_expr((value).as_ref(), depth + 1)?)
                    },
                    target: self.annotation(target, depth + 1)?,
                }
            }
            CoreExprKind::IntegerConvert { value, target } => {
                SemanticOriginalExprKind::IntegerConvert {
                    value: {
                        self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                        Box::new(self.core_expr((value).as_ref(), depth + 1)?)
                    },
                    target: self.annotation(target, depth + 1)?,
                }
            }
            CoreExprKind::Bool(value_0) => SemanticOriginalExprKind::Bool((value_0).clone()),
            CoreExprKind::String(value_0) => SemanticOriginalExprKind::String(self.text(value_0)?),
            CoreExprKind::Unit => SemanticOriginalExprKind::Unit,
            CoreExprKind::Name(value_0) => SemanticOriginalExprKind::Name(self.text(value_0)?),
            CoreExprKind::Unary { op, operand } => SemanticOriginalExprKind::Unary {
                op: (op).clone(),
                operand: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((operand).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::Binary { op, left, right } => SemanticOriginalExprKind::Binary {
                op: (op).clone(),
                left: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((left).as_ref(), depth + 1)?)
                },
                right: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((right).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => SemanticOriginalExprKind::TypeApply {
                callee: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((callee).as_ref(), depth + 1)?)
                },
                type_arguments: {
                    self.rows::<SemanticTypeRecipe>(type_arguments.len())?;
                    let mut result = Vec::with_capacity(type_arguments.len());
                    for value in type_arguments {
                        result.push(self.annotation(value, depth + 1)?);
                    }
                    result
                },
            },
            CoreExprKind::Call { callee, arguments } => SemanticOriginalExprKind::Call {
                callee: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((callee).as_ref(), depth + 1)?)
                },
                arguments: {
                    self.rows::<SemanticOriginalExpr>(arguments.len())?;
                    let mut result = Vec::with_capacity(arguments.len());
                    for value in arguments {
                        result.push(self.core_expr(value, depth + 1)?);
                    }
                    result
                },
            },
            CoreExprKind::Tuple(value_0) => SemanticOriginalExprKind::Tuple({
                self.rows::<SemanticOriginalExpr>(value_0.len())?;
                let mut result = Vec::with_capacity(value_0.len());
                for value in value_0 {
                    result.push(self.core_expr(value, depth + 1)?);
                }
                result
            }),
            CoreExprKind::Array(value_0) => SemanticOriginalExprKind::Array({
                self.rows::<SemanticOriginalExpr>(value_0.len())?;
                let mut result = Vec::with_capacity(value_0.len());
                for value in value_0 {
                    result.push(self.core_expr(value, depth + 1)?);
                }
                result
            }),
            CoreExprKind::Index { object, index } => SemanticOriginalExprKind::Index {
                object: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((object).as_ref(), depth + 1)?)
                },
                index: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((index).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::Slice { object, start, end } => SemanticOriginalExprKind::Slice {
                object: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((object).as_ref(), depth + 1)?)
                },
                start: match start {
                    Some(value) => Some({
                        self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                        Box::new(self.core_expr((value).as_ref(), depth + 1)?)
                    }),
                    None => None,
                },
                end: match end {
                    Some(value) => Some({
                        self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                        Box::new(self.core_expr((value).as_ref(), depth + 1)?)
                    }),
                    None => None,
                },
            },
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => SemanticOriginalExprKind::Member {
                object: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((object).as_ref(), depth + 1)?)
                },
                member: self.text(member)?,
                owner: match owner {
                    Some(value) => Some(self.annotation(value, depth + 1)?),
                    None => None,
                },
            },
            CoreExprKind::ReceiverMember(value_0) => {
                SemanticOriginalExprKind::ReceiverMember(self.text(value_0)?)
            }
            CoreExprKind::Record { ty, fields } => SemanticOriginalExprKind::Record {
                ty: self.annotation(ty, depth + 1)?,
                fields: {
                    self.rows::<(String, SemanticOriginalExpr)>(fields.len())?;
                    let mut result = Vec::with_capacity(fields.len());
                    for value in fields {
                        result.push((
                            self.text(&(value).0)?,
                            self.core_expr(&(value).1, depth + 1)?,
                        ));
                    }
                    result
                },
            },
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                SemanticOriginalExprKind::RecordUpdate {
                    ty: match ty {
                        Some(value) => Some(self.annotation(value, depth + 1)?),
                        None => None,
                    },
                    base: {
                        self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                        Box::new(self.core_expr((base).as_ref(), depth + 1)?)
                    },
                    fields: {
                        self.rows::<(String, SemanticOriginalExpr)>(fields.len())?;
                        let mut result = Vec::with_capacity(fields.len());
                        for value in fields {
                            result.push((
                                self.text(&(value).0)?,
                                self.core_expr(&(value).1, depth + 1)?,
                            ));
                        }
                        result
                    },
                }
            }
            CoreExprKind::Sequence { bindings, value } => SemanticOriginalExprKind::Sequence {
                bindings: {
                    self.rows::<(String, SemanticOriginalExpr)>(bindings.len())?;
                    let mut result = Vec::with_capacity(bindings.len());
                    for value in bindings {
                        result.push((
                            self.text(&(value).0)?,
                            self.core_expr(&(value).1, depth + 1)?,
                        ));
                    }
                    result
                },
                value: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((value).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::CompileTime(value_0) => SemanticOriginalExprKind::CompileTime({
                self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                Box::new(self.core_expr((value_0).as_ref(), depth + 1)?)
            }),
            CoreExprKind::Try(value_0) => SemanticOriginalExprKind::Try({
                self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                Box::new(self.core_expr((value_0).as_ref(), depth + 1)?)
            }),
            CoreExprKind::Closure {
                move_capture,
                parameters,
                return_type,
                body,
            } => SemanticOriginalExprKind::Closure {
                move_capture: (move_capture).clone(),
                parameters: {
                    self.rows::<SemanticOriginalClosureParameter>(parameters.len())?;
                    let mut result = Vec::with_capacity(parameters.len());
                    for value in parameters {
                        result.push(self.core_closure_parameter(value, depth + 1)?);
                    }
                    result
                },
                return_type: match return_type {
                    Some(value) => Some(self.annotation(value, depth + 1)?),
                    None => None,
                },
                body: self.core_block(body, depth + 1)?,
            },
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => SemanticOriginalExprKind::If {
                condition: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((condition).as_ref(), depth + 1)?)
                },
                then_value: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((then_value).as_ref(), depth + 1)?)
                },
                else_value: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((else_value).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::Unsupported(value_0) => {
                SemanticOriginalExprKind::Unsupported(self.text(value_0)?)
            }
            CoreExprKind::Spread(value_0) => SemanticOriginalExprKind::Spread({
                self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                Box::new(self.core_expr((value_0).as_ref(), depth + 1)?)
            }),
            CoreExprKind::Group(value_0) => SemanticOriginalExprKind::Group({
                self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                Box::new(self.core_expr((value_0).as_ref(), depth + 1)?)
            }),
            CoreExprKind::Character(value_0) => {
                SemanticOriginalExprKind::Character(self.text(value_0)?)
            }
            CoreExprKind::Range { start, end } => SemanticOriginalExprKind::Range {
                start: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((start).as_ref(), depth + 1)?)
                },
                end: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((end).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::Pipeline { input, target } => SemanticOriginalExprKind::Pipeline {
                input: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((input).as_ref(), depth + 1)?)
                },
                target: {
                    self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                    Box::new(self.core_expr((target).as_ref(), depth + 1)?)
                },
            },
            CoreExprKind::RecordConstructor { definition, fields } => {
                SemanticOriginalExprKind::RecordConstructor {
                    definition: match definition {
                        Some(value) => Some((value).clone()),
                        None => None,
                    },
                    fields: {
                        self.rows::<(String, SemanticOriginalExpr)>(fields.len())?;
                        let mut result = Vec::with_capacity(fields.len());
                        for value in fields {
                            result.push((
                                self.text(&(value).0)?,
                                self.core_expr(&(value).1, depth + 1)?,
                            ));
                        }
                        result
                    },
                }
            }
            CoreExprKind::Splice(value_0) => SemanticOriginalExprKind::Splice({
                self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                Box::new(self.core_expr((value_0).as_ref(), depth + 1)?)
            }),
            CoreExprKind::CompileTimeBlock(value_0) => {
                SemanticOriginalExprKind::CompileTimeBlock(self.core_block(value_0, depth + 1)?)
            }
            CoreExprKind::SyntaxQuote { category, template } => {
                SemanticOriginalExprKind::SyntaxQuote {
                    category: self.core_syntax_category(category, depth + 1)?,
                    template: self.core_quote_template(template, depth + 1)?,
                }
            }
            CoreExprKind::SyntaxIsland { path, tree } => SemanticOriginalExprKind::SyntaxIsland {
                path: {
                    self.rows::<String>(path.len())?;
                    let mut result = Vec::with_capacity(path.len());
                    for value in path {
                        result.push(self.text(value)?);
                    }
                    result
                },
                tree: self.core_syntax_tree(tree, depth + 1)?,
            },
            CoreExprKind::GenericApply { callee, arguments } => {
                SemanticOriginalExprKind::GenericApply {
                    callee: {
                        self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                        Box::new(self.core_expr((callee).as_ref(), depth + 1)?)
                    },
                    arguments: {
                        self.rows::<SemanticOriginalGenericArgument>(arguments.len())?;
                        let mut result = Vec::with_capacity(arguments.len());
                        for value in arguments {
                            result.push(self.core_generic_argument(value, depth + 1)?);
                        }
                        result
                    },
                }
            }
        })
    }
    fn core_generic_argument(
        &mut self,
        value: &CoreGenericArgument,
        depth: usize,
    ) -> CompileResult<SemanticOriginalGenericArgument> {
        self.node(depth)?;
        Ok(match value {
            CoreGenericArgument::Type(value_0) => {
                SemanticOriginalGenericArgument::Type(self.annotation(value_0, depth + 1)?)
            }
            CoreGenericArgument::Const(value_0) => {
                SemanticOriginalGenericArgument::Const(self.core_expr(value_0, depth + 1)?)
            }
            CoreGenericArgument::Lifetime(value_0) => {
                SemanticOriginalGenericArgument::Lifetime(self.text(value_0)?)
            }
        })
    }
    fn core_syntax_category(
        &mut self,
        value: &CoreSyntaxCategory,
        depth: usize,
    ) -> CompileResult<SemanticOriginalSyntaxCategory> {
        self.node(depth)?;
        Ok(match value {
            CoreSyntaxCategory::Expr => SemanticOriginalSyntaxCategory::Expr,
            CoreSyntaxCategory::Type => SemanticOriginalSyntaxCategory::Type,
            CoreSyntaxCategory::StmtList => SemanticOriginalSyntaxCategory::StmtList,
            CoreSyntaxCategory::ItemList => SemanticOriginalSyntaxCategory::ItemList,
        })
    }
    fn core_syntax_delimiter(
        &mut self,
        value: &CoreSyntaxDelimiter,
        depth: usize,
    ) -> CompileResult<SemanticOriginalSyntaxDelimiter> {
        self.node(depth)?;
        Ok(match value {
            CoreSyntaxDelimiter::Parenthesis => SemanticOriginalSyntaxDelimiter::Parenthesis,
            CoreSyntaxDelimiter::Bracket => SemanticOriginalSyntaxDelimiter::Bracket,
            CoreSyntaxDelimiter::Brace => SemanticOriginalSyntaxDelimiter::Brace,
        })
    }
    fn core_quote_name_namespace(
        &mut self,
        value: &CoreQuoteNameNamespace,
        depth: usize,
    ) -> CompileResult<SemanticOriginalQuoteNameNamespace> {
        self.node(depth)?;
        Ok(match value {
            CoreQuoteNameNamespace::Value => SemanticOriginalQuoteNameNamespace::Value,
            CoreQuoteNameNamespace::Type => SemanticOriginalQuoteNameNamespace::Type,
            CoreQuoteNameNamespace::Member => SemanticOriginalQuoteNameNamespace::Member,
            CoreQuoteNameNamespace::Module => SemanticOriginalQuoteNameNamespace::Module,
            CoreQuoteNameNamespace::Label => SemanticOriginalQuoteNameNamespace::Label,
        })
    }
    fn core_quote_name_role(
        &mut self,
        value: &CoreQuoteNameRole,
        depth: usize,
    ) -> CompileResult<SemanticOriginalQuoteNameRole> {
        self.node(depth)?;
        Ok(match value {
            CoreQuoteNameRole::Binding => SemanticOriginalQuoteNameRole::Binding,
            CoreQuoteNameRole::Reference => SemanticOriginalQuoteNameRole::Reference,
        })
    }
    fn core_quote_hole_kind(
        &mut self,
        value: &CoreQuoteHoleKind,
        depth: usize,
    ) -> CompileResult<SemanticOriginalQuoteHoleKind> {
        self.node(depth)?;
        Ok(match value {
            CoreQuoteHoleKind::Expr => SemanticOriginalQuoteHoleKind::Expr,
            CoreQuoteHoleKind::Type => SemanticOriginalQuoteHoleKind::Type,
            CoreQuoteHoleKind::StmtList => SemanticOriginalQuoteHoleKind::StmtList,
            CoreQuoteHoleKind::ItemList => SemanticOriginalQuoteHoleKind::ItemList,
            CoreQuoteHoleKind::Ident { namespace, role } => SemanticOriginalQuoteHoleKind::Ident {
                namespace: self.core_quote_name_namespace(namespace, depth + 1)?,
                role: self.core_quote_name_role(role, depth + 1)?,
            },
        })
    }
    fn core_quote_hole(
        &mut self,
        value: &CoreQuoteHole,
        depth: usize,
    ) -> CompileResult<SemanticOriginalQuoteHole> {
        self.node(depth)?;
        let CoreQuoteHole { kind, value, span } = value;
        Ok(SemanticOriginalQuoteHole {
            kind: self.core_quote_hole_kind(kind, depth + 1)?,
            value: {
                self.reserve(std::mem::size_of::<SemanticOriginalExpr>())?;
                Box::new(self.core_expr((value).as_ref(), depth + 1)?)
            },
            span: (span).clone(),
        })
    }
    fn core_quote_template(
        &mut self,
        value: &CoreQuoteTemplate,
        depth: usize,
    ) -> CompileResult<SemanticOriginalQuoteTemplate> {
        self.node(depth)?;
        let CoreQuoteTemplate {
            tree,
            holes,
            name_sites,
        } = value;
        Ok(SemanticOriginalQuoteTemplate {
            tree: self.core_syntax_tree(tree, depth + 1)?,
            holes: {
                self.rows::<SemanticOriginalQuoteHole>(holes.len())?;
                let mut result = Vec::with_capacity(holes.len());
                for value in holes {
                    result.push(self.core_quote_hole(value, depth + 1)?);
                }
                result
            },
            name_sites: {
                self.rows::<SemanticOriginalQuoteNameSite>(name_sites.len())?;
                let mut result = Vec::with_capacity(name_sites.len());
                for value in name_sites {
                    result.push(self.core_quote_name_site(value, depth + 1)?);
                }
                result
            },
        })
    }
    fn core_quote_name_site(
        &mut self,
        value: &CoreQuoteNameSite,
        depth: usize,
    ) -> CompileResult<SemanticOriginalQuoteNameSite> {
        self.node(depth)?;
        let CoreQuoteNameSite { span, token_index } = value;
        Ok(SemanticOriginalQuoteNameSite {
            span: (span).clone(),
            token_index: (token_index).clone(),
        })
    }
    fn core_syntax_tree(
        &mut self,
        value: &CoreSyntaxTree,
        depth: usize,
    ) -> CompileResult<SemanticOriginalSyntaxTree> {
        self.node(depth)?;
        let CoreSyntaxTree {
            delimiter,
            children,
            span,
        } = value;
        Ok(SemanticOriginalSyntaxTree {
            delimiter: self.core_syntax_delimiter(delimiter, depth + 1)?,
            children: {
                self.rows::<SemanticOriginalSyntaxTreeEntry>(children.len())?;
                let mut result = Vec::with_capacity(children.len());
                for value in children {
                    result.push(self.core_syntax_tree_entry(value, depth + 1)?);
                }
                result
            },
            span: (span).clone(),
        })
    }
    fn core_syntax_tree_entry(
        &mut self,
        value: &CoreSyntaxTreeEntry,
        depth: usize,
    ) -> CompileResult<SemanticOriginalSyntaxTreeEntry> {
        self.node(depth)?;
        Ok(match value {
            CoreSyntaxTreeEntry::Token { text, span } => SemanticOriginalSyntaxTreeEntry::Token {
                text: self.text(text)?,
                span: (span).clone(),
            },
            CoreSyntaxTreeEntry::Tree(value_0) => {
                SemanticOriginalSyntaxTreeEntry::Tree(self.core_syntax_tree(value_0, depth + 1)?)
            }
            CoreSyntaxTreeEntry::Hole { index, span } => SemanticOriginalSyntaxTreeEntry::Hole {
                index: (index).clone(),
                span: (span).clone(),
            },
        })
    }
}

struct NodeIndex<'a> {
    expressions: BTreeMap<NodeId, &'a CoreExpr>,
    statements: BTreeMap<NodeId, &'a CoreStatement>,
    paths: BTreeMap<NodeId, Vec<u32>>,
}
enum Scan<'a> {
    CoreClosureParameter(&'a CoreClosureParameter),
    CorePattern(&'a CorePattern),
    CorePatternKind(&'a CorePatternKind),
    CoreMatchArm(&'a CoreMatchArm),
    CoreBlock(&'a CoreBlock),
    CoreStatement(&'a CoreStatement),
    CoreStatementKind(&'a CoreStatementKind),
    CoreExpr(&'a CoreExpr),
    CoreExprKind(&'a CoreExprKind),
    CoreGenericArgument(&'a CoreGenericArgument),
    CoreSyntaxCategory(&'a CoreSyntaxCategory),
    CoreSyntaxDelimiter(&'a CoreSyntaxDelimiter),
    CoreQuoteNameNamespace(&'a CoreQuoteNameNamespace),
    CoreQuoteNameRole(&'a CoreQuoteNameRole),
    CoreQuoteHoleKind(&'a CoreQuoteHoleKind),
    CoreQuoteHole(&'a CoreQuoteHole),
    CoreQuoteTemplate(&'a CoreQuoteTemplate),
    CoreQuoteNameSite(&'a CoreQuoteNameSite),
    CoreSyntaxTree(&'a CoreSyntaxTree),
    CoreSyntaxTreeEntry(&'a CoreSyntaxTreeEntry),
}
impl Builder<'_, '_> {
    fn child_path(&mut self, parent: &[u32], suffix: &[usize]) -> CompileResult<Vec<u32>> {
        self.rows::<u32>(parent.len().saturating_add(suffix.len()))?;
        let mut path = Vec::with_capacity(parent.len() + suffix.len());
        path.extend_from_slice(parent);
        for index in suffix {
            path.push(
                u32::try_from(*index)
                    .map_err(|_| invalid("CT original path ordinal overflow", Span::default()))?,
            );
        }
        Ok(path)
    }
    fn nodes<'a>(&mut self, root: &'a CoreBlock) -> CompileResult<NodeIndex<'a>> {
        self.reserve(64)?;
        let mut pending = vec![(Scan::CoreBlock(root), 0, Vec::new())];
        let mut expressions = BTreeMap::new();
        let mut statements = BTreeMap::new();
        let mut paths = BTreeMap::new();
        while let Some((next, depth, path)) = pending.pop() {
            self.node(depth)?;
            match next {
                Scan::CoreClosureParameter(value) => {
                    let _ = value;
                    if let Some(value_child) = &value.ty {
                        self.transport_type(value_child, depth + 1)?;
                    }
                }
                Scan::CorePattern(value) => {
                    let _ = value;
                    {
                        let child_path = self.child_path(&path, &[0])?;
                        self.reserve(64)?;
                        pending.push((Scan::CorePatternKind(&value.kind), depth + 1, child_path));
                    }
                }
                Scan::CorePatternKind(value) => {
                    let _ = value;
                    match value {
                        CorePatternKind::Name(_) => {}
                        CorePatternKind::Wildcard => {}
                        CorePatternKind::Tuple(value_0) => {
                            for (index, value_child) in (value_0).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[2, 0, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CorePattern(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CorePatternKind::Record { ty, fields } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[3, 1, index, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CorePattern(&(value_child).1),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            self.transport_type(ty, depth + 1)?;
                        }
                        CorePatternKind::Reference {
                            mutable: _,
                            pattern,
                        } => {
                            let child_path = self.child_path(&path, &[4, 1])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CorePattern((pattern).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CorePatternKind::ErrorArm {
                            is_error: _,
                            pattern,
                        } => {
                            let child_path = self.child_path(&path, &[5, 1])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CorePattern((pattern).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CorePatternKind::Variant {
                            ty,
                            variant: _,
                            fields,
                        } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[6, 2, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CorePattern(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            self.transport_type(ty, depth + 1)?;
                        }
                        CorePatternKind::Integer(_) => {}
                        CorePatternKind::Bool(_) => {}
                        CorePatternKind::Constructor {
                            definition: _,
                            variant: _,
                            fields,
                        } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[9, 2, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CorePattern(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CorePatternKind::RecordConstructor {
                            definition: _,
                            fields,
                        } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[10, 1, index, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CorePattern(&(value_child).1),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CorePatternKind::Float(_) => {}
                        CorePatternKind::String(_) => {}
                        CorePatternKind::Character(_) => {}
                    }
                }
                Scan::CoreMatchArm(value) => {
                    let _ = value;
                    {
                        let child_path = self.child_path(&path, &[1])?;
                        self.reserve(64)?;
                        pending.push((Scan::CoreBlock(&value.body), depth + 1, child_path));
                    }
                    {
                        let child_path = self.child_path(&path, &[0])?;
                        self.reserve(64)?;
                        pending.push((Scan::CorePattern(&value.pattern), depth + 1, child_path));
                    }
                }
                Scan::CoreBlock(value) => {
                    let _ = value;
                    for (index, value_child) in (&value.statements).iter().enumerate().rev() {
                        {
                            let child_path = self.child_path(&path, &[0, index])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreStatement(value_child), depth + 1, child_path));
                        }
                    }
                }
                Scan::CoreStatement(value) => {
                    let _ = value;
                    self.reserve(24 * std::mem::size_of::<(NodeId, usize)>())?;
                    if expressions.contains_key(&value.id)
                        || statements.insert(value.id, value).is_some()
                    {
                        return Err(invalid("duplicate actual Core identity", value.span));
                    }
                    self.rows::<u32>(path.len())?;
                    paths.insert(value.id, path.clone());
                    {
                        let child_path = self.child_path(&path, &[1])?;
                        self.reserve(64)?;
                        pending.push((Scan::CoreStatementKind(&value.kind), depth + 1, child_path));
                    }
                }
                Scan::CoreStatementKind(value) => {
                    let _ = value;
                    match value {
                        CoreStatementKind::Binding {
                            name: _,
                            mutable: _,
                            ty,
                            initializer,
                        } => {
                            if let Some(value_child) = initializer {
                                {
                                    let child_path = self.child_path(&path, &[0, 3])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            if let Some(value_child) = ty {
                                self.transport_type(value_child, depth + 1)?;
                            }
                        }
                        CoreStatementKind::TupleBinding {
                            pattern,
                            mutable: _,
                            ty,
                            initializer,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[1, 3])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(initializer), depth + 1, child_path));
                            }
                            if let Some(value_child) = ty {
                                self.transport_type(value_child, depth + 1)?;
                            }
                            {
                                let child_path = self.child_path(&path, &[1, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CorePattern(pattern), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::Assign { target, value } => {
                            {
                                let child_path = self.child_path(&path, &[2, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(value), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[2, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(target), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::Expression(value_0) => {
                            let child_path = self.child_path(&path, &[3, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreExpr(value_0), depth + 1, child_path));
                        }
                        CoreStatementKind::Return(value_0) => {
                            if let Some(value_child) = value_0 {
                                {
                                    let child_path = self.child_path(&path, &[4, 0])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CoreStatementKind::If {
                            condition,
                            then_body,
                            else_body,
                        } => {
                            if let Some(value_child) = else_body {
                                {
                                    let child_path = self.child_path(&path, &[5, 2])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreBlock(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[5, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(then_body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[5, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(condition), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::IfLet {
                            pattern,
                            value,
                            then_body,
                            else_body,
                        } => {
                            if let Some(value_child) = else_body {
                                {
                                    let child_path = self.child_path(&path, &[6, 3])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreBlock(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[6, 2])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(then_body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[6, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(value), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[6, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CorePattern(pattern), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::While { condition, body } => {
                            {
                                let child_path = self.child_path(&path, &[7, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[7, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(condition), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::WhileLet {
                            pattern,
                            value,
                            body,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[8, 2])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[8, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(value), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[8, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CorePattern(pattern), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::ForRange {
                            name: _,
                            start,
                            end,
                            body,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[9, 3])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[9, 2])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(end), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[9, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(start), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::Match { value, arms } => {
                            for (index, value_child) in (arms).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[10, 1, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreMatchArm(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[10, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(value), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::Block(value_0) => {
                            let child_path = self.child_path(&path, &[11, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreBlock(value_0), depth + 1, child_path));
                        }
                        CoreStatementKind::Defer(value_0) => {
                            let child_path = self.child_path(&path, &[12, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreBlock(value_0), depth + 1, child_path));
                        }
                        CoreStatementKind::Unsafe(value_0) => {
                            let child_path = self.child_path(&path, &[13, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreBlock(value_0), depth + 1, child_path));
                        }
                        CoreStatementKind::Break => {}
                        CoreStatementKind::Continue => {}
                        CoreStatementKind::Unsupported(_) => {}
                        CoreStatementKind::ForEach {
                            pattern,
                            iterator,
                            body,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[17, 2])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[17, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(iterator), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[17, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CorePattern(pattern), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::CompoundAssign {
                            target,
                            op: _,
                            value,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[18, 2])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(value), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[18, 0])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(target), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::Yield(value_0) => {
                            let child_path = self.child_path(&path, &[19, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreExpr(value_0), depth + 1, child_path));
                        }
                        CoreStatementKind::Loop(value_0) => {
                            let child_path = self.child_path(&path, &[20, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreBlock(value_0), depth + 1, child_path));
                        }
                        CoreStatementKind::With {
                            name: _,
                            value,
                            body,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[21, 2])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(body), depth + 1, child_path));
                            }
                            {
                                let child_path = self.child_path(&path, &[21, 1])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreExpr(value), depth + 1, child_path));
                            }
                        }
                        CoreStatementKind::CompileTime(value_0) => {
                            let child_path = self.child_path(&path, &[22, 0])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreStatement((value_0).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                    }
                }
                Scan::CoreExpr(value) => {
                    let _ = value;
                    self.reserve(24 * std::mem::size_of::<(NodeId, usize)>())?;
                    if statements.contains_key(&value.id)
                        || expressions.insert(value.id, value).is_some()
                    {
                        return Err(invalid("duplicate actual Core identity", value.span));
                    }
                    self.rows::<u32>(path.len())?;
                    paths.insert(value.id, path.clone());
                    {
                        let child_path = self.child_path(&path, &[1])?;
                        self.reserve(64)?;
                        pending.push((Scan::CoreExprKind(&value.kind), depth + 1, child_path));
                    }
                }
                Scan::CoreExprKind(value) => {
                    let _ = value;
                    match value {
                        CoreExprKind::MakeDynBorrow {
                            interface: _,
                            vtable: _,
                            source,
                            mutable: _,
                        } => {
                            let child_path = self.child_path(&path, &[0, 2])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((source).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::MakeDynOwned {
                            interface: _,
                            vtable: _,
                            source,
                            route: _,
                        } => {
                            let child_path = self.child_path(&path, &[1, 2])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((source).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::CallDynSlot {
                            callee,
                            interface: _,
                            slot: _,
                            arguments,
                        } => {
                            for (index, value_child) in (arguments).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[2, 3, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[2, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((callee).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Literal(_) => {}
                        CoreExprKind::Ascribed { ty, value } => {
                            {
                                let child_path = self.child_path(&path, &[4, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((value).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            self.transport_type(ty, depth + 1)?;
                        }
                        CoreExprKind::Integer(_) => {}
                        CoreExprKind::Float(_) => {}
                        CoreExprKind::NumericConvert { value, target } => {
                            self.transport_type(target, depth + 1)?;
                            {
                                let child_path = self.child_path(&path, &[7, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((value).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::IntegerConvert { value, target } => {
                            self.transport_type(target, depth + 1)?;
                            {
                                let child_path = self.child_path(&path, &[8, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((value).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Bool(_) => {}
                        CoreExprKind::String(_) => {}
                        CoreExprKind::Unit => {}
                        CoreExprKind::Name(_) => {}
                        CoreExprKind::Unary { op: _, operand } => {
                            let child_path = self.child_path(&path, &[13, 1])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((operand).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::Binary { op: _, left, right } => {
                            {
                                let child_path = self.child_path(&path, &[14, 2])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((right).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[14, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((left).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::TypeApply {
                            callee,
                            type_arguments,
                        } => {
                            for (index, value_child) in (type_arguments).iter().enumerate().rev() {
                                self.transport_type(value_child, depth + 1)?;
                            }
                            {
                                let child_path = self.child_path(&path, &[15, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((callee).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Call { callee, arguments } => {
                            for (index, value_child) in (arguments).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[16, 1, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[16, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((callee).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Tuple(value_0) => {
                            for (index, value_child) in (value_0).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[17, 0, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CoreExprKind::Array(value_0) => {
                            for (index, value_child) in (value_0).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[18, 0, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CoreExprKind::Index { object, index } => {
                            {
                                let child_path = self.child_path(&path, &[19, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((index).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[19, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((object).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Slice { object, start, end } => {
                            if let Some(value_child) = end {
                                {
                                    let child_path = self.child_path(&path, &[20, 2])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr((value_child).as_ref()),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            if let Some(value_child) = start {
                                {
                                    let child_path = self.child_path(&path, &[20, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr((value_child).as_ref()),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[20, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((object).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Member {
                            object,
                            member: _,
                            owner,
                        } => {
                            if let Some(value_child) = owner {
                                self.transport_type(value_child, depth + 1)?;
                            }
                            {
                                let child_path = self.child_path(&path, &[21, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((object).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::ReceiverMember(_) => {}
                        CoreExprKind::Record { ty, fields } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[23, 1, index, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(&(value_child).1),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            self.transport_type(ty, depth + 1)?;
                        }
                        CoreExprKind::RecordUpdate { ty, base, fields } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[24, 2, index, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(&(value_child).1),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[24, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((base).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            if let Some(value_child) = ty {
                                self.transport_type(value_child, depth + 1)?;
                            }
                        }
                        CoreExprKind::Sequence { bindings, value } => {
                            {
                                let child_path = self.child_path(&path, &[25, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((value).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            for (index, value_child) in (bindings).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[25, 0, index, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(&(value_child).1),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CoreExprKind::CompileTime(value_0) => {
                            let child_path = self.child_path(&path, &[26, 0])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((value_0).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::Try(value_0) => {
                            let child_path = self.child_path(&path, &[27, 0])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((value_0).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::Closure {
                            move_capture: _,
                            parameters,
                            return_type,
                            body,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[28, 3])?;
                                self.reserve(64)?;
                                pending.push((Scan::CoreBlock(body), depth + 1, child_path));
                            }
                            if let Some(value_child) = return_type {
                                self.transport_type(value_child, depth + 1)?;
                            }
                            for (index, value_child) in (parameters).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[28, 1, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreClosureParameter(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CoreExprKind::If {
                            condition,
                            then_value,
                            else_value,
                        } => {
                            {
                                let child_path = self.child_path(&path, &[29, 2])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((else_value).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[29, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((then_value).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[29, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((condition).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Unsupported(_) => {}
                        CoreExprKind::Spread(value_0) => {
                            let child_path = self.child_path(&path, &[31, 0])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((value_0).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::Group(value_0) => {
                            let child_path = self.child_path(&path, &[32, 0])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((value_0).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::Character(_) => {}
                        CoreExprKind::Range { start, end } => {
                            {
                                let child_path = self.child_path(&path, &[34, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((end).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[34, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((start).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::Pipeline { input, target } => {
                            {
                                let child_path = self.child_path(&path, &[35, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((target).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[35, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((input).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::RecordConstructor {
                            definition: _,
                            fields,
                        } => {
                            for (index, value_child) in (fields).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[36, 1, index, 1])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreExpr(&(value_child).1),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                        }
                        CoreExprKind::Splice(value_0) => {
                            let child_path = self.child_path(&path, &[37, 0])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreExpr((value_0).as_ref()),
                                depth + 1,
                                child_path,
                            ));
                        }
                        CoreExprKind::CompileTimeBlock(value_0) => {
                            let child_path = self.child_path(&path, &[38, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreBlock(value_0), depth + 1, child_path));
                        }
                        CoreExprKind::SyntaxQuote { category, template } => {
                            {
                                let child_path = self.child_path(&path, &[39, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreQuoteTemplate(template),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[39, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreSyntaxCategory(category),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                        CoreExprKind::SyntaxIsland { path: _, tree } => {
                            let child_path = self.child_path(&path, &[40, 1])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreSyntaxTree(tree), depth + 1, child_path));
                        }
                        CoreExprKind::GenericApply { callee, arguments } => {
                            for (index, value_child) in (arguments).iter().enumerate().rev() {
                                {
                                    let child_path = self.child_path(&path, &[41, 1, index])?;
                                    self.reserve(64)?;
                                    pending.push((
                                        Scan::CoreGenericArgument(value_child),
                                        depth + 1,
                                        child_path,
                                    ));
                                }
                            }
                            {
                                let child_path = self.child_path(&path, &[41, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreExpr((callee).as_ref()),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                    }
                }
                Scan::CoreGenericArgument(value) => {
                    let _ = value;
                    match value {
                        CoreGenericArgument::Type(value_0) => {
                            self.transport_type(value_0, depth + 1)?;
                        }
                        CoreGenericArgument::Const(value_0) => {
                            let child_path = self.child_path(&path, &[1, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreExpr(value_0), depth + 1, child_path));
                        }
                        CoreGenericArgument::Lifetime(_) => {}
                    }
                }
                Scan::CoreSyntaxCategory(value) => {
                    let _ = value;
                    match value {
                        CoreSyntaxCategory::Expr => {}
                        CoreSyntaxCategory::Type => {}
                        CoreSyntaxCategory::StmtList => {}
                        CoreSyntaxCategory::ItemList => {}
                    }
                }
                Scan::CoreSyntaxDelimiter(value) => {
                    let _ = value;
                    match value {
                        CoreSyntaxDelimiter::Parenthesis => {}
                        CoreSyntaxDelimiter::Bracket => {}
                        CoreSyntaxDelimiter::Brace => {}
                    }
                }
                Scan::CoreQuoteNameNamespace(value) => {
                    let _ = value;
                    match value {
                        CoreQuoteNameNamespace::Value => {}
                        CoreQuoteNameNamespace::Type => {}
                        CoreQuoteNameNamespace::Member => {}
                        CoreQuoteNameNamespace::Module => {}
                        CoreQuoteNameNamespace::Label => {}
                    }
                }
                Scan::CoreQuoteNameRole(value) => {
                    let _ = value;
                    match value {
                        CoreQuoteNameRole::Binding => {}
                        CoreQuoteNameRole::Reference => {}
                    }
                }
                Scan::CoreQuoteHoleKind(value) => {
                    let _ = value;
                    match value {
                        CoreQuoteHoleKind::Expr => {}
                        CoreQuoteHoleKind::Type => {}
                        CoreQuoteHoleKind::StmtList => {}
                        CoreQuoteHoleKind::ItemList => {}
                        CoreQuoteHoleKind::Ident { namespace, role } => {
                            {
                                let child_path = self.child_path(&path, &[4, 1])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreQuoteNameRole(role),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                            {
                                let child_path = self.child_path(&path, &[4, 0])?;
                                self.reserve(64)?;
                                pending.push((
                                    Scan::CoreQuoteNameNamespace(namespace),
                                    depth + 1,
                                    child_path,
                                ));
                            }
                        }
                    }
                }
                Scan::CoreQuoteHole(value) => {
                    let _ = value;
                    {
                        let child_path = self.child_path(&path, &[1])?;
                        self.reserve(64)?;
                        pending.push((
                            Scan::CoreExpr((&value.value).as_ref()),
                            depth + 1,
                            child_path,
                        ));
                    }
                    {
                        let child_path = self.child_path(&path, &[0])?;
                        self.reserve(64)?;
                        pending.push((Scan::CoreQuoteHoleKind(&value.kind), depth + 1, child_path));
                    }
                }
                Scan::CoreQuoteTemplate(value) => {
                    let _ = value;
                    for (index, value_child) in (&value.name_sites).iter().enumerate().rev() {
                        {
                            let child_path = self.child_path(&path, &[2, index])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreQuoteNameSite(value_child),
                                depth + 1,
                                child_path,
                            ));
                        }
                    }
                    for (index, value_child) in (&value.holes).iter().enumerate().rev() {
                        {
                            let child_path = self.child_path(&path, &[1, index])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreQuoteHole(value_child), depth + 1, child_path));
                        }
                    }
                    {
                        let child_path = self.child_path(&path, &[0])?;
                        self.reserve(64)?;
                        pending.push((Scan::CoreSyntaxTree(&value.tree), depth + 1, child_path));
                    }
                }
                Scan::CoreQuoteNameSite(value) => {
                    let _ = value;
                }
                Scan::CoreSyntaxTree(value) => {
                    let _ = value;
                    for (index, value_child) in (&value.children).iter().enumerate().rev() {
                        {
                            let child_path = self.child_path(&path, &[1, index])?;
                            self.reserve(64)?;
                            pending.push((
                                Scan::CoreSyntaxTreeEntry(value_child),
                                depth + 1,
                                child_path,
                            ));
                        }
                    }
                    {
                        let child_path = self.child_path(&path, &[0])?;
                        self.reserve(64)?;
                        pending.push((
                            Scan::CoreSyntaxDelimiter(&value.delimiter),
                            depth + 1,
                            child_path,
                        ));
                    }
                }
                Scan::CoreSyntaxTreeEntry(value) => {
                    let _ = value;
                    match value {
                        CoreSyntaxTreeEntry::Token { text: _, span: _ } => {}
                        CoreSyntaxTreeEntry::Tree(value_0) => {
                            let child_path = self.child_path(&path, &[1, 0])?;
                            self.reserve(64)?;
                            pending.push((Scan::CoreSyntaxTree(value_0), depth + 1, child_path));
                        }
                        CoreSyntaxTreeEntry::Hole { index: _, span: _ } => {}
                    }
                }
            }
        }
        Ok(NodeIndex {
            expressions,
            statements,
            paths,
        })
    }
}

struct OriginalNodeIndex<'a> {
    expressions: BTreeMap<NodeId, &'a SemanticOriginalExpr>,
    statements: BTreeMap<NodeId, &'a SemanticOriginalStatement>,
}
enum OriginalScan<'a> {
    CoreClosureParameter(&'a SemanticOriginalClosureParameter),
    CorePattern(&'a SemanticOriginalPattern),
    CorePatternKind(&'a SemanticOriginalPatternKind),
    CoreMatchArm(&'a SemanticOriginalMatchArm),
    CoreBlock(&'a SemanticOriginalBlock),
    CoreStatement(&'a SemanticOriginalStatement),
    CoreStatementKind(&'a SemanticOriginalStatementKind),
    CoreExpr(&'a SemanticOriginalExpr),
    CoreExprKind(&'a SemanticOriginalExprKind),
    CoreGenericArgument(&'a SemanticOriginalGenericArgument),
    CoreSyntaxCategory(&'a SemanticOriginalSyntaxCategory),
    CoreSyntaxDelimiter(&'a SemanticOriginalSyntaxDelimiter),
    CoreQuoteNameNamespace(&'a SemanticOriginalQuoteNameNamespace),
    CoreQuoteNameRole(&'a SemanticOriginalQuoteNameRole),
    CoreQuoteHoleKind(&'a SemanticOriginalQuoteHoleKind),
    CoreQuoteHole(&'a SemanticOriginalQuoteHole),
    CoreQuoteTemplate(&'a SemanticOriginalQuoteTemplate),
    CoreQuoteNameSite(&'a SemanticOriginalQuoteNameSite),
    CoreSyntaxTree(&'a SemanticOriginalSyntaxTree),
    CoreSyntaxTreeEntry(&'a SemanticOriginalSyntaxTreeEntry),
}
impl Builder<'_, '_> {
    fn original_nodes<'a>(
        &mut self,
        root: &'a SemanticOriginalBlock,
    ) -> CompileResult<OriginalNodeIndex<'a>> {
        self.reserve(64)?;
        let mut pending = vec![(OriginalScan::CoreBlock(root), 0)];
        let mut expressions = BTreeMap::new();
        let mut statements = BTreeMap::new();
        while let Some((next, depth)) = pending.pop() {
            self.node(depth)?;
            match next {
                OriginalScan::CoreClosureParameter(value) => {
                    let _ = value;
                }
                OriginalScan::CorePattern(value) => {
                    let _ = value;

                    self.reserve(64)?;
                    pending.push((OriginalScan::CorePatternKind(&value.kind), depth + 1));
                }
                OriginalScan::CorePatternKind(value) => {
                    let _ = value;
                    match value {
                        CorePatternKind::Name(_) => {}
                        CorePatternKind::Wildcard => {}
                        CorePatternKind::Tuple(value_0) => {
                            for child in (value_0).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CorePattern(child), depth + 1));
                            }
                        }
                        CorePatternKind::Record { ty: _, fields } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CorePattern(&(child).1), depth + 1));
                            }
                        }
                        CorePatternKind::Reference {
                            mutable: _,
                            pattern,
                        } => {
                            self.reserve(64)?;
                            pending
                                .push((OriginalScan::CorePattern((pattern).as_ref()), depth + 1));
                        }
                        CorePatternKind::ErrorArm {
                            is_error: _,
                            pattern,
                        } => {
                            self.reserve(64)?;
                            pending
                                .push((OriginalScan::CorePattern((pattern).as_ref()), depth + 1));
                        }
                        CorePatternKind::Variant {
                            ty: _,
                            variant: _,
                            fields,
                        } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CorePattern(child), depth + 1));
                            }
                        }
                        CorePatternKind::Integer(_) => {}
                        CorePatternKind::Bool(_) => {}
                        CorePatternKind::Constructor {
                            definition: _,
                            variant: _,
                            fields,
                        } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CorePattern(child), depth + 1));
                            }
                        }
                        CorePatternKind::RecordConstructor {
                            definition: _,
                            fields,
                        } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CorePattern(&(child).1), depth + 1));
                            }
                        }
                        CorePatternKind::Float(_) => {}
                        CorePatternKind::String(_) => {}
                        CorePatternKind::Character(_) => {}
                    }
                }
                OriginalScan::CoreMatchArm(value) => {
                    let _ = value;

                    self.reserve(64)?;
                    pending.push((OriginalScan::CoreBlock(&value.body), depth + 1));
                    self.reserve(64)?;
                    pending.push((OriginalScan::CorePattern(&value.pattern), depth + 1));
                }
                OriginalScan::CoreBlock(value) => {
                    let _ = value;

                    for child in (&value.statements).iter().rev() {
                        self.reserve(64)?;
                        pending.push((OriginalScan::CoreStatement(child), depth + 1));
                    }
                }
                OriginalScan::CoreStatement(value) => {
                    let _ = value;
                    self.reserve(12 * std::mem::size_of::<(NodeId, usize)>())?;
                    if expressions.contains_key(&value.id)
                        || statements.insert(value.id, value).is_some()
                    {
                        return Err(invalid(
                            "duplicate Core operation identity in its actual creator",
                            value.span,
                        ));
                    }

                    self.reserve(64)?;
                    pending.push((OriginalScan::CoreStatementKind(&value.kind), depth + 1));
                }
                OriginalScan::CoreStatementKind(value) => {
                    let _ = value;
                    match value {
                        CoreStatementKind::Binding {
                            name: _,
                            mutable: _,
                            ty: _,
                            initializer,
                        } => {
                            if let Some(child) = initializer {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(child), depth + 1));
                            }
                        }
                        CoreStatementKind::TupleBinding {
                            pattern,
                            mutable: _,
                            ty: _,
                            initializer,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(initializer), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CorePattern(pattern), depth + 1));
                        }
                        CoreStatementKind::Assign { target, value } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(target), depth + 1));
                        }
                        CoreStatementKind::Expression(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value_0), depth + 1));
                        }
                        CoreStatementKind::Return(value_0) => {
                            if let Some(child) = value_0 {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(child), depth + 1));
                            }
                        }
                        CoreStatementKind::If {
                            condition,
                            then_body,
                            else_body,
                        } => {
                            if let Some(child) = else_body {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreBlock(child), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(then_body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(condition), depth + 1));
                        }
                        CoreStatementKind::IfLet {
                            pattern,
                            value,
                            then_body,
                            else_body,
                        } => {
                            if let Some(child) = else_body {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreBlock(child), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(then_body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CorePattern(pattern), depth + 1));
                        }
                        CoreStatementKind::While { condition, body } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(condition), depth + 1));
                        }
                        CoreStatementKind::WhileLet {
                            pattern,
                            value,
                            body,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CorePattern(pattern), depth + 1));
                        }
                        CoreStatementKind::ForRange {
                            name: _,
                            start,
                            end,
                            body,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(end), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(start), depth + 1));
                        }
                        CoreStatementKind::Match { value, arms } => {
                            for child in (arms).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreMatchArm(child), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value), depth + 1));
                        }
                        CoreStatementKind::Block(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(value_0), depth + 1));
                        }
                        CoreStatementKind::Defer(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(value_0), depth + 1));
                        }
                        CoreStatementKind::Unsafe(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(value_0), depth + 1));
                        }
                        CoreStatementKind::Break => {}
                        CoreStatementKind::Continue => {}
                        CoreStatementKind::Unsupported(_) => {}
                        CoreStatementKind::ForEach {
                            pattern,
                            iterator,
                            body,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(iterator), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CorePattern(pattern), depth + 1));
                        }
                        CoreStatementKind::CompoundAssign {
                            target,
                            op: _,
                            value,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(target), depth + 1));
                        }
                        CoreStatementKind::Yield(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value_0), depth + 1));
                        }
                        CoreStatementKind::Loop(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(value_0), depth + 1));
                        }
                        CoreStatementKind::With {
                            name: _,
                            value,
                            body,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(body), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value), depth + 1));
                        }
                        CoreStatementKind::CompileTime(value_0) => {
                            self.reserve(64)?;
                            pending
                                .push((OriginalScan::CoreStatement((value_0).as_ref()), depth + 1));
                        }
                    }
                }
                OriginalScan::CoreExpr(value) => {
                    let _ = value;
                    self.reserve(12 * std::mem::size_of::<(NodeId, usize)>())?;
                    if statements.contains_key(&value.id)
                        || expressions.insert(value.id, value).is_some()
                    {
                        return Err(invalid(
                            "duplicate Core operation identity in its actual creator",
                            value.span,
                        ));
                    }

                    self.reserve(64)?;
                    pending.push((OriginalScan::CoreExprKind(&value.kind), depth + 1));
                }
                OriginalScan::CoreExprKind(value) => {
                    let _ = value;
                    match value {
                        CoreExprKind::MakeDynBorrow {
                            interface: _,
                            vtable: _,
                            source,
                            mutable: _,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((source).as_ref()), depth + 1));
                        }
                        CoreExprKind::MakeDynOwned {
                            interface: _,
                            vtable: _,
                            source,
                            route: _,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((source).as_ref()), depth + 1));
                        }
                        CoreExprKind::CallDynSlot {
                            callee,
                            interface: _,
                            slot: _,
                            arguments,
                        } => {
                            for child in (arguments).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(child), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((callee).as_ref()), depth + 1));
                        }
                        CoreExprKind::Literal(_) => {}
                        CoreExprKind::Ascribed { ty: _, value } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value).as_ref()), depth + 1));
                        }
                        CoreExprKind::Integer(_) => {}
                        CoreExprKind::Float(_) => {}
                        CoreExprKind::NumericConvert { value, target: _ } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value).as_ref()), depth + 1));
                        }
                        CoreExprKind::IntegerConvert { value, target: _ } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value).as_ref()), depth + 1));
                        }
                        CoreExprKind::Bool(_) => {}
                        CoreExprKind::String(_) => {}
                        CoreExprKind::Unit => {}
                        CoreExprKind::Name(_) => {}
                        CoreExprKind::Unary { op: _, operand } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((operand).as_ref()), depth + 1));
                        }
                        CoreExprKind::Binary { op: _, left, right } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((right).as_ref()), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((left).as_ref()), depth + 1));
                        }
                        CoreExprKind::TypeApply {
                            callee,
                            type_arguments: _,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((callee).as_ref()), depth + 1));
                        }
                        CoreExprKind::Call { callee, arguments } => {
                            for child in (arguments).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(child), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((callee).as_ref()), depth + 1));
                        }
                        CoreExprKind::Tuple(value_0) => {
                            for child in (value_0).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(child), depth + 1));
                            }
                        }
                        CoreExprKind::Array(value_0) => {
                            for child in (value_0).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(child), depth + 1));
                            }
                        }
                        CoreExprKind::Index { object, index } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((index).as_ref()), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((object).as_ref()), depth + 1));
                        }
                        CoreExprKind::Slice { object, start, end } => {
                            if let Some(child) = end {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr((child).as_ref()), depth + 1));
                            }
                            if let Some(child) = start {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr((child).as_ref()), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((object).as_ref()), depth + 1));
                        }
                        CoreExprKind::Member {
                            object,
                            member: _,
                            owner: _,
                        } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((object).as_ref()), depth + 1));
                        }
                        CoreExprKind::ReceiverMember(_) => {}
                        CoreExprKind::Record { ty: _, fields } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(&(child).1), depth + 1));
                            }
                        }
                        CoreExprKind::RecordUpdate {
                            ty: _,
                            base,
                            fields,
                        } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(&(child).1), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((base).as_ref()), depth + 1));
                        }
                        CoreExprKind::Sequence { bindings, value } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value).as_ref()), depth + 1));
                            for child in (bindings).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(&(child).1), depth + 1));
                            }
                        }
                        CoreExprKind::CompileTime(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value_0).as_ref()), depth + 1));
                        }
                        CoreExprKind::Try(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value_0).as_ref()), depth + 1));
                        }
                        CoreExprKind::Closure {
                            move_capture: _,
                            parameters,
                            return_type: _,
                            body: _,
                        } => {
                            for child in (parameters).iter().rev() {
                                self.reserve(64)?;
                                pending
                                    .push((OriginalScan::CoreClosureParameter(child), depth + 1));
                            }
                        }
                        CoreExprKind::If {
                            condition,
                            then_value,
                            else_value,
                        } => {
                            self.reserve(64)?;
                            pending
                                .push((OriginalScan::CoreExpr((else_value).as_ref()), depth + 1));
                            self.reserve(64)?;
                            pending
                                .push((OriginalScan::CoreExpr((then_value).as_ref()), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((condition).as_ref()), depth + 1));
                        }
                        CoreExprKind::Unsupported(_) => {}
                        CoreExprKind::Spread(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value_0).as_ref()), depth + 1));
                        }
                        CoreExprKind::Group(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value_0).as_ref()), depth + 1));
                        }
                        CoreExprKind::Character(_) => {}
                        CoreExprKind::Range { start, end } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((end).as_ref()), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((start).as_ref()), depth + 1));
                        }
                        CoreExprKind::Pipeline { input, target } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((target).as_ref()), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((input).as_ref()), depth + 1));
                        }
                        CoreExprKind::RecordConstructor {
                            definition: _,
                            fields,
                        } => {
                            for child in (fields).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreExpr(&(child).1), depth + 1));
                            }
                        }
                        CoreExprKind::Splice(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((value_0).as_ref()), depth + 1));
                        }
                        CoreExprKind::CompileTimeBlock(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreBlock(value_0), depth + 1));
                        }
                        CoreExprKind::SyntaxQuote { category, template } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreQuoteTemplate(template), depth + 1));
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreSyntaxCategory(category), depth + 1));
                        }
                        CoreExprKind::SyntaxIsland { path: _, tree } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreSyntaxTree(tree), depth + 1));
                        }
                        CoreExprKind::GenericApply { callee, arguments } => {
                            for child in (arguments).iter().rev() {
                                self.reserve(64)?;
                                pending.push((OriginalScan::CoreGenericArgument(child), depth + 1));
                            }
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr((callee).as_ref()), depth + 1));
                        }
                    }
                }
                OriginalScan::CoreGenericArgument(value) => {
                    let _ = value;
                    match value {
                        CoreGenericArgument::Type(_) => {}
                        CoreGenericArgument::Const(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreExpr(value_0), depth + 1));
                        }
                        CoreGenericArgument::Lifetime(_) => {}
                    }
                }
                OriginalScan::CoreSyntaxCategory(value) => {
                    let _ = value;
                    match value {
                        CoreSyntaxCategory::Expr => {}
                        CoreSyntaxCategory::Type => {}
                        CoreSyntaxCategory::StmtList => {}
                        CoreSyntaxCategory::ItemList => {}
                    }
                }
                OriginalScan::CoreSyntaxDelimiter(value) => {
                    let _ = value;
                    match value {
                        CoreSyntaxDelimiter::Parenthesis => {}
                        CoreSyntaxDelimiter::Bracket => {}
                        CoreSyntaxDelimiter::Brace => {}
                    }
                }
                OriginalScan::CoreQuoteNameNamespace(value) => {
                    let _ = value;
                    match value {
                        CoreQuoteNameNamespace::Value => {}
                        CoreQuoteNameNamespace::Type => {}
                        CoreQuoteNameNamespace::Member => {}
                        CoreQuoteNameNamespace::Module => {}
                        CoreQuoteNameNamespace::Label => {}
                    }
                }
                OriginalScan::CoreQuoteNameRole(value) => {
                    let _ = value;
                    match value {
                        CoreQuoteNameRole::Binding => {}
                        CoreQuoteNameRole::Reference => {}
                    }
                }
                OriginalScan::CoreQuoteHoleKind(value) => {
                    let _ = value;
                    match value {
                        CoreQuoteHoleKind::Expr => {}
                        CoreQuoteHoleKind::Type => {}
                        CoreQuoteHoleKind::StmtList => {}
                        CoreQuoteHoleKind::ItemList => {}
                        CoreQuoteHoleKind::Ident { namespace, role } => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreQuoteNameRole(role), depth + 1));
                            self.reserve(64)?;
                            pending
                                .push((OriginalScan::CoreQuoteNameNamespace(namespace), depth + 1));
                        }
                    }
                }
                OriginalScan::CoreQuoteHole(value) => {
                    let _ = value;

                    self.reserve(64)?;
                    pending.push((OriginalScan::CoreExpr((&value.value).as_ref()), depth + 1));
                    self.reserve(64)?;
                    pending.push((OriginalScan::CoreQuoteHoleKind(&value.kind), depth + 1));
                }
                OriginalScan::CoreQuoteTemplate(value) => {
                    let _ = value;
                    for child in (&value.name_sites).iter().rev() {
                        self.reserve(64)?;
                        pending.push((OriginalScan::CoreQuoteNameSite(child), depth + 1));
                    }
                    for child in (&value.holes).iter().rev() {
                        self.reserve(64)?;
                        pending.push((OriginalScan::CoreQuoteHole(child), depth + 1));
                    }
                    self.reserve(64)?;
                    pending.push((OriginalScan::CoreSyntaxTree(&value.tree), depth + 1));
                }
                OriginalScan::CoreQuoteNameSite(value) => {
                    let _ = value;
                }
                OriginalScan::CoreSyntaxTree(value) => {
                    let _ = value;

                    for child in (&value.children).iter().rev() {
                        self.reserve(64)?;
                        pending.push((OriginalScan::CoreSyntaxTreeEntry(child), depth + 1));
                    }
                    self.reserve(64)?;
                    pending.push((
                        OriginalScan::CoreSyntaxDelimiter(&value.delimiter),
                        depth + 1,
                    ));
                }
                OriginalScan::CoreSyntaxTreeEntry(value) => {
                    let _ = value;
                    match value {
                        CoreSyntaxTreeEntry::Token { text: _, span: _ } => {}
                        CoreSyntaxTreeEntry::Tree(value_0) => {
                            self.reserve(64)?;
                            pending.push((OriginalScan::CoreSyntaxTree(value_0), depth + 1));
                        }
                        CoreSyntaxTreeEntry::Hole { index: _, span: _ } => {}
                    }
                }
            }
        }
        Ok(OriginalNodeIndex {
            expressions,
            statements,
        })
    }
}

/// Exact source identity of the compiler-created wrapper. These are raw
/// selectors, never a completion receipt or executable target in background S.
pub(super) struct QueryOrigin<'a> {
    pub(super) definition: &'a SemanticDefinition,
    pub(super) instance: &'a SemanticInstance,
    pub(super) creator: NodeId,
    pub(super) wrapper: NodeId,
    pub(super) original: NodeId,
}

pub(super) fn query_origin<'p>(
    collector: &mut Collector<'p>,
    module: ModuleId,
    expression: &CoreExpr,
    bytes_left: &mut usize,
) -> CompileResult<Option<QueryOrigin<'p>>> {
    let program = collector.program;
    let source = program
        .modules
        .iter()
        .find(|row| row.core.module_id == module)
        .ok_or_else(|| invalid("CT query root module is absent", expression.span))?;
    let Some(semantic) = source.core.semantic.as_ref() else {
        return Ok(None);
    };
    let mut builder = Builder {
        collector,
        module,
        bytes_left,
    };
    let mut found = None;
    for function in &source.core.functions {
        let nodes = builder.nodes(&function.body)?;
        for candidate in nodes.expressions.values() {
            let CoreExprKind::CompileTime(inner) = &candidate.kind else {
                continue;
            };
            if inner.id != expression.id {
                continue;
            }
            // Both trees have been bounded before full structural equality.
            // An equal span or machine type is never an occurrence selector.
            if inner.as_ref() != expression {
                return Err(invalid(
                    "CT query body differs from its actual wrapper operand",
                    candidate.span,
                ));
            }
            if found.is_some() {
                return Err(invalid(
                    "CT query occurs in multiple actual Core wrappers",
                    candidate.span,
                ));
            }
            let mut exact = None;
            for site in &semantic.sites {
                builder.node(0)?;
                if !matches!(site.at, SemanticSiteAddress::Core { expression, occurrence: 0, role: SemanticEmissionRole::Ascription } if expression == candidate.id)
                {
                    continue;
                }
                let SemanticSiteAction::TypeUse {
                    subject:
                        SemanticOperand::CoreExpression {
                            expression: subject,
                            occurrence: 0,
                        },
                    ..
                } = &site.action
                else {
                    continue;
                };
                if *subject != candidate.id {
                    return Err(invalid(
                        "CT query wrapper type use names another actual expression",
                        candidate.span,
                    ));
                }
                let mut instances = semantic
                    .instances
                    .iter()
                    .filter(|row| row.id == site.instance);
                let instance = instances.next().ok_or_else(|| {
                    invalid("CT wrapper source instance is absent", candidate.span)
                })?;
                if instances.next().is_some() {
                    return Err(invalid(
                        "CT wrapper source instance is duplicated",
                        candidate.span,
                    ));
                }
                let actual_creator = match instance.target {
                    SemanticInstanceTarget::Function(SemanticFunctionTarget::Core {
                        module: owner,
                        function,
                    }) if owner == module => function,
                    SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
                        module: owner,
                        creator,
                        ..
                    }) if owner == module => creator,
                    _ => {
                        return Err(invalid(
                            "CT wrapper has no real Core function owner",
                            candidate.span,
                        ));
                    }
                };
                if actual_creator != function.id {
                    return Err(invalid(
                        "CT wrapper was rebound to another actual creator",
                        candidate.span,
                    ));
                }
                let mut definitions = semantic
                    .definitions
                    .iter()
                    .filter(|row| row.id == instance.definition);
                let definition = definitions.next().ok_or_else(|| {
                    invalid("CT wrapper original definition is absent", candidate.span)
                })?;
                if definitions.next().is_some() {
                    return Err(invalid(
                        "CT wrapper original definition is duplicated",
                        candidate.span,
                    ));
                }
                let SemanticDefinitionBody::FunctionWithBody {
                    sites,
                    site_nodes,
                    body: SemanticOriginalFunctionBody::Defined(body),
                    ..
                } = &definition.body
                else {
                    return Err(invalid(
                        "CT wrapper lacks its complete original body",
                        candidate.span,
                    ));
                };
                let mut recipes = sites.iter().filter(|row| row.id == site.recipe);
                let recipe = recipes.next().ok_or_else(|| {
                    invalid("CT wrapper has no original site recipe", candidate.span)
                })?;
                if recipes.next().is_some() {
                    return Err(invalid(
                        "CT wrapper original recipe is duplicated",
                        candidate.span,
                    ));
                }
                let mut links = site_nodes.iter().filter(|row| row.site == site.recipe);
                let link = links.next().ok_or_else(|| {
                    invalid(
                        "CT wrapper has no exact original operation link",
                        candidate.span,
                    )
                })?;
                if links.next().is_some() {
                    return Err(invalid(
                        "CT wrapper original operation link is duplicated",
                        candidate.span,
                    ));
                }
                let SemanticOriginalNode::Expression(original) = link.node else {
                    return Err(invalid(
                        "CT wrapper original link is not an expression",
                        candidate.span,
                    ));
                };
                if !matches!(&recipe.kind, SemanticSiteRecipeKind::TypeUse { expected: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(node) } } if *node == original)
                {
                    return Err(invalid(
                        "CT wrapper recipe does not select its actual original expression",
                        candidate.span,
                    ));
                }
                let original_nodes = builder.original_nodes(body)?;
                let original_expression =
                    original_nodes.expressions.get(&original).ok_or_else(|| {
                        invalid(
                            "CT wrapper original expression is not in its own lexical body",
                            candidate.span,
                        )
                    })?;
                if !matches!(
                    original_expression.kind,
                    SemanticOriginalExprKind::CompileTime(_)
                ) {
                    return Err(invalid(
                        "CT query origin is not an original compile-time operation",
                        candidate.span,
                    ));
                }
                if exact.is_some() {
                    return Err(invalid(
                        "CT wrapper has multiple original source anchors",
                        candidate.span,
                    ));
                }
                exact = Some(QueryOrigin {
                    definition,
                    instance,
                    creator: function.id,
                    wrapper: candidate.id,
                    original,
                });
            }
            found = Some(exact.ok_or_else(|| {
                invalid(
                    "CT query wrapper has no complete original source anchor",
                    candidate.span,
                )
            })?);
        }
    }
    found.map(Some).ok_or_else(|| {
        invalid(
            "CT query is not the operand of its actual source wrapper",
            expression.span,
        )
    })
}

struct Transport<'a, 'b, 'p> {
    builder: &'a mut Builder<'b, 'p>,
    copy: bool,
}
impl DynRecipeVisitor for Transport<'_, '_, '_> {
    type Error = Vec<Diagnostic>;
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        self.builder.node(depth)?;
        // This is a storage upper only. No represented type is admitted here.
        let width = if self.copy {
            std::mem::size_of::<SemanticTypeRecipe>()
                .saturating_add(std::mem::size_of::<SemanticOriginalExpr>())
        } else {
            64
        };
        self.builder.reserve(width)
    }
    fn rows(&mut self, count: usize, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.builder.collector.text_len(count)?;
        let width = if self.copy {
            std::mem::size_of::<SemanticDefinition>()
        } else {
            64
        };
        self.builder
            .reserve(count.checked_mul(width).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT metadata vector storage overflow",
                    None,
                )]
            })?)
    }
    fn ty(&mut self, value: &Type, depth: usize) -> CompileResult<()> {
        zil_ct::visit_runtime_type(value, self, depth)
    }
    fn life(&mut self, value: &StorageLifeShape, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.builder
            .collector
            .text_len(value.binder_count as usize)?;
        self.rows(
            value
                .slots
                .len()
                .saturating_add(value.retained.len())
                .saturating_add(value.nominal_arguments.len()),
            depth + 1,
        )?;
        for row in &value.slots {
            self.rows(row.path.len(), depth + 1)?;
        }
        for row in &value.retained {
            self.rows(row.path.len(), depth + 1)?;
        }
        for row in &value.nominal_arguments {
            self.rows(row.path.len(), depth + 1)?;
        }
        Ok(())
    }
    fn identity(&mut self, value: &LifeDeclarationIdentity, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.text(&value.package, depth + 1)?;
        self.rows(
            value
                .module_path
                .len()
                .saturating_add(value.declaration_path.len())
                .saturating_add(value.generation_path.len()),
            depth + 1,
        )?;
        for text in value.module_path.iter().chain(&value.declaration_path) {
            self.text(text, depth + 1)?;
        }
        Ok(())
    }
    fn position(&mut self, value: &StoragePosition, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        self.rows(value.path.len(), depth + 1)
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<()> {
        self.node(depth)?;
        if let Constant::WideString(units) = value {
            self.rows(units.len(), depth + 1)?;
        }
        Ok(())
    }
    fn text(&mut self, value: &str, depth: usize) -> CompileResult<()> {
        self.builder.node(depth)?;
        self.builder.collector.text(value)?;
        if self.copy {
            self.builder.reserve(value.len())?;
        }
        Ok(())
    }
}
impl Builder<'_, '_> {
    fn transport_type(&mut self, value: &Type, depth: usize) -> CompileResult<()> {
        zil_ct::visit_runtime_type(
            value,
            &mut Transport {
                builder: self,
                copy: false,
            },
            depth,
        )
    }
}

// Moving a name recipe into the real child wrapper adds exactly one lexical
// frame. Bound/Invocation frames, concrete instance IDs and intrinsic class
// selectors are separate namespaces and are deliberately untouched.
enum Rebase<'a> {
    SemanticOriginalNameValue(&'a mut SemanticOriginalNameValue),
    SemanticScopeRef(&'a mut SemanticScopeRef),
    SemanticRecipeOperand(&'a mut SemanticRecipeOperand),
    SemanticFormalRef(&'a mut SemanticFormalRef),
    SemanticLifeRecipe(&'a mut SemanticLifeRecipe),
    SemanticOuterLifeRecipe(&'a mut SemanticOuterLifeRecipe),
    SemanticTypeRecipe(&'a mut SemanticTypeRecipe),
    SemanticExactTypeRecipe(&'a mut SemanticExactTypeRecipe),
    SemanticActualRecipe(&'a mut SemanticActualRecipe),
    SemanticClassSequenceRecipe(&'a mut SemanticClassSequenceRecipe),
    SemanticClassSequencePart(&'a mut SemanticClassSequencePart),
    SemanticStorageShapeRecipe(&'a mut SemanticStorageShapeRecipe),
    SemanticSlotRecipe(&'a mut SemanticSlotRecipe),
    SemanticRetainedPart(&'a mut SemanticRetainedPart),
    SemanticNominalPart(&'a mut SemanticNominalPart),
    SemanticRetainedShapeRecipe(&'a mut SemanticRetainedShapeRecipe),
    SemanticPortPart(&'a mut SemanticPortPart),
    SemanticConstRecipe(&'a mut SemanticConstRecipe),
    SemanticTraitRecipe(&'a mut SemanticTraitRecipe),
    SemanticSignatureRecipe(&'a mut SemanticSignatureRecipe),
    SemanticParameterRecipe(&'a mut SemanticParameterRecipe),
    SemanticDynContextRecipe(&'a mut SemanticDynContextRecipe),
    SemanticDynInterfaceRecipe(&'a mut SemanticDynInterfaceRecipe),
    SemanticAssociatedBindingRecipe(&'a mut SemanticAssociatedBindingRecipe),
    SemanticAssociatedRecipe(&'a mut SemanticAssociatedRecipe),
}
impl Builder<'_, '_> {
    fn rebase_name(&mut self, value: &mut SemanticOriginalNameValue) -> CompileResult<()> {
        self.reserve(64)?;
        let mut pending = vec![(Rebase::SemanticOriginalNameValue(value), 0)];
        while let Some((next, depth)) = pending.pop() {
            self.node(depth)?;
            match next {
                Rebase::SemanticOriginalNameValue(value) => match value {
                    SemanticOriginalNameValue::Recipe(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticActualRecipe(value_0), depth + 1));
                    }
                    SemanticOriginalNameValue::Definition(_) => {}
                },
                Rebase::SemanticScopeRef(value) => {
                    let SemanticScopeRef::Ancestor(ordinal) = value;
                    *ordinal = ordinal.checked_add(1).ok_or_else(|| {
                        invalid("CT wrapper lexical rebasing overflow", Span::default())
                    })?;
                }
                Rebase::SemanticRecipeOperand(value) => match value {
                    SemanticRecipeOperand::TypeFormal(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticFormalRef(value_0), depth + 1));
                    }
                    SemanticRecipeOperand::Environment { scope, ordinal: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                    SemanticRecipeOperand::Parameter(_) => {}
                    SemanticRecipeOperand::Receiver => {}
                },
                Rebase::SemanticFormalRef(value) => {
                    self.reserve(64)?;
                    pending.push((Rebase::SemanticScopeRef(&mut value.scope), depth + 1));
                }
                Rebase::SemanticLifeRecipe(value) => match value {
                    SemanticLifeRecipe::Formal(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticFormalRef(value_0), depth + 1));
                    }
                    SemanticLifeRecipe::Implicit { scope, ordinal: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                    SemanticLifeRecipe::Environment { scope, ordinal: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                    SemanticLifeRecipe::Bound { depth: _, index: _ } => {}
                    SemanticLifeRecipe::Invocation(_) => {}
                    SemanticLifeRecipe::OuterOccurrence {
                        operand,
                        path: _,
                        mode: _,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticRecipeOperand(operand), depth + 1));
                    }
                    SemanticLifeRecipe::IntrinsicClass {
                        operand,
                        path: _,
                        class: _,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticRecipeOperand(operand), depth + 1));
                    }
                },
                Rebase::SemanticOuterLifeRecipe(value) => match value {
                    SemanticOuterLifeRecipe::Class(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(value_0), depth + 1));
                    }
                    SemanticOuterLifeRecipe::Static => {}
                },
                Rebase::SemanticTypeRecipe(value) => match value {
                    SemanticTypeRecipe::Atom(_) => {}
                    SemanticTypeRecipe::Exact(value_0) => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticExactTypeRecipe((value_0).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::Formal(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticFormalRef(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Environment { scope, ordinal: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                    SemanticTypeRecipe::SelfType { scope } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                    SemanticTypeRecipe::Nominal {
                        definition: _,
                        arguments,
                    } => {
                        for child in arguments {
                            self.reserve(64)?;
                            pending.push((Rebase::SemanticActualRecipe(child), depth + 1));
                        }
                    }
                    SemanticTypeRecipe::AssociatedType {
                        receiver,
                        application,
                        member: _,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((receiver).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticTraitRecipe((application).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::Tuple(value_0) => {
                        for child in value_0 {
                            self.reserve(64)?;
                            pending.push((Rebase::SemanticTypeRecipe(child), depth + 1));
                        }
                    }
                    SemanticTypeRecipe::Array { element, length } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((element).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe((length).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Slice(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Address {
                        mutable: _,
                        lifetime,
                        inner,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticOuterLifeRecipe(lifetime), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((inner).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Desc { lifetime, inner } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticOuterLifeRecipe(lifetime), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((inner).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Managed(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::OwnedStore(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Mutex(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::MutexGuard { payload, retained } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((payload).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticRetainedShapeRecipe((retained).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::Registration(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::AsyncRegistration(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Task { result, retained } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((result).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticRetainedShapeRecipe((retained).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::ErrorUnion { error, result } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((error).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((result).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::Qualified { storage, life } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((storage).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticStorageShapeRecipe((life).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::Code { kind: _, signature } => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticSignatureRecipe((signature).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::Closure {
                        definition: _,
                        arguments,
                        environment,
                    } => {
                        for child in arguments {
                            self.reserve(64)?;
                            pending.push((Rebase::SemanticActualRecipe(child), depth + 1));
                        }
                        for child in environment {
                            self.reserve(64)?;
                            pending.push((Rebase::SemanticActualRecipe(child), depth + 1));
                        }
                    }
                    SemanticTypeRecipe::Dyn {
                        owned: _,
                        interface,
                    } => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticDynInterfaceRecipe((interface).as_mut()),
                            depth + 1,
                        ));
                    }
                    SemanticTypeRecipe::Opaque {
                        definition: _,
                        arguments,
                        hidden,
                    } => {
                        for child in arguments {
                            self.reserve(64)?;
                            pending.push((Rebase::SemanticActualRecipe(child), depth + 1));
                        }
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((hidden).as_mut()), depth + 1));
                    }
                    SemanticTypeRecipe::AnonymousSum(value_0) => {
                        for child in value_0 {
                            self.reserve(64)?;
                            pending.push((Rebase::SemanticTypeRecipe(child), depth + 1));
                        }
                    }
                    SemanticTypeRecipe::UnresolvedNamed(_) => {}
                    SemanticTypeRecipe::Unsupported(_) => {}
                    SemanticTypeRecipe::OriginalType { scope, source: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                },
                Rebase::SemanticExactTypeRecipe(value) => {
                    for child in &mut value.classes {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(child), depth + 1));
                    }
                }
                Rebase::SemanticActualRecipe(value) => match value {
                    SemanticActualRecipe::Type(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe(value_0), depth + 1));
                    }
                    SemanticActualRecipe::Const { carrier, value } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe(carrier), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe(value), depth + 1));
                    }
                    SemanticActualRecipe::Life(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(value_0), depth + 1));
                    }
                    SemanticActualRecipe::Infer => {}
                },
                Rebase::SemanticClassSequenceRecipe(value) => {
                    for child in &mut value.parts {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticClassSequencePart(child), depth + 1));
                    }
                }
                Rebase::SemanticClassSequencePart(value) => match value {
                    SemanticClassSequencePart::One(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(value_0), depth + 1));
                    }
                    SemanticClassSequencePart::AllClasses { ty, path: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((ty).as_mut()), depth + 1));
                    }
                },
                Rebase::SemanticStorageShapeRecipe(value) => {
                    self.reserve(64)?;
                    pending.push((
                        Rebase::SemanticClassSequenceRecipe(&mut value.domain),
                        depth + 1,
                    ));
                    for child in &mut value.slots {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticSlotRecipe(child), depth + 1));
                    }
                    for child in &mut value.retained {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticRetainedPart(child), depth + 1));
                    }
                    for child in &mut value.nominal {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticNominalPart(child), depth + 1));
                    }
                }
                Rebase::SemanticSlotRecipe(value) => {
                    self.reserve(64)?;
                    pending.push((Rebase::SemanticLifeRecipe(&mut value.class), depth + 1));
                }
                Rebase::SemanticRetainedPart(value) => match value {
                    SemanticRetainedPart::One {
                        path: _,
                        child_class: _,
                        parent,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(parent), depth + 1));
                    }
                    SemanticRetainedPart::All { path: _, parents } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticClassSequenceRecipe(parents), depth + 1));
                    }
                },
                Rebase::SemanticNominalPart(value) => match value {
                    SemanticNominalPart::One {
                        path: _,
                        declaration: _,
                        argument: _,
                        class,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(class), depth + 1));
                    }
                    SemanticNominalPart::FromType {
                        path: _,
                        ty,
                        classes,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((ty).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticClassSequenceRecipe(classes), depth + 1));
                    }
                },
                Rebase::SemanticRetainedShapeRecipe(value) => {
                    self.reserve(64)?;
                    pending.push((
                        Rebase::SemanticClassSequenceRecipe(&mut value.domain),
                        depth + 1,
                    ));
                    for child in &mut value.ports {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticPortPart(child), depth + 1));
                    }
                    for child in &mut value.retained {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticRetainedPart(child), depth + 1));
                    }
                    for child in &mut value.nominal {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticNominalPart(child), depth + 1));
                    }
                }
                Rebase::SemanticPortPart(value) => match value {
                    SemanticPortPart::One {
                        leaf,
                        mode: _,
                        class,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((leaf).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticLifeRecipe(class), depth + 1));
                    }
                    SemanticPortPart::FromType { ty, classes } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((ty).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticClassSequenceRecipe(classes), depth + 1));
                    }
                },
                Rebase::SemanticConstRecipe(value) => match value {
                    SemanticConstRecipe::Value(_) => {}
                    SemanticConstRecipe::IntegerLiteral(_) => {}
                    SemanticConstRecipe::FloatLiteral(_) => {}
                    SemanticConstRecipe::Formal(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticFormalRef(value_0), depth + 1));
                    }
                    SemanticConstRecipe::Environment { scope, ordinal: _ } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticScopeRef(scope), depth + 1));
                    }
                    SemanticConstRecipe::Unary {
                        op: _,
                        operand_type,
                        value,
                    } => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticTypeRecipe((operand_type).as_mut()),
                            depth + 1,
                        ));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe((value).as_mut()), depth + 1));
                    }
                    SemanticConstRecipe::Binary {
                        op: _,
                        operand_type,
                        left,
                        right,
                    } => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticTypeRecipe((operand_type).as_mut()),
                            depth + 1,
                        ));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe((left).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe((right).as_mut()), depth + 1));
                    }
                    SemanticConstRecipe::Convert {
                        source_type,
                        target_type,
                        value,
                    } => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticTypeRecipe((source_type).as_mut()),
                            depth + 1,
                        ));
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticTypeRecipe((target_type).as_mut()),
                            depth + 1,
                        ));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe((value).as_mut()), depth + 1));
                    }
                    SemanticConstRecipe::AssociatedConst {
                        receiver,
                        application,
                        member: _,
                    } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((receiver).as_mut()), depth + 1));
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticTraitRecipe((application).as_mut()),
                            depth + 1,
                        ));
                    }
                },
                Rebase::SemanticTraitRecipe(value) => {
                    for child in &mut value.arguments {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticActualRecipe(child), depth + 1));
                    }
                }
                Rebase::SemanticSignatureRecipe(value) => {
                    for child in &mut value.parameters {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticParameterRecipe(child), depth + 1));
                    }
                    self.reserve(64)?;
                    pending.push((Rebase::SemanticTypeRecipe(&mut value.result), depth + 1));
                }
                Rebase::SemanticParameterRecipe(value) => {
                    self.reserve(64)?;
                    pending.push((Rebase::SemanticTypeRecipe(&mut value.ty), depth + 1));
                }
                Rebase::SemanticDynContextRecipe(value) => match value {
                    SemanticDynContextRecipe::Closed => {}
                    SemanticDynContextRecipe::Type(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe((value_0).as_mut()), depth + 1));
                    }
                    SemanticDynContextRecipe::Shape(value_0) => {
                        self.reserve(64)?;
                        pending.push((
                            Rebase::SemanticRetainedShapeRecipe((value_0).as_mut()),
                            depth + 1,
                        ));
                    }
                },
                Rebase::SemanticDynInterfaceRecipe(value) => {
                    self.reserve(64)?;
                    pending.push((
                        Rebase::SemanticTraitRecipe(&mut value.application),
                        depth + 1,
                    ));
                    self.reserve(64)?;
                    pending.push((
                        Rebase::SemanticDynContextRecipe(&mut value.context),
                        depth + 1,
                    ));
                    for child in &mut value.associated {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticAssociatedBindingRecipe(child), depth + 1));
                    }
                }
                Rebase::SemanticAssociatedBindingRecipe(value) => {
                    self.reserve(64)?;
                    pending.push((
                        Rebase::SemanticTraitRecipe(&mut value.application),
                        depth + 1,
                    ));

                    self.reserve(64)?;
                    pending.push((
                        Rebase::SemanticAssociatedRecipe(&mut value.value),
                        depth + 1,
                    ));
                }
                Rebase::SemanticAssociatedRecipe(value) => match value {
                    SemanticAssociatedRecipe::Type(value_0) => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe(value_0), depth + 1));
                    }
                    SemanticAssociatedRecipe::Const { carrier, value } => {
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticTypeRecipe(carrier), depth + 1));
                        self.reserve(64)?;
                        pending.push((Rebase::SemanticConstRecipe(value), depth + 1));
                    }
                },
            }
        }
        Ok(())
    }
}

fn selected_target(
    target: &SemanticInstanceTarget,
    module: ModuleId,
    functions: &[CoreFunction],
) -> bool {
    matches!(target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module:owner,function})if *owner==module&&functions.iter().any(|f|f.id==*function))
}
pub(super) fn selected(
    collector: &mut Collector<'_>,
    source: &ProgramModule,
    functions: &[CoreFunction],
    bytes_left: &mut usize,
) -> CompileResult<Option<SemanticModule>> {
    let Some(raw) = &source.core.semantic else {
        return Ok(None);
    };
    let mut builder = Builder {
        collector,
        module: source.core.module_id,
        bytes_left,
    };
    let mut result = SemanticModule {
        definitions: Vec::new(),
        instances: Vec::new(),
        sites: Vec::new(),
        dyn_scopes: Vec::new(),
    };
    for row in &raw.instances {
        builder.node(0)?;
        if !selected_target(&row.target, source.core.module_id, functions) {
            continue;
        }
        row.visit_dyn_metadata(
            &mut Transport {
                builder: &mut builder,
                copy: true,
            },
            0,
        )?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { function, .. }) =
            row.target
        else {
            return Err(invalid(
                "selected semantic target is not a real Core function",
                Span::default(),
            ));
        };
        let actual = functions
            .iter()
            .find(|f| f.id == function)
            .ok_or_else(|| invalid("selected semantic function is absent", Span::default()))?;
        let original = source
            .core
            .functions
            .iter()
            .find(|f| f.id == function)
            .ok_or_else(|| invalid("selected function has no frozen source body", actual.span))?;
        let before = builder.nodes(&original.body)?;
        let after = builder.nodes(&actual.body)?;
        let mut relocation = BTreeMap::new();
        for expr in before.expressions.values() {
            if let CoreExprKind::CompileTime(inner) = &expr.kind {
                builder.reserve(12 * std::mem::size_of::<(NodeId, NodeId)>())?;
                relocation.insert(inner.id, expr.id);
            }
        }
        let mut instance = row.clone();
        builder.relocate_instance(
            &mut instance,
            &relocation,
            (source.core.module_id, function),
        )?;
        builder.push(&mut result.instances, instance)?;
        for site in &raw.sites {
            builder.node(0)?;
            if site.instance != row.id {
                continue;
            }
            site.visit_dyn_metadata(
                &mut Transport {
                    builder: &mut builder,
                    copy: true,
                },
                0,
            )?;
            let mut site = site.clone();
            let SemanticSiteAddress::Core { expression, .. } = &mut site.at else {
                return Err(invalid(
                    "Core CT selection contains a Typed emission site",
                    actual.span,
                ));
            };
            *expression = builder.relocated(*expression, &relocation)?;
            if !after.expressions.contains_key(expression)
                && !after.statements.contains_key(expression)
            {
                return Err(invalid(
                    "selected CT site has no surviving actual operation",
                    actual.span,
                ));
            }
            builder.relocate_action(
                &mut site.action,
                &relocation,
                (source.core.module_id, function),
            )?;
            builder.push(&mut result.sites, site)?;
        }
    }
    Ok(Some(result))
}
impl Builder<'_, '_> {
    fn relocated(
        &mut self,
        mut node: NodeId,
        map: &BTreeMap<NodeId, NodeId>,
    ) -> CompileResult<NodeId> {
        for _ in 0..=map.len() {
            self.node(0)?;
            let Some(next) = map.get(&node) else {
                return Ok(node);
            };
            node = *next;
        }
        Err(invalid(
            "CT emission relocation contains a cycle",
            Span::default(),
        ))
    }
    fn relocate_operand(
        &mut self,
        value: &mut SemanticOperand,
        map: &BTreeMap<NodeId, NodeId>,
    ) -> CompileResult<()> {
        self.node(0)?;
        if let SemanticOperand::CoreExpression { expression, .. } = value {
            *expression = self.relocated(*expression, map)?;
        }
        Ok(())
    }
    fn relocate_class(
        &mut self,
        value: &mut SemanticClassExpr,
        map: &BTreeMap<NodeId, NodeId>,
    ) -> CompileResult<()> {
        self.node(0)?;
        match value {
            SemanticClassExpr::OuterOccurrence { operand, .. }
            | SemanticClassExpr::IntrinsicClass { operand, .. } => {
                self.relocate_operand(operand, map)
            }
            SemanticClassExpr::Formal { .. }
            | SemanticClassExpr::Implicit { .. }
            | SemanticClassExpr::Bound { .. }
            | SemanticClassExpr::Environment { .. } => Ok(()),
        }
    }
    fn relocate_value(
        &mut self,
        value: &mut SemanticTypeValue,
        map: &BTreeMap<NodeId, NodeId>,
        owner: (ModuleId, NodeId),
    ) -> CompileResult<()> {
        self.node(0)?;
        match value {
            SemanticTypeValue::Resolved { classes, .. } => {
                for class in classes {
                    self.relocate_class(class, map)?;
                }
            }
            SemanticTypeValue::Core { source, .. } => match source {
                SemanticCoreTypeSource::Expression {
                    module,
                    function,
                    expression,
                    ..
                }
                | SemanticCoreTypeSource::OriginalExpression {
                    module,
                    function,
                    expression,
                    ..
                } => {
                    if (*module, *function) == owner {
                        *expression = self.relocated(*expression, map)?;
                    }
                }
                SemanticCoreTypeSource::OriginalRecipe(_) => {}
            },
        }
        Ok(())
    }
    fn relocate_actual(
        &mut self,
        value: &mut SemanticActual,
        map: &BTreeMap<NodeId, NodeId>,
        owner: (ModuleId, NodeId),
    ) -> CompileResult<()> {
        self.node(0)?;
        match value {
            SemanticActual::Type(value) => self.relocate_value(value, map, owner)?,
            SemanticActual::Const { carrier, .. } => self.relocate_value(carrier, map, owner)?,
            SemanticActual::Life(value) => self.relocate_class(value, map)?,
            SemanticActual::CoreRecipe { .. } | SemanticActual::CoreInferred { .. } => {}
        }
        Ok(())
    }
    fn relocate_instance(
        &mut self,
        value: &mut SemanticInstance,
        map: &BTreeMap<NodeId, NodeId>,
        owner: (ModuleId, NodeId),
    ) -> CompileResult<()> {
        for actual in &mut value.actuals {
            self.relocate_actual(actual, map, owner)?;
        }
        for binding in &mut value.header_embedding {
            self.relocate_class(&mut binding.actual, map)?;
        }
        if let Some(value) = &mut value.self_type {
            self.relocate_value(value, map, owner)?;
        }
        if let Some(environment) = &mut value.environment {
            for value in &mut environment.actuals {
                self.relocate_actual(value, map, owner)?;
            }
        }
        Ok(())
    }
    fn relocate_action(
        &mut self,
        value: &mut SemanticSiteAction,
        map: &BTreeMap<NodeId, NodeId>,
        owner: (ModuleId, NodeId),
    ) -> CompileResult<()> {
        self.node(0)?;
        match value {
            SemanticSiteAction::Construct { result, value } => {
                self.relocate_operand(result, map)?;
                self.relocate_value(value, map, owner)?;
            }
            SemanticSiteAction::Call { actuals, .. } | SemanticSiteAction::BodyCall { actuals } => {
                for value in actuals {
                    self.relocate_actual(value, map, owner)?;
                }
            }
            SemanticSiteAction::CallableCall { callee } => self.relocate_operand(callee, map)?,
            SemanticSiteAction::TypeUse { subject, expected } => {
                self.relocate_operand(subject, map)?;
                self.relocate_value(expected, map, owner)?;
            }
            SemanticSiteAction::DeclareLocal { local, expected } => {
                if let SemanticLocalTarget::Core { statement, .. } = local {
                    *statement = self.relocated(*statement, map)?;
                }
                self.relocate_value(expected, map, owner)?;
            }
            SemanticSiteAction::Closure {
                result,
                environment,
                ..
            } => {
                self.relocate_operand(result, map)?;
                for value in environment {
                    self.relocate_actual(value, map, owner)?;
                }
            }
            SemanticSiteAction::CoreBodyCall => {}
        }
        Ok(())
    }
}

pub(super) fn fresh_nodes(
    collector: &mut Collector<'_>,
    module: ModuleId,
    bytes_left: &mut usize,
) -> CompileResult<(NodeId, NodeId)> {
    let program = collector.program;
    let mut builder = Builder {
        collector,
        module,
        bytes_left,
    };
    let mut used = BTreeSet::new();
    for source in &program.modules {
        for function in &source.core.functions {
            builder.reserve(16 * std::mem::size_of::<NodeId>())?;
            used.insert(function.id);
            let nodes = builder.nodes(&function.body)?;
            for id in nodes.expressions.keys().chain(nodes.statements.keys()) {
                builder.node(0)?;
                builder.reserve(16 * std::mem::size_of::<NodeId>())?;
                used.insert(*id);
            }
        }
    }
    let mut ordinal = u32::MAX;
    let mut next = || -> CompileResult<NodeId> {
        loop {
            builder.node(0)?;
            let id = NodeId(ordinal);
            ordinal = ordinal.checked_sub(1).ok_or_else(|| {
                invalid("CT wrapper Core identity space exhausted", Span::default())
            })?;
            if !used.contains(&id) {
                return Ok(id);
            }
        }
    };
    Ok((next()?, next()?))
}

pub(super) fn wrapper(
    collector: &mut Collector<'_>,
    module: ModuleId,
    origin: &QueryOrigin<'_>,
    function: &CoreFunction,
    execution: &CoreProgram,
    bytes_left: &mut usize,
) -> CompileResult<(SemanticDefinition, SemanticInstance, Vec<SemanticSite>)> {
    let program = collector.program;
    let mut definition_index = 0u32;
    let mut instance_index = 0u32;
    let mut builder = Builder {
        collector,
        module,
        bytes_left,
    };
    for source in &program.modules {
        if let Some(semantic) = &source.core.semantic {
            for definition in &semantic.definitions {
                builder.node(0)?;
                if definition.id.module == module {
                    definition_index =
                        definition_index.max(definition.id.index.checked_add(1).ok_or_else(
                            || invalid("CT definition namespace exhausted", function.span),
                        )?);
                }
            }
            for instance in &semantic.instances {
                builder.node(0)?;
                if instance.id.module == module {
                    instance_index =
                        instance_index.max(instance.id.index.checked_add(1).ok_or_else(|| {
                            invalid("CT instance namespace exhausted", function.span)
                        })?);
                }
            }
        }
    }
    let definition_id = SemanticDefinitionId {
        module,
        index: definition_index,
    };
    let instance_id = SemanticInstanceId {
        module,
        index: instance_index,
    };
    let result = builder.annotation(&function.return_type, 0)?;
    let body = builder.core_block(&function.body, 0)?;
    let SemanticDefinitionBody::FunctionWithBody { names, .. } = &origin.definition.body else {
        return Err(invalid(
            "CT caller lacks complete original names",
            function.span,
        ));
    };
    builder.rows::<SemanticOriginalNameBinding>(names.len())?;
    let mut original_names = Vec::with_capacity(names.len());
    for name in names {
        name.visit_dyn_metadata(
            &mut Transport {
                builder: &mut builder,
                copy: true,
            },
            0,
        )?;
        let mut name = name.clone();
        builder.rebase_name(&mut name.value)?;
        original_names.push(name);
    }
    // Compiler-generated specialization names are not Source spellings.
    // Link each real selected Core target to its unique frozen original owner.
    // This does not infer erased const/life actuals from its machine ABI.
    for selected_module in &execution.modules {
        for selected in &selected_module.core.functions {
            let mut owner = None;
            for source in &program.modules {
                if let Some(semantic) = &source.core.semantic {
                    for instance in &semantic.instances {
                        builder.node(0)?;
                        if matches!(&instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module:actual_module,function})if *actual_module==selected_module.core.module_id&&*function==selected.id)
                        {
                            if owner.replace(instance.definition).is_some() {
                                return Err(invalid(
                                    "selected Core callee has multiple frozen original instance owners",
                                    selected.span,
                                ));
                            }
                        }
                    }
                }
            }
            let owner = owner.ok_or_else(|| {
                invalid(
                    "selected Core callee has no complete frozen original instance owner",
                    selected.span,
                )
            })?;
            if selected_module.core.module_id == module {
                builder.generated_name(
                    &mut original_names,
                    &[selected.name.as_str()],
                    owner,
                    selected.span,
                )?;
            } else {
                let caller = execution
                    .modules
                    .iter()
                    .find(|m| m.core.module_id == module)
                    .ok_or_else(|| invalid("CT wrapper module is absent", function.span))?;
                for import in &caller.imports {
                    builder.node(0)?;
                    if import.module == selected_module.core.module_id {
                        builder.generated_name(
                            &mut original_names,
                            &[import.alias.as_str(), selected.name.as_str()],
                            owner,
                            selected.span,
                        )?;
                    }
                }
            }
        }
    }
    Transport {
        builder: &mut builder,
        copy: true,
    }
    .identity(&origin.definition.identity, 0)?;
    let mut identity = origin.definition.identity.clone();
    let query_name = builder.text(QUERY)?;
    builder.push(&mut identity.declaration_path, query_name)?;
    builder.push(&mut identity.generation_path, origin.wrapper.0)?;
    builder.push(&mut identity.generation_path, function.id.0)?;
    let nodes = builder.nodes(&function.body)?;
    builder.rows::<SemanticSiteRecipe>(nodes.expressions.len())?;
    builder.rows::<SemanticOriginalSiteNode>(nodes.expressions.len())?;
    builder.rows::<SemanticSite>(nodes.expressions.len())?;
    let mut recipes = Vec::with_capacity(nodes.expressions.len());
    let mut site_nodes = Vec::with_capacity(nodes.expressions.len());
    let mut actual_sites = Vec::with_capacity(nodes.expressions.len());
    for expression in nodes.expressions.values() {
        let expected = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(expression.id),
        };
        let value = SemanticTypeValue::Core {
            scope: instance_id,
            source: SemanticCoreTypeSource::OriginalExpression {
                module,
                function: function.id,
                expression: expression.id,
                occurrence: 0,
                source: SemanticOriginalTypeSource::Expression(expression.id),
            },
        };
        let operand = SemanticOperand::CoreExpression {
            expression: expression.id,
            occurrence: 0,
        };
        let proposal = match &expression.kind {
            CoreExprKind::Call { .. } => Some((
                SemanticSiteRecipeKind::OriginalCall,
                SemanticSiteAction::CoreBodyCall,
                SemanticEmissionRole::Call,
            )),
            CoreExprKind::Tuple(_) | CoreExprKind::Array(_) | CoreExprKind::Record { .. } => {
                Some((
                    SemanticSiteRecipeKind::Construct { result: expected },
                    SemanticSiteAction::Construct {
                        result: operand,
                        value,
                    },
                    SemanticEmissionRole::Construct,
                ))
            }
            CoreExprKind::Ascribed { .. }
            | CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                ..
            } => Some((
                SemanticSiteRecipeKind::TypeUse { expected },
                SemanticSiteAction::TypeUse {
                    subject: operand,
                    expected: value,
                },
                SemanticEmissionRole::Ascription,
            )),
            _ => None,
        };
        let Some((kind, action, role)) = proposal else {
            continue;
        };
        let id = SemanticSiteId(
            u32::try_from(recipes.len())
                .map_err(|_| invalid("CT wrapper site namespace exhausted", function.span))?,
        );
        let path = nodes
            .paths
            .get(&expression.id)
            .ok_or_else(|| invalid("CT wrapper operation path is absent", expression.span))?;
        builder.rows::<u32>(path.len())?;
        recipes.push(SemanticSiteRecipe {
            id,
            occurrence_path: path.clone(),
            kind,
        });
        site_nodes.push(SemanticOriginalSiteNode {
            site: id,
            node: SemanticOriginalNode::Expression(expression.id),
        });
        actual_sites.push(SemanticSite {
            instance: instance_id,
            recipe: id,
            at: SemanticSiteAddress::Core {
                expression: expression.id,
                occurrence: 0,
                role,
            },
            action,
        });
    }
    let definition = SemanticDefinition {
        id: definition_id,
        identity,
        parent: Some(origin.definition.id),
        kind: SemanticDefinitionKind::Function,
        parameters: Vec::new(),
        implicit_lifetimes: Vec::new(),
        requirements: Vec::new(),
        environment: Vec::new(),
        body: SemanticDefinitionBody::FunctionWithBody {
            signature: SemanticSignatureRecipe {
                bound_lifetimes: 0,
                invocation_lifetimes: 0,
                parameters: Vec::new(),
                result,
                result_mode: function.result_mode,
                call_kind: CallKind::Direct,
            },
            sites: recipes,
            parameters: Vec::new(),
            receiver_parameter: None,
            names: original_names,
            site_nodes,
            body: SemanticOriginalFunctionBody::Defined(body),
        },
    };
    let instance = SemanticInstance {
        id: instance_id,
        definition: definition_id,
        target: SemanticInstanceTarget::Function(SemanticFunctionTarget::Core {
            module,
            function: function.id,
        }),
        lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
        parent: Some(origin.instance.id),
        actuals: Vec::new(),
        header_embedding: Vec::new(),
        environment: None,
        self_type: None,
    };
    Ok((definition, instance, actual_sites))
}

impl Builder<'_, '_> {
    fn generated_name(
        &mut self,
        names: &mut Vec<SemanticOriginalNameBinding>,
        path: &[&str],
        definition: SemanticDefinitionId,
        span: Span,
    ) -> CompileResult<()> {
        for existing in names.iter() {
            self.node(0)?;
            self.collector.text_len(
                existing
                    .path
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(path.iter().map(|s| s.len()).sum::<usize>()),
            )?;
            if existing.namespace == SemanticOriginalNamespace::Value
                && existing.path.len() == path.len()
                && existing.path.iter().zip(path).all(|(a, b)| a == b)
            {
                return if existing.value == SemanticOriginalNameValue::Definition(definition) {
                    Ok(())
                } else {
                    Err(invalid(
                        "generated CT callee collides with another original value binding",
                        span,
                    ))
                };
            }
        }
        self.rows::<String>(path.len())?;
        let mut owned = Vec::with_capacity(path.len());
        for text in path {
            owned.push(self.text(text)?);
        }
        self.push(
            names,
            SemanticOriginalNameBinding {
                path: owned,
                namespace: SemanticOriginalNamespace::Value,
                value: SemanticOriginalNameValue::Definition(definition),
            },
        )?;
        Ok(())
    }
}

impl Builder<'_, '_> {
    fn push<T>(&mut self, values: &mut Vec<T>, value: T) -> CompileResult<()> {
        self.node(0)?;
        if values.len() == values.capacity() {
            let old = values.capacity();
            let capacity = old
                .checked_mul(2)
                .map(|n| n.max(4))
                .ok_or_else(|| invalid("CT transport vector capacity overflow", Span::default()))?;
            let bytes = capacity
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| invalid("CT transport vector size overflow", Span::default()))?;
            self.reserve(bytes)?;
            self.collector.text_len(values.len())?;
            values
                .try_reserve_exact(capacity - values.len())
                .map_err(|_| invalid("CT transport vector allocation failed", Span::default()))?;
            if values.capacity() > capacity {
                self.reserve(
                    (values.capacity() - capacity)
                        .checked_mul(std::mem::size_of::<T>())
                        .ok_or_else(|| {
                            invalid("CT transport allocator capacity overflow", Span::default())
                        })?,
                )?;
            }
            *self.bytes_left = self
                .bytes_left
                .checked_add(old * std::mem::size_of::<T>())
                .ok_or_else(|| {
                    invalid("CT transport storage accounting overflow", Span::default())
                })?;
        }
        values.push(value);
        Ok(())
    }
}
pub(super) fn append_wrapper(
    collector: &mut Collector<'_>,
    module: ModuleId,
    metadata: &mut SemanticModule,
    definition: SemanticDefinition,
    instance: SemanticInstance,
    sites: Vec<SemanticSite>,
    bytes_left: &mut usize,
) -> CompileResult<()> {
    let mut builder = Builder {
        collector,
        module,
        bytes_left,
    };
    builder.push(&mut metadata.definitions, definition)?;
    builder.push(&mut metadata.instances, instance)?;
    for site in sites {
        builder.push(&mut metadata.sites, site)?;
    }
    Ok(())
}
