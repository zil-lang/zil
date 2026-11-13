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
        }
    }
    Ok(())
}
