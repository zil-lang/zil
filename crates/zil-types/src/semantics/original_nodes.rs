//! Original node directory, independent of actual Core/Typed numeric IDs.
use crate::*;
use std::collections::BTreeMap;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Binding,
    Statement,
    Call,
    Closure,
    Construct,
    ConstructorCandidate,
    Expression,
}
fn fail(message: &str) -> Diagnostic {
    error("E-SEMANTIC-ORIGINAL-NODE", Span::default(), message)
}
struct Index<'a> {
    nodes: BTreeMap<zil_base::NodeId, Kind>,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl super::original_walk::Context for Index<'_> {
    fn descend_closure(&self) -> bool {
        false
    }
    fn node(&mut self, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original body index work exceeded", None)
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original body index depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, _: usize) -> R<()> {
        Ok(())
    }
    fn statement(
        &mut self,
        id: zil_base::NodeId,
        kind: &SemanticOriginalStatementKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        self.insert(
            id,
            if matches!(
                kind,
                CoreStatementKind::Binding { .. } | CoreStatementKind::TupleBinding { .. }
            ) {
                Kind::Binding
            } else {
                Kind::Statement
            },
        )
    }
    fn expression(
        &mut self,
        id: zil_base::NodeId,
        kind: &SemanticOriginalExprKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        let kind = match kind {
            CoreExprKind::Call { .. } | CoreExprKind::CallDynSlot { .. } => Kind::Call,
            CoreExprKind::Closure { .. } => Kind::Closure,
            CoreExprKind::Record { .. }
            | CoreExprKind::RecordUpdate { .. }
            | CoreExprKind::Tuple(_)
            | CoreExprKind::Array(_) => Kind::Construct,
            CoreExprKind::Member { .. } => Kind::ConstructorCandidate,
            CoreExprKind::TypeApply { callee, .. } | CoreExprKind::GenericApply { callee, .. }
                if matches!(callee.kind, CoreExprKind::Member { .. }) =>
            {
                Kind::ConstructorCandidate
            }
            _ => Kind::Expression,
        };
        self.insert(id, kind)
    }
}
impl Index<'_> {
    fn insert(&mut self, id: zil_base::NodeId, kind: Kind) -> R<()> {
        *self.left = self
            .left
            .checked_sub(self.nodes.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original node identity index exceeded", None)
            })?;
        if self.nodes.insert(id, kind).is_some() {
            return Err(fail(
                "one original definition reuses an expression/statement node identity",
            ));
        }
        Ok(())
    }
}
pub(super) fn definition(
    definition: &SemanticDefinition,
    limits: &Limits,
    left: &mut usize,
) -> R<BTreeMap<zil_base::NodeId, Kind>> {
    let SemanticDefinitionBody::FunctionWithBody {
        signature,
        sites,
        parameters,
        receiver_parameter,
        site_nodes,
        body,
        ..
    } = &definition.body
    else {
        return Ok(BTreeMap::new());
    };
    if parameters.len() != signature.parameters.len() {
        return Err(fail(
            "original ordered parameter binders differ from original signature",
        ));
    }
    if receiver_parameter.is_some_and(|index| index as usize >= parameters.len())
        || receiver_parameter.is_some() && definition.kind != SemanticDefinitionKind::Method
    {
        return Err(fail(
            "original receiver is not a real source method parameter",
        ));
    }
    for (index, parameter) in parameters.iter().enumerate() {
        *left = left
            .checked_sub(index.saturating_add(parameter.name.len()))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original parameter binder work exceeded",
                    None,
                )
            })?;
        if parameters[..index]
            .iter()
            .any(|old| old.name == parameter.name)
        {
            return Err(fail(
                "original ordered parameters reuse one lexical binder name",
            ));
        }
    }
    crate::dyn_metadata::semantic_operand_snapshot(&definition.body, limits, left, 1)?;
    let mut index = Index {
        nodes: BTreeMap::new(),
        limits,
        left,
    };
    if let SemanticOriginalFunctionBody::Defined(body) = body {
        let mut body = body.clone();
        super::original_walk::block(&mut body, &mut index, 0)?;
    }
    if site_nodes.len() != sites.len() {
        return Err(fail(
            "original operation sites lack a complete explicit body-node directory",
        ));
    }
    let mut seen = BTreeMap::new();
    for row in site_nodes {
        *index.left = index
            .left
            .checked_sub(sites.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original site-node work exceeded", None)
            })?;
        let recipe = sites
            .iter()
            .find(|s| s.id == row.site)
            .ok_or_else(|| fail("original node directory names an absent operation site"))?;
        if seen.insert(row.site, row.node).is_some() {
            return Err(fail("original node directory repeats an operation site"));
        }
        let (node, statement) = match row.node {
            SemanticOriginalNode::Expression(node) => (node, false),
            SemanticOriginalNode::Statement(node) => (node, true),
        };
        let kind = *index
            .nodes
            .get(&node)
            .ok_or_else(|| fail("original operation site names an absent original body node"))?;
        let valid = match recipe.kind {
            SemanticSiteRecipeKind::DeclareLocal { .. } => statement && kind == Kind::Binding,
            SemanticSiteRecipeKind::Call { .. }
            | SemanticSiteRecipeKind::CallableCall { .. }
            | SemanticSiteRecipeKind::OriginalCall => !statement && kind == Kind::Call,
            SemanticSiteRecipeKind::Closure { .. } => !statement && kind == Kind::Closure,
            SemanticSiteRecipeKind::Construct { .. } => {
                // This is only the node grammar gate. Original Gamma must
                // independently produce the exact nominal/variant receipt
                // for a bare member constructor before it can complete.
                !statement
                    && matches!(
                        kind,
                        Kind::Construct | Kind::Call | Kind::ConstructorCandidate
                    )
            }
            SemanticSiteRecipeKind::TypeUse { .. } => {
                !statement && !matches!(kind, Kind::Statement | Kind::Binding)
            }
        };
        if !valid {
            return Err(fail(
                "original operation site changes the actual original node kind",
            ));
        }
    }
    Ok(index.nodes)
}
