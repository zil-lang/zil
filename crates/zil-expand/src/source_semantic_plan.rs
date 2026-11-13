//! Source records original operations before rewriting. Core mint events later
//! consume these tokens; Core/Typed trees never supply the original recipes.
use super::emission::{Action, Event};
use super::*;

fn type_expression(expression: &s::Expr, budget: &mut Budget<'_>) -> R<s::TypeRef> {
    budget.node(expression.span, 0)?;
    match &expression.kind {
        s::ExprKind::Name(name) => {
            budget.text(name, expression.span)?;
            budget.lineage_bytes(name.len() + std::mem::size_of::<String>(), expression.span)?;
            Ok(s::TypeRef {
                kind: s::TypeKind::Named {
                    path: vec![name.clone()],
                    args: Vec::new(),
                },
                span: expression.span,
            })
        }
        s::ExprKind::TypeApply { callee, arguments } => {
            let mut ty = type_expression(callee, budget)?;
            let s::TypeKind::Named { args, .. } = &mut ty.kind else {
                return Err(error(
                    expression.span,
                    "constructor type application has no original nominal path",
                ));
            };
            budget.const_expression(expression)?;
            budget.lineage_bytes(
                arguments.len() * std::mem::size_of::<s::GenericArg>(),
                expression.span,
            )?;
            *args = arguments.clone();
            Ok(ty)
        }
        s::ExprKind::Member {
            object,
            member: s::MemberName::Name(name),
        } => {
            let mut ty = type_expression(object, budget)?;
            let s::TypeKind::Named { path, args } = &mut ty.kind else {
                return Err(error(
                    expression.span,
                    "constructor module path is not nominal",
                ));
            };
            if !args.is_empty() {
                return Err(unsupported(
                    expression.span,
                    "nested constructor path requires its actual original owner",
                ));
            }
            budget.text(name, expression.span)?;
            budget.lineage_bytes(name.len() + std::mem::size_of::<String>(), expression.span)?;
            path.push(name.clone());
            Ok(ty)
        }
        _ => Err(unsupported(
            expression.span,
            "constructor needs its complete original type expression",
        )),
    }
}
fn add_site(
    program: &mut program::Program,
    definition: SemanticDefinitionId,
    path: Vec<u32>,
    kind: SemanticSiteRecipeKind,
    action: Action,
    role: SemanticEmissionRole,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<s::SemanticContextMarker> {
    let module = program
        .modules
        .iter_mut()
        .find(|(id, _)| *id == definition.module)
        .ok_or_else(|| error(span, "source operation module is absent"))?;
    let row = module
        .1
        .definitions
        .iter_mut()
        .find(|row| row.id == definition)
        .ok_or_else(|| error(span, "source operation definition is absent"))?;
    let SemanticDefinitionBody::Function { sites, .. } = &mut row.body else {
        return Err(error(span, "source operation owner is not a function"));
    };
    let id = SemanticSiteId(
        u32::try_from(sites.len()).map_err(|_| error(span, "source site namespace exhausted"))?,
    );
    budget.lineage_bytes(
        std::mem::size_of::<SemanticSiteRecipe>() + std::mem::size_of::<Event>(),
        span,
    )?;
    sites.push(SemanticSiteRecipe {
        id,
        occurrence_path: path,
        kind,
    });
    program.ledger.template(
        definition,
        vec![Event {
            recipe: id,
            role,
            action,
        }],
        budget,
        span,
    )
}
fn occurrence_mut<'a>(root: &'a mut s::Expr, path: &[u32]) -> Option<&'a mut s::Expr> {
    let mut node = root;
    for field in path {
        node = match (&mut node.kind, *field) {
            (s::ExprKind::Member { object, .. }, 0) => object,
            (s::ExprKind::Binary { left, .. }, 1) => left,
            (s::ExprKind::Binary { right, .. }, 2) => right,
            (s::ExprKind::Unary { operand, .. }, 1) => operand,
            (s::ExprKind::Group(inner), 0) => inner,
            (s::ExprKind::Ascribed { value, .. }, 1) => value,
            _ => return None,
        };
    }
    Some(node)
}
pub(super) fn function(
    program: &mut program::Program,
    normalizer: &mut Builder<'_, '_, '_, '_>,
    function: &mut s::Function,
    closures: &[closures::ClosureTemplate],
) -> R<()> {
    let definition = normalizer.id(normalizer.owner);
    let s::FunctionBody::Block(body) = &mut function.body else {
        return Ok(());
    };
    for (ordinal, statement) in body.statements.iter_mut().enumerate() {
        if let s::StmtKind::Binding {
            ty: Some(annotation),
            value,
            ..
        } = &mut statement.kind
        {
            let expected = normalizer.ty(annotation, &mut vec![10, ordinal as u32, 0], 0, false)?;
            prepay_recipe(&expected, normalizer.budget, statement.span, 0)?;
            statement.semantic = add_site(
                program,
                definition,
                vec![10, ordinal as u32],
                SemanticSiteRecipeKind::DeclareLocal {
                    expected: expected.clone(),
                },
                Action::DeclareLocal(expected),
                SemanticEmissionRole::Binding,
                normalizer.budget,
                statement.span,
            )?;
            if let s::ExprKind::Record {
                ty: Some(type_expr),
                ..
            } = &value.kind
            {
                // The constructor's own source type is independent of its
                // destination annotation, including a/b nearest mismatches.
                let actual = type_expression(type_expr, normalizer.budget)?;
                let recipe = normalizer.ty(&actual, &mut vec![10, ordinal as u32, 1], 0, false)?;
                prepay_recipe(&recipe, normalizer.budget, value.span, 0)?;
                value.semantic = add_site(
                    program,
                    definition,
                    vec![10, ordinal as u32, 1],
                    SemanticSiteRecipeKind::Construct {
                        result: recipe.clone(),
                    },
                    Action::Construct(recipe),
                    SemanticEmissionRole::Construct,
                    normalizer.budget,
                    value.span,
                )?;
            }
        }
        let expression = match &mut statement.kind {
            s::StmtKind::Binding { value, .. } | s::StmtKind::Return(Some(value)) => Some(value),
            _ => None,
        };
        if let Some(expression) = expression
            && matches!(expression.kind, s::ExprKind::Closure { .. })
        {
            let original = closures
                .iter()
                .find(|closure| {
                    closure.owner == normalizer.owner && closure.ordinal == ordinal as u32
                })
                .ok_or_else(|| {
                    error(
                        expression.span,
                        "actual original closure declaration is absent",
                    )
                })?;
            for actual in &original.environment {
                prepay_recipe(actual, normalizer.budget, expression.span, 0)?;
            }
            let environment = original.environment.clone();
            let marker = add_site(
                program,
                definition,
                vec![10, ordinal as u32, 2],
                SemanticSiteRecipeKind::Closure {
                    definition: original.definition.id,
                    environment: environment.clone(),
                },
                Action::Closure { environment },
                SemanticEmissionRole::Closure,
                normalizer.budget,
                expression.span,
            )?;
            expression.semantic = marker;
            let s::ExprKind::Closure {
                body: s::ArrowBody::Expression(body),
                ..
            } = &mut expression.kind
            else {
                return Err(unsupported(
                    expression.span,
                    "closure occurrence requires its full original body",
                ));
            };
            for usage in &original.uses {
                let path = usage.path.strip_prefix(&[0]).ok_or_else(|| {
                    error(
                        expression.span,
                        "closure use has another original body root",
                    )
                })?;
                let name = occurrence_mut(body, path).ok_or_else(|| {
                    error(
                        expression.span,
                        "original capture use changed during Source cloning",
                    )
                })?;
                if !matches!(name.kind, s::ExprKind::Name(_)) {
                    return Err(error(
                        name.span,
                        "capture occurrence no longer names its actual source use",
                    ));
                }
                prepay_recipe(&usage.ty, normalizer.budget, name.span, 0)?;
                name.semantic = add_site(
                    program,
                    original.definition.id,
                    usage.path.clone(),
                    SemanticSiteRecipeKind::TypeUse {
                        expected: usage.ty.clone(),
                    },
                    Action::TypeUse(usage.ty.clone()),
                    SemanticEmissionRole::Ascription,
                    normalizer.budget,
                    name.span,
                )?;
            }
        }
    }
    Ok(())
}

fn instantiate_result(
    ty: &SemanticTypeRecipe,
    actuals: &[SemanticActualRecipe],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<SemanticTypeRecipe> {
    fn life(
        value: &SemanticLifeRecipe,
        actuals: &[SemanticActualRecipe],
        span: Span,
    ) -> R<SemanticLifeRecipe> {
        match value {
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => match actuals.get(*ordinal as usize) {
                Some(SemanticActualRecipe::Life(value)) => Ok(value.clone()),
                _ => Err(error(span, "call result lifetime has no original actual")),
            },
            SemanticLifeRecipe::Bound { .. } | SemanticLifeRecipe::Invocation(_) => {
                Ok(value.clone())
            }
            _ => Err(unsupported(
                span,
                "call result lifetime needs its exact lexical substitution",
            )),
        }
    }
    fn actual(
        value: &SemanticActualRecipe,
        actuals: &[SemanticActualRecipe],
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<SemanticActualRecipe> {
        match value {
            SemanticActualRecipe::Type(ty) => Ok(SemanticActualRecipe::Type(instantiate_result(
                ty, actuals, budget, span,
            )?)),
            SemanticActualRecipe::Life(value) => {
                Ok(SemanticActualRecipe::Life(life(value, actuals, span)?))
            }
            _ => Err(unsupported(
                span,
                "Const return application needs its original complete carrier substitution",
            )),
        }
    }
    budget.node(span, 0)?;
    match ty {
        SemanticTypeRecipe::Atom(_) => {
            prepay_recipe(ty, budget, span, 0)?;
            Ok(ty.clone())
        }
        SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        }) => match actuals.get(*ordinal as usize) {
            Some(SemanticActualRecipe::Type(ty)) => {
                prepay_recipe(ty, budget, span, 0)?;
                Ok(ty.clone())
            }
            _ => Err(error(span, "call result whole Type has no original actual")),
        },
        SemanticTypeRecipe::Opaque {
            definition,
            arguments,
            hidden,
        } => {
            let mut result = Vec::new();
            for value in arguments {
                budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), span)?;
                result.push(actual(value, actuals, budget, span)?);
            }
            Ok(SemanticTypeRecipe::Opaque {
                definition: *definition,
                arguments: result,
                hidden: Box::new(instantiate_result(hidden, actuals, budget, span)?),
            })
        }
        SemanticTypeRecipe::Closure {
            definition,
            arguments,
            environment,
        } => {
            let mut args = Vec::new();
            let mut env = Vec::new();
            for value in arguments {
                budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), span)?;
                args.push(actual(value, actuals, budget, span)?);
            }
            for value in environment {
                budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), span)?;
                env.push(actual(value, actuals, budget, span)?);
            }
            Ok(SemanticTypeRecipe::Closure {
                definition: *definition,
                arguments: args,
                environment: env,
            })
        }
        SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } => {
            let mut args = Vec::new();
            for value in arguments {
                budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), span)?;
                args.push(actual(value, actuals, budget, span)?);
            }
            Ok(SemanticTypeRecipe::Nominal {
                definition: *definition,
                arguments: args,
            })
        }
        SemanticTypeRecipe::Address {
            mutable,
            lifetime: SemanticOuterLifeRecipe::Class(value),
            inner,
        } => Ok(SemanticTypeRecipe::Address {
            mutable: *mutable,
            lifetime: SemanticOuterLifeRecipe::Class(life(value, actuals, span)?),
            inner: Box::new(instantiate_result(inner, actuals, budget, span)?),
        }),
        _ => Err(unsupported(
            span,
            "call result requires its complete original scoped type substitution",
        )),
    }
}

struct Calls<'a, 's, 'i, 'b, 'l> {
    program: &'a mut program::Program,
    normalizer: &'a mut Builder<'s, 'i, 'b, 'l>,
    definition: SemanticDefinitionId,
    locals: std::collections::BTreeMap<String, Option<SemanticTypeRecipe>>,
    ordinal: u32,
}
impl Calls<'_, '_, '_, '_, '_> {
    fn function_row(&mut self, names: &[String], span: Span) -> R<usize> {
        let local = self.normalizer.inventory.declarations[self.normalizer.owner].module;
        let (module, name) = match names {
            [name] => (local, name),
            [alias, name] => {
                if self.locals.contains_key(alias) {
                    return Err(unsupported(
                        span,
                        "receiver call requires its actual original method application",
                    ));
                }
                let mut target = None;
                for (owner, edges) in self.normalizer.imports {
                    self.normalizer.budget.node(span, 0)?;
                    if *owner != local {
                        continue;
                    }
                    for edge in *edges {
                        self.normalizer.budget.text(&edge.alias, span)?;
                        if edge.alias == *alias && target.replace(edge.module).is_some() {
                            return Err(error(span, "original callable import alias is ambiguous"));
                        }
                    }
                }
                (
                    target.ok_or_else(|| {
                        unsupported(span, "static member call lacks its actual source import")
                    })?,
                    name,
                )
            }
            _ => {
                return Err(unsupported(
                    span,
                    "static call needs a complete original module path",
                ));
            }
        };
        let mut target = None;
        for (index, row) in self.normalizer.inventory.declarations.iter().enumerate() {
            self.normalizer.budget.node(span, 0)?;
            if row.module == module
                && row.parent.is_none()
                && row.function().is_some_and(|f| f.name == *name)
            {
                if module != local && !row.public {
                    return Err(error(span, "original call target is private"));
                }
                if target.replace(index).is_some() {
                    return Err(error(span, "original call target is ambiguous"));
                }
            }
        }
        target.ok_or_else(|| {
            unsupported(
                span,
                "static call original declaration is not in the actual source inventory",
            )
        })
    }
    fn value_type(&mut self, e: &s::Expr) -> R<SemanticTypeRecipe> {
        self.normalizer.budget.node(e.span, 0)?;
        match &e.kind {
            s::ExprKind::Name(name) => {
                let ty = self
                    .locals
                    .get(name)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        unsupported(
                            e.span,
                            "call actual needs its original complete source type",
                        )
                    })?;
                prepay_recipe(ty, self.normalizer.budget, e.span, 0)?;
                Ok(ty.clone())
            }
            s::ExprKind::Literal(s::Literal::Integer(text)) => {
                self.normalizer.budget.text(text, e.span)?;
                let mut ty = Type::I32;
                for (suffix, kind) in [
                    ("usize", Type::Usize),
                    ("isize", Type::Isize),
                    ("u64", Type::U64),
                    ("i64", Type::I64),
                    ("u32", Type::U32),
                    ("i32", Type::I32),
                    ("u16", Type::U16),
                    ("i16", Type::I16),
                    ("u8", Type::U8),
                    ("i8", Type::I8),
                ] {
                    if text.ends_with(suffix) {
                        ty = kind;
                        break;
                    }
                }
                Ok(SemanticTypeRecipe::Atom(ty))
            }
            s::ExprKind::Literal(s::Literal::Bool(_)) => Ok(SemanticTypeRecipe::Atom(Type::Bool)),
            s::ExprKind::Literal(s::Literal::Float(text)) => {
                Ok(SemanticTypeRecipe::Atom(if text.ends_with("f32") {
                    Type::F32
                } else {
                    Type::F64
                }))
            }
            s::ExprKind::Group(inner)
            | s::ExprKind::Unary {
                operand: inner,
                op: s::UnaryOp::Negate | s::UnaryOp::BitNot | s::UnaryOp::Give,
            } => self.value_type(inner),
            s::ExprKind::Unary {
                op: s::UnaryOp::Not,
                ..
            } => Ok(SemanticTypeRecipe::Atom(Type::Bool)),
            s::ExprKind::Ascribed { ty, .. } => {
                self.normalizer
                    .ty(ty, &mut vec![30, self.ordinal], 0, false)
            }
            s::ExprKind::Tuple(fields) => {
                let mut output = Vec::new();
                for field in fields {
                    let ty = self.value_type(field)?;
                    self.normalizer.push(&mut output, ty, field.span)?;
                }
                Ok(SemanticTypeRecipe::Tuple(output))
            }
            s::ExprKind::Record { ty: Some(ty), .. } => {
                let ty = type_expression(ty, self.normalizer.budget)?;
                self.normalizer
                    .ty(&ty, &mut vec![30, self.ordinal], 0, false)
            }
            s::ExprKind::Call { callee, .. } if matches!(&callee.kind,s::ExprKind::Name(name) if crate::integer_name(name)) =>
            {
                let ty = type_expression(callee, self.normalizer.budget)?;
                self.normalizer
                    .ty(&ty, &mut vec![30, self.ordinal], 0, false)
            }
            s::ExprKind::Member {
                object,
                member: s::MemberName::TupleIndex(index),
            } => {
                let SemanticTypeRecipe::Tuple(fields) = self.value_type(object)? else {
                    return Err(unsupported(
                        e.span,
                        "tuple field inference requires its complete source tuple",
                    ));
                };
                let ty = fields
                    .get(*index as usize)
                    .ok_or_else(|| error(e.span, "tuple source field ordinal is out of range"))?;
                prepay_recipe(ty, self.normalizer.budget, e.span, 0)?;
                Ok(ty.clone())
            }
            _ => Err(unsupported(
                e.span,
                "call Type inference requires its complete original expression constraints",
            )),
        }
    }
    fn life_argument(
        &mut self,
        argument: &mut s::Expr,
        index: u32,
        occurrence: u32,
    ) -> R<SemanticLifeRecipe> {
        if let s::ExprKind::Unary {
            op: s::UnaryOp::Borrow | s::UnaryOp::BorrowMut,
            operand,
        } = &argument.kind
        {
            let mutable = matches!(
                argument.kind,
                s::ExprKind::Unary {
                    op: s::UnaryOp::BorrowMut,
                    ..
                }
            );
            let inner = self.value_type(operand)?;
            let module = self
                .program
                .modules
                .iter_mut()
                .find(|(id, _)| *id == self.definition.module)
                .ok_or_else(|| error(argument.span, "call lifetime module is absent"))?;
            let definition = module
                .1
                .definitions
                .iter_mut()
                .find(|row| row.id == self.definition)
                .ok_or_else(|| error(argument.span, "call lifetime owner is absent"))?;
            let ordinal = u32::try_from(definition.implicit_lifetimes.len())
                .map_err(|_| error(argument.span, "call implicit namespace exhausted"))?;
            self.normalizer.budget.lineage_bytes(
                std::mem::size_of::<SemanticImplicitLifetime>() + 16,
                argument.span,
            )?;
            definition
                .implicit_lifetimes
                .push(SemanticImplicitLifetime {
                    ordinal,
                    occurrence_path: vec![20, occurrence, 0, index],
                });
            let life = SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            };
            let ty = SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life.clone()),
                inner: Box::new(inner),
            };
            prepay_recipe(&ty, self.normalizer.budget, argument.span, 0)?;
            argument.semantic = add_site(
                self.program,
                self.definition,
                vec![20, occurrence, 0, index],
                SemanticSiteRecipeKind::TypeUse {
                    expected: ty.clone(),
                },
                Action::TypeUse(ty),
                SemanticEmissionRole::Ascription,
                self.normalizer.budget,
                argument.span,
            )?;
            Ok(life)
        } else {
            let ty = self.value_type(argument)?;
            let SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(life),
                ..
            } = ty
            else {
                return Err(unsupported(
                    argument.span,
                    "inferred Life actual has no actual source address occurrence",
                ));
            };
            Ok(life)
        }
    }
}
impl crate::generated_spans::Visitor for Calls<'_, '_, '_, '_, '_> {
    fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
        self.normalizer
            .budget
            .node(*span, depth)
            .map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.normalizer.budget.text(text, span).map_err(|e| vec![e])
    }
    fn statement(&mut self, s: &mut s::Stmt, depth: usize) -> CompileResult<()> {
        crate::generated_spans::walk_statement(self, s, depth)?;
        if let s::StmtKind::Binding {
            pattern,
            ty: annotation,
            value,
            ..
        } = &s.kind
            && let s::PatternKind::Name(name) = &pattern.kind
        {
            let ty = if let Some(annotation) = annotation {
                Some(
                    self.normalizer
                        .ty(annotation, &mut vec![10, self.ordinal, 0], 0, false)
                        .map_err(|e| vec![e])?,
                )
            } else if let Some((owner, site)) = self.program.ledger.template_site(value.semantic) {
                let module = self
                    .program
                    .modules
                    .iter()
                    .find(|(id, _)| *id == owner.module)
                    .ok_or_else(|| {
                        vec![error(s.span, "local constructor original module is absent")]
                    })?;
                let original = module
                    .1
                    .definitions
                    .iter()
                    .find(|row| row.id == owner)
                    .ok_or_else(|| {
                        vec![error(s.span, "local constructor original owner is absent")]
                    })?;
                let SemanticDefinitionBody::Function { sites, .. } = &original.body else {
                    return Err(vec![error(
                        s.span,
                        "local constructor owner is not a function",
                    )]);
                };
                match &sites
                    .iter()
                    .find(|row| row.id == site)
                    .ok_or_else(|| {
                        vec![error(s.span, "local constructor original site is absent")]
                    })?
                    .kind
                {
                    SemanticSiteRecipeKind::Closure {
                        definition,
                        environment,
                    } => Some(SemanticTypeRecipe::Closure {
                        definition: *definition,
                        arguments: Vec::new(),
                        environment: environment.clone(),
                    }),
                    SemanticSiteRecipeKind::Construct { result } => Some(result.clone()),
                    SemanticSiteRecipeKind::Call { target, actuals } => {
                        let declaration = self
                            .program
                            .modules
                            .iter()
                            .flat_map(|(_, m)| &m.definitions)
                            .find(|row| row.id == *target)
                            .ok_or_else(|| {
                                vec![error(s.span, "call result original definition is absent")]
                            })?;
                        let SemanticDefinitionBody::Function { signature, .. } = &declaration.body
                        else {
                            return Err(vec![error(
                                s.span,
                                "call result original target is not a function",
                            )]);
                        };
                        Some(
                            instantiate_result(
                                &signature.result,
                                actuals,
                                self.normalizer.budget,
                                s.span,
                            )
                            .map_err(|e| vec![e])?,
                        )
                    }
                    _ => None,
                }
            } else {
                match self.value_type(value) {
                    Ok(ty) => Some(ty),
                    Err(error) if error.category == zil_base::ResultCategory::Unsupported => None,
                    Err(error) => return Err(vec![error]),
                }
            };
            self.normalizer
                .budget
                .lineage_bytes(
                    name.len() + std::mem::size_of::<(String, Option<SemanticTypeRecipe>)>(),
                    s.span,
                )
                .map_err(|e| vec![e])?;
            self.locals.insert(name.clone(), ty);
        }
        Ok(())
    }
    fn expression(&mut self, e: &mut s::Expr, depth: usize) -> CompileResult<()> {
        if matches!(
            e.kind,
            s::ExprKind::Closure { .. } | s::ExprKind::MetaEval(_)
        ) {
            return Ok(());
        }
        crate::generated_spans::walk_expression(self, e, depth)?;
        let s::ExprKind::Call { callee, arguments } = &mut e.kind else {
            return Ok(());
        };
        let local_name = if let s::ExprKind::Name(name) = &callee.kind {
            Some(name.as_str())
        } else {
            None
        };
        if local_name
            .is_some_and(|name| crate::primitive_name(name) && !self.locals.contains_key(name))
        {
            // A sealed language operation is lowered by its actual builtin
            // kernel, not disguised as an original user Function declaration.
            return Ok(());
        }
        let written = type_expression(callee, self.normalizer.budget).map_err(|e| vec![e])?;
        let s::TypeKind::Named {
            path: names,
            args: explicit,
        } = &written.kind
        else {
            unreachable!()
        };
        let site = self.ordinal;
        self.ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or_else(|| vec![error(e.span, "call source occurrence namespace exhausted")])?;
        if let Some(callee) = local_name.and_then(|name| self.locals.get(name)) {
            let ty = callee.as_ref().ok_or_else(|| {
                vec![unsupported(
                    e.span,
                    "callable local requires its complete source type",
                )]
            })?;
            prepay_recipe(ty, self.normalizer.budget, e.span, 0).map_err(|e| vec![e])?;
            e.semantic = add_site(
                self.program,
                self.definition,
                vec![20, site],
                SemanticSiteRecipeKind::CallableCall { callee: ty.clone() },
                Action::CallableCall,
                SemanticEmissionRole::Call,
                self.normalizer.budget,
                e.span,
            )
            .map_err(|e| vec![e])?;
            return Ok(());
        }
        let row_index = self.function_row(names, e.span).map_err(|e| vec![e])?;
        let row = &self.normalizer.inventory.declarations[row_index];
        let function = row.function().expect("source function selected");
        let target = SemanticDefinitionId {
            module: row.module,
            index: row.declaration.0,
        };
        let mut actuals = Vec::new();
        let mut emitted = Vec::new();
        if !explicit.is_empty() && explicit.len() != function.generics.len() {
            return Err(vec![error(
                e.span,
                "explicit call actuals must retain the complete original mixed order",
            )]);
        }
        for (formal_ordinal, formal) in function.generics.iter().enumerate() {
            if let Some(actual) = explicit.get(formal_ordinal) {
                let recipe = match (&formal.kind, actual) {
                    (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                        SemanticActualRecipe::Type(
                            self.normalizer
                                .ty(ty, &mut vec![20, site, formal_ordinal as u32], 0, false)
                                .map_err(|e| vec![e])?,
                        )
                    }
                    (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                        SemanticActualRecipe::Life(
                            self.normalizer
                                .life(Some(name), &[20, site, formal_ordinal as u32], e.span)
                                .map_err(|e| vec![e])?,
                        )
                    }
                    _ => {
                        return Err(vec![unsupported(
                            e.span,
                            "explicit Const call actual needs its original declaration carrier substitution",
                        )]);
                    }
                };
                prepay_recipe(&recipe, self.normalizer.budget, e.span, 0).map_err(|e| vec![e])?;
                emitted.push(emission::Actual {
                    recipe: recipe.clone(),
                    inferred_argument: None,
                });
                actuals.push(recipe);
                continue;
            }
            match &formal.kind {
                s::GenericParamKind::Type(_) => {
                    let index=function.params.iter().position(|p|matches!(&p.ty.kind,s::TypeKind::Named{path,args}if args.is_empty()&&path.as_slice()==[formal.name.as_str()])).ok_or_else(||vec![unsupported(formal.span,"generic call needs its original complete inference relation")])?;
                    let argument = arguments.get(index).ok_or_else(|| {
                        vec![error(e.span, "generic source call argument is absent")]
                    })?;
                    let ty = self.value_type(&argument.value).map_err(|e| vec![e])?;
                    prepay_recipe(&ty, self.normalizer.budget, e.span, 0).map_err(|e| vec![e])?;
                    let recipe = SemanticActualRecipe::Type(ty);
                    emitted.push(emission::Actual {
                        recipe: recipe.clone(),
                        inferred_argument: Some(index as u32),
                    });
                    actuals.push(recipe);
                }
                s::GenericParamKind::Lifetime => {
                    let index=function.params.iter().position(|p|matches!(&p.ty.kind,s::TypeKind::Reference{lifetime:Some(name),..}if name==&formal.name)).ok_or_else(||vec![unsupported(formal.span,"generic Life call needs its original input occurrence")])?;
                    let argument = arguments.get_mut(index).ok_or_else(|| {
                        vec![error(e.span, "Life source call argument is absent")]
                    })?;
                    let life = self
                        .life_argument(&mut argument.value, index as u32, site)
                        .map_err(|e| vec![e])?;
                    let recipe = SemanticActualRecipe::Life(life);
                    emitted.push(emission::Actual {
                        recipe: recipe.clone(),
                        inferred_argument: None,
                    });
                    actuals.push(recipe);
                }
                _ => {
                    return Err(vec![unsupported(
                        formal.span,
                        "Const call needs its complete original source carrier recipe",
                    )]);
                }
            }
        }
        e.semantic = add_site(
            self.program,
            self.definition,
            vec![20, site],
            SemanticSiteRecipeKind::Call { target, actuals },
            Action::DeferredCall {
                definition: target,
                actuals: emitted,
            },
            SemanticEmissionRole::Call,
            self.normalizer.budget,
            e.span,
        )
        .map_err(|e| vec![e])?;
        Ok(())
    }
}
pub(super) fn calls(
    program: &mut program::Program,
    normalizer: &mut Builder<'_, '_, '_, '_>,
    function: &mut s::Function,
) -> R<()> {
    use crate::generated_spans::Visitor;
    let definition = normalizer.id(normalizer.owner);
    let mut locals = std::collections::BTreeMap::new();
    for (i, p) in function.params.iter().enumerate() {
        let ty = normalizer.ty(&p.ty, &mut vec![1, i as u32], 0, false)?;
        locals.insert(p.name.clone(), Some(ty));
    }
    let mut pass = Calls {
        program,
        normalizer,
        definition,
        locals,
        ordinal: 0,
    };
    match &mut function.body {
        s::FunctionBody::Block(body) => pass.block(body, 0),
        s::FunctionBody::Expression(e) => pass.expression(e, 0),
        s::FunctionBody::Declaration => Ok(()),
    }
    .map_err(|mut e| e.remove(0))
}
