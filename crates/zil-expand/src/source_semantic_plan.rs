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
// Type-only stages have original sites too. These rows describe actual
// original operations, not runtime mint obligations: no ledger event or AST
// proof token is created for an operation which a CT rewrite may consume.
fn directory_site(
    program: &mut program::Program,
    definition: SemanticDefinitionId,
    node: SemanticOriginalNode,
    path: Vec<u32>,
    kind: SemanticSiteRecipeKind,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    let row = program
        .modules
        .iter_mut()
        .find(|(id, _)| *id == definition.module)
        .and_then(|(_, module)| {
            module
                .definitions
                .iter_mut()
                .find(|row| row.id == definition)
        })
        .ok_or_else(|| error(span, "type-stage original owner is absent"))?;
    let SemanticDefinitionBody::FunctionWithBody {
        sites, site_nodes, ..
    } = &mut row.body
    else {
        return Err(error(
            span,
            "type-stage original owner has no complete body",
        ));
    };
    let site = SemanticSiteId(
        u32::try_from(sites.len())
            .map_err(|_| error(span, "type-stage original site namespace exhausted"))?,
    );
    budget.lineage_push(
        sites,
        SemanticSiteRecipe {
            id: site,
            occurrence_path: path,
            kind,
        },
        span,
    )?;
    budget.lineage_push(site_nodes, SemanticOriginalSiteNode { site, node }, span)
}
// Only a projection used as a value owns a value Ascription. Prefixes and
// places used to form an address have no separate field-value SSA result.
// Names retain their independent actual Local/Parameter/Capture receipts;
// this classification is used only for projected value observations.
fn projection_value(
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
        match parent.node {
            body_nodes::Node::Statement(statement) => {
                return Ok(!matches!(&statement.kind,s::StmtKind::Assign {..} if child==0));
            }
            body_nodes::Node::Expression(expression) => match &expression.kind {
                s::ExprKind::Group(_) if child == 0 => length -= 1,
                s::ExprKind::Call { .. }
                | s::ExprKind::TypeApply { .. }
                | s::ExprKind::Member { .. }
                | s::ExprKind::Index { .. }
                | s::ExprKind::Unary {
                    op: s::UnaryOp::Borrow | s::UnaryOp::BorrowMut,
                    ..
                } if child == 0 => return Ok(false),
                // The index expression is still evaluated as a value even
                // when the indexed object is a borrowed or assigned place.
                _ => return Ok(true),
            },
            body_nodes::Node::NonRuntime => return Ok(false),
        }
    }
    Ok(true)
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
// Assignment navigation selects a place; it does not evaluate a whole
// aggregate value at each Name/Member prefix. Index operands and explicit
// dereference operands remain genuine value evaluations.
fn assignment_navigation(
    body: &mut SourceBody<'_>,
    path: &[u32],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<bool> {
    let mut length = path.len();
    while length > 0 {
        let child = path[length - 1];
        let Some(parent) = body.locate(&path[..length - 1], budget, span)? else {
            return Ok(false);
        };
        match parent.node {
            body_nodes::Node::Statement(statement) => {
                return Ok(matches!(&statement.kind, s::StmtKind::Assign { .. } if child == 0));
            }
            body_nodes::Node::Expression(expression) => match &expression.kind {
                s::ExprKind::Group(_) | s::ExprKind::Member { .. } | s::ExprKind::Index { .. }
                    if child == 0 =>
                {
                    length -= 1
                }
                _ => return Ok(false),
            },
            body_nodes::Node::NonRuntime => return Ok(false),
        }
    }
    Ok(false)
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
        let projection_value = projection_value(body, &original.path, budget, span)?;
        let navigation = assignment_navigation(body, &original.path, budget, span)?;
        let Some(located) = body.locate(&original.path, budget, span)? else {
            return Err(error(
                span,
                "original operation no longer identifies its actual syntax node",
            ));
        };
        if located.child || matches!(located.node, body_nodes::Node::NonRuntime) {
            continue;
        }
        let runtime = located.runtime;
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
                if !runtime {
                    directory_site(
                        program,
                        definition,
                        SemanticOriginalNode::Statement(original.id),
                        path,
                        SemanticSiteRecipeKind::DeclareLocal { expected },
                        budget,
                        statement.span,
                    )?;
                    continue;
                }
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
                let observed_projection = observation
                    && projection_value
                    && matches!(
                        expression.kind,
                        s::ExprKind::Member { .. } | s::ExprKind::TypeApply { .. }
                    );
                if observed_name || observed_projection {
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
                        s::ExprKind::Member { .. } | s::ExprKind::TypeApply { .. }
                            if observed_projection =>
                        {
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
                            implicit_observation(
                                program,
                                definition,
                                &path,
                                budget,
                                expression.span,
                            )?;
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
                        s::ExprKind::Closure { .. } if !runtime => {
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
                    if !runtime
                        || (navigation && matches!(kind, SemanticSiteRecipeKind::TypeUse { .. }))
                    {
                        directory_site(
                            program,
                            definition,
                            SemanticOriginalNode::Expression(original.id),
                            path,
                            kind,
                            budget,
                            expression.span,
                        )?;
                        continue;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    fn prepare(text: &str) -> (Vec<s::Module>, program::Program) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "original-stage-sites.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        program::prepare(&[&parsed], &origins, &[Vec::new()], &mut limits.clone()).unwrap()
    }

    #[test]
    fn ct_inner_calls_have_original_sites_without_runtime_mint_tokens() {
        let (mut syntax, program) = prepare(
            "fn inc(value:i32)->i32{return value+1}fn main()->i32{let folded=#((inc(41),42));return folded.0}",
        );
        let main = program.modules[0]
            .1
            .definitions
            .iter()
            .find(|row| row.id.index == 1)
            .unwrap();
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &main.body
        else {
            panic!()
        };
        let calls: Vec<_> = sites
            .iter()
            .filter(|site| matches!(site.kind, SemanticSiteRecipeKind::OriginalCall))
            .collect();
        assert_eq!(calls.len(), 1);
        let site = calls[0];
        assert!(
            site_nodes.iter().any(|row| row.site == site.id
                && matches!(row.node, SemanticOriginalNode::Expression(_)))
        );
        let s::ItemKind::Function(main) = &mut syntax[0].items[1].kind else {
            panic!()
        };
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut body = SourceBody::Function(&mut main.body);
        let located = body
            .locate(&site.occurrence_path[1..], &mut budget, main.span)
            .unwrap()
            .unwrap();
        assert!(!located.runtime);
        let body_nodes::Node::Expression(expression) = located.node else {
            panic!()
        };
        assert!(matches!(expression.kind, s::ExprKind::Call { .. }));
        assert!(expression.semantic.raw_index().is_none());
        let wrapper = sites
            .iter()
            .filter(|site| matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. }))
            .find(|site| {
                let located = body
                    .locate(&site.occurrence_path[1..], &mut budget, main.span)
                    .unwrap()
                    .unwrap();
                matches!(
                    located.node,
                    body_nodes::Node::Expression(s::Expr {
                        kind: s::ExprKind::MetaEval(_),
                        ..
                    })
                )
            })
            .unwrap();
        let located = body
            .locate(&wrapper.occurrence_path[1..], &mut budget, main.span)
            .unwrap()
            .unwrap();
        let body_nodes::Node::Expression(expression) = located.node else {
            panic!()
        };
        assert!(expression.semantic.raw_index().is_some());
    }

    #[test]
    fn function_value_apply_has_one_outer_value_site_and_no_callee_token() {
        let (mut syntax, program) = prepare(
            "fn identity<T:Copy>(value:T)->T{return value}fn main()->i32{let callbacks=[identity::<i32>];return callbacks[0](42)}",
        );
        let main = program.modules[0]
            .1
            .definitions
            .iter()
            .find(|row| row.id.index == 1)
            .unwrap();
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &main.body else {
            panic!()
        };
        let s::ItemKind::Function(main) = &mut syntax[0].items[1].kind else {
            panic!()
        };
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut body = SourceBody::Function(&mut main.body);
        let mut applies = 0;
        for site in sites {
            if !matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. }) {
                continue;
            }
            let located = body
                .locate(&site.occurrence_path[1..], &mut budget, main.span)
                .unwrap()
                .unwrap();
            if let body_nodes::Node::Expression(s::Expr {
                kind: s::ExprKind::TypeApply { callee, .. },
                semantic,
                ..
            }) = located.node
            {
                applies += 1;
                assert!(semantic.raw_index().is_some());
                assert!(callee.semantic.raw_index().is_none());
            }
        }
        assert_eq!(applies, 1);
    }

    #[test]
    fn observed_resource_field_has_its_own_potential_head_occurrence() {
        let (mut syntax, program) = prepare(
            "resource R{code:i32}package Owner{manages item:R}fn observe(owner:&Owner){let descriptor=owner.item;return ()}",
        );
        let definition = program.modules[0]
            .1
            .definitions
            .iter()
            .find(|row| row.id.index == 2)
            .unwrap();
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &definition.body else {
            panic!()
        };
        let s::ItemKind::Function(function) = &mut syntax[0].items[2].kind else {
            panic!()
        };
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut body = SourceBody::Function(&mut function.body);
        let mut projections = 0;
        for site in sites {
            if !matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. }) {
                continue;
            }
            let located = body
                .locate(&site.occurrence_path[1..], &mut budget, function.span)
                .unwrap()
                .unwrap();
            if matches!(
                located.node,
                body_nodes::Node::Expression(s::Expr {
                    kind: s::ExprKind::Member { .. },
                    ..
                })
            ) {
                projections += 1;
                assert!(
                    definition
                        .implicit_lifetimes
                        .iter()
                        .any(|implicit| implicit.occurrence_path == site.occurrence_path)
                );
            }
        }
        assert_eq!(projections, 1);
    }

    #[test]
    fn borrowed_projection_places_do_not_invent_field_value_events() {
        let (mut syntax, program) = prepare(
            "struct Cell{n:i32}struct Holder{left:Cell;table:[Cell;2];index:i32}fn examine(value:&mut Holder){let first=&value.left;let second=&mut (value.left);let third=&value.table[value.index];let fourth=&mut ((value.table[0]));let read=value.left.n;(value.left).n=read;return ()}",
        );
        let definition = program.modules[0]
            .1
            .definitions
            .iter()
            .find(|row| row.id.index == 2)
            .unwrap();
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &definition.body else {
            panic!()
        };
        let s::ItemKind::Function(function) = &mut syntax[0].items[2].kind else {
            panic!()
        };
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut body = SourceBody::Function(&mut function.body);
        let mut borrow_sites = 0;
        let mut observed_fields = Vec::new();
        let mut observed_names = 0;
        for site in sites {
            if !matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. }) {
                continue;
            }
            let located = body
                .locate(&site.occurrence_path[1..], &mut budget, function.span)
                .unwrap()
                .unwrap();
            let body_nodes::Node::Expression(expression) = located.node else {
                continue;
            };
            match &expression.kind {
                s::ExprKind::Unary {
                    op: s::UnaryOp::Borrow | s::UnaryOp::BorrowMut,
                    ..
                } => {
                    borrow_sites += 1;
                    assert!(expression.semantic.raw_index().is_some());
                    assert!(
                        definition
                            .implicit_lifetimes
                            .iter()
                            .any(|implicit| implicit.occurrence_path == site.occurrence_path)
                    );
                }
                s::ExprKind::Member {
                    member: s::MemberName::Name(name),
                    ..
                } => observed_fields.push(name.clone()),
                s::ExprKind::Name(name) if name == "value" => observed_names += 1,
                _ => {}
            }
        }
        assert_eq!(borrow_sites, 4);
        // The true index operand and ordinary field read still emit values.
        // Neither grouped borrow places nor the assignment destination do.
        assert_eq!(observed_fields, ["index", "n"]);
        assert!(observed_names > 0);
    }
}
