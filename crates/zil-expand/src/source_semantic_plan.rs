//! Source records original operations before rewriting. Core mint events later
//! consume these tokens; Core/Typed trees never supply the original recipes.
use super::emission::{Action, Event};
use super::*;

pub(super) fn type_expression(expression: &s::Expr, budget: &mut Budget<'_>) -> R<s::TypeRef> {
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
    let sites = match &mut row.body {
        SemanticDefinitionBody::Function { sites, .. }
        | SemanticDefinitionBody::FunctionWithBody { sites, .. } => sites,
        _ => return Err(error(span, "source operation owner is not a function")),
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
// Every runtime proposal is tied to the original operation node directory.
// Original bodies carry all branches and mixed call arguments; this pass does
// not infer a type, resolve a callee from its eventual ABI, or omit a binding.
enum SourceBody<'a> {
    Function(&'a mut s::FunctionBody),
    Closure(&'a mut s::ArrowBody),
}
impl SourceBody<'_> {
    fn locate<'a>(
        &'a mut self,
        path: &[u32],
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<body_nodes::Located<'a>>> {
        match self {
            Self::Function(body) => body_nodes::function(body, path, budget, span),
            Self::Closure(body) => body_nodes::arrow_body(body, path, budget, span),
        }
    }
}
fn original_site(
    program: &mut program::Program,
    definition: SemanticDefinitionId,
    node: SemanticOriginalNode,
    path: Vec<u32>,
    kind: SemanticSiteRecipeKind,
    action: Action,
    role: SemanticEmissionRole,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<s::SemanticContextMarker> {
    let marker = add_site(program, definition, path, kind, action, role, budget, span)?;
    let (_, site) = program
        .ledger
        .template_site(marker)
        .ok_or_else(|| error(span, "original operation token has no site"))?;
    let row = program
        .modules
        .iter_mut()
        .find(|(module, _)| *module == definition.module)
        .and_then(|(_, module)| {
            module
                .definitions
                .iter_mut()
                .find(|row| row.id == definition)
        })
        .ok_or_else(|| error(span, "original operation definition is absent"))?;
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &mut row.body else {
        return Err(error(
            span,
            "original operation requires its complete original body",
        ));
    };
    budget.lineage_bytes(std::mem::size_of::<SemanticOriginalSiteNode>(), span)?;
    site_nodes.push(SemanticOriginalSiteNode { site, node });
    Ok(marker)
}
fn observation_operand(
    body: &mut SourceBody<'_>,
    path: &[u32],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<bool> {
    let mut length = path.len();
    while length > 0 {
        let child = path[length - 1];
        let Some(parent) = body.locate(&path[..length - 1], budget, span)? else {
            return Ok(true);
        };
        let body_nodes::Node::Expression(parent) = parent.node else {
            return Ok(true);
        };
        match &parent.kind {
            s::ExprKind::Group(_) if child == 0 => length -= 1,
            s::ExprKind::TypeApply { .. } if child == 0 => length -= 1,
            s::ExprKind::Call { .. } if child == 0 => return Ok(false),
            s::ExprKind::Unary {
                op: s::UnaryOp::Give,
                ..
            } if child == 0 => return Ok(false),
            _ => return Ok(true),
        }
    }
    Ok(true)
}
fn implicit_observation(
    program: &mut program::Program,
    definition: SemanticDefinitionId,
    path: &[u32],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    let original = program
        .modules
        .iter_mut()
        .find(|(module, _)| *module == definition.module)
        .and_then(|(_, module)| {
            module
                .definitions
                .iter_mut()
                .find(|row| row.id == definition)
        })
        .ok_or_else(|| error(span, "description observation original owner is absent"))?;
    budget.identity_work(
        original
            .implicit_lifetimes
            .len()
            .saturating_mul(path.len() + 1),
        span,
    )?;
    if original
        .implicit_lifetimes
        .iter()
        .any(|row| row.occurrence_path == path)
    {
        return Ok(());
    }
    let ordinal = u32::try_from(original.implicit_lifetimes.len())
        .map_err(|_| error(span, "description observation implicit namespace overflow"))?;
    budget.lineage_bytes(
        path.len()
            .checked_mul(std::mem::size_of::<u32>())
            .ok_or_else(|| error(span, "description observation path overflow"))?,
        span,
    )?;
    budget.lineage_push(
        &mut original.implicit_lifetimes,
        SemanticImplicitLifetime {
            ordinal,
            occurrence_path: path.to_vec(),
        },
        span,
    )
}
fn operations(
    program: &mut program::Program,
    definition: SemanticDefinitionId,
    body: &mut SourceBody<'_>,
    closures: &[closures::ClosureTemplate],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    budget.identity_work(program.original_nodes.len(), span)?;
    let Some(index) = program
        .original_nodes
        .iter()
        .position(|row| row.definition == definition)
    else {
        return Err(error(
            span,
            "complete original operation directory is absent",
        ));
    };
    let directory = program.original_nodes.remove(index);
    let mut value_names = directory.value_names;
    budget.identity_work(
        value_names
            .len()
            .saturating_mul(value_names.len().checked_ilog2().unwrap_or(0) as usize + 1),
        span,
    )?;
    value_names.sort_unstable();
    let nodes = directory.nodes;
    for original in nodes {
        let observation = observation_operand(body, &original.path, budget, span)?;
        let Some(located) = body.locate(&original.path, budget, span)? else {
            return Err(error(
                span,
                "original operation no longer identifies its actual syntax node",
            ));
        };
        if !located.runtime || located.child {
            continue;
        }
        let mut path = Vec::new();
        budget.lineage_bytes((original.path.len() + 1) * 4, span)?;
        path.push(40);
        path.extend_from_slice(&original.path);
        match (original.kind, located.node) {
            (body::OriginalBodyNodeKind::Binding, body_nodes::Node::Statement(statement)) => {
                if !matches!(statement.kind, s::StmtKind::Binding { .. }) {
                    continue;
                }
                // A raw Address/Desc result may acquire a local role at this
                // declaration. The original solver activates this potential
                // occurrence only for that actual outer mode; it never
                // substitutes for a whole child's intrinsic class domain.
                implicit_observation(program, definition, &path, budget, statement.span)?;
                let expected = SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Binding(original.id),
                };
                prepay_recipe(&expected, budget, statement.span, 0)?;
                statement.semantic = original_site(
                    program,
                    definition,
                    SemanticOriginalNode::Statement(original.id),
                    path,
                    SemanticSiteRecipeKind::DeclareLocal {
                        expected: expected.clone(),
                    },
                    Action::DeclareLocal(expected),
                    SemanticEmissionRole::Binding,
                    budget,
                    statement.span,
                )?;
            }
            (body::OriginalBodyNodeKind::Expression, body_nodes::Node::Expression(expression)) => {
                budget.identity_work(closures.len(), expression.span)?;
                let capture = closures
                    .iter()
                    .find(|closure| closure.definition.id == definition)
                    .and_then(|closure| {
                        closure.uses.iter().find(|usage| usage.node == original.id)
                    });
                budget.identity_work(
                    value_names.len().checked_ilog2().unwrap_or(0) as usize + 1,
                    expression.span,
                )?;
                let observed_name = observation
                    && value_names.binary_search(&original.id).is_ok()
                    && matches!(
                        expression.kind,
                        s::ExprKind::Name(_) | s::ExprKind::ReceiverMember(_)
                    );
                if observed_name {
                    implicit_observation(program, definition, &path, budget, expression.span)?;
                }
                let action = if let Some(usage) = capture {
                    prepay_recipe(&usage.ty, budget, expression.span, 0)?;
                    Some((
                        SemanticSiteRecipeKind::TypeUse {
                            expected: usage.ty.clone(),
                        },
                        Action::TypeUse(usage.ty.clone()),
                        SemanticEmissionRole::Ascription,
                    ))
                } else {
                    match &expression.kind {
                        s::ExprKind::Call { .. } => Some((
                            SemanticSiteRecipeKind::OriginalCall,
                            Action::BodyCall,
                            SemanticEmissionRole::Call,
                        )),
                        // The actual CT wrapper survives value reification.
                        // Its original node anchors the selected query's full
                        // body; the inner stage is not a fake runtime call.
                        s::ExprKind::Name(_) | s::ExprKind::ReceiverMember(_) if observed_name => {
                            let expected = SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: SemanticOriginalTypeSource::Expression(original.id),
                            };
                            prepay_recipe(&expected, budget, expression.span, 0)?;
                            Some((
                                SemanticSiteRecipeKind::TypeUse {
                                    expected: expected.clone(),
                                },
                                Action::TypeUse(expected),
                                SemanticEmissionRole::Ascription,
                            ))
                        }
                        s::ExprKind::MetaEval(_)
                        | s::ExprKind::Unary {
                            op: s::UnaryOp::Borrow | s::UnaryOp::BorrowMut,
                            ..
                        } => {
                            let expected = SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: SemanticOriginalTypeSource::Expression(original.id),
                            };
                            prepay_recipe(&expected, budget, expression.span, 0)?;
                            Some((
                                SemanticSiteRecipeKind::TypeUse {
                                    expected: expected.clone(),
                                },
                                Action::TypeUse(expected),
                                SemanticEmissionRole::Ascription,
                            ))
                        }
                        s::ExprKind::Record { .. }
                        | s::ExprKind::Tuple(_)
                        | s::ExprKind::Array(_) => {
                            let expected = SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: SemanticOriginalTypeSource::Expression(original.id),
                            };
                            prepay_recipe(&expected, budget, expression.span, 0)?;
                            Some((
                                SemanticSiteRecipeKind::Construct {
                                    result: expected.clone(),
                                },
                                Action::Construct(expected),
                                SemanticEmissionRole::Construct,
                            ))
                        }
                        s::ExprKind::Closure { .. } => {
                            budget.identity_work(closures.len(), expression.span)?;
                            let closure = closures
                                .iter()
                                .find(|row| {
                                    row.definition.parent == Some(definition)
                                        && row.path == original.path
                                })
                                .ok_or_else(|| {
                                    error(
                                        expression.span,
                                        "original closure lexical owner is absent",
                                    )
                                })?;
                            for value in &closure.environment {
                                prepay_recipe(value, budget, expression.span, 0)?;
                            }
                            let environment = closure.environment.clone();
                            Some((
                                SemanticSiteRecipeKind::Closure {
                                    definition: closure.definition.id,
                                    environment: environment.clone(),
                                },
                                Action::Closure { environment },
                                SemanticEmissionRole::Closure,
                            ))
                        }
                        _ => None,
                    }
                };
                if let Some((kind, action, role)) = action {
                    expression.semantic = original_site(
                        program,
                        definition,
                        SemanticOriginalNode::Expression(original.id),
                        path,
                        kind,
                        action,
                        role,
                        budget,
                        expression.span,
                    )?;
                }
                if let s::ExprKind::Closure { body, .. } = &mut expression.kind {
                    let closure = closures
                        .iter()
                        .find(|row| {
                            row.definition.parent == Some(definition) && row.path == original.path
                        })
                        .ok_or_else(|| {
                            error(expression.span, "original closure operation has no child")
                        })?;
                    operations(
                        program,
                        closure.definition.id,
                        &mut SourceBody::Closure(body),
                        closures,
                        budget,
                        expression.span,
                    )?;
                }
            }
            _ => {
                return Err(error(
                    span,
                    "original operation kind differs from its recorded syntax",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn function(
    program: &mut program::Program,
    normalizer: &mut Builder<'_, '_, '_, '_>,
    function: &mut s::Function,
    closures: &[closures::ClosureTemplate],
) -> R<()> {
    operations(
        program,
        normalizer.id(normalizer.owner),
        &mut SourceBody::Function(&mut function.body),
        closures,
        normalizer.budget,
        function.span,
    )
}
