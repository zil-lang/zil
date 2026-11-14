//! Associated definitions are original declaration programs. Selection uses a
//! complete symbolic receiver/application; runtime vtables and actual RHS
//! values never participate in this module.
use super::super::original_context::Declaration;
use super::*;

impl<'a> Check<'a, '_> {
    pub(super) fn original_arguments(
        &mut self,
        scope: SemanticDefinitionId,
        hops: u32,
        depth: usize,
    ) -> R<Vec<SemanticActualRecipe>> {
        let definition = self.definition(scope)?;
        self.pay(definition.parameters.len(), depth)?;
        let empty = BTreeMap::new();
        let shift = substitute::Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: hops,
        };
        let mut arguments = Vec::new();
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            let reference = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(hops),
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
                    carrier: substitute::ty(carrier, &shift, self.limits, self.left)?,
                    value: SemanticConstRecipe::Formal(reference),
                },
            });
        }
        Ok(arguments)
    }
    pub(super) fn resolve_application(
        &mut self,
        owner: SemanticDefinitionId,
        application: &SemanticTraitRecipe,
        depth: usize,
    ) -> R<SemanticTraitRecipe> {
        self.pay(application.arguments.len(), depth)?;
        let mut arguments = Vec::new();
        for actual in &application.arguments {
            arguments.push(self.resolved_actual(owner, actual, depth + 1)?);
        }
        Ok(SemanticTraitRecipe {
            declaration: application.declaration,
            arguments,
        })
    }
    fn declaration_scope(
        &mut self,
        input: &Declaration,
        depth: usize,
    ) -> R<&'a SemanticDefinition> {
        self.pay(1, depth)?;
        let scope = self.definition(input.scope)?;
        let (id, kind) = match &scope.body {
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) => {
                (*declaration, DynDeclarationKind::Trait)
            }
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                declaration, ..
            }) => (*declaration, DynDeclarationKind::Impl),
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl { .. }) => {
                (input.row.id, DynDeclarationKind::Impl)
            }
            _ => {
                return Err(bad(
                    "associated declaration does not select its original trait/impl Scope",
                ))
            }
        };
        crate::dyn_metadata::semantic_operand_snapshot(scope, self.limits, self.left, 1)?;
        crate::dyn_metadata::actual_declaration_snapshot(&input.row, self.limits, self.left, 1)?;
        if input.row.id != id
            || input.row.identity != scope.identity
            || input.row.kind != kind
            || input.row.parameters.len() != scope.parameters.len()
        {
            return Err(bad(
                "associated declaration changes its original Scope identity or mixed arity",
            ));
        }
        self.pay(scope.parameters.len(), depth)?;
        for (formal, kind) in scope.parameters.iter().zip(&input.row.parameters) {
            if !matches!(
                (formal, kind),
                (SemanticFormal::Type { .. }, LifeParameterKind::Type)
                    | (SemanticFormal::Lifetime, LifeParameterKind::Lifetime)
                    | (SemanticFormal::Const { .. }, LifeParameterKind::Const(_))
            ) {
                return Err(bad(
                    "associated declaration changes a full mixed formal kind",
                ));
            }
        }
        if matches!(
            scope.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl { .. })
        ) {
            // This source row carries no user associated program or trait
            // authority. Keep its exact data, while the sealed impl/body are
            // checked by their original builtin rules independently.
            if input
                .row
                .members
                .iter()
                .any(|member| !matches!(member.kind, DynMemberKind::Method(_)))
                || input
                    .row
                    .associated_definitions
                    .as_ref()
                    .is_some_and(|rows| !rows.is_empty())
            {
                return Err(bad(
                    "sealed builtin impl is disguised as a user associated provider",
                ));
            }
            return Ok(scope);
        }
        match (
            &scope.body,
            &input.row.self_pattern,
            &input.row.implemented_trait,
        ) {
            (SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. }), None, None) => {}
            (
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                    self_type,
                    trait_application,
                    ..
                }),
                Some(raw_self),
                Some(raw_trait),
            ) => {
                let actual_self = self.pattern_type(scope.id, raw_self, depth + 1)?;
                let actual_trait = self.pattern_application(scope.id, raw_trait, depth + 1)?;
                if &actual_self != self_type || &actual_trait != trait_application {
                    return Err(bad("associated provider changes its complete original Self or trait application"));
                }
            }
            _ => {
                return Err(pending(
                    "associated provider omits its complete original Self/trait program",
                ))
            }
        }
        let raw_requirements = input.row.requirements.as_ref().ok_or_else(|| {
            pending("associated declaration has an unknown complete where program")
        })?;
        let mut expected = Vec::new();
        for requirement in scope
            .requirements
            .iter()
            .chain(scope.parameters.iter().flat_map(|formal| match formal {
                SemanticFormal::Type { requirements } => requirements.as_slice(),
                _ => &[],
            }))
        {
            crate::dyn_metadata::semantic_operand_snapshot(requirement, self.limits, self.left, 1)?;
            expected.push(canonical_requirement(requirement.clone()));
        }
        let mut actual = Vec::new();
        for requirement in raw_requirements {
            actual.push(canonical_requirement(self.pattern_requirement(
                scope.id,
                requirement,
                depth + 1,
            )?));
        }
        if expected != actual {
            return Err(bad(
                "associated declaration changes its original complete where requirements",
            ));
        }
        Ok(scope)
    }
    fn original_declaration(
        &mut self,
        id: LifeDeclarationId,
        depth: usize,
    ) -> R<Option<Declaration>> {
        self.pay(self.proof.declarations.len(), depth)?;
        let mut selected = None;
        for row in self.proof.declarations.iter() {
            if row.row.id == id {
                if selected.is_some() {
                    return Err(bad("original associated declaration is duplicated"));
                }
                crate::dyn_metadata::actual_declaration_snapshot(
                    &row.row,
                    self.limits,
                    self.left,
                    1,
                )?;
                selected = Some(row.clone());
            }
        }
        Ok(selected)
    }
    pub(super) fn associated_inventory(&mut self, depth: usize) -> R<()> {
        let declarations = self.proof.declarations.clone();
        self.pay(declarations.len().saturating_add(1), depth)?;
        let mut seen = BTreeSet::new();
        let mut operands = Vec::new();
        let mut requirements = Vec::new();
        for input in declarations.iter() {
            self.pay(seen.len().saturating_add(input.row.members.len()), depth)?;
            if !seen.insert(input.row.id) {
                return Err(bad("original associated declaration identity is repeated"));
            }
            if self.proof.associated_complete.contains(&input.row.id) {
                continue;
            }
            let scope = self.declaration_scope(input, depth + 1)?;
            if matches!(
                scope.body,
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl { .. })
            ) {
                continue;
            }
            let mut names = BTreeSet::new();
            for member in &input.row.members {
                self.pay(member.name.len().saturating_add(names.len()), depth)?;
                if !names.insert(member.name.as_str()) {
                    return Err(bad("original declaration repeats a member spelling"));
                }
            }
            let definitions = input.row.associated_definitions.as_ref().ok_or_else(|| {
                pending("original declaration omits its complete associated-definition program")
            })?;
            let mut members = BTreeSet::new();
            for definition in definitions {
                self.pay(members.len().saturating_add(1), depth)?;
                let member = input
                    .row
                    .members
                    .get(definition.member as usize)
                    .ok_or_else(|| {
                        bad("associated definition selects an absent member in its own declaration")
                    })?;
                if !members.insert(definition.member)
                    || matches!(member.kind, DynMemberKind::Method(_))
                {
                    return Err(bad(
                        "associated definition duplicates a member or selects a method",
                    ));
                }
                match (&member.kind, &definition.carrier, &definition.value) {
                    (DynMemberKind::Type, None, None | Some(DynAssociatedValueRecipe::Type(_))) => {
                    }
                    (
                        DynMemberKind::Constant,
                        Some(_),
                        None | Some(DynAssociatedValueRecipe::Constant { .. }),
                    ) => {}
                    _ => return Err(bad(
                        "associated definition changes its declared Type/Const category or carrier",
                    )),
                }
                if input.row.kind == DynDeclarationKind::Impl && definition.value.is_none() {
                    return Err(bad(
                        "original implementation leaves an associated definition abstract",
                    ));
                }
                for obligation in &definition.requirements {
                    requirements.push((
                        scope.id,
                        self.pattern_requirement(scope.id, obligation, depth + 1)?,
                    ));
                }
                if let Some(carrier) = &definition.carrier {
                    operands.push((
                        scope.id,
                        SemanticActualRecipe::Type(self.pattern_type(
                            scope.id,
                            carrier,
                            depth + 1,
                        )?),
                    ));
                }
                if let Some(value) = &definition.value {
                    match value {
                        DynAssociatedValueRecipe::Type(ty) => {
                            operands.push((
                                scope.id,
                                SemanticActualRecipe::Type(self.pattern_type(
                                    scope.id,
                                    ty,
                                    depth + 1,
                                )?),
                            ));
                        }
                        DynAssociatedValueRecipe::Constant { ty, value } => {
                            operands.push((
                                scope.id,
                                SemanticActualRecipe::Const {
                                    carrier: self.pattern_type(scope.id, ty, depth + 1)?,
                                    value: self.pattern_constant(scope.id, value, depth + 1)?,
                                },
                            ));
                        }
                    }
                }
            }
            for (ordinal, member) in input.row.members.iter().enumerate() {
                self.pay(1, depth)?;
                if !matches!(member.kind, DynMemberKind::Method(_))
                    && !members.contains(&(ordinal as u32))
                {
                    return Err(bad(
                        "original declaration omits an associated member definition/bounds row",
                    ));
                }
            }
        }
        super::super::well_formed::declaration_operands(
            self.program,
            &operands,
            &requirements,
            self.limits,
            self.left,
        )?;
        Ok(())
    }
    fn pattern_life(
        &mut self,
        scope: SemanticDefinitionId,
        value: DynLifePattern,
        depth: usize,
    ) -> R<SemanticLifeRecipe> {
        self.pay(1, depth)?;
        Ok(match value {
            DynLifePattern::DeclarationParameter(ordinal) => {
                if !matches!(
                    self.definition(scope)?.parameters.get(ordinal as usize),
                    Some(SemanticFormal::Lifetime)
                ) {
                    return Err(bad(
                        "associated recipe Life operand changes its full formal kind",
                    ));
                }
                SemanticLifeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                })
            }
            DynLifePattern::Bound { depth, index } => SemanticLifeRecipe::Bound { depth, index },
            DynLifePattern::Invocation(index) => SemanticLifeRecipe::Invocation(index),
            DynLifePattern::MethodParameter(_) => {
                return Err(bad(
                    "associated declaration recipe borrows an unrelated method parameter domain",
                ))
            }
            DynLifePattern::ImplicitDeclaration(_) => return Err(pending(
                "legacy anonymous associated lifetime lacks its real Semantic occurrence mapping",
            )),
        })
    }
    fn pattern_application(
        &mut self,
        scope: SemanticDefinitionId,
        value: &DynSelfPattern,
        depth: usize,
    ) -> R<SemanticTraitRecipe> {
        let arguments = self.pattern_arguments(scope, value, depth + 1)?;
        Ok(SemanticTraitRecipe {
            declaration: value.declaration,
            arguments,
        })
    }
    fn pattern_arguments(
        &mut self,
        scope: SemanticDefinitionId,
        value: &DynSelfPattern,
        depth: usize,
    ) -> R<Vec<SemanticActualRecipe>> {
        self.pay(
            self.program
                .definitions
                .len()
                .saturating_add(value.arguments.len()),
            depth,
        )?;
        let mut target = None;
        for candidate in &self.program.definitions {
            let id = match &candidate.body {
                SemanticDefinitionBody::Nominal { declaration, .. }
                | SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration })
                | SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                    declaration,
                    ..
                }) => *declaration,
                _ => continue,
            };
            if id == value.declaration && target.replace(candidate).is_some() {
                return Err(bad("associated application declaration is ambiguous"));
            }
        }
        let target =
            target.ok_or_else(|| bad("associated application has no real original declaration"))?;
        if target.parameters.len() != value.arguments.len() {
            return Err(bad(
                "associated application omits full mixed actual positions",
            ));
        }
        let mut arguments = Vec::new();
        for (formal, actual) in target.parameters.iter().zip(&value.arguments) {
            let actual = match (formal, actual) {
                (SemanticFormal::Type { .. }, DynPatternActual::Type(ty)) => {
                    SemanticActualRecipe::Type(self.pattern_type(scope, ty, depth + 1)?)
                }
                (SemanticFormal::Lifetime, DynPatternActual::Life(life)) => {
                    SemanticActualRecipe::Life(self.pattern_life(scope, *life, depth + 1)?)
                }
                (SemanticFormal::Const { carrier }, DynPatternActual::Const(value)) => {
                    SemanticActualRecipe::Const {
                        carrier: copy_recipe(carrier, self.limits, self.left)?,
                        value: self.pattern_constant(scope, value, depth + 1)?,
                    }
                }
                _ => {
                    return Err(bad(
                        "associated application changes a full mixed actual kind",
                    ))
                }
            };
            arguments.push(actual);
        }
        // Carriers are source declaration expressions, not the claimant's
        // concrete Const type. Complete Type actuals may occur after the Const.
        let mut carriers = Vec::new();
        self.pay(target.parameters.len(), depth)?;
        for (ordinal, formal) in target.parameters.iter().enumerate() {
            carriers.push(match formal {
                SemanticFormal::Const { .. } => Some(super::super::instantiate::carrier(
                    self.program,
                    &self.proof,
                    super::super::instantiate::DomainMapping {
                        source: target.id,
                        use_definition: scope,
                        arguments: &arguments,
                        parent_call: None,
                    },
                    ordinal as u32,
                    self.limits,
                    self.left,
                )?),
                _ => None,
            });
        }
        for (actual, carrier) in arguments.iter_mut().zip(carriers) {
            if let (
                SemanticActualRecipe::Const {
                    carrier: actual, ..
                },
                Some(carrier),
            ) = (actual, carrier)
            {
                *actual = carrier;
            }
        }
        Ok(arguments)
    }
    fn pattern_type(
        &mut self,
        scope: SemanticDefinitionId,
        value: &DynTypePattern,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        Ok(match value {
            DynTypePattern::Semantic { scope: owner, ty } => {
                if *owner != scope {
                    return Err(bad(
                        "associated Semantic recipe changes its real declaration owner",
                    ));
                }
                copy_recipe(ty, self.limits, self.left)?
            }
            DynTypePattern::SelfType => SemanticTypeRecipe::SelfType {
                scope: SemanticScopeRef::Ancestor(0),
            },
            DynTypePattern::Parameter(ordinal) => {
                if !matches!(
                    self.definition(scope)?.parameters.get(*ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(bad(
                        "associated Type operand changes its original full formal kind",
                    ));
                }
                SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: *ordinal,
                })
            }
            DynTypePattern::MethodParameter(_) => {
                return Err(bad(
                    "associated Type definition selects an unrelated method parameter",
                ))
            }
            DynTypePattern::Concrete(ty) => {
                if !(ty.is_b0_scalar()
                    || matches!(
                        ty,
                        Type::Unit
                            | Type::ForeignHandle
                            | Type::CWideString
                            | Type::ByteBuffer
                            | Type::TaskI32
                    ))
                {
                    return Err(pending(
                        "closed associated Type requires its complete original Semantic bridge",
                    ));
                }
                crate::inference::charge_type_snapshot(
                    ty,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                SemanticTypeRecipe::Atom(ty.clone())
            }
            DynTypePattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => SemanticTypeRecipe::AssociatedType {
                receiver: Box::new(self.pattern_type(scope, receiver, depth + 1)?),
                application: Box::new(self.pattern_application(scope, application, depth + 1)?),
                member: *member,
            },
            DynTypePattern::Nominal(application) => {
                let arguments = self.pattern_arguments(scope, application, depth + 1)?;
                let mut selected = None;
                self.pay(self.program.definitions.len(), depth)?;
                for definition in &self.program.definitions {
                    if matches!(&definition.body, SemanticDefinitionBody::Nominal { declaration, .. } if *declaration == application.declaration)
                        && selected.replace(definition.id).is_some()
                    {
                        return Err(bad("associated nominal declaration is duplicated"));
                    }
                }
                SemanticTypeRecipe::Nominal {
                    definition: selected.ok_or_else(|| {
                        bad("associated nominal has no real original declaration")
                    })?,
                    arguments,
                }
            }
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            } => SemanticTypeRecipe::Address {
                mutable: *mutable,
                lifetime: SemanticOuterLifeRecipe::Class(self.pattern_life(
                    scope,
                    lifetime.ok_or_else(|| {
                        pending("associated address lacks its real anonymous occurrence")
                    })?,
                    depth + 1,
                )?),
                inner: Box::new(self.pattern_type(scope, inner, depth + 1)?),
            },
            DynTypePattern::Desc { lifetime, inner } => SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(self.pattern_life(
                    scope,
                    lifetime.ok_or_else(|| {
                        pending("associated description lacks its real anonymous occurrence")
                    })?,
                    depth + 1,
                )?),
                inner: Box::new(self.pattern_type(scope, inner, depth + 1)?),
            },
            DynTypePattern::Tuple(fields) => {
                self.pay(fields.len(), depth)?;
                let mut output = Vec::new();
                for field in fields {
                    output.push(self.pattern_type(scope, field, depth + 1)?);
                }
                SemanticTypeRecipe::Tuple(output)
            }
            DynTypePattern::Array { element, length } => SemanticTypeRecipe::Array {
                element: Box::new(self.pattern_type(scope, element, depth + 1)?),
                length: Box::new(self.pattern_constant(scope, length, depth + 1)?),
            },
            DynTypePattern::Slice(inner) => {
                SemanticTypeRecipe::Slice(Box::new(self.pattern_type(scope, inner, depth + 1)?))
            }
            DynTypePattern::Managed(inner) => {
                SemanticTypeRecipe::Managed(Box::new(self.pattern_type(scope, inner, depth + 1)?))
            }
            DynTypePattern::OwnedStore(inner) => SemanticTypeRecipe::OwnedStore(Box::new(
                self.pattern_type(scope, inner, depth + 1)?,
            )),
            DynTypePattern::ErrorUnion { error, result } => SemanticTypeRecipe::ErrorUnion {
                error: Box::new(self.pattern_type(scope, error, depth + 1)?),
                result: Box::new(self.pattern_type(scope, result, depth + 1)?),
            },
        })
    }
    fn pattern_constant(
        &mut self,
        scope: SemanticDefinitionId,
        value: &DynConstPattern,
        depth: usize,
    ) -> R<SemanticConstRecipe> {
        self.pay(1, depth)?;
        Ok(match value {
            DynConstPattern::Value(constant) => {
                crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
                SemanticConstRecipe::Value(constant.clone())
            }
            DynConstPattern::IntegerLiteral(text) => {
                self.pay(text.len(), depth)?;
                SemanticConstRecipe::IntegerLiteral(text.clone())
            }
            DynConstPattern::FloatLiteral(text) => {
                self.pay(text.len(), depth)?;
                SemanticConstRecipe::FloatLiteral(text.clone())
            }
            DynConstPattern::Parameter(ordinal) => {
                if !matches!(
                    self.definition(scope)?.parameters.get(*ordinal as usize),
                    Some(SemanticFormal::Const { .. })
                ) {
                    return Err(bad(
                        "associated Const operand changes its original full formal kind",
                    ));
                }
                SemanticConstRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: *ordinal,
                })
            }
            DynConstPattern::MethodParameter(_) => {
                return Err(bad(
                    "associated Const definition selects an unrelated method parameter",
                ))
            }
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => SemanticConstRecipe::AssociatedConst {
                receiver: Box::new(self.pattern_type(scope, receiver, depth + 1)?),
                application: Box::new(self.pattern_application(scope, application, depth + 1)?),
                member: *member,
            },
            DynConstPattern::Unary { op, value, ty } => SemanticConstRecipe::Unary {
                op: *op,
                operand_type: Box::new(self.pattern_type(scope, ty, depth + 1)?),
                value: Box::new(self.pattern_constant(scope, value, depth + 1)?),
            },
            DynConstPattern::Binary {
                op,
                left,
                right,
                ty,
            } => SemanticConstRecipe::Binary {
                op: *op,
                operand_type: Box::new(self.pattern_type(scope, ty, depth + 1)?),
                left: Box::new(self.pattern_constant(scope, left, depth + 1)?),
                right: Box::new(self.pattern_constant(scope, right, depth + 1)?),
            },
            DynConstPattern::Convert { value, ty } => {
                let source_type =
                    match value.as_ref() {
                        DynConstPattern::Value(value) => SemanticTypeRecipe::Atom(value.ty()),
                        _ => return Err(pending(
                            "associated Const conversion lacks its full original source carrier",
                        )),
                    };
                SemanticConstRecipe::Convert {
                    source_type: Box::new(source_type),
                    target_type: Box::new(self.pattern_type(scope, ty, depth + 1)?),
                    value: Box::new(self.pattern_constant(scope, value, depth + 1)?),
                }
            }
        })
    }
    fn pattern_requirement(
        &mut self,
        scope: SemanticDefinitionId,
        value: &DynObligation,
        depth: usize,
    ) -> R<SemanticRequirementRecipe> {
        self.pay(1, depth)?;
        Ok(match value {
            DynObligation::Structural { receiver, bound } => {
                SemanticRequirementRecipe::Structural {
                    ty: self.pattern_type(scope, receiver, depth + 1)?,
                    bound: *bound,
                }
            }
            DynObligation::Trait {
                receiver,
                application,
            } => SemanticRequirementRecipe::Trait {
                ty: self.pattern_type(scope, receiver, depth + 1)?,
                application: self.pattern_application(scope, application, depth + 1)?,
            },
            DynObligation::TypeEqual { left, right } => SemanticRequirementRecipe::TypeEqual {
                left: self.pattern_type(scope, left, depth + 1)?,
                right: self.pattern_type(scope, right, depth + 1)?,
            },
            DynObligation::ConstEqual { ty, left, right } => {
                SemanticRequirementRecipe::ConstEqual {
                    carrier: self.pattern_type(scope, ty, depth + 1)?,
                    left: self.pattern_constant(scope, left, depth + 1)?,
                    right: self.pattern_constant(scope, right, depth + 1)?,
                }
            }
            DynObligation::Callable {
                receiver,
                kind,
                quantified_lifetimes,
                signature,
            } => {
                self.pay(signature.parameters.len(), depth)?;
                let mut parameters = Vec::new();
                for parameter in &signature.parameters {
                    parameters.push(SemanticParameterRecipe {
                        ty: self.pattern_type(scope, &parameter.ty, depth + 1)?,
                        mode: parameter.mode,
                    });
                }
                SemanticRequirementRecipe::Callable {
                    ty: self.pattern_type(scope, receiver, depth + 1)?,
                    kind: *kind,
                    signature: Box::new(SemanticSignatureRecipe {
                        bound_lifetimes: *quantified_lifetimes,
                        invocation_lifetimes: signature.invocation_binder_count,
                        parameters,
                        result: self.pattern_type(scope, &signature.result, depth + 1)?,
                        result_mode: signature.result_mode,
                        call_kind: signature.call_kind,
                    }),
                }
            }
        })
    }
}

fn canonical_requirement(value: SemanticRequirementRecipe) -> SemanticRequirementRecipe {
    match value {
        SemanticRequirementRecipe::Builtin { ty, trait_kind } => {
            let bound = match trait_kind {
                SemanticBuiltinTrait::Copy => DynStructuralBound::Copy,
                SemanticBuiltinTrait::Value => DynStructuralBound::Value,
                SemanticBuiltinTrait::StoreElement => DynStructuralBound::StoreElement,
                SemanticBuiltinTrait::SyncDrop => DynStructuralBound::SyncDrop,
                _ => return SemanticRequirementRecipe::Builtin { ty, trait_kind },
            };
            SemanticRequirementRecipe::Structural { ty, bound }
        }
        value => value,
    }
}

impl Check<'_, '_> {
    fn associated_member<'a>(
        &mut self,
        input: &'a Declaration,
        member: u32,
        depth: usize,
    ) -> R<&'a DynAssociatedDefinition> {
        self.pay(input.row.members.len(), depth)?;
        if !matches!(
            input
                .row
                .members
                .get(member as usize)
                .map(|member| &member.kind),
            Some(DynMemberKind::Type)
        ) {
            return Err(bad(
                "associated Type projection does not select a Type in its real trait declaration",
            ));
        }
        let definitions = input
            .row
            .associated_definitions
            .as_ref()
            .ok_or_else(|| pending("associated member program is unknown"))?;
        let mut rows = definitions.iter().filter(|row| row.member == member);
        let selected = rows
            .next()
            .ok_or_else(|| bad("associated member has no original definition/bounds row"))?;
        if rows.next().is_some() {
            return Err(bad("associated member has duplicate original definitions"));
        }
        Ok(selected)
    }
    fn associated_bounds(
        &mut self,
        owner: SemanticDefinitionId,
        input: &Declaration,
        receiver: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        member: u32,
        depth: usize,
    ) -> R<Vec<SemanticRequirementRecipe>> {
        let member = self.associated_member(input, member, depth + 1)?;
        self.pay(member.requirements.len(), depth)?;
        let mut output = Vec::new();
        for bound in &member.requirements {
            let raw = self.pattern_requirement(input.scope, bound, depth + 1)?;
            output.push(super::super::instantiate::requirement_with_self(
                self.program,
                &self.proof,
                &raw,
                super::super::instantiate::DomainMapping {
                    source: input.scope,
                    use_definition: owner,
                    arguments: &application.arguments,
                    parent_call: None,
                },
                receiver,
                self.limits,
                self.left,
            )?);
        }
        Ok(output)
    }
    /// The only extra assumptions are implications of an already present
    /// caller trait premise and that trait's original associated bounds.
    pub(super) fn extend_associated_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        requirements: &mut Vec<SemanticRequirementRecipe>,
        depth: usize,
    ) -> R<()> {
        let original_len = requirements.len();
        self.pay(original_len, depth)?;
        let mut derived = Vec::new();
        for requirement in requirements.iter().take(original_len) {
            let SemanticRequirementRecipe::Trait { ty, application } = requirement else {
                continue;
            };
            let Some(input) = self.original_declaration(application.declaration, depth + 1)? else {
                continue;
            };
            let scope = self.declaration_scope(&input, depth + 1)?;
            for requirement in scope
                .requirements
                .iter()
                .chain(scope.parameters.iter().flat_map(|formal| match formal {
                    SemanticFormal::Type { requirements } => requirements.as_slice(),
                    _ => &[],
                }))
            {
                derived.push(super::super::instantiate::requirement_with_self(
                    self.program,
                    &self.proof,
                    requirement,
                    super::super::instantiate::DomainMapping {
                        source: scope.id,
                        use_definition: owner,
                        arguments: &application.arguments,
                        parent_call: None,
                    },
                    ty,
                    self.limits,
                    self.left,
                )?);
            }
            let definitions = input
                .row
                .associated_definitions
                .as_ref()
                .ok_or_else(|| pending("caller trait associated bounds are unknown"))?;
            for definition in definitions {
                self.pay(1, depth)?;
                if matches!(
                    input
                        .row
                        .members
                        .get(definition.member as usize)
                        .map(|m| &m.kind),
                    Some(DynMemberKind::Type)
                ) {
                    derived.extend(self.associated_bounds(
                        owner,
                        &input,
                        ty,
                        application,
                        definition.member,
                        depth + 1,
                    )?);
                }
            }
        }
        self.pay(derived.len(), depth)?;
        requirements.extend(derived);
        Ok(())
    }
    pub(super) fn resolve_associated(
        &mut self,
        owner: SemanticDefinitionId,
        receiver: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        member: u32,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let receiver = self.observation_type(owner, receiver, depth + 1)?;
        let application = self.resolve_application(owner, application, depth + 1)?;
        self.pay(2, depth)?;
        let projection = SemanticTypeRecipe::AssociatedType {
            receiver: Box::new(copy_recipe(&receiver, self.limits, self.left)?),
            application: Box::new(application.clone()),
            member,
        };
        self.pay(self.predicate_active.len().saturating_add(1), depth)?;
        if self
            .predicate_active
            .iter()
            .any(|(domain, kind, active)| *domain == owner && *kind == 10 && active == &projection)
        {
            return Err(bad(
                "original associated definitions form a cyclic equation",
            ));
        }
        let active = copy_recipe(&projection, self.limits, self.left)?;
        self.predicate_active.push((owner, 10, active));
        let result = self.resolve_associated_inner(
            owner,
            &receiver,
            &application,
            member,
            &projection,
            depth + 1,
        );
        self.predicate_active.pop();
        result
    }
    fn resolve_associated_inner(
        &mut self,
        owner: SemanticDefinitionId,
        receiver: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        member: u32,
        projection: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let input = self
            .original_declaration(application.declaration, depth + 1)?
            .ok_or_else(|| {
                pending("associated Type needs its complete original declaration catalog")
            })?;
        let trait_scope = self.declaration_scope(&input, depth + 1)?;
        if input.row.kind != DynDeclarationKind::Trait {
            return Err(bad(
                "associated projection application names an impl instead of its declaring trait",
            ));
        }
        let declared = self.associated_member(&input, member, depth + 1)?;
        // An explicit caller equation remains an original premise; concrete
        // equality of a later instance supplies no such equation.
        for requirement in self.original_requirements(owner, depth + 1)? {
            if let SemanticRequirementRecipe::TypeEqual { left, right } = requirement {
                if &left == projection {
                    return self.resolve(owner, &right, depth + 1);
                }
                if &right == projection {
                    return self.resolve(owner, &left, depth + 1);
                }
            }
        }
        if matches!(
            receiver,
            SemanticTypeRecipe::Formal(_)
                | SemanticTypeRecipe::SelfType { .. }
                | SemanticTypeRecipe::AssociatedType { .. }
                | SemanticTypeRecipe::Environment { .. }
                | SemanticTypeRecipe::OriginalType { .. }
                | SemanticTypeRecipe::Opaque { .. }
        ) {
            // Abstract I.Item remains abstract. Only the declaring trait's
            // independently retained bounds can be used by its caller.
            return copy_recipe(projection, self.limits, self.left);
        }
        let Some((provider, actuals)) =
            self.select_trait_provider(owner, receiver, application, depth + 1)?
        else {
            return Err(bad(
                "original associated projection lacks a unique valid full provider",
            ));
        };
        let mut provider_input = None;
        let inputs = self.proof.declarations.clone();
        self.pay(inputs.len(), depth)?;
        for candidate in inputs.iter() {
            if candidate.scope == provider {
                if provider_input.is_some() {
                    return Err(bad(
                        "associated provider has duplicate original declaration bindings",
                    ));
                }
                crate::dyn_metadata::actual_declaration_snapshot(
                    &candidate.row,
                    self.limits,
                    self.left,
                    1,
                )?;
                provider_input = Some(candidate.clone());
            }
        }
        let provider_input = provider_input.ok_or_else(|| {
            pending("selected associated provider omits its complete original catalog")
        })?;
        self.declaration_scope(&provider_input, depth + 1)?;
        let name = &input.row.members[member as usize].name;
        let mut override_member = None;
        for (ordinal, candidate) in provider_input.row.members.iter().enumerate() {
            self.pay(candidate.name.len().saturating_add(name.len()), depth)?;
            if candidate.name == *name {
                if !matches!(candidate.kind, DynMemberKind::Type)
                    || override_member.replace(ordinal as u32).is_some()
                {
                    return Err(bad(
                        "associated override changes or duplicates its true trait member identity",
                    ));
                }
            }
        }
        let (source, arguments, raw_value) = if let Some(ordinal) = override_member {
            let definition = self.associated_member(&provider_input, ordinal, depth + 1)?;
            let Some(DynAssociatedValueRecipe::Type(value)) = &definition.value else {
                return Err(bad("selected implementation omits its associated Type RHS"));
            };
            (provider_input.scope, actuals.as_slice(), value)
        } else {
            let Some(DynAssociatedValueRecipe::Type(value)) = &declared.value else {
                return Err(bad(
                    "associated member has neither an override nor a true trait default",
                ));
            };
            (trait_scope.id, application.arguments.as_slice(), value)
        };
        let value = self.pattern_type(source, raw_value, depth + 1)?;
        let value = super::super::instantiate::ty_with_self(
            self.program,
            &self.proof,
            &value,
            super::super::instantiate::DomainMapping {
                source,
                use_definition: owner,
                arguments,
                parent_call: None,
            },
            receiver,
            self.limits,
            self.left,
        )?;
        let value = self.resolve(owner, &value, depth + 1)?;
        let equation = [(
            copy_recipe(projection, self.limits, self.left)?,
            copy_recipe(&value, self.limits, self.left)?,
        )];
        for bound in
            self.associated_bounds(owner, &input, receiver, application, member, depth + 1)?
        {
            let bound =
                substitute::requirement_replacing(&bound, &equation, self.limits, self.left)?;
            self.satisfy_requirement(owner, &bound, depth + 1)?;
        }
        Ok(value)
    }
}

impl Check<'_, '_> {
    fn check_member_bounds(
        &mut self,
        owner: SemanticDefinitionId,
        declaring: &Declaration,
        receiver: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        member: u32,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(2, depth)?;
        let projection = SemanticTypeRecipe::AssociatedType {
            receiver: Box::new(copy_recipe(receiver, self.limits, self.left)?),
            application: Box::new(application.clone()),
            member,
        };
        let equation = [(projection, copy_recipe(value, self.limits, self.left)?)];
        for bound in
            self.associated_bounds(owner, declaring, receiver, application, member, depth + 1)?
        {
            let bound =
                substitute::requirement_replacing(&bound, &equation, self.limits, self.left)?;
            self.satisfy_requirement(owner, &bound, depth + 1)?;
        }
        Ok(())
    }
    /// Every unused provider/default is checked in its own rigid original
    /// parameters. Its requirements are assumptions only in that definition;
    /// each use separately proves them in the real caller Gamma.
    pub(super) fn associated_obligations(&mut self, depth: usize) -> R<()> {
        let inputs = self.proof.declarations.clone();
        self.pay(inputs.len().saturating_add(1), depth)?;
        for input in inputs.iter() {
            if self.proof.associated_complete.contains(&input.row.id) {
                continue;
            }
            let scope = self.declaration_scope(input, depth + 1)?;
            if matches!(
                scope.body,
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl { .. })
            ) {
                continue;
            }
            self.contexts.push(scope.id);
            let result = self.associated_definition_obligations(input, scope, depth + 1);
            self.contexts.pop();
            result?;
            self.pay(1, depth)?;
            self.proof.associated_complete.insert(input.row.id);
        }
        Ok(())
    }
    fn associated_definition_obligations(
        &mut self,
        input: &Declaration,
        scope: &SemanticDefinition,
        depth: usize,
    ) -> R<()> {
        let (trait_input, receiver, application) = match &scope.body {
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) => (
                input.clone(),
                SemanticTypeRecipe::SelfType {
                    scope: SemanticScopeRef::Ancestor(0),
                },
                SemanticTraitRecipe {
                    declaration: *declaration,
                    arguments: self.original_arguments(scope.id, 0, depth + 1)?,
                },
            ),
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                self_type,
                trait_application,
                ..
            }) => {
                let declaring = self
                    .original_declaration(trait_application.declaration, depth + 1)?
                    .ok_or_else(|| {
                        pending("implementation omits its original declaring trait catalog")
                    })?;
                let declared_scope = self.declaration_scope(&declaring, depth + 1)?;
                // Validate the implemented trait's own full where/super program;
                // none of these target requirements become caller assumptions.
                for requirement in declared_scope.requirements.iter().chain(
                    declared_scope
                        .parameters
                        .iter()
                        .flat_map(|formal| match formal {
                            SemanticFormal::Type { requirements } => requirements.as_slice(),
                            _ => &[],
                        }),
                ) {
                    let requirement = super::super::instantiate::requirement_with_self(
                        self.program,
                        &self.proof,
                        requirement,
                        super::super::instantiate::DomainMapping {
                            source: declared_scope.id,
                            use_definition: scope.id,
                            arguments: &trait_application.arguments,
                            parent_call: None,
                        },
                        self_type,
                        self.limits,
                        self.left,
                    )?;
                    self.satisfy_requirement(scope.id, &requirement, depth + 1)?;
                }
                (
                    declaring,
                    copy_recipe(self_type, self.limits, self.left)?,
                    trait_application.clone(),
                )
            }
            _ => {
                return Err(bad(
                    "associated definition has no real original trait/impl Scope",
                ))
            }
        };
        let definitions = input
            .row
            .associated_definitions
            .as_ref()
            .ok_or_else(|| pending("original associated definition program is unknown"))?;
        // Every impl member is tied to this one implemented trait. Written
        // ordinals may differ; names are compared only within those two owners.
        for definition in definitions {
            let own = &input.row.members[definition.member as usize];
            if matches!(own.kind, DynMemberKind::Constant) {
                return Err(pending("associated Const definitions require their full original carrier/value theorem"));
            }
            let mut declared_member = None;
            for (ordinal, member) in trait_input.row.members.iter().enumerate() {
                self.pay(member.name.len().saturating_add(own.name.len()), depth)?;
                if member.name == own.name {
                    if !matches!(member.kind, DynMemberKind::Type)
                        || declared_member.replace(ordinal as u32).is_some()
                    {
                        return Err(bad(
                            "associated definition changes its true declaring trait member",
                        ));
                    }
                }
            }
            let member = declared_member
                .ok_or_else(|| bad("implementation adds an undeclared associated member"))?;
            let Some(DynAssociatedValueRecipe::Type(value)) = &definition.value else {
                continue;
            };
            let value = self.pattern_type(scope.id, value, depth + 1)?;
            for occurrence in substitute::delayed_types(&value, self.limits, self.left)? {
                if matches!(&occurrence.ty, SemanticTypeRecipe::AssociatedType { receiver, .. }
                    if matches!(receiver.as_ref(), SemanticTypeRecipe::SelfType { .. }))
                {
                    return Err(pending("Self-dependent associated RHS needs its complete original dependency graph"));
                }
            }
            // Own override bounds are obligations too, not assumptions about
            // an arbitrary RHS. Its subject is the real declaring trait member.
            let own_projection = SemanticTypeRecipe::AssociatedType {
                receiver: Box::new(SemanticTypeRecipe::SelfType {
                    scope: SemanticScopeRef::Ancestor(0),
                }),
                application: Box::new(application.clone()),
                member,
            };
            let equation = [(own_projection, copy_recipe(&value, self.limits, self.left)?)];
            for bound in &definition.requirements {
                let bound = self.pattern_requirement(scope.id, bound, depth + 1)?;
                let bound =
                    substitute::requirement_replacing(&bound, &equation, self.limits, self.left)?;
                self.satisfy_requirement(scope.id, &bound, depth + 1)?;
            }
            // No concrete instance is available here. Check the complete RHS
            // under its original rigid formals and original declared bounds.
            self.check_member_bounds(
                scope.id,
                &trait_input,
                &receiver,
                &application,
                member,
                &value,
                depth + 1,
            )?;
        }
        if input.row.kind == DynDeclarationKind::Impl {
            for (ordinal, member) in trait_input.row.members.iter().enumerate() {
                if !matches!(member.kind, DynMemberKind::Type) {
                    continue;
                }
                let supplied =
                    input.row.members.iter().any(|own| {
                        own.name == member.name && matches!(own.kind, DynMemberKind::Type)
                    });
                if !supplied {
                    let declared =
                        self.associated_member(&trait_input, ordinal as u32, depth + 1)?;
                    let Some(DynAssociatedValueRecipe::Type(value)) = &declared.value else {
                        return Err(bad("implementation omits a required associated Type without a trait default"));
                    };
                    let raw = self.pattern_type(trait_input.scope, value, depth + 1)?;
                    let value = super::super::instantiate::ty_with_self(
                        self.program,
                        &self.proof,
                        &raw,
                        super::super::instantiate::DomainMapping {
                            source: trait_input.scope,
                            use_definition: scope.id,
                            arguments: &application.arguments,
                            parent_call: None,
                        },
                        &receiver,
                        self.limits,
                        self.left,
                    )?;
                    self.check_member_bounds(
                        scope.id,
                        &trait_input,
                        &receiver,
                        &application,
                        ordinal as u32,
                        &value,
                        depth + 1,
                    )?;
                }
            }
        }
        Ok(())
    }
}
