//! Static foreign constructors are complete original literal operations.
use super::*;

impl Solver<'_, '_> {
    pub(super) fn ffi_literal(
        &mut self,
        definition: SemanticDefinitionId,
        node: NodeId,
        literal: &Constant,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        if let Constant::WideString(units) = literal {
            b.spend(units.len(), WORK)?;
            if !crate::budget::valid_wide_string(units) {
                return Err(fail(
                    "original foreign constant contains invalid static UTF-16",
                ));
            }
        }
        let owner = self.inventory.definition(definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &owner.body else {
            return Err(fail("original foreign constant has no body directory"));
        };
        let index = self.inventory.original_index(definition, b)?;
        let mut site = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.node == SemanticOriginalNode::Expression(node)
                && matches!(
                    index.recipe(link.site, b)?.kind,
                    SemanticSiteRecipeKind::TypeUse { .. }
                )
                && site.replace(link.site).is_some()
            {
                return Err(fail("original foreign constant has competing value sites"));
            }
        }
        let site = site.ok_or_else(|| fail("original foreign constant has no value site"))?;
        b.lookup(self.state.ffi_literals.len(), WORK)?;
        self.state.ffi_literals.insert((definition, site), node);
        self.atom(literal.ty(), b)
    }
}

impl Checked {
    pub(in crate::semantic) fn ffi_literal_at<'a>(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'a>,
        b: &mut Budget,
    ) -> CompileResult<Option<&'a Constant>> {
        if self.ffi_literals.is_empty() {
            return Ok(None);
        }
        b.lookup(self.ffi_literals.len(), WORK)?;
        let Some(&node) = self.ffi_literals.get(&(instance.definition, site)) else {
            return Ok(None);
        };
        let index = inventory.original_index(instance.definition, b)?;
        let CoreExprKind::Literal(literal) = &index.get_expr(node, b)?.kind else {
            return Err(fail("original foreign constant changes its operation"));
        };
        Ok(Some(literal))
    }
}
