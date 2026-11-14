//! Prove every original provider under its own rigid mixed context, including
//! unused member definitions. A concrete selected caller cannot add a premise.
use super::*;

struct Signature {
    parameters: Vec<Term>,
    result: Term,
    modes: Vec<PassingMode>,
    result_mode: PassingMode,
    kind: CallKind,
}

impl Solver<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn conformance_signature(
        &mut self,
        scope: SemanticDefinitionId,
        scope_actuals: &[Term],
        method: SemanticDefinitionId,
        receiver: Term,
        method_actuals: &[Term],
        rigid: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Signature> {
        self.step(depth, b)?;
        let definition = self.inventory.definition(method, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            protocol_contracts,
            foreign,
            ..
        } = &definition.body
        else {
            return Err(fail(
                "original provider method omits its full declaration and body",
            ));
        };
        if definition.parent != Some(scope) || signature.bound_lifetimes != 0 || foreign.is_some() {
            return Err(unknown(
                "original provider member needs its complete quantified or foreign conformance theorem",
            ));
        }
        if !protocol_contracts.is_empty() {
            return Err(unknown(
                "trait method protocol refinement needs its complete universal entry/exit implication",
            ));
        }
        if method_actuals.len() != definition.parameters.len() {
            return Err(fail(
                "original provider method changes its full mixed method arity",
            ));
        }
        b.spend(
            scope_actuals.len().saturating_add(method_actuals.len()),
            WORK,
        )?;
        let context = trait_frame(method, method_actuals.to_vec());
        let saved = self
            .state
            .frames
            .remove(&scope)
            .ok_or_else(|| fail("original method conformance has no checked parent scope"))?;
        self.state
            .frames
            .insert(scope, trait_frame(scope, scope_actuals.to_vec()));
        Self::push(&mut self.trait_state.selves, (scope, receiver), b)?;
        let serial = self.next_role;
        self.next_role = serial
            .checked_add(1)
            .ok_or_else(|| resource("original method conformance role namespace overflow"))?;
        let mut lifetimes = Vec::new();
        let mut implicits = BTreeMap::new();
        for index in 0..signature.invocation_lifetimes {
            let term = if rigid {
                self.term(Kind::Life(Life::Role(method, serial, index)), b)?
            } else {
                self.term(Kind::LifeVariable(None), b)?
            };
            Self::push(&mut lifetimes, term, b)?;
        }
        for row in &definition.implicit_lifetimes {
            let index = signature
                .invocation_lifetimes
                .checked_add(row.ordinal)
                .ok_or_else(|| resource("original method implicit role ordinal overflow"))?;
            let term = if rigid {
                self.term(Kind::Life(Life::Role(method, serial, index)), b)?
            } else {
                self.term(Kind::LifeVariable(None), b)?
            };
            b.lookup(implicits.len(), WORK)?;
            if implicits.insert(row.ordinal, term).is_some() {
                return Err(fail(
                    "original method conformance repeats an implicit occurrence",
                ));
            }
        }
        Self::push(
            &mut self.role_frames,
            roles::RoleFrame {
                owner: method,
                lifetimes,
                implicits,
            },
            b,
        )?;
        let result: CompileResult<Signature> = (|| {
            let mut parameters = Vec::new();
            let mut modes = Vec::new();
            for parameter in &signature.parameters {
                let term = self.recipe(&context, &parameter.ty, b, depth + 1)?;
                Self::push(&mut parameters, term, b)?;
                Self::push(&mut modes, parameter.mode, b)?;
            }
            let result = self.recipe(&context, &signature.result, b, depth + 1)?;
            // Only implementation prerequisites become obligations here. The
            // expected method's original premises are represented by the very
            // same fresh rigid original method formals, not caller actuals.
            if !rigid {
                for formal in &definition.parameters {
                    if let SemanticFormal::Type { requirements } = formal {
                        self.premise_needs(&context, requirements, b)?;
                    }
                }
                self.premise_needs(&context, &definition.requirements, b)?;
            }
            Ok(Signature {
                parameters,
                result,
                modes,
                result_mode: signature.result_mode,
                kind: signature.call_kind,
            })
        })();
        self.role_frames.pop();
        self.trait_state.selves.pop();
        self.state.frames.insert(scope, saved);
        result
    }

    pub(in crate::semantic::original) fn original_provider_conformance(
        &mut self,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.original_associated_declarations(b)?;
        for scope in self.inventory.definitions.values() {
            let SemanticDefinitionBody::Scope(recipe) = &scope.body else {
                continue;
            };
            let SemanticScopeRecipe::TraitImpl {
                declaration,
                trait_application,
                ..
            } = recipe
            else {
                continue;
            };
            let original =
                self.state.frames.get(&scope.id).ok_or_else(|| {
                    fail("original provider conformance lacks its full rigid frame")
                })?;
            b.spend(original.formals.len(), WORK)?;
            let context = trait_frame(scope.id, original.formals.clone());
            let receiver = self.trait_self(&context, SemanticScopeRef::Ancestor(0), b, 0)?;
            let arguments = self.trait_arguments(&context, trait_application, b, 0)?;
            let trait_scope = self.trait_scope(trait_application.declaration, b)?;
            let trait_raw = self
                .inventory
                .original_dyn(trait_application.declaration, b)?;
            let implementation = self.inventory.original_dyn(*declaration, b)?;
            for (ordinal, member) in trait_raw.members.iter().enumerate() {
                b.spend(1, WORK)?;
                let ordinal = u32::try_from(ordinal)
                    .map_err(|_| resource("original member ordinal overflow"))?;
                match &member.kind {
                    DynMemberKind::Type => {
                        b.spend(arguments.len(), WORK)?;
                        self.provider_associated(
                            &context,
                            receiver,
                            trait_application.declaration,
                            arguments.clone(),
                            ordinal,
                            b,
                            0,
                        )?;
                    }
                    DynMemberKind::Constant => {
                        return Err(unknown(
                            "original associated constant conformance needs its exact substituted carrier and RHS proof",
                        ));
                    }
                    DynMemberKind::Method(_) => {
                        let expected = trait_members::member_scope(
                            self.program,
                            trait_application.declaration,
                            ordinal,
                            self.inventory,
                            b,
                        )?;
                        let mut actual = None;
                        for (index, candidate) in implementation.members.iter().enumerate() {
                            b.spend(
                                candidate.name.len().saturating_add(member.name.len() + 1),
                                WORK,
                            )?;
                            if candidate.name == member.name {
                                if candidate.kind != member.kind
                                    || actual.replace(index as u32).is_some()
                                {
                                    return Err(fail(
                                        "original provider changes or repeats its trait method identity",
                                    ));
                                }
                            }
                        }
                        let Some(actual) = actual else {
                            if trait_raw
                                .method_signatures
                                .as_ref()
                                .and_then(|rows| rows.iter().find(|row| row.member == ordinal))
                                .is_some_and(|row| row.default_definition.is_some())
                            {
                                continue;
                            }
                            return Err(fail(
                                "original provider omits a required method without an original default",
                            ));
                        };
                        let actual = trait_members::member_scope(
                            self.program,
                            *declaration,
                            actual,
                            self.inventory,
                            b,
                        )?;
                        let method = self.inventory.definition(expected, b)?;
                        let mut method_actuals = Vec::new();
                        for (ordinal, formal) in method.parameters.iter().enumerate() {
                            let actual =
                                self.rigid_formal(expected, ordinal, formal, &method_actuals, b)?;
                            Self::push(&mut method_actuals, actual, b)?;
                        }
                        let expected = self.conformance_signature(
                            trait_scope,
                            &arguments,
                            expected,
                            receiver,
                            &method_actuals,
                            true,
                            b,
                            0,
                        )?;
                        let actual = self.conformance_signature(
                            scope.id,
                            &context.formals,
                            actual,
                            receiver,
                            &method_actuals,
                            false,
                            b,
                            0,
                        )?;
                        if expected.kind != actual.kind
                            || expected.modes != actual.modes
                            || expected.result_mode != actual.result_mode
                            || expected.parameters.len() != actual.parameters.len()
                        {
                            return Err(fail(
                                "original provider method changes its trait ABI, full modes or arity",
                            ));
                        }
                        for (expected, actual) in expected.parameters.iter().zip(actual.parameters)
                        {
                            self.unify(*expected, actual, 0, b)?;
                        }
                        self.unify(expected.result, actual.result, 0, b)?;
                    }
                }
            }
            for member in &implementation.members {
                b.spend(
                    trait_raw
                        .members
                        .len()
                        .saturating_mul(member.name.len() + 1),
                    WORK,
                )?;
                if !trait_raw
                    .members
                    .iter()
                    .any(|expected| expected.name == member.name && expected.kind == member.kind)
                {
                    return Err(fail(
                        "original trait provider contains an undeclared member identity",
                    ));
                }
            }
        }
        Ok(())
    }

    fn original_associated_declarations(&mut self, b: &mut Budget) -> CompileResult<()> {
        for scope in self.inventory.definitions.values() {
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) =
                &scope.body
            else {
                continue;
            };
            let raw = self.inventory.original_dyn(*declaration, b)?;
            let frame = self.state.frames.get(&scope.id).ok_or_else(|| {
                fail("original associated declaration has no rigid lexical frame")
            })?;
            b.spend(frame.formals.len(), WORK)?;
            let context = trait_frame(scope.id, frame.formals.clone());
            let receiver = self.trait_self(&context, SemanticScopeRef::Ancestor(0), b, 0)?;
            for row in raw
                .associated_definitions
                .as_ref()
                .ok_or_else(|| unknown("original associated declaration inventory is unknown"))?
            {
                match raw
                    .members
                    .get(row.member as usize)
                    .map(|member| &member.kind)
                {
                    Some(DynMemberKind::Type) => {
                        b.spend(context.formals.len(), WORK)?;
                        let projection = self.associated_type(
                            &context,
                            receiver,
                            *declaration,
                            context.formals.clone(),
                            row.member,
                            b,
                            0,
                        )?;
                        let Some(value) = &row.value else {
                            continue;
                        };
                        let DynAssociatedValueRecipe::Type(DynTypePattern::Semantic {
                            scope: original,
                            ty,
                        }) = value
                        else {
                            return Err(unknown(
                                "original associated default needs its complete scoped RHS",
                            ));
                        };
                        if *original != scope.id {
                            return Err(fail(
                                "original associated default changes its lexical declaration scope",
                            ));
                        }
                        let result = self.recipe(&context, ty, b, 0)?;
                        if self.occurs(projection, result, 0, b)? {
                            return Err(fail(
                                "original associated default depends on its own unresolved result",
                            ));
                        }
                        b.spend(context.formals.len(), WORK)?;
                        Self::push(
                            &mut self.trait_state.associated_values,
                            (
                                receiver,
                                *declaration,
                                context.formals.clone(),
                                row.member,
                                result,
                            ),
                            b,
                        )?;
                        let result = self.member_bound_needs(&context, &row.requirements, b, 0);
                        self.trait_state.associated_values.pop();
                        result?;
                    }
                    Some(DynMemberKind::Constant) => {
                        let Some(DynTypePattern::Semantic {
                            scope: original,
                            ty,
                        }) = &row.carrier
                        else {
                            return Err(unknown(
                                "original associated constant needs its full scoped carrier",
                            ));
                        };
                        if *original != scope.id {
                            return Err(fail(
                                "original associated constant carrier changes its declaration scope",
                            ));
                        }
                        self.recipe(&context, ty, b, 0)?;
                        if row.value.is_some() {
                            return Err(unknown(
                                "original associated constant default needs its complete carrier-aware evaluator",
                            ));
                        }
                    }
                    _ => {
                        return Err(fail(
                            "original associated row has no proper type/constant declaration",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
