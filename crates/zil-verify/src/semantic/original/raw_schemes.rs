//! Original associated inventories are part of the universal source programme,
//! including unused definitions. They are not runtime tables or permissions.
use super::*;

impl CheckedOriginal {
    pub(super) fn raw_source_schemes(&mut self, module: &TypedModule, context: &CheckedContext, active: &mut ActiveQueries, b: &mut Budget) -> Check<()> {
        let mut identities = BTreeSet::new();
        for declaration in &module.dyn_declarations {
            dynamic_metadata::declaration(declaration, b)?;
            if !identities.insert(declaration.id) { return Err(bad("original trait/impl inventory repeats a declaration identity")); }
            let scope = dynamic_witness::semantic_external::original_scope(module, declaration.id, b)?;
            self.current = Some(scope.id);
            let env = self.environment(module, context, scope.id, active, b)?;
            if !matches!((&scope.body, declaration.kind),
                (SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration: id }), DynDeclarationKind::Trait)
                | (SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration: id, .. } | SemanticScopeRecipe::InherentImpl { declaration: id, .. }), DynDeclarationKind::Impl) if *id == declaration.id) {
                return Err(bad("original dynamic declaration changes its exact lexical owner/kind"));
            }
            let requirements = declaration.requirements.as_ref().ok_or_else(|| unsupported("original dynamic requirement inventory is Unknown", None))?;
            for row in dynamic_witness::semantic_external::original_requirement_recipes(module, declaration.id, requirements, b)? {
                self.scheme_requirement(module, context, &row, &env, active, b)?;
            }
            let rows = declaration.associated_definitions.as_ref().ok_or_else(|| unsupported("original associated member inventory is Unknown", None))?;
            b.spend(rows.len().saturating_add(declaration.members.len()))?;
            let mut members = BTreeSet::new();
            for row in rows {
                if !members.insert(row.member) { return Err(bad("original associated inventory repeats a member ordinal")); }
                let member = declaration.members.get(row.member as usize).ok_or_else(|| bad("original associated member ordinal is absent"))?;
                match (&member.kind, &row.carrier, &row.value) {
                    (DynMemberKind::Type, None, Some(DynAssociatedValueRecipe::Type(pattern))) => {
                        let recipe = dynamic_witness::semantic_external::original_type_recipe(module, declaration.id, pattern, b)?;
                        let ty = self.ty(module, context, &recipe, &env, active, b)?;
                        self.scheme_term(module, context, &ty, &env, active, b)?;
                    }
                    (DynMemberKind::Type, None, None) if declaration.kind == DynDeclarationKind::Trait => {}
                    (DynMemberKind::Constant, Some(carrier), value) => {
                        let recipe = dynamic_witness::semantic_external::original_type_recipe(module, declaration.id, carrier, b)?;
                        let carrier = self.ty(module, context, &recipe, &env, active, b)?;
                        self.scheme_term(module, context, &carrier, &env, active, b)?;
                        self.scheme_carrier(&carrier, b)?;
                        match value {
                            Some(DynAssociatedValueRecipe::Constant { ty, value }) => {
                                let recipe = dynamic_witness::semantic_external::original_type_recipe(module, declaration.id, ty, b)?;
                                let rhs_carrier = self.ty(module, context, &recipe, &env, active, b)?;
                                if !self.same(&carrier, &rhs_carrier, b)? { return Err(bad("original associated Const changes its complete declared carrier")); }
                                let recipe = dynamic_witness::semantic_external::original_constant_recipe(module, declaration.id, value, b)?;
                                self.constant(module, context, &recipe, &carrier, &env, active, b)?;
                            }
                            None if declaration.kind == DynDeclarationKind::Trait => {}
                            _ => return Err(bad("original associated Const lacks its actual original definition")),
                        }
                    }
                    _ => return Err(bad("original associated member changes kind/carrier or omits its impl RHS")),
                }
                for requirement in dynamic_witness::semantic_external::original_requirement_recipes(module, declaration.id, &row.requirements, b)? {
                    self.scheme_requirement(module, context, &requirement, &env, active, b)?;
                    if row.value.is_some() {
                        // A default or override is a definition obligation in
                        // this original Γ, not its own bound as an assumption.
                        self.obligation(module, context, &requirement, &env, &env, active, b)?;
                    }
                }
            }
            for (ordinal, member) in declaration.members.iter().enumerate() {
                if matches!(member.kind, DynMemberKind::Type | DynMemberKind::Constant) && !members.contains(&(ordinal as u32)) {
                    return Err(bad("original associated inventory omits an unused member"));
                }
            }
        }
        Ok(())
    }
}
