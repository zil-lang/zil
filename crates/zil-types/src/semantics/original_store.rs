//! The sealed Store scheme has one complete Type formal. It is language
//! syntax, not a user function or an invented external declaration.
use super::*;
impl Check<'_, '_> {
    pub(super) fn store_intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!()
        };
        let Some(path) = self.declaration_path(callee, variables, depth + 1)? else {
            return Ok(None);
        };
        if path.len() != 1 || self.declaration_target(owner, &path, depth + 1)?.is_some() {
            return Ok(None);
        }
        let kind = match path[0].as_str() {
            "store_new" => IntrinsicKind::StoreNew,
            "store_reserve" => IntrinsicKind::StoreReserve,
            "store_push" => IntrinsicKind::StorePush,
            "store_pop" => IntrinsicKind::StorePop,
            "store_len" => IntrinsicKind::StoreLen,
            "store_capacity" => IntrinsicKind::StoreCapacity,
            "store_at" => IntrinsicKind::StoreBorrow { mutable: false },
            "store_at_mut" => IntrinsicKind::StoreBorrow { mutable: true },
            _ => return Ok(None),
        };
        self.pay(arguments.len().saturating_add(8), depth)?;
        let mut parameters = Vec::new();
        let (element, result) = if kind == IntrinsicKind::StoreNew {
            if !arguments.is_empty() {
                return Err(bad("original Store constructor has nonzero value arity"));
            }
            let element = match &callee.kind {
                CoreExprKind::TypeApply { type_arguments, .. } if type_arguments.len() == 1 => {
                    self.resolve(owner, &type_arguments[0], depth + 1)?
                }
                CoreExprKind::GenericApply { arguments, .. } if arguments.len() == 1 => {
                    let CoreGenericArgument::Type(ty) = &arguments[0] else {
                        return Err(bad(
                            "original Store constructor changes its Type formal kind",
                        ));
                    };
                    self.resolve(owner, ty, depth + 1)?
                }
                CoreExprKind::Name(_) => match wanted {
                    Some(SemanticTypeRecipe::Managed(inner)) => {
                        let SemanticTypeRecipe::OwnedStore(element) = inner.as_ref() else {
                            return Err(bad("original Store constructor has another result head"));
                        };
                        copy_recipe(element, self.limits, self.left)?
                    }
                    _ => return Err(pending(
                        "original Store constructor requires its complete element result context",
                    )),
                },
                _ => {
                    return Err(bad(
                        "original Store constructor changes its one-Type mixed scheme",
                    ))
                }
            };
            let result = SemanticTypeRecipe::Managed(Box::new(SemanticTypeRecipe::OwnedStore(
                Box::new(copy_recipe(&element, self.limits, self.left)?),
            )));
            (element, result)
        } else {
            let arity = if matches!(
                kind,
                IntrinsicKind::StoreReserve
                    | IntrinsicKind::StorePush
                    | IntrinsicKind::StoreBorrow { .. }
            ) {
                2
            } else {
                1
            };
            if arguments.len() != arity {
                return Err(bad(
                    "original Store operation changes its declared operand arity",
                ));
            }
            if !matches!(callee.kind, CoreExprKind::Name(_)) {
                return Err(bad(
                    "original Store operation has no written generic application scheme",
                ));
            }
            let descriptor = self.expr(owner, &arguments[0], variables, None, depth + 1)?;
            let SemanticTypeRecipe::Desc { lifetime, inner } = &descriptor else {
                return Err(bad("original Store operation requires a scoped descriptor"));
            };
            let SemanticTypeRecipe::OwnedStore(element) = inner.as_ref() else {
                return Err(bad("original Store operation changes its sealed head kind"));
            };
            let element = copy_recipe(element, self.limits, self.left)?;
            crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
            let lifetime = lifetime.clone();
            parameters.push(descriptor);
            if arity == 2 {
                let expected = if kind == IntrinsicKind::StorePush {
                    copy_recipe(&element, self.limits, self.left)?
                } else {
                    SemanticTypeRecipe::Atom(Type::I64)
                };
                let value =
                    self.expr(owner, &arguments[1], variables, Some(&expected), depth + 1)?;
                self.same(&value, &expected)?;
                parameters.push(value);
            }
            let result = match &kind {
                IntrinsicKind::StoreReserve => SemanticTypeRecipe::Atom(Type::Bool),
                IntrinsicKind::StorePush => SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(copy_recipe(&element, self.limits, self.left)?),
                    result: Box::new(SemanticTypeRecipe::Atom(Type::Unit)),
                },
                IntrinsicKind::StorePop => SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(SemanticTypeRecipe::Atom(Type::Unit)),
                    result: Box::new(copy_recipe(&element, self.limits, self.left)?),
                },
                IntrinsicKind::StoreLen | IntrinsicKind::StoreCapacity => {
                    SemanticTypeRecipe::Atom(Type::I64)
                }
                IntrinsicKind::StoreBorrow { mutable } => SemanticTypeRecipe::Address {
                    mutable: *mutable,
                    lifetime,
                    inner: Box::new(copy_recipe(&element, self.limits, self.left)?),
                },
                _ => unreachable!(),
            };
            (element, result)
        };
        if !self.store_element(owner, &element, depth + 1)? {
            return Err(bad(
                "original Store operation lacks a complete StoreElement premise",
            ));
        }
        self.record_intrinsic(owner, site, kind, parameters, &result, wanted, depth + 1)?;
        self.pay(1, depth)?;
        self.proof
            .calls
            .insert((owner, site), vec![SemanticActualRecipe::Type(element)]);
        Ok(Some(result))
    }
}

impl Check<'_, '_> {
    /// A managed resource element retains its real declaration and complete
    /// synchronous teardown body. It is never inferred from a concrete i32
    /// specialization of an unconstrained original payload.
    pub(super) fn resource_store_element(
        &mut self,
        owner: SemanticDefinitionId,
        payload: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        let payload = self.resolve(owner, payload, depth + 1)?;
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = &payload
        else {
            return Ok(None);
        };
        let declaration = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Resource,
            body: SemanticNominalBody::Product { fields },
            ..
        } = &declaration.body
        else {
            return Ok(None);
        };
        if fields.is_empty() {
            return Ok(Some(false));
        }
        self.pay(arguments.len().saturating_add(fields.len()), depth)?;
        for argument in arguments {
            match argument {
                SemanticActualRecipe::Life(_) => return Ok(Some(false)),
                SemanticActualRecipe::Type(ty) => {
                    if self.sealed_capability(
                        owner,
                        ty,
                        SemanticBuiltinTrait::ClosedEnvironment,
                        depth + 1,
                    )? != Some(true)
                    {
                        return Ok(None);
                    }
                }
                SemanticActualRecipe::Const { carrier, .. } => {
                    if self.sealed_capability(
                        owner,
                        carrier,
                        SemanticBuiltinTrait::ClosedEnvironment,
                        depth + 1,
                    )? != Some(true)
                    {
                        return Ok(None);
                    }
                }
                SemanticActualRecipe::Infer => return Ok(None),
            }
        }
        for field in fields {
            let field = super::super::instantiate::ty(
                self.program,
                &self.proof,
                &field.ty,
                super::super::instantiate::DomainMapping {
                    source: declaration.id,
                    use_definition: owner,
                    arguments,
                    parent_call: None,
                },
                self.limits,
                self.left,
            )?;
            if !self.copy_type(owner, &field, depth + 1)?
                || self.sealed_capability(
                    owner,
                    &field,
                    SemanticBuiltinTrait::ClosedEnvironment,
                    depth + 1,
                )? != Some(true)
            {
                return Ok(Some(false));
            }
        }
        self.pay(self.program.definitions.len(), depth)?;
        let mut selected = None;
        for scope in &self.program.definitions {
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl {
                trait_kind: SemanticBuiltinTrait::Drop,
                ..
            }) = &scope.body
            else {
                continue;
            };
            let actuals = match self.solve_parent(owner, scope, &payload, depth + 1) {
                Ok(actuals) => actuals,
                Err(diagnostic) if diagnostic.category == zil_base::ResultCategory::SafeReject => {
                    continue
                }
                Err(diagnostic) => return Err(diagnostic),
            };
            self.pay(
                self.program.definitions.len().saturating_add(actuals.len()),
                depth,
            )?;
            let mut methods = self.program.definitions.iter().filter(|method| {
                method.parent == Some(scope.id) && method.kind == SemanticDefinitionKind::Method
            });
            let method = methods
                .next()
                .ok_or_else(|| bad("resource Drop provider omits its real original method"))?;
            if methods.next().is_some() {
                return Err(bad("resource Drop provider has ambiguous original methods"));
            }
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            } = &method.body
            else {
                return Err(bad(
                    "resource teardown omits its complete original operation body",
                ));
            };
            if signature.call_kind != CallKind::Direct {
                return Ok(Some(false));
            }
            if !self.proof.complete.contains(&method.id) && !self.active.contains(&method.id) {
                self.function(method, None)?;
            }
            if selected.replace(scope.id).is_some() {
                return Err(bad(
                    "resource element has ambiguous synchronous Drop providers",
                ));
            }
        }
        // With no authored Drop implementation this is the closed language
        // resource's built-in release; no user cleanup capability is invented.
        Ok(Some(true))
    }
}
