//! Original nominal/resource construction before actual opcode matching.
use super::*;
impl Check<'_, '_> {
    /// Recognize a language resource head from its original constructor.
    /// This answers only the ownership-carrier question: no shape, lifetime,
    /// cleanup, Transfer or ClosedEnvironment theorem follows from it.
    pub(super) fn resource_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let ty = self.observation_type(owner, ty, depth + 1)?;
        Ok(match ty {
            SemanticTypeRecipe::Atom(Type::ByteBuffer)
            | SemanticTypeRecipe::OwnedStore(_)
            | SemanticTypeRecipe::Mutex(_)
            | SemanticTypeRecipe::Task { .. }
            | SemanticTypeRecipe::Registration(_)
            | SemanticTypeRecipe::AsyncRegistration(_) => true,
            SemanticTypeRecipe::Nominal { definition, .. } => matches!(
                self.definition(definition)?.body,
                SemanticDefinitionBody::Nominal {
                    kind: LifeNominalKind::Resource,
                    ..
                }
            ),
            _ => false,
        })
    }
    pub(super) fn record_constructor(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        fields: &[(String, SemanticOriginalExpr)],
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let ty = self.resolve(owner, ty, depth + 1)?;
        let mut result = copy_recipe(&ty, self.limits, self.left)?;
        let nominal = match &ty {
            SemanticTypeRecipe::Desc { inner, .. } | SemanticTypeRecipe::Managed(inner) => {
                inner.as_ref()
            }
            _ => &ty,
        };
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = nominal
        else {
            return Err(bad(
                "original record constructor has no nominal declaration",
            ));
        };
        if matches!(
            self.definition(*definition)?.body,
            SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Resource,
                ..
            }
        ) {
            self.pay(1, depth)?;
            result = SemanticTypeRecipe::Managed(Box::new(copy_recipe(
                nominal,
                self.limits,
                self.left,
            )?));
        } else if !matches!(ty, SemanticTypeRecipe::Nominal { .. }) {
            return Err(bad(
                "original nonresource constructor has an owning or descriptor annotation",
            ));
        }
        let original = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product {
                fields: declarations,
            },
            ..
        } = &original.body
        else {
            return Err(bad(
                "original record constructor changes its declaration kind",
            ));
        };
        if fields.len() != declarations.len() {
            return Err(bad("original record constructor omits or adds fields"));
        }
        self.pay(
            fields
                .len()
                .saturating_mul(declarations.len())
                .saturating_add(arguments.len()),
            depth,
        )?;
        let mut seen = BTreeSet::new();
        for (name, value) in fields {
            if !seen.insert(name.as_str()) {
                return Err(bad("original record constructor repeats a field"));
            }
            let field = declarations
                .iter()
                .find(|field| field.name == *name)
                .ok_or_else(|| bad("original record constructor changes a field identity"))?;
            let expected = super::super::instantiate::ty(
                self.program,
                &self.proof,
                &field.ty,
                super::super::instantiate::DomainMapping {
                    source: original.id,
                    use_definition: owner,
                    arguments,
                    parent_call: None,
                },
                self.limits,
                self.left,
            )?;
            let actual = self.expr(owner, value, variables, Some(&expected), depth + 1)?;
            self.same(&actual, &expected)?;
        }
        self.call_requirements(owner, original, arguments, depth + 1)?;
        if let Some(wanted) = wanted {
            self.same(&result, wanted)?;
        }
        Ok(result)
    }
}

impl Check<'_, '_> {
    pub(super) fn enum_constructor(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let (member, values) = match &expression.kind {
            CoreExprKind::Call { callee, arguments } => (callee.as_ref(), arguments.as_slice()),
            CoreExprKind::Member { .. } => (expression, &[][..]),
            _ => return Ok(None),
        };
        let CoreExprKind::Member {
            object,
            member: variant,
            ..
        } = &member.kind
        else {
            return Ok(None);
        };
        let Some(path) = self.declaration_path(object, variables, depth + 1)? else {
            return Ok(None);
        };
        let current = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &current.body else {
            return Err(bad(
                "original constructor has no lexical declaration environment",
            ));
        };
        self.pay(names.len().saturating_add(path.len()), depth)?;
        let mut selected = None;
        for name in names {
            if name.namespace != SemanticOriginalNamespace::Type || name.path != path {
                continue;
            }
            if selected.is_some() {
                return Err(bad("original enum constructor type binding is ambiguous"));
            }
            if let SemanticOriginalNameValue::Definition(id) = name.value {
                selected = Some(id);
            } else {
                return Ok(None);
            }
        }
        let Some(id) = selected else {
            return Ok(None);
        };
        let definition = self.definition(id)?;
        self.canonical_carrier(definition, depth + 1)?;
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Enum,
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &definition.body
        else {
            return Ok(None);
        };
        self.pay(variants.len(), depth)?;
        let (ordinal, body) = variants
            .iter()
            .enumerate()
            .find(|(_, body)| body.name == *variant)
            .ok_or_else(|| bad("original constructor selects an absent enum variant"))?;
        if body.fields.len() != values.len() {
            return Err(bad(
                "original enum constructor changes its real variant arity",
            ));
        }
        self.pay(
            definition
                .parameters
                .len()
                .saturating_add(body.fields.len()),
            depth,
        )?;
        let mut arguments = Vec::new();
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            let reference = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: ordinal as u32,
            };
            arguments.push(match formal {
                SemanticFormal::Type { .. } => {
                    SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
                }
                SemanticFormal::Lifetime => {
                    SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
                }
                SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                    carrier: copy_recipe(carrier, self.limits, self.left)?,
                    value: SemanticConstRecipe::Formal(reference),
                },
            });
        }
        let signature = SemanticSignatureRecipe {
            bound_lifetimes: 0,
            invocation_lifetimes: 0,
            parameters: body
                .fields
                .iter()
                .map(|ty| {
                    Ok(SemanticParameterRecipe {
                        ty: copy_recipe(ty, self.limits, self.left)?,
                        mode: PassingMode::Infer,
                    })
                })
                .collect::<R<Vec<_>>>()?,
            result: SemanticTypeRecipe::Nominal {
                definition: id,
                arguments,
            },
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
        };
        let written = self
            .written(owner, object, &definition.parameters, variables, depth + 1)?
            .into_iter()
            .map(|value| value.unwrap_or(SemanticActualRecipe::Infer))
            .collect::<Vec<_>>();
        let operands = values.iter().collect::<Vec<_>>();
        let (actuals, solved) = self.solve_signature(
            owner,
            object,
            &operands,
            &signature,
            &definition.parameters,
            &written,
            variables,
            wanted,
            false,
            depth + 1,
        )?;
        self.call_requirements(owner, definition, &actuals, depth + 1)?;
        if let Some(site) = self.site(owner, SemanticOriginalNode::Expression(expression.id))? {
            self.pay(solved.parameters.len(), depth)?;
            let parameters = solved
                .parameters
                .iter()
                .map(|parameter| copy_recipe(&parameter.ty, self.limits, self.left))
                .collect::<R<Vec<_>>>()?;
            self.record_intrinsic(
                owner,
                site.id,
                IntrinsicKind::NominalConstructor {
                    definition: id,
                    variant: ordinal as u32,
                },
                parameters,
                &solved.result,
                wanted,
                depth + 1,
            )?;
            self.proof.calls.insert((owner, site.id), actuals);
        }
        Ok(Some(solved.result))
    }

    pub(super) fn sealed_capability(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        kind: SemanticBuiltinTrait,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        if !matches!(
            kind,
            SemanticBuiltinTrait::ClosedEnvironment
                | SemanticBuiltinTrait::Transfer
                | SemanticBuiltinTrait::ThreadCallable
                | SemanticBuiltinTrait::SyncDrop
        ) {
            return Ok(None);
        }
        let ty = self.resolve(owner, ty, depth + 1)?;
        if self.original_requirements(owner,depth+1)?.iter().any(|requirement|matches!(requirement,SemanticRequirementRecipe::Builtin{ty:actual,trait_kind}if actual==&ty&&*trait_kind==kind)){return Ok(Some(true));}
        let result = match &ty {
            SemanticTypeRecipe::Atom(ty) => Some(
                ty.is_b0_scalar()
                    || matches!(
                        ty,
                        Type::ByteBuffer | Type::ForeignHandle | Type::CWideString
                    ),
            ),
            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. } => Some(false),
            SemanticTypeRecipe::Formal(_)
            | SemanticTypeRecipe::SelfType { .. }
            | SemanticTypeRecipe::Environment { .. }
            | SemanticTypeRecipe::AssociatedType { .. } => None,
            SemanticTypeRecipe::Managed(inner) => match inner.as_ref() {
                SemanticTypeRecipe::Atom(Type::ByteBuffer) => Some(true),
                SemanticTypeRecipe::OwnedStore(element) => {
                    self.sealed_capability(owner, element, kind, depth + 1)?
                }
                _ if kind == SemanticBuiltinTrait::SyncDrop => {
                    self.resource_store_element(owner, inner, depth + 1)?
                }
                _ if kind == SemanticBuiltinTrait::ClosedEnvironment => {
                    self.sealed_capability(owner, inner, kind, depth + 1)?
                }
                _ => None,
            },
            SemanticTypeRecipe::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    match self.sealed_capability(owner, field, kind, depth + 1)? {
                        Some(value) => all &= value,
                        None => return Ok(None),
                    }
                }
                Some(all)
            }
            SemanticTypeRecipe::Array { element, .. } => {
                self.sealed_capability(owner, element, kind, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => match (
                self.sealed_capability(owner, error, kind, depth + 1)?,
                self.sealed_capability(owner, result, kind, depth + 1)?,
            ) {
                (Some(a), Some(b)) => Some(a && b),
                _ => None,
            },
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let declaration = self.definition(*definition)?;
                if kind != SemanticBuiltinTrait::ClosedEnvironment
                    && matches!(
                        declaration.body,
                        SemanticDefinitionBody::Nominal {
                            kind: LifeNominalKind::Resource,
                            ..
                        }
                    )
                {
                    return Ok(None);
                }
                self.pay(arguments.len(), depth)?;
                for argument in arguments {
                    match argument {
                        SemanticActualRecipe::Life(_) => return Ok(Some(false)),
                        SemanticActualRecipe::Type(ty) => {
                            if self.sealed_capability(owner, ty, kind, depth + 1)? != Some(true) {
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
                let count = match &declaration.body {
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => fields.len(),
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => variants.iter().try_fold(0usize, |n, v| {
                        n.checked_add(v.fields.len()).ok_or_else(|| {
                            Diagnostic::limit(
                                Phase::Types,
                                "original capability field count exceeded",
                                None,
                            )
                        })
                    })?,
                    _ => return Ok(None),
                };
                self.pay(count, depth)?;
                let fields = match &declaration.body {
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => fields.iter().map(|field| &field.ty).collect::<Vec<_>>(),
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => variants
                        .iter()
                        .flat_map(|variant| variant.fields.iter())
                        .collect(),
                    _ => return Ok(None),
                };
                let mut all = true;
                for field in fields {
                    let field = super::super::instantiate::ty(
                        self.program,
                        &self.proof,
                        field,
                        super::super::instantiate::DomainMapping {
                            source: declaration.id,
                            use_definition: owner,
                            arguments,
                            parent_call: None,
                        },
                        self.limits,
                        self.left,
                    )?;
                    match self.sealed_capability(owner, &field, kind, depth + 1)? {
                        Some(value) => all &= value,
                        None => return Ok(None),
                    }
                }
                Some(all)
            }
            SemanticTypeRecipe::Closure {
                definition,
                environment,
                ..
            } => {
                if kind == SemanticBuiltinTrait::ClosedEnvironment
                    && self.proof.borrowed_capture_heads.contains(definition)
                {
                    return Ok(Some(false));
                }
                if !self.proof.complete.contains(definition)
                    || !self.proof.kinds.contains_key(definition)
                {
                    return Ok(None);
                }
                let mut all = true;
                self.pay(environment.len(), depth)?;
                let requirement = if kind == SemanticBuiltinTrait::ThreadCallable {
                    SemanticBuiltinTrait::Transfer
                } else {
                    kind
                };
                for actual in environment {
                    match actual {
                        SemanticActualRecipe::Type(ty) => {
                            match self.sealed_capability(owner, ty, requirement, depth + 1)? {
                                Some(value) => all &= value,
                                None => return Ok(None),
                            }
                        }
                        SemanticActualRecipe::Life(_) => all = false,
                        SemanticActualRecipe::Const { carrier, .. } => match self
                            .sealed_capability(
                                owner,
                                carrier,
                                SemanticBuiltinTrait::ClosedEnvironment,
                                depth + 1,
                            )? {
                            Some(value) => all &= value,
                            None => return Ok(None),
                        },
                        _ => return Ok(None),
                    }
                }
                if kind == SemanticBuiltinTrait::ThreadCallable && all {
                    let mut pending = vec![*definition];
                    let mut seen = BTreeSet::new();
                    while let Some(function) = pending.pop() {
                        self.pay(
                            self.proof
                                .call_targets
                                .len()
                                .saturating_add(self.proof.intrinsics.len())
                                .saturating_add(1),
                            depth,
                        )?;
                        if !seen.insert(function) {
                            continue;
                        }
                        let body = self.definition(function)?;
                        if matches!(
                            body.body,
                            SemanticDefinitionBody::FunctionWithBody {
                                body: SemanticOriginalFunctionBody::External,
                                ..
                            }
                        ) {
                            return Ok(None);
                        }
                        if self.proof.intrinsics.iter().any(|((owner, _), call)| {
                            *owner == function && call.effect == CallKind::Suspend
                        }) {
                            all = false;
                            break;
                        }
                        for ((caller, _), target) in &self.proof.call_targets {
                            if *caller == function {
                                pending.push(*target);
                            }
                        }
                    }
                }
                Some(all)
            }
            SemanticTypeRecipe::Exact(exact) => Some(
                exact.life.binder_count == 0 && exact.classes.is_empty() && exact.ty.is_b0_scalar(),
            ),
            _ => None,
        };
        Ok(result)
    }
}
