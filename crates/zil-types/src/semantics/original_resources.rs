//! Original nominal/resource construction before actual opcode matching.
use super::*;
impl Proof {
    /// Prove the complete original environment's CE property under its own
    /// declaration Gamma. Runtime move modes and every concrete child remain
    /// independently checked by the actual environment consumer.
    pub(in crate::semantics) fn closed_closure_environment(
        &self, program: &SemanticModule, owner: SemanticDefinitionId,
        limits: &Limits, left: &mut usize,
    ) -> R<Option<bool>> {
        *left = left.checked_sub(self.complete.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "original environment capability lookup work exceeded", None))?;
        if !self.complete.contains(&owner) { return Ok(None); }
        let mut check = super::nominal_where::borrowed_check(program, self, owner, limits, left)?;
        let declaration = check.definition(owner)?;
        if declaration.kind != SemanticDefinitionKind::Closure { return Ok(None); }
        check.pay(self.borrowed_capture_heads.len().saturating_add(1).ilog2() as usize + 1, 0)?;
        if self.borrowed_capture_heads.contains(&owner) { return Ok(Some(false)); }
        let mut known = Some(true);
        for (ordinal, slot) in declaration.environment.iter().enumerate() {
            check.pay(1, 0)?;
            let value = match &slot.kind {
                SemanticFormal::Type { .. } => {
                    let ordinal = u32::try_from(ordinal).map_err(|_| bad("original environment capability ordinal overflows"))?;
                    check.sealed_capability(owner, &SemanticTypeRecipe::Environment { scope: SemanticScopeRef::Ancestor(0), ordinal },
                        SemanticBuiltinTrait::ClosedEnvironment, 1)?
                }
                SemanticFormal::Lifetime => Some(false),
                SemanticFormal::Const { carrier } => check.sealed_capability(owner, carrier, SemanticBuiltinTrait::ClosedEnvironment, 1)?,
            };
            known = match (known, value) { (Some(a), Some(b)) => Some(a && b), _ => None };
        }
        Ok(known)
    }
}

impl Check<'_, '_> {
    /// Registration construction may synchronously destroy the transferred
    /// environment on allocation/install failure and after drain. This proves
    /// the callback's cleanup obligation in the creator's original Gamma; it
    /// does not add a premise for that type or for a registration owner.
    pub(super) fn registration_cleanup(
        &mut self,
        owner: SemanticDefinitionId,
        callback: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if self.copy_type(owner, callback, depth + 1)? {
            return Ok(());
        }
        if self.sealed_capability(owner, callback, SemanticBuiltinTrait::SyncDrop, depth + 1)?
            == Some(true)
        {
            return Ok(());
        }
        for premise in self.original_requirements(owner, depth + 1)?.iter() {
            self.pay(1, depth)?;
            let ty = match premise {
                SemanticRequirementRecipe::Structural {
                    ty,
                    bound:
                        DynStructuralBound::SyncDrop
                        | DynStructuralBound::Copy
                        | DynStructuralBound::StoreElement,
                } => Some(ty),
                SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind:
                        SemanticBuiltinTrait::SyncDrop
                        | SemanticBuiltinTrait::Copy
                        | SemanticBuiltinTrait::StoreElement,
                } => Some(ty),
                _ => None,
            };
            if let Some(ty) = ty {
                if self.premise_type_equal(owner, callback, ty, depth + 1)? {
                    return Ok(());
                }
            }
        }
        Err(bad(
            "registration construction lacks the callback's independent original SyncDrop premise",
        ))
    }
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
            SemanticTypeRecipe::Atom(Type::ByteBuffer | Type::TaskI32)
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
    /// A constructor with an omitted application is an inference operation,
    /// not a nominal type with an empty mixed argument vector. Solve every
    /// declaration formal against the wanted type and all real field values.
    /// These are rigid data classes; no callable lifetime is introduced here.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn inferred_record_constructor(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        selected: Option<SemanticDefinitionId>,
        fields: &[(String, SemanticOriginalExpr)],
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(fields.len(), depth)?;
        let empty = BTreeMap::new();
        let context = wanted
            .map(|wanted| {
                substitute::caller_context(
                    wanted,
                    &substitute::Substitution {
                        frames: &[],
                        selves: &[],
                        environments: &[],
                        implicit: &empty,
                        invocation: &empty,
                        shift: 0,
                    },
                    self.limits,
                    self.left,
                )
            })
            .transpose()?
            .flatten();
        let nominal_context = context.as_ref().map(|ty| match ty {
            SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::Desc { inner, .. } => {
                inner.as_ref()
            }
            _ => ty,
        });
        let contextual_definition = match nominal_context {
            Some(SemanticTypeRecipe::Nominal { definition, .. }) => Some(*definition),
            _ => None,
        };
        let selected = selected.or(contextual_definition).ok_or_else(|| {
            bad("original inferred record has no selected declaration or complete nominal context")
        })?;
        if contextual_definition.is_some_and(|context| context != selected) {
            return Err(bad(
                "original inferred record changes its wanted nominal declaration",
            ));
        }
        let definition = self.definition(selected)?;
        self.canonical_carrier(definition, depth + 1)?;
        let SemanticDefinitionBody::Nominal {
            kind,
            body: SemanticNominalBody::Product { fields: declared },
            ..
        } = &definition.body
        else {
            return Err(bad(
                "original inferred record selects a nonproduct declaration",
            ));
        };
        if declared.len() != fields.len() {
            return Err(bad("original inferred record omits or adds fields"));
        }
        self.pay(
            definition
                .parameters
                .len()
                .saturating_add(fields.len().saturating_mul(declared.len())),
            depth,
        )?;
        let mut identity_arguments = Vec::new();
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            let reference = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: ordinal as u32,
            };
            identity_arguments.push(match formal {
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
            parameters: declared
                .iter()
                .map(|field| {
                    Ok(SemanticParameterRecipe {
                        ty: copy_recipe(&field.ty, self.limits, self.left)?,
                        mode: PassingMode::Infer,
                    })
                })
                .collect::<R<Vec<_>>>()?,
            result: SemanticTypeRecipe::Nominal {
                definition: selected,
                arguments: identity_arguments,
            },
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
        };
        let template =
            self.inference_template(&signature, &definition.parameters, false, depth + 1)?;
        let mut solutions = vec![None; definition.parameters.len()];
        let mut implicit = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        if let Some(context) = nominal_context {
            self.unify(
                &template.result,
                context,
                &definition.parameters,
                &mut solutions,
                &mut implicit,
                &mut invocation,
                depth + 1,
            )?;
        }
        let mut seen = BTreeSet::new();
        let mut actual_fields = Vec::new();
        let mut outer_inputs = Vec::new();
        let mut ordered_fields = Vec::new();
        // A bare numeric literal has no evaluation effect. Delay only these
        // contextual values so later rigid fields can determine their type.
        for contextual in [false, true] {
            for field in fields {
                let bare = self.contextual_integer(&field.1, depth + 1)?
                    || matches!(&field.1.kind, CoreExprKind::Float(text)
                        if !text.ends_with("f32") && !text.ends_with("f64"));
                if bare == contextual {
                    ordered_fields.push(field);
                }
            }
        }
        for (name, value) in ordered_fields {
            if !seen.insert(name.as_str()) {
                return Err(bad("original inferred record repeats a field"));
            }
            let ordinal = declared
                .iter()
                .position(|field| field.name == *name)
                .ok_or_else(|| bad("original inferred record changes a field identity"))?;
            let complete = solutions.iter().all(Option::is_some);
            let hint = if complete {
                self.pay(solutions.len(), depth)?;
                for actual in solutions.iter().flatten() {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                }
                let arguments = solutions
                    .iter()
                    .map(|actual| actual.clone().expect("complete mixed inference"))
                    .collect::<Vec<_>>();
                Some(super::super::instantiate::ty(
                    self.program,
                    &self.proof,
                    &declared[ordinal].ty,
                    super::super::instantiate::DomainMapping {
                        source: selected,
                        use_definition: owner,
                        arguments: &arguments,
                        parent_call: None,
                    },
                    self.limits,
                    self.left,
                )?)
            } else {
                let SemanticTypeRecipe::Nominal {
                    arguments: identity,
                    ..
                } = &template.result
                else {
                    return Err(bad("original constructor template lost its nominal result"));
                };
                self.pay(identity.len(), depth)?;
                let mut partial_actuals = Vec::new();
                for (identity, solution) in identity.iter().zip(&solutions) {
                    let actual = solution.as_ref().unwrap_or(identity);
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    partial_actuals.push(actual.clone());
                }
                let frames = [(u32::MAX, partial_actuals.as_slice())];
                let partial = substitute::ty(
                    &template.parameters[ordinal].ty,
                    &substitute::Substitution {
                        frames: &frames,
                        selves: &[],
                        environments: &[],
                        implicit: &empty,
                        invocation: &empty,
                        shift: 0,
                    },
                    self.limits,
                    self.left,
                )?;
                let context = substitute::caller_context(
                    &partial,
                    &substitute::Substitution {
                        frames: &[],
                        selves: &[],
                        environments: &[],
                        implicit: &empty,
                        invocation: &empty,
                        shift: 0,
                    },
                    self.limits,
                    self.left,
                )?;
                if context.is_some() {
                    context
                } else if let (
                    SemanticTypeRecipe::Array { element, .. },
                    CoreExprKind::Array(values),
                ) = (&partial, &value.kind)
                {
                    substitute::caller_context(
                        element,
                        &substitute::Substitution {
                            frames: &[],
                            selves: &[],
                            environments: &[],
                            implicit: &empty,
                            invocation: &empty,
                            shift: 0,
                        },
                        self.limits,
                        self.left,
                    )?
                    .map(|element| SemanticTypeRecipe::Array {
                        element: Box::new(element),
                        length: Box::new(SemanticConstRecipe::Value(Constant::Usize(
                            values.len() as u64
                        ))),
                    })
                } else {
                    None
                }
            };
            let pattern = &template.parameters[ordinal].ty;
            let actual = if matches!(pattern, SemanticTypeRecipe::Address { .. })
                && matches!(value.kind, CoreExprKind::Name(_))
            {
                self.place(owner, value, variables, depth + 1)?
            } else {
                self.expr(owner, value, variables, hint.as_ref(), depth + 1)?
            };
            let roots = match (pattern, &actual) {
                (
                    SemanticTypeRecipe::Address {
                        mutable: requested,
                        lifetime,
                        inner,
                    },
                    SemanticTypeRecipe::Address {
                        mutable: available,
                        inner: actual_inner,
                        ..
                    },
                ) if !*requested || *available => {
                    Some((lifetime, inner.as_ref(), actual_inner.as_ref()))
                }
                (
                    SemanticTypeRecipe::Desc { lifetime, inner },
                    SemanticTypeRecipe::Desc {
                        inner: actual_inner,
                        ..
                    },
                ) => Some((lifetime, inner.as_ref(), actual_inner.as_ref())),
                _ => None,
            };
            let borrowed_root = roots.is_some();
            if let Some((lifetime, inner, actual_inner)) = roots {
                // First solve the entire invariant pointee. Nominal slots and
                // all inner address classes constrain the same mixed vector
                // before any outer field is allowed to select a short region.
                self.unify(
                    inner,
                    actual_inner,
                    &definition.parameters,
                    &mut solutions,
                    &mut implicit,
                    &mut invocation,
                    depth + 1,
                )?;
                let variable = call_regions::variable(lifetime);
                let selector = match variable {
                    Some(call_regions::Variable::Life(ordinal)) => {
                        Some(call_regions::Role::ConstructorLife(ordinal))
                    }
                    Some(_) => {
                        return Err(bad(
                            "original product field invents a callable lifetime variable",
                        ));
                    }
                    None => None,
                };
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                outer_inputs.push(call_regions::Input {
                    variable,
                    selector,
                    expression: value,
                    actual: copy_recipe(&actual, self.limits, self.left)?,
                    expected: lifetime.clone(),
                    parameter: u32::try_from(ordinal)
                        .map_err(|_| bad("original record field ordinal overflows"))?,
                });
            } else {
                self.unify(
                    pattern,
                    &actual,
                    &definition.parameters,
                    &mut solutions,
                    &mut implicit,
                    &mut invocation,
                    depth + 1,
                )?;
            }
            actual_fields.push((ordinal, actual, borrowed_root));
        }
        self.settle_call_inputs(
            owner,
            expression,
            &outer_inputs,
            &definition.parameters,
            &mut solutions,
            &mut implicit,
            &mut invocation,
            variables,
            depth + 1,
        )?;
        let arguments = solutions
            .into_iter()
            .map(|actual| {
                actual.ok_or_else(|| {
                    bad("original inferred record has an unconstrained mixed formal")
                })
            })
            .collect::<R<Vec<_>>>()?;
        if !implicit.is_empty() || !invocation.is_empty() {
            return Err(bad(
                "original inferred record invents a callable lifetime domain",
            ));
        }
        // Check every complete field after solving the joint vector; an early
        // field is never authoritative for a later, conflicting life source.
        for (ordinal, actual, borrowed_root) in actual_fields {
            let expected = super::super::instantiate::ty(
                self.program,
                &self.proof,
                &declared[ordinal].ty,
                super::super::instantiate::DomainMapping {
                    source: selected,
                    use_definition: owner,
                    arguments: &arguments,
                    parent_call: None,
                },
                self.limits,
                self.left,
            )?;
            if borrowed_root {
                match (&actual, &expected) {
                    (
                        SemanticTypeRecipe::Address { inner: actual, .. },
                        SemanticTypeRecipe::Address {
                            inner: expected, ..
                        },
                    )
                    | (
                        SemanticTypeRecipe::Desc { inner: actual, .. },
                        SemanticTypeRecipe::Desc {
                            inner: expected, ..
                        },
                    ) => self.same(actual, expected)?,
                    _ => {
                        return Err(bad(
                            "original constructor field changes its complete borrowed carrier",
                        ));
                    }
                }
            } else {
                self.constructor_field_value(owner, &actual, &expected, depth + 1)?;
            }
        }
        self.call_requirements(owner, definition, &arguments, depth + 1)?;
        let nominal = SemanticTypeRecipe::Nominal {
            definition: selected,
            arguments,
        };
        let result = if *kind == LifeNominalKind::Resource {
            SemanticTypeRecipe::Managed(Box::new(nominal))
        } else {
            nominal
        };
        if let Some(context) = context.as_ref() {
            self.same(&result, context)?;
        }
        Ok(result)
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
            self.constructor_field_value(owner, &actual, &expected, depth + 1)?;
        }
        self.call_requirements(owner, original, arguments, depth + 1)?;
        if let Some(wanted) = wanted {
            self.same(&result, wanted)?;
        }
        Ok(result)
    }
    /// Constant shared storage may be shortened at this actual value
    /// placement. This is a source-bound theorem, never a LifeEqual premise;
    /// the complete referent and its intrinsic classes remain exact.
    fn constructor_field_value(
        &mut self,
        owner: SemanticDefinitionId,
        actual: &SemanticTypeRecipe,
        expected: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if let (
            SemanticTypeRecipe::Address {
                mutable: false,
                lifetime,
                inner: a,
            },
            SemanticTypeRecipe::Address {
                mutable: false,
                inner: b,
                ..
            },
        ) = (actual, expected)
        {
            if self.static_outer_source(owner, lifetime, depth + 1)? {
                return self.same(a, b);
            }
        }
        self.same(actual, expected)
    }
    pub(super) fn static_outer_source(
        &mut self,
        owner: SemanticDefinitionId,
        lifetime: &SemanticOuterLifeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        if let Some((owner, ordinal)) = self.region_key(owner, lifetime, depth + 1)? {
            return self.static_call_region_sources(
                owner,
                ordinal,
                &mut BTreeSet::new(),
                depth + 1,
            );
        }
        let SemanticOuterLifeRecipe::Class(lifetime) = lifetime else {
            return Ok(true);
        };
        self.pay(self.proof.local_roles.len(), depth)?;
        let mut selected = None;
        for ((scope, binding), role) in &self.proof.local_roles {
            if *scope == owner && &role.lifetime == lifetime {
                if selected.replace(*binding).is_some() {
                    return Err(bad(
                        "one original local region has ambiguous source declarations",
                    ));
                }
            }
        }
        let Some(binding) = selected else {
            return Ok(false);
        };
        self.static_role_sources(owner, binding, &mut BTreeSet::new(), depth + 1)
    }
    pub(super) fn static_role_sources(
        &mut self,
        owner: SemanticDefinitionId,
        binding: NodeId,
        active: &mut BTreeSet<(SemanticDefinitionId, Option<NodeId>, Option<u32>)>,
        depth: usize,
    ) -> R<bool> {
        self.pay(active.len().saturating_add(1), depth)?;
        if !active.insert((owner, Some(binding), None)) {
            return Err(bad("original static region has a cyclic source equation"));
        }
        let count = self
            .proof
            .local_roles
            .get(&(owner, binding))
            .ok_or_else(|| bad("original static region lost its actual declaration"))?
            .sources
            .len();
        self.pay(count, depth)?;
        let role = self
            .proof
            .local_roles
            .get(&(owner, binding))
            .ok_or_else(|| bad("original static region lost its actual declaration"))?;
        let sources = role
            .sources
            .iter()
            .map(|source| source.escape.clone())
            .collect::<Vec<_>>();
        let mut all = !sources.is_empty();
        for source in sources {
            all &= match source {
                OriginalRoleEscape::Static => true,
                OriginalRoleEscape::Role { owner, binding } => {
                    self.static_role_sources(owner, binding, active, depth + 1)?
                }
                OriginalRoleEscape::CallRegion { owner, ordinal } => {
                    self.static_call_region_sources(owner, ordinal, active, depth + 1)?
                }
                OriginalRoleEscape::Input { .. } | OriginalRoleEscape::LocalPlace { .. } => false,
            };
        }
        active.remove(&(owner, Some(binding), None));
        Ok(all)
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
            expression,
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
        self.pay(1, depth)?;
        if self
            .proof
            .proof_mut()?
            .nominal_constructors
            .insert((owner, expression.id), (id, ordinal as u32))
            .is_some_and(|previous| previous != (id, ordinal as u32))
        {
            return Err(bad(
                "original constructor expression changes its exact nominal variant",
            ));
        }
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
            self.proof
                .proof_mut()?
                .calls
                .insert((owner, site.id), actuals);
        }
        Ok(Some(solved.result))
    }

    // A Task owns its frame, but its complete declared retained domain can
    // still name external input storage. Prove each original child separately;
    // this capability neither erases that domain nor proves synchronous Drop.
    fn closed_task_shape(
        &mut self,
        owner: SemanticDefinitionId,
        shape: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(
            shape.ports.len().saturating_add(shape.retained.len())
                .saturating_add(shape.nominal.len()).saturating_add(1),
            depth,
        )?;
        let mut known = self.closed_class_sequence(owner, &shape.domain, depth + 1)?;
        for port in &shape.ports {
            let child = match port {
                SemanticPortPart::FromType { ty, classes } => {
                    let ty = self.sealed_capability(owner, ty, SemanticBuiltinTrait::ClosedEnvironment, depth + 1)?;
                    let classes = self.closed_class_sequence(owner, classes, depth + 1)?;
                    match (ty, classes) {
                        (Some(a), Some(b)) => Some(a && b),
                        _ => None,
                    }
                }
                // A real borrowed port is not an owned child environment.
                SemanticPortPart::One { .. } => Some(false),
            };
            known = match (known, child) {
                (Some(a), Some(b)) => Some(a && b),
                _ => None,
            };
        }
        for row in &shape.retained {
            let child = match row {
                SemanticRetainedPart::All { parents, .. } =>
                    self.closed_class_sequence(owner, parents, depth + 1)?,
                SemanticRetainedPart::One { .. } => Some(false),
            };
            known = match (known, child) {
                (Some(a), Some(b)) => Some(a && b),
                _ => None,
            };
        }
        for row in &shape.nominal {
            let child = match row {
                SemanticNominalPart::FromType { ty, classes, .. } => {
                    let ty = self.sealed_capability(owner, ty, SemanticBuiltinTrait::ClosedEnvironment, depth + 1)?;
                    let classes = self.closed_class_sequence(owner, classes, depth + 1)?;
                    match (ty, classes) {
                        (Some(a), Some(b)) => Some(a && b),
                        _ => None,
                    }
                }
                SemanticNominalPart::One { .. } => Some(false),
            };
            known = match (known, child) {
                (Some(a), Some(b)) => Some(a && b),
                _ => None,
            };
        }
        Ok(known)
    }

    fn closed_class_sequence(
        &mut self,
        owner: SemanticDefinitionId,
        sequence: &SemanticClassSequenceRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(sequence.parts.len().saturating_add(1), depth)?;
        let mut known = Some(true);
        for part in &sequence.parts {
            let child = match part {
                SemanticClassSequencePart::AllClasses { ty, path } if path.is_empty() =>
                    self.sealed_capability(owner, ty, SemanticBuiltinTrait::ClosedEnvironment, depth + 1)?,
                SemanticClassSequencePart::One(_) => Some(false),
                // A projected domain requires its own complete child theorem.
                SemanticClassSequencePart::AllClasses { .. } => None,
            };
            known = match (known, child) {
                (Some(a), Some(b)) => Some(a && b),
                _ => None,
            };
        }
        Ok(known)
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
            // This releases only the view. The referent's cleanup, closed
            // environment and transfer capabilities remain independent.
            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                if kind == SemanticBuiltinTrait::SyncDrop =>
            {
                Some(true)
            }
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Static,
                ..
            }
            | SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Static,
                ..
            } if kind == SemanticBuiltinTrait::ClosedEnvironment => {
                Some(self.original_zero_domain(owner, &ty, depth + 1)?)
            }
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
            SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner)
                if kind == SemanticBuiltinTrait::ClosedEnvironment =>
            {
                self.sealed_capability(owner, inner, kind, depth + 1)?
            }
            SemanticTypeRecipe::Task { result, retained }
                if kind == SemanticBuiltinTrait::ClosedEnvironment =>
            {
                let result = self.sealed_capability(owner, result, kind, depth + 1)?;
                let frame = self.closed_task_shape(owner, retained, depth + 1)?;
                match (result, frame) {
                    (Some(result), Some(frame)) => Some(result && frame),
                    _ => None,
                }
            }
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
            SemanticTypeRecipe::Opaque { definition, arguments, hidden } => {
                let source = self.definition(*definition)?;
                if matches!(kind, SemanticBuiltinTrait::ClosedEnvironment | SemanticBuiltinTrait::SyncDrop)
                    && source.parent.is_none() && source.environment.is_empty()
                    && self.proof.complete.contains(definition)
                    && matches!(&source.body, SemanticDefinitionBody::FunctionWithBody {
                        signature: SemanticSignatureRecipe { result: SemanticTypeRecipe::Opaque { definition: declared, .. }, .. },
                        foreign: None, body: SemanticOriginalFunctionBody::Defined(_), ..
                    } if declared == definition)
                {
                    // An opaque capability depends on its complete all-return
                    // equation and caller obligations, not a zero-class domain.
                    let result = self.opaque_application_result(owner, *definition, arguments, hidden, depth + 1)?;
                    self.contexts.push(*definition);
                    let declared = self.sealed_capability(*definition, &result, kind, depth + 1);
                    self.contexts.pop();
                    if declared? == Some(true) { Some(true) }
                    else {
                        let specialized = super::super::instantiate::ty(
                            self.program, &self.proof, &result,
                            super::super::instantiate::DomainMapping {
                                source: *definition, use_definition: owner, arguments, parent_call: None,
                            }, self.limits, self.left,
                        )?;
                        self.sealed_capability(owner, &specialized, kind, depth + 1)?
                    }
                } else {
                    // Other lexical parents still require their exact original
                    // application boundary; no receiver/ancestor is guessed.
                    match self.closed_opaque_result(&ty, depth + 1)? {
                        Some((definition, result)) => {
                            self.contexts.push(definition);
                            let result = self.sealed_capability(definition, &result, kind, depth + 1);
                            self.contexts.pop();
                            result?
                        }
                        None => None,
                    }
                }
            }
            SemanticTypeRecipe::Exact(exact) => Some(
                exact.life.binder_count == 0 && exact.classes.is_empty() && exact.ty.is_b0_scalar(),
            ),
            _ => None,
        };
        Ok(result)
    }
}
