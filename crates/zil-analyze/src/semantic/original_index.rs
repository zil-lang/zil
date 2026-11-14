//! Bounded borrowed structure of one original definition. A node or name in
//! this directory is never a type, initialized value, loan or stage receipt.
use crate::{bad, budget::Budget};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{CompileResult, NodeId};
use zil_ir::*;

const WORK: &str = "original definition structural index";
#[derive(Clone, Copy)]
pub(super) enum Node<'a> {
    Expression(&'a SemanticOriginalExpr),
    Statement(&'a SemanticOriginalStatement),
}
pub(super) struct Index<'a> {
    nodes: BTreeMap<NodeId, Node<'a>>,
    erased: BTreeSet<NodeId>,
    navigation: BTreeSet<NodeId>,
}
impl<'a> Index<'a> {
    pub(super) fn erased(&self, id: NodeId, b: &mut Budget) -> CompileResult<bool> {
        b.lookup(self.erased.len(), WORK)?;
        Ok(self.erased.contains(&id))
    }
    pub(super) fn navigation(&self, id: NodeId, b: &mut Budget) -> CompileResult<bool> {
        b.lookup(self.navigation.len(), WORK)?;
        Ok(self.navigation.contains(&id))
    }
    pub(super) fn node(&self, id: NodeId, b: &mut Budget) -> CompileResult<Node<'a>> {
        b.lookup(self.nodes.len(), WORK)?;
        self.nodes
            .get(&id)
            .copied()
            .ok_or_else(|| bad("original node is absent from its definition"))
    }
    pub(super) fn get_expr(
        &self,
        id: NodeId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticOriginalExpr> {
        match self.node(id, b)? {
            Node::Expression(value) => Ok(value),
            Node::Statement(_) => Err(bad("original expression selects a statement node")),
        }
    }
    pub(super) fn get_stmt(
        &self,
        id: NodeId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticOriginalStatement> {
        match self.node(id, b)? {
            Node::Statement(value) => Ok(value),
            Node::Expression(_) => Err(bad("original statement selects an expression node")),
        }
    }
}
enum Work<'a> {
    EndErase,
    Block(&'a SemanticOriginalBlock),
    Expr(&'a SemanticOriginalExpr),
    Stmt(&'a SemanticOriginalStatement),
}
fn push<'a>(stack: &mut Vec<Work<'a>>, value: Work<'a>, b: &mut Budget) -> CompileResult<()> {
    b.spend(1, WORK)?;
    stack
        .try_reserve(1)
        .map_err(|_| bad("original traversal allocation failed"))?;
    stack.push(value);
    Ok(())
}
fn insert<'a>(
    nodes: &mut BTreeMap<NodeId, Node<'a>>,
    id: NodeId,
    node: Node<'a>,
    b: &mut Budget,
) -> CompileResult<()> {
    b.lookup(nodes.len(), WORK)?;
    if nodes.insert(id, node).is_some() {
        return Err(bad(
            "original expression and statement entities reuse a node ID",
        ));
    }
    Ok(())
}
fn walk<'a>(body: &'a SemanticOriginalBlock, b: &mut Budget) -> CompileResult<Index<'a>> {
    let mut nodes = BTreeMap::new();
    let mut erased = BTreeSet::new();
    let mut navigation = BTreeSet::new();
    let mut erased_depth = 0usize;
    let mut stack = Vec::new();
    push(&mut stack, Work::Block(body), b)?;
    while let Some(next) = stack.pop() {
        b.spend(1, WORK)?;
        match next {
            Work::EndErase => {
                erased_depth = erased_depth
                    .checked_sub(1)
                    .ok_or_else(|| bad("original phase stack loses its enclosing stage"))?;
            }
            Work::Block(block) => {
                for statement in block.statements.iter().rev() {
                    push(&mut stack, Work::Stmt(statement), b)?;
                }
            }
            Work::Stmt(statement) => {
                if erased_depth > 0 {
                    b.lookup(erased.len(), WORK)?;
                    erased.insert(statement.id);
                }
                insert(&mut nodes, statement.id, Node::Statement(statement), b)?;
                match &statement.kind {
                    CoreStatementKind::Binding { initializer, .. } => {
                        if let Some(value) = initializer {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                    }
                    CoreStatementKind::TupleBinding { initializer, .. } => {
                        push(&mut stack, Work::Expr(initializer), b)?
                    }
                    CoreStatementKind::Assign { target, value } => {
                        // A bare destination denotes a declared place, not a
                        // read of its (possibly uninitialized) old occupant.
                        if matches!(target.kind, CoreExprKind::Name(_)) {
                            navigation_node(target, &mut navigation, b)?;
                        }
                        push(&mut stack, Work::Expr(value), b)?;
                        push(&mut stack, Work::Expr(target), b)?;
                    }
                    CoreStatementKind::CompoundAssign { target, value, .. } => {
                        push(&mut stack, Work::Expr(value), b)?;
                        push(&mut stack, Work::Expr(target), b)?;
                    }
                    CoreStatementKind::Expression(value) | CoreStatementKind::Yield(value) => {
                        push(&mut stack, Work::Expr(value), b)?
                    }
                    CoreStatementKind::Return(value) => {
                        if let Some(value) = value {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                    }
                    CoreStatementKind::If {
                        condition,
                        then_body,
                        else_body,
                    } => {
                        if let Some(body) = else_body {
                            push(&mut stack, Work::Block(body), b)?;
                        }
                        push(&mut stack, Work::Block(then_body), b)?;
                        push(&mut stack, Work::Expr(condition), b)?;
                    }
                    CoreStatementKind::IfLet {
                        value,
                        then_body,
                        else_body,
                        ..
                    } => {
                        if let Some(body) = else_body {
                            push(&mut stack, Work::Block(body), b)?;
                        }
                        push(&mut stack, Work::Block(then_body), b)?;
                        push(&mut stack, Work::Expr(value), b)?;
                    }
                    CoreStatementKind::While { condition, body } => {
                        push(&mut stack, Work::Block(body), b)?;
                        push(&mut stack, Work::Expr(condition), b)?;
                    }
                    CoreStatementKind::WhileLet { value, body, .. } => {
                        push(&mut stack, Work::Block(body), b)?;
                        push(&mut stack, Work::Expr(value), b)?;
                    }
                    CoreStatementKind::ForRange {
                        start, end, body, ..
                    } => {
                        push(&mut stack, Work::Block(body), b)?;
                        push(&mut stack, Work::Expr(end), b)?;
                        push(&mut stack, Work::Expr(start), b)?;
                    }
                    CoreStatementKind::ForEach { iterator, body, .. } => {
                        push(&mut stack, Work::Block(body), b)?;
                        push(&mut stack, Work::Expr(iterator), b)?;
                    }
                    CoreStatementKind::With { value, body, .. } => {
                        push(&mut stack, Work::Block(body), b)?;
                        push(&mut stack, Work::Expr(value), b)?;
                    }
                    CoreStatementKind::Match { value, arms } => {
                        for arm in arms.iter().rev() {
                            push(&mut stack, Work::Block(&arm.body), b)?;
                        }
                        push(&mut stack, Work::Expr(value), b)?;
                    }
                    CoreStatementKind::Block(body)
                    | CoreStatementKind::Defer(body)
                    | CoreStatementKind::Unsafe(body)
                    | CoreStatementKind::Loop(body) => push(&mut stack, Work::Block(body), b)?,
                    CoreStatementKind::CompileTime(inner) => {
                        erased_depth = erased_depth
                            .checked_add(1)
                            .ok_or_else(|| bad("original phase depth overflow"))?;
                        push(&mut stack, Work::EndErase, b)?;
                        push(&mut stack, Work::Stmt(inner), b)?
                    }
                    CoreStatementKind::Break
                    | CoreStatementKind::Continue
                    | CoreStatementKind::Unsupported(_) => {}
                }
            }
            Work::Expr(expression) => {
                if erased_depth > 0 {
                    b.lookup(erased.len(), WORK)?;
                    erased.insert(expression.id);
                }
                insert(&mut nodes, expression.id, Node::Expression(expression), b)?;
                match &expression.kind {
                    CoreExprKind::MakeDynBorrow { source, .. }
                    | CoreExprKind::MakeDynOwned { source, .. } => {
                        push(&mut stack, Work::Expr(source), b)?
                    }
                    CoreExprKind::Ascribed { value, .. }
                    | CoreExprKind::NumericConvert { value, .. }
                    | CoreExprKind::IntegerConvert { value, .. }
                    | CoreExprKind::Try(value)
                    | CoreExprKind::Spread(value)
                    | CoreExprKind::Group(value)
                    | CoreExprKind::Splice(value)
                    | CoreExprKind::Unary { operand: value, .. } => {
                        if matches!(
                            expression.kind,
                            CoreExprKind::Unary {
                                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                                ..
                            }
                        ) {
                            navigation_node(value, &mut navigation, b)?;
                        }
                        push(&mut stack, Work::Expr(value), b)?
                    }
                    CoreExprKind::CompileTime(value) => {
                        erased_depth = erased_depth
                            .checked_add(1)
                            .ok_or_else(|| bad("original phase depth overflow"))?;
                        push(&mut stack, Work::EndErase, b)?;
                        push(&mut stack, Work::Expr(value), b)?;
                    }
                    CoreExprKind::Binary { left, right, .. } => {
                        push(&mut stack, Work::Expr(right), b)?;
                        push(&mut stack, Work::Expr(left), b)?;
                    }
                    CoreExprKind::Range { start, end } => {
                        push(&mut stack, Work::Expr(end), b)?;
                        push(&mut stack, Work::Expr(start), b)?;
                    }
                    CoreExprKind::Pipeline { input, target } => {
                        push(&mut stack, Work::Expr(target), b)?;
                        push(&mut stack, Work::Expr(input), b)?;
                    }
                    CoreExprKind::TypeApply { callee, .. } => {
                        push(&mut stack, Work::Expr(callee), b)?
                    }
                    CoreExprKind::GenericApply { callee, arguments } => {
                        for argument in arguments.iter().rev() {
                            b.spend(1, WORK)?;
                            match argument {
                                CoreGenericArgument::Const(value) => {
                                    push(&mut stack, Work::Expr(value), b)?
                                }
                                CoreGenericArgument::Type(_) | CoreGenericArgument::Lifetime(_) => {
                                }
                            }
                        }
                        push(&mut stack, Work::Expr(callee), b)?;
                    }
                    CoreExprKind::Call { callee, arguments }
                    | CoreExprKind::CallDynSlot {
                        callee, arguments, ..
                    } => {
                        for argument in arguments.iter().rev() {
                            push(&mut stack, Work::Expr(argument), b)?;
                        }
                        push(&mut stack, Work::Expr(callee), b)?;
                    }
                    CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                        for value in values.iter().rev() {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                    }
                    CoreExprKind::Index { object, index } => {
                        navigation_node(object, &mut navigation, b)?;
                        push(&mut stack, Work::Expr(index), b)?;
                        push(&mut stack, Work::Expr(object), b)?;
                    }
                    CoreExprKind::Slice { object, start, end } => {
                        navigation_node(object, &mut navigation, b)?;
                        if let Some(value) = end {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                        if let Some(value) = start {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                        push(&mut stack, Work::Expr(object), b)?;
                    }
                    CoreExprKind::Member { object, .. } => {
                        navigation_node(object, &mut navigation, b)?;
                        push(&mut stack, Work::Expr(object), b)?;
                    }
                    CoreExprKind::Record { fields, .. }
                    | CoreExprKind::RecordConstructor { fields, .. } => {
                        for (_, value) in fields.iter().rev() {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                    }
                    CoreExprKind::RecordUpdate { base, fields, .. } => {
                        for (_, value) in fields.iter().rev() {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                        push(&mut stack, Work::Expr(base), b)?;
                    }
                    CoreExprKind::Sequence { bindings, value } => {
                        push(&mut stack, Work::Expr(value), b)?;
                        for (_, value) in bindings.iter().rev() {
                            push(&mut stack, Work::Expr(value), b)?;
                        }
                    }
                    CoreExprKind::If {
                        condition,
                        then_value,
                        else_value,
                    } => {
                        push(&mut stack, Work::Expr(else_value), b)?;
                        push(&mut stack, Work::Expr(then_value), b)?;
                        push(&mut stack, Work::Expr(condition), b)?;
                    }
                    CoreExprKind::CompileTimeBlock(body) => {
                        erased_depth = erased_depth
                            .checked_add(1)
                            .ok_or_else(|| bad("original phase depth overflow"))?;
                        push(&mut stack, Work::EndErase, b)?;
                        push(&mut stack, Work::Block(body), b)?;
                    }
                    CoreExprKind::SyntaxQuote { template, .. } => {
                        for hole in template.holes.iter().rev() {
                            push(&mut stack, Work::Expr(&hole.value), b)?;
                        }
                    }
                    // Params/types were fully prepaid above. Child body IDs
                    // belong exclusively to the child's original definition.
                    CoreExprKind::Closure { .. } => {}
                    CoreExprKind::Literal(_)
                    | CoreExprKind::Integer(_)
                    | CoreExprKind::Float(_)
                    | CoreExprKind::Bool(_)
                    | CoreExprKind::String(_)
                    | CoreExprKind::Unit
                    | CoreExprKind::Name(_)
                    | CoreExprKind::ReceiverMember(_)
                    | CoreExprKind::Character(_)
                    | CoreExprKind::SyntaxIsland { .. }
                    | CoreExprKind::Unsupported(_) => {}
                }
            }
        }
    }
    Ok(Index {
        nodes,
        erased,
        navigation,
    })
}

fn navigation_node(
    mut expression: &SemanticOriginalExpr,
    rows: &mut BTreeSet<NodeId>,
    b: &mut Budget,
) -> CompileResult<()> {
    loop {
        b.lookup(rows.len(), WORK)?;
        rows.insert(expression.id);
        if let CoreExprKind::Group(inner) = &expression.kind {
            expression = inner;
        } else {
            break;
        }
    }
    Ok(())
}

// Names are not an inference context. An Infer hidden in a nominal argument,
// associated binding or nested Code/retained type is equally invalid here.
enum Recipe<'a> {
    Actual(&'a SemanticActualRecipe),
    Ty(&'a SemanticTypeRecipe),
    Const(&'a SemanticConstRecipe),
    Signature(&'a SemanticSignatureRecipe),
    Sequence(&'a SemanticClassSequenceRecipe),
    Storage(&'a SemanticStorageShapeRecipe),
    Retained(&'a SemanticRetainedShapeRecipe),
}
fn recipe_push<'a>(
    stack: &mut Vec<Recipe<'a>>,
    value: Recipe<'a>,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(1, WORK)?;
    stack
        .try_reserve(1)
        .map_err(|_| bad("original name recipe traversal allocation failed"))?;
    stack.push(value);
    Ok(())
}
fn arguments<'a>(
    stack: &mut Vec<Recipe<'a>>,
    values: &'a [SemanticActualRecipe],
    b: &mut Budget,
) -> CompileResult<()> {
    for value in values {
        recipe_push(stack, Recipe::Actual(value), b)?;
    }
    Ok(())
}
fn retained_parts<'a>(
    stack: &mut Vec<Recipe<'a>>,
    retained: &'a [SemanticRetainedPart],
    nominal: &'a [SemanticNominalPart],
    b: &mut Budget,
) -> CompileResult<()> {
    for part in retained {
        b.spend(1, WORK)?;
        match part {
            SemanticRetainedPart::All { parents, .. } => {
                recipe_push(stack, Recipe::Sequence(parents), b)?
            }
            SemanticRetainedPart::One { .. } => {}
        }
    }
    for part in nominal {
        b.spend(1, WORK)?;
        match part {
            SemanticNominalPart::FromType { ty, classes, .. } => {
                recipe_push(stack, Recipe::Ty(ty), b)?;
                recipe_push(stack, Recipe::Sequence(classes), b)?;
            }
            SemanticNominalPart::One { .. } => {}
        }
    }
    Ok(())
}
fn no_infer(value: &SemanticActualRecipe, b: &mut Budget) -> CompileResult<()> {
    let mut stack = Vec::new();
    recipe_push(&mut stack, Recipe::Actual(value), b)?;
    while let Some(value) = stack.pop() {
        b.spend(1, WORK)?;
        match value {
            Recipe::Actual(value) => match value {
                SemanticActualRecipe::Infer => {
                    return Err(bad(
                        "original name binding cannot contain a Call inference slot",
                    ));
                }
                SemanticActualRecipe::Type(value) => recipe_push(&mut stack, Recipe::Ty(value), b)?,
                SemanticActualRecipe::Const { carrier, value } => {
                    recipe_push(&mut stack, Recipe::Ty(carrier), b)?;
                    recipe_push(&mut stack, Recipe::Const(value), b)?;
                }
                SemanticActualRecipe::Life(_) => {}
            },
            Recipe::Ty(value) => match value {
                SemanticTypeRecipe::Nominal {
                    arguments: values, ..
                } => arguments(&mut stack, values, b)?,
                SemanticTypeRecipe::Closure {
                    arguments: values,
                    environment,
                    ..
                } => {
                    arguments(&mut stack, values, b)?;
                    arguments(&mut stack, environment, b)?;
                }
                SemanticTypeRecipe::Opaque {
                    arguments: values,
                    hidden,
                    ..
                } => {
                    arguments(&mut stack, values, b)?;
                    recipe_push(&mut stack, Recipe::Ty(hidden), b)?;
                }
                SemanticTypeRecipe::AssociatedType {
                    receiver,
                    application,
                    ..
                } => {
                    recipe_push(&mut stack, Recipe::Ty(receiver), b)?;
                    arguments(&mut stack, &application.arguments, b)?;
                }
                SemanticTypeRecipe::Tuple(values) | SemanticTypeRecipe::AnonymousSum(values) => {
                    for value in values {
                        recipe_push(&mut stack, Recipe::Ty(value), b)?;
                    }
                }
                SemanticTypeRecipe::Array { element, length } => {
                    recipe_push(&mut stack, Recipe::Ty(element), b)?;
                    recipe_push(&mut stack, Recipe::Const(length), b)?;
                }
                SemanticTypeRecipe::Slice(value)
                | SemanticTypeRecipe::Managed(value)
                | SemanticTypeRecipe::OwnedStore(value)
                | SemanticTypeRecipe::Mutex(value)
                | SemanticTypeRecipe::Registration(value)
                | SemanticTypeRecipe::AsyncRegistration(value)
                | SemanticTypeRecipe::Address { inner: value, .. }
                | SemanticTypeRecipe::Desc { inner: value, .. } => {
                    recipe_push(&mut stack, Recipe::Ty(value), b)?
                }
                SemanticTypeRecipe::MutexGuard { payload, retained } => {
                    recipe_push(&mut stack, Recipe::Ty(payload), b)?;
                    recipe_push(&mut stack, Recipe::Retained(retained), b)?;
                }
                SemanticTypeRecipe::Task { result, retained } => {
                    recipe_push(&mut stack, Recipe::Ty(result), b)?;
                    recipe_push(&mut stack, Recipe::Retained(retained), b)?;
                }
                SemanticTypeRecipe::ErrorUnion { error, result } => {
                    recipe_push(&mut stack, Recipe::Ty(error), b)?;
                    recipe_push(&mut stack, Recipe::Ty(result), b)?;
                }
                SemanticTypeRecipe::Qualified { storage, life } => {
                    recipe_push(&mut stack, Recipe::Ty(storage), b)?;
                    recipe_push(&mut stack, Recipe::Storage(life), b)?;
                }
                SemanticTypeRecipe::Code { signature, .. } => {
                    recipe_push(&mut stack, Recipe::Signature(signature), b)?
                }
                SemanticTypeRecipe::Dyn { interface, .. } => {
                    arguments(&mut stack, &interface.application.arguments, b)?;
                    match &interface.context {
                        SemanticDynContextRecipe::Type(value) => {
                            recipe_push(&mut stack, Recipe::Ty(value), b)?
                        }
                        SemanticDynContextRecipe::Shape(value) => {
                            recipe_push(&mut stack, Recipe::Retained(value), b)?;
                        }
                        SemanticDynContextRecipe::Closed => {}
                    }
                    for binding in &interface.associated {
                        b.spend(1, WORK)?;
                        arguments(&mut stack, &binding.application.arguments, b)?;
                        match &binding.value {
                            SemanticAssociatedRecipe::Type(value) => {
                                recipe_push(&mut stack, Recipe::Ty(value), b)?
                            }
                            SemanticAssociatedRecipe::Const { carrier, value } => {
                                recipe_push(&mut stack, Recipe::Ty(carrier), b)?;
                                recipe_push(&mut stack, Recipe::Const(value), b)?;
                            }
                        }
                    }
                }
                SemanticTypeRecipe::Atom(_)
                | SemanticTypeRecipe::Exact(_)
                | SemanticTypeRecipe::Formal(_)
                | SemanticTypeRecipe::Environment { .. }
                | SemanticTypeRecipe::SelfType { .. }
                | SemanticTypeRecipe::OriginalType { .. }
                | SemanticTypeRecipe::UnresolvedNamed(_)
                | SemanticTypeRecipe::Unsupported(_) => {}
            },
            Recipe::Const(value) => match value {
                SemanticConstRecipe::Unary {
                    operand_type,
                    value,
                    ..
                } => {
                    recipe_push(&mut stack, Recipe::Ty(operand_type), b)?;
                    recipe_push(&mut stack, Recipe::Const(value), b)?;
                }
                SemanticConstRecipe::Binary {
                    operand_type,
                    left,
                    right,
                    ..
                } => {
                    recipe_push(&mut stack, Recipe::Ty(operand_type), b)?;
                    recipe_push(&mut stack, Recipe::Const(left), b)?;
                    recipe_push(&mut stack, Recipe::Const(right), b)?;
                }
                SemanticConstRecipe::Convert {
                    source_type,
                    target_type,
                    value,
                } => {
                    recipe_push(&mut stack, Recipe::Ty(source_type), b)?;
                    recipe_push(&mut stack, Recipe::Ty(target_type), b)?;
                    recipe_push(&mut stack, Recipe::Const(value), b)?;
                }
                SemanticConstRecipe::AssociatedConst {
                    receiver,
                    application,
                    ..
                } => {
                    recipe_push(&mut stack, Recipe::Ty(receiver), b)?;
                    arguments(&mut stack, &application.arguments, b)?;
                }
                SemanticConstRecipe::Value(_)
                | SemanticConstRecipe::IntegerLiteral(_)
                | SemanticConstRecipe::FloatLiteral(_)
                | SemanticConstRecipe::Formal(_)
                | SemanticConstRecipe::Environment { .. } => {}
            },
            Recipe::Signature(value) => {
                for parameter in &value.parameters {
                    recipe_push(&mut stack, Recipe::Ty(&parameter.ty), b)?;
                }
                recipe_push(&mut stack, Recipe::Ty(&value.result), b)?;
            }
            Recipe::Sequence(value) => {
                for part in &value.parts {
                    b.spend(1, WORK)?;
                    match part {
                        SemanticClassSequencePart::AllClasses { ty, .. } => {
                            recipe_push(&mut stack, Recipe::Ty(ty), b)?
                        }
                        SemanticClassSequencePart::One(_) => {}
                    }
                }
            }
            Recipe::Storage(value) => {
                recipe_push(&mut stack, Recipe::Sequence(&value.domain), b)?;
                retained_parts(&mut stack, &value.retained, &value.nominal, b)?;
            }
            Recipe::Retained(value) => {
                recipe_push(&mut stack, Recipe::Sequence(&value.domain), b)?;
                for port in &value.ports {
                    b.spend(1, WORK)?;
                    match port {
                        SemanticPortPart::One { leaf, .. } => {
                            recipe_push(&mut stack, Recipe::Ty(leaf), b)?
                        }
                        SemanticPortPart::FromType { ty, classes } => {
                            recipe_push(&mut stack, Recipe::Ty(ty), b)?;
                            recipe_push(&mut stack, Recipe::Sequence(classes), b)?;
                        }
                    }
                }
                retained_parts(&mut stack, &value.retained, &value.nominal, b)?;
            }
        }
    }
    Ok(())
}
fn names(values: &[SemanticOriginalNameBinding], b: &mut Budget) -> CompileResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        b.spend(1, WORK)?;
        if value.path.is_empty() || value.path.iter().any(String::is_empty) {
            return Err(bad("original name binding has an empty path component"));
        }
        let namespace = match value.namespace {
            SemanticOriginalNamespace::Value => 0u8,
            SemanticOriginalNamespace::Type => 1,
            SemanticOriginalNamespace::Lifetime => 2,
        };
        let width = value
            .path
            .iter()
            .try_fold(value.path.len(), |width, part| {
                width.checked_add(part.len())
            })
            .ok_or_else(|| bad("original name comparison size overflows"))?;
        b.product(
            width,
            seen.len().checked_ilog2().map_or(1, |n| n as usize + 2),
            1,
            WORK,
        )?;
        if !seen.insert((namespace, value.path.as_slice())) {
            return Err(bad("original name path and namespace are bound twice"));
        }
        match (&value.namespace, &value.value) {
            (
                SemanticOriginalNamespace::Value | SemanticOriginalNamespace::Type,
                SemanticOriginalNameValue::Definition(_),
            ) => {}
            (
                SemanticOriginalNamespace::Value,
                SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Const { .. }),
            )
            | (
                SemanticOriginalNamespace::Type,
                SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Type(_)),
            )
            | (
                SemanticOriginalNamespace::Lifetime,
                SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Life(_)),
            ) => no_infer(recipe, b)?,
            _ => {
                return Err(bad(
                    "original name namespace disagrees with its declared recipe kind",
                ));
            }
        }
    }
    Ok(())
}
fn role(kind: &SemanticSiteRecipeKind) -> u8 {
    match kind {
        SemanticSiteRecipeKind::Construct { .. } => 0,
        SemanticSiteRecipeKind::Call { .. }
        | SemanticSiteRecipeKind::CallableCall { .. }
        | SemanticSiteRecipeKind::OriginalCall => 1,
        SemanticSiteRecipeKind::TypeUse { .. } => 2,
        SemanticSiteRecipeKind::DeclareLocal { .. } => 3,
        SemanticSiteRecipeKind::Closure { .. } => 4,
    }
}
fn event(kind: &SemanticSiteRecipeKind, node: Node<'_>) -> CompileResult<()> {
    match (kind, node) {
        (SemanticSiteRecipeKind::DeclareLocal { .. }, Node::Statement(_))
        | (
            SemanticSiteRecipeKind::Construct { .. } | SemanticSiteRecipeKind::TypeUse { .. },
            Node::Expression(_),
        ) => Ok(()),
        (SemanticSiteRecipeKind::Closure { .. }, Node::Expression(value))
            if matches!(value.kind, CoreExprKind::Closure { .. }) =>
        {
            Ok(())
        }
        (
            SemanticSiteRecipeKind::Call { .. }
            | SemanticSiteRecipeKind::CallableCall { .. }
            | SemanticSiteRecipeKind::OriginalCall,
            Node::Expression(value),
        ) if matches!(
            value.kind,
            CoreExprKind::Call { .. }
                | CoreExprKind::CallDynSlot { .. }
                | CoreExprKind::Pipeline { .. }
        ) =>
        {
            Ok(())
        }
        _ => Err(bad(
            "original site event role does not match its node entity",
        )),
    }
}
pub(super) fn index<'a>(
    definition: &'a SemanticDefinition,
    b: &mut Budget,
) -> CompileResult<Index<'a>> {
    // Includes all Type/Const/path/name/body metadata before any lookup,
    // comparison or indexing allocation. This structural visit grants nothing.
    crate::dyn_metadata::recipe(definition, b)?;
    let SemanticDefinitionBody::FunctionWithBody {
        sites,
        site_nodes,
        names: bindings,
        body,
        ..
    } = &definition.body
    else {
        return Err(bad(
            "original operation index requires a complete original function body",
        ));
    };
    names(bindings, b)?;
    let output = match body {
        SemanticOriginalFunctionBody::Defined(body) => walk(body, b)?,
        SemanticOriginalFunctionBody::External | SemanticOriginalFunctionBody::Declaration => {
            Index {
                nodes: BTreeMap::new(),
                erased: BTreeSet::new(),
                navigation: BTreeSet::new(),
            }
        }
    };
    let mut recipes = BTreeMap::new();
    for recipe in sites {
        b.lookup(recipes.len(), WORK)?;
        if recipes.insert(recipe.id.0, recipe).is_some() {
            return Err(bad("original site recipe ID is duplicated"));
        }
    }
    let mut bound = BTreeSet::new();
    let mut events = BTreeSet::new();
    for binding in site_nodes {
        b.lookup(bound.len(), WORK)?;
        if !bound.insert(binding.site.0) {
            return Err(bad("original site is rebound to a second node"));
        }
        b.lookup(recipes.len(), WORK)?;
        let recipe = recipes
            .get(&binding.site.0)
            .ok_or_else(|| bad("original site node has no original recipe"))?;
        let (id, node) = match binding.node {
            SemanticOriginalNode::Expression(id) => (id, Node::Expression(output.get_expr(id, b)?)),
            SemanticOriginalNode::Statement(id) => (id, Node::Statement(output.get_stmt(id, b)?)),
        };
        event(&recipe.kind, node)?;
        b.lookup(events.len(), WORK)?;
        if !events.insert((id, role(&recipe.kind))) {
            return Err(bad("original node repeats the same event role"));
        }
    }
    if bound.len() != recipes.len() {
        return Err(bad("original recipe is missing its unique node binding"));
    }
    Ok(output)
}
