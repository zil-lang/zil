//! Forward interpretation of an original Self recipe. Neither a concrete
//! provider type nor an outside class image may determine a missing formal.
use super::*;

impl<'a> Proof<'a, '_> {
    fn semantic_definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        let mut found = None;
        for core in self.modules {
            self.pay(1)?;
            let Some(program) = &core.semantic else {
                continue;
            };
            self.pay(program.definitions.len())?;
            for definition in &program.definitions {
                if definition.id != id {
                    continue;
                }
                crate::dyn_metadata::semantic_operand_snapshot(
                    definition,
                    self.limits,
                    self.left,
                    1,
                )?;
                if let Some(previous) = found {
                    if previous != definition {
                        return Err(self.fail());
                    }
                } else {
                    found = Some(definition);
                }
            }
        }
        found.ok_or_else(|| self.fail())
    }
    fn semantic_life(&mut self, life: &SemanticLifeRecipe, depth: usize) -> R<DynLifePattern> {
        self.semantic_depth(depth)?;
        Ok(match life {
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                self.actual(*ordinal, &LifeParameterKind::Lifetime)?;
                DynLifePattern::DeclarationParameter(*ordinal)
            }
            _ => return Err(unsupported(
                Span::default(),
                "dynamic semantic Self lifetime needs its complete original lexical class mapping",
            )),
        })
    }
    fn semantic_depth(&mut self, depth: usize) -> R<()> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dynamic original Self recipe depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn semantic_constant(
        &mut self,
        value: &SemanticConstRecipe,
        depth: usize,
    ) -> R<DynConstPattern> {
        self.semantic_depth(depth)?;
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        Ok(match value {
            SemanticConstRecipe::Value(value) => DynConstPattern::Value(value.clone()),
            SemanticConstRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                if !matches!(
                    self.implementation.parameters.get(*ordinal as usize),
                    Some(LifeParameterKind::Const(_))
                ) {
                    return Err(self.fail());
                }
                DynConstPattern::Parameter(*ordinal)
            }
            _ => {
                return Err(unsupported(
                    Span::default(),
                    "dynamic semantic Self Const needs its complete symbolic carrier evaluation",
                ))
            }
        })
    }
    fn semantic_actual(
        &mut self,
        actual: &SemanticActualRecipe,
        depth: usize,
    ) -> R<DynPatternActual> {
        self.semantic_depth(depth)?;
        Ok(match actual {
            SemanticActualRecipe::Type(ty) => {
                DynPatternActual::Type(self.semantic_pattern(ty, depth + 1)?)
            }
            SemanticActualRecipe::Life(life) => {
                DynPatternActual::Life(self.semantic_life(life, depth + 1)?)
            }
            SemanticActualRecipe::Const { value, .. } => {
                DynPatternActual::Const(self.semantic_constant(value, depth + 1)?)
            }
            SemanticActualRecipe::Infer => return Err(self.fail()),
        })
    }
    fn semantic_pattern(&mut self, ty: &SemanticTypeRecipe, depth: usize) -> R<DynTypePattern> {
        self.semantic_depth(depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Atom(ty) => {
                if !ty.is_b0_scalar() {
                    return Err(self.fail());
                }
                inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
                DynTypePattern::Concrete(ty.clone())
            }
            SemanticTypeRecipe::Exact(value)
                if value.life.binder_count == 0 && value.classes.is_empty() =>
            {
                self.closed(&value.ty)?;
                inference::charge_type_snapshot(
                    &value.ty,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                DynTypePattern::Concrete(value.ty.clone())
            }
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                self.actual(*ordinal, &LifeParameterKind::Type)?;
                DynTypePattern::Parameter(*ordinal)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let definition = self.semantic_definition(*definition)?;
                let SemanticDefinitionBody::Nominal { declaration, .. } = &definition.body else {
                    return Err(self.fail());
                };
                let mut header = None;
                for core in self.modules {
                    self.pay(core.life_declarations.len())?;
                    for candidate in &core.life_declarations {
                        if candidate.id != *declaration {
                            continue;
                        }
                        let identity = &candidate.identity;
                        let rows = identity
                            .module_path
                            .len()
                            .saturating_add(identity.declaration_path.len())
                            .saturating_add(identity.generation_path.len())
                            .saturating_add(candidate.parameters.len());
                        if rows > self.limits.max_nodes {
                            return Err(Diagnostic::limit(
                                Phase::Types,
                                "dynamic nominal identity directory exceeds node limit",
                                None,
                            ));
                        }
                        self.pay(rows.saturating_add(identity.package.len()))?;
                        for text in identity
                            .module_path
                            .iter()
                            .chain(&identity.declaration_path)
                        {
                            self.pay(text.len())?;
                        }
                        for parameter in &candidate.parameters {
                            if let LifeParameterKind::Const(ty) = parameter {
                                inference::charge_type_snapshot(
                                    ty,
                                    Span::default(),
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                            }
                        }
                        if header.is_some_and(|previous| previous != candidate) {
                            return Err(self.fail());
                        }
                        header = Some(candidate);
                    }
                }
                let header = header.ok_or_else(|| self.fail())?;
                let SemanticDefinitionBody::Nominal { kind, .. } = &definition.body else {
                    unreachable!()
                };
                if header.identity != definition.identity
                    || header.kind != *kind
                    || header.parameters.len() != definition.parameters.len()
                {
                    return Err(self.fail());
                }
                if arguments.len() != definition.parameters.len() {
                    return Err(self.fail());
                }
                self.pay(header.parameters.len())?;
                for (kind, formal) in header.parameters.iter().zip(&definition.parameters) {
                    match (kind, formal) {
                        (LifeParameterKind::Type, SemanticFormal::Type { .. })
                        | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime) => {}
                        (
                            LifeParameterKind::Const(actual),
                            SemanticFormal::Const {
                                carrier: SemanticTypeRecipe::Atom(expected),
                            },
                        ) if actual == expected => {}
                        _ => return Err(self.fail()),
                    }
                }
                self.pay(arguments.len())?;
                let mut actuals = Vec::new();
                for (argument, formal) in arguments.iter().zip(&definition.parameters) {
                    actuals.push(match (argument,formal) {
                        (SemanticActualRecipe::Type(ty),SemanticFormal::Type{..})=>DynPatternActual::Type(self.semantic_pattern(ty,depth+1)?),
                        (SemanticActualRecipe::Life(life),SemanticFormal::Lifetime)=>DynPatternActual::Life(self.semantic_life(life,depth+1)?),
                        (SemanticActualRecipe::Const{carrier,value},SemanticFormal::Const{carrier:expected})=>{
                            crate::dyn_metadata::semantic_operand_snapshot(carrier,self.limits,self.left,1)?;
                            crate::dyn_metadata::semantic_operand_snapshot(expected,self.limits,self.left,1)?;
                            if carrier!=expected{return Err(unsupported(Span::default(),"dynamic nominal Const carrier requires complete earlier-formal substitution"));}
                            DynPatternActual::Const(self.semantic_constant(value,depth+1)?)
                        },
                        _=>return Err(self.fail()),
                    });
                }
                DynTypePattern::Nominal(Box::new(DynSelfPattern {
                    declaration: *declaration,
                    arguments: actuals,
                }))
            }
            SemanticTypeRecipe::Tuple(fields) => {
                self.pay(fields.len())?;
                let mut patterns = Vec::new();
                for field in fields {
                    patterns.push(self.semantic_pattern(field, depth + 1)?);
                }
                DynTypePattern::Tuple(patterns)
            }
            SemanticTypeRecipe::Array { element, length } => DynTypePattern::Array {
                element: Box::new(self.semantic_pattern(element, depth + 1)?),
                length: self.semantic_constant(length, depth + 1)?,
            },
            SemanticTypeRecipe::Slice(inner) => {
                DynTypePattern::Slice(Box::new(self.semantic_pattern(inner, depth + 1)?))
            }
            SemanticTypeRecipe::Managed(inner) => {
                DynTypePattern::Managed(Box::new(self.semantic_pattern(inner, depth + 1)?))
            }
            SemanticTypeRecipe::OwnedStore(inner) => {
                DynTypePattern::OwnedStore(Box::new(self.semantic_pattern(inner, depth + 1)?))
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => DynTypePattern::ErrorUnion {
                error: Box::new(self.semantic_pattern(error, depth + 1)?),
                result: Box::new(self.semantic_pattern(result, depth + 1)?),
            },
            _ => return Err(unsupported(
                Span::default(),
                "dynamic semantic Self requires its complete original constructor and class recipe",
            )),
        })
    }
    pub(super) fn semantic_self(
        &mut self,
        scope: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        actual: &Type,
        path: &mut Vec<LifeTypeProjection>,
        depth: usize,
    ) -> R<()> {
        self.semantic_depth(depth)?;
        let mut binding = None;
        for core in self.modules {
            let Some(program) = &core.semantic else {
                continue;
            };
            self.pay(program.dyn_scopes.len())?;
            for row in &program.dyn_scopes {
                if row.owner != SemanticDynOwner::Declaration(self.implementation.id) {
                    continue;
                }
                if binding.is_some_and(|previous| previous != row.scope) {
                    return Err(self.fail());
                }
                binding = Some(row.scope);
            }
        }
        if binding != Some(scope) {
            return Err(self.fail());
        }
        let original = self.semantic_definition(scope)?;
        crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
            declaration,
            self_type,
            trait_application,
        }) = &original.body
        else {
            return Err(self.fail());
        };
        if *declaration != self.implementation.id
            || original.kind != SemanticDefinitionKind::Scope
            || original.identity != self.implementation.identity
            || recipe != self_type
            || original.parameters.len() != self.implementation.parameters.len()
        {
            return Err(self.fail());
        }
        self.pay(original.parameters.len())?;
        for (formal, kind) in original
            .parameters
            .iter()
            .zip(&self.implementation.parameters)
        {
            match (formal, kind) {
                (SemanticFormal::Type { .. }, LifeParameterKind::Type)
                | (SemanticFormal::Lifetime, LifeParameterKind::Lifetime) => {}
                (
                    SemanticFormal::Const {
                        carrier: SemanticTypeRecipe::Atom(expected),
                    },
                    LifeParameterKind::Const(actual),
                ) if expected == actual => {}
                _ => return Err(self.fail()),
            }
        }
        let implemented = self
            .implementation
            .implemented_trait
            .as_ref()
            .ok_or_else(|| self.fail())?;
        if implemented.declaration != trait_application.declaration
            || implemented.arguments.len() != trait_application.arguments.len()
        {
            return Err(self.fail());
        }
        self.pay(trait_application.arguments.len())?;
        let mut arguments = Vec::new();
        for actual in &trait_application.arguments {
            arguments.push(self.semantic_actual(actual, depth + 1)?);
        }
        let original_application = DynSelfPattern {
            declaration: trait_application.declaration,
            arguments,
        };
        crate::dyn_metadata::semantic_operand_snapshot(implemented, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(
            &original_application,
            self.limits,
            self.left,
            1,
        )?;
        if implemented != &original_application {
            return Err(self.fail());
        }
        let pattern = self.semantic_pattern(recipe, depth + 1)?;
        self.ty(&pattern, actual, path, depth + 1)
    }
}
