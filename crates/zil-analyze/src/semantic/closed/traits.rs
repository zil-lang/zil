//! Actual lexical trait applications. These queries re-evaluate original
//! provider recipes; an actual method ABI or a selected vtable is not a RHS.
use super::*;
use crate::semantic::value::{self, Argument, Value};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Query {
    receiver: Value,
    declaration: LifeDeclarationId,
    arguments: Vec<Argument>,
    member: Option<u32>,
}
fn charge_value(value: &Value, b: &mut Budget) -> CompileResult<()> {
    crate::dyn_metadata::ty(&value.ty, b)?;
    crate::dyn_metadata::shape(&value.life, b)?;
    b.spend(value.classes.len(), WORK)
}
fn charge_arguments(arguments: &[Argument], b: &mut Budget) -> CompileResult<()> {
    for argument in arguments {
        match argument {
            Argument::Type(value) => charge_value(value, b)?,
            Argument::Const { carrier, .. } => crate::dyn_metadata::ty(carrier, b)?,
            Argument::Life(_) => b.spend(1, WORK)?,
        }
    }
    Ok(())
}
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn trait_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let Some(boundary) =
            self.original
                .trait_boundary(instance, site, self.program, self.inventory, b)?
        else {
            return Ok(None);
        };
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|item| item.id == block)
            .and_then(|item| item.instructions.get(index))
            .ok_or_else(|| fail("original trait invocation has no actual operation"))?;
        let (target, arguments) = match &instruction.kind {
            InstructionKind::Call {
                function,
                arguments,
            } if boundary.call_kind == CallKind::Direct => (*function, arguments),
            InstructionKind::SuspendCall {
                function: callee,
                arguments,
            } if boundary.call_kind == CallKind::Suspend
                && function.call_kind == CallKind::Suspend =>
            {
                (*callee, arguments)
            }
            _ => {
                return Err(fail(
                    "actual trait invocation changes its independently declared call ABI or opcode",
                ));
            }
        };
        if actuals.len() != boundary.method_actuals.len()
            || arguments.len() != boundary.parameters.len()
        {
            return Err(fail(
                "actual trait call changes its complete mixed or value arity",
            ));
        }
        for (actual, expected) in actuals.iter().zip(&boundary.method_actuals) {
            if self.actual_value(instance, actual, b, 0)? != *expected {
                return Err(fail(
                    "actual trait method changes its original mixed specialization",
                ));
            }
        }
        let trait_raw = self.inventory.original_dyn(boundary.declaration, b)?;
        let trait_member = trait_raw
            .members
            .get(boundary.member as usize)
            .ok_or_else(|| fail("actual trait call selects another original member"))?;
        let signature_scope = original::member_scope(
            self.program,
            boundary.declaration,
            boundary.member,
            self.inventory,
            b,
        )?;
        if signature_scope != boundary.method {
            return Err(fail(
                "actual trait invocation changes its original signature declaration",
            ));
        }
        let provider = self.actual_provider(
            &boundary.receiver,
            boundary.declaration,
            &boundary.arguments,
            b,
            0,
        )?;
        let provider_definition = self.inventory.definition(provider.definition, b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration, .. }) =
            &provider_definition.body
        else {
            return Err(fail("actual trait invocation has another provider kind"));
        };
        let implementation = self.inventory.original_dyn(*declaration, b)?;
        let mut definition = None;
        for (ordinal, row) in implementation.members.iter().enumerate() {
            b.spend(
                row.name.len().saturating_add(trait_member.name.len() + 1),
                WORK,
            )?;
            if row.name != trait_member.name {
                continue;
            }
            if row.kind != trait_member.kind {
                return Err(fail(
                    "actual trait override changes its original method generic kinds",
                ));
            }
            for body in implementation
                .method_definitions
                .as_ref()
                .ok_or_else(|| unknown("actual provider method definition inventory is unknown"))?
            {
                if body.member == ordinal as u32 && definition.replace(body.id).is_some() {
                    return Err(fail(
                        "actual provider repeats its original method definition",
                    ));
                }
            }
        }
        let definition = if let Some(definition) = definition {
            definition
        } else {
            trait_raw
                .method_signatures
                .as_ref()
                .and_then(|rows| rows.iter().find(|row| row.member == boundary.member))
                .and_then(|row| row.default_definition)
                .ok_or_else(|| {
                    fail("actual provider omits its original method implementation or default")
                })?
        };
        let expected = original::definition_scope(self.program, definition, self.inventory, b)?;
        let target_instance = self.function_instance(target, b)?;
        if target_instance.definition != expected {
            return Err(fail(
                "actual trait call selects another member/default definition even though its ABI may match",
            ));
        }
        let callee = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(target), b)?;
        if callee.call_kind != boundary.call_kind
            || callee.foreign.is_some()
            || callee.environment.is_some()
            || callee.parameters.len() != boundary.parameters.len()
        {
            return Err(fail(
                "actual trait method changes its original concrete entry, ABI or environment role",
            ));
        }
        let head = *arguments
            .first()
            .ok_or_else(|| fail("actual trait call omits its receiver"))?;
        let anchored = self.anchored_local(instance, function, boundary.receiver_anchor, b)?;
        if self.receiver_local(function, head, b, 0)? != anchored {
            return Err(fail(
                "actual trait invocation takes another original receiver head",
            ));
        }
        for ((argument, expected), parameter) in arguments
            .iter()
            .zip(&boundary.parameters)
            .zip(&callee.parameters)
        {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *argument,
                b,
            )?;
            let actual = function
                .values
                .get(argument.0 as usize)
                .ok_or_else(|| fail("actual trait argument has no complete SSA type"))?;
            let parameter = callee
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| fail("actual method parameter local is absent"))?;
            if actual != &expected.ty || parameter.ty != expected.ty {
                return Err(fail(
                    "actual trait invocation changes a complete original argument type",
                ));
            }
            if self.uses_full_value(actual, b)? {
                let actual = self.ssa_value(instance, function, *argument, b, 0)?;
                let valid = if matches!(expected.ty, Type::Address { .. }) {
                    self.outer_role(instance, function, &actual, expected, b, 0)?
                } else {
                    actual == *expected
                };
                if !valid {
                    return Err(fail(
                        "actual trait argument changes its full intrinsic classes or outer call role",
                    ));
                }
            }
        }
        if callee.return_type != boundary.result.ty {
            return Err(fail(
                "actual trait method result differs from its independently instantiated original member",
            ));
        }
        if !boundary.result.classes.is_empty() {
            return Err(unknown(
                "trait result needs its complete concrete entry result-role substitution",
            ));
        }
        if let Some(result) = instruction.result {
            if function.values.get(result.0 as usize) != Some(&boundary.result.ty) {
                return Err(fail(
                    "actual trait call result SSA has another complete type",
                ));
            }
        } else if boundary.result.ty != Type::Unit {
            return Err(fail("actual trait call omits its promised result value"));
        }
        Ok(Some(boundary.result))
    }
    pub(super) fn trait_result(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let mut result = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: at, instruction, edge: SemanticSiteEdge::After } if at == block && instruction as usize == index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            if let Some(value) =
                self.trait_operation(instance, site.recipe, function, block, index, actuals, b)?
            {
                if result.replace(value).is_some() {
                    return Err(fail(
                        "actual trait result has several original member roles",
                    ));
                }
            }
        }
        Ok(result)
    }
    fn enter_trait_query(
        &self,
        receiver: &Value,
        declaration: LifeDeclarationId,
        arguments: &[Argument],
        member: Option<u32>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual associated dependency exceeds its complete query depth",
            ));
        }
        charge_value(receiver, b)?;
        charge_arguments(arguments, b)?;
        let query = Query {
            receiver: receiver.clone(),
            declaration,
            arguments: arguments.to_vec(),
            member,
        };
        for old in self.trait_queries.borrow().iter() {
            charge_value(&old.receiver, b)?;
            charge_arguments(&old.arguments, b)?;
            if old == &query {
                return Err(fail(
                    "actual original associated/trait query graph is cyclic",
                ));
            }
        }
        self.trait_queries.borrow_mut().push(query);
        Ok(())
    }

    pub(super) fn trait_actuals(
        &self,
        instance: &SemanticInstance,
        application: &SemanticTraitRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Argument>> {
        let scope =
            original::declaration_scope(self.program, application.declaration, self.inventory, b)?;
        let declaration = self.inventory.definition(scope, b)?;
        if !matches!(
            declaration.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
        ) || application.arguments.len() != declaration.parameters.len()
        {
            return Err(fail(
                "actual trait application changes its original kind or full mixed arity",
            ));
        }
        let mut actuals = Vec::new();
        for (formal, recipe) in declaration.parameters.iter().zip(&application.arguments) {
            self.push_value(
                &mut actuals,
                self.argument_recipe(instance, formal, recipe, b, depth + 1)?,
                b,
            )?;
        }
        Ok(actuals)
    }

    pub(super) fn original_scope_self(
        &self,
        instance: &SemanticInstance,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original Self substitution exceeds its bounded lexical depth",
            ));
        }
        let declaration = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::Scope(recipe) = &declaration.body else {
            return Err(fail(
                "actual Self is not attached to its exact original scope",
            ));
        };
        let actual = instance
            .self_type
            .as_ref()
            .ok_or_else(|| fail("actual trait/impl scope omits its complete receiver"))?;
        let actual = self.resolved(instance, actual, b, depth + 1)?;
        match recipe {
            SemanticScopeRecipe::TraitImpl { self_type, .. }
            | SemanticScopeRecipe::InherentImpl { self_type, .. }
            | SemanticScopeRecipe::BuiltinImpl { self_type, .. } => {
                let expected = self.recipe_value(instance, self_type, b, depth + 1)?;
                if expected != actual {
                    return Err(fail(
                        "actual impl Self differs from its complete original self-pattern substitution",
                    ));
                }
            }
            SemanticScopeRecipe::Trait { declaration } => {
                let mut arguments = Vec::new();
                for actual in &instance.actuals {
                    self.push_value(
                        &mut arguments,
                        self.actual_value(instance, actual, b, depth + 1)?,
                        b,
                    )?;
                }
                self.actual_provider(&actual, *declaration, &arguments, b, depth + 1)?;
            }
        }
        Ok(actual)
    }

    pub(super) fn actual_provider(
        &self,
        receiver: &Value,
        declaration: LifeDeclarationId,
        arguments: &[Argument],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<&SemanticInstance> {
        self.enter_trait_query(receiver, declaration, arguments, None, b, depth)?;
        let result = (|| {
            let mut selected: Option<&SemanticInstance> = None;
            for candidate in &self.program.instances {
                b.spend(1, WORK)?;
                if !matches!(candidate.target, SemanticInstanceTarget::Scope) {
                    continue;
                }
                let original = self.inventory.definition(candidate.definition, b)?;
                let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                    self_type,
                    trait_application,
                    ..
                }) = &original.body
                else {
                    continue;
                };
                if trait_application.declaration != declaration {
                    continue;
                }
                // Start at the original Self recipe. The supplied Self claim
                // is checked separately and never selects this provider.
                let self_value = self.recipe_value(candidate, self_type, b, depth + 1)?;
                charge_value(&self_value, b)?;
                charge_value(receiver, b)?;
                if self_value != *receiver {
                    continue;
                }
                let actuals = self.trait_actuals(candidate, trait_application, b, depth + 1)?;
                charge_arguments(&actuals, b)?;
                charge_arguments(arguments, b)?;
                if actuals != arguments {
                    continue;
                }
                if let Some(old) = selected {
                    crate::dyn_metadata::recipe(old, b)?;
                    crate::dyn_metadata::recipe(candidate, b)?;
                    if old.definition != candidate.definition || old.actuals != candidate.actuals {
                        return Err(fail(
                            "actual receiver trait application has several original providers",
                        ));
                    }
                } else {
                    selected = Some(candidate);
                }
            }
            let selected = selected.ok_or_else(|| unknown("actual trait application has no complete original provider scope and full class substitution"))?;
            self.original_scope_self(selected, b, depth + 1)?;
            self.scope_arguments(selected, b)?;
            Ok(selected)
        })();
        self.trait_queries.borrow_mut().pop();
        result
    }

    pub(super) fn scope_arguments(
        &self,
        instance: &SemanticInstance,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let declaration = self.inventory.definition(instance.definition, b)?;
        if !matches!(instance.target, SemanticInstanceTarget::Scope)
            || instance.parent.is_some()
            || instance.environment.is_some()
            || !instance.header_embedding.is_empty()
            || !matches!(
                instance.lifetime_instantiation,
                SemanticLifetimeInstantiation::Rigid
            )
            || instance.actuals.len() != declaration.parameters.len()
        {
            return Err(unknown(
                "actual original scope needs its complete rigid mixed and lexical context",
            ));
        }
        for (formal, actual) in declaration.parameters.iter().zip(&instance.actuals) {
            match (formal, actual) {
                (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) => {
                    let value = self.resolved(instance, value, b, 0)?;
                    self.actual_type(&value.ty, b)?;
                    self.requirements(instance, requirements, b)?;
                }
                (SemanticFormal::Lifetime, SemanticActual::Life(class)) => {
                    value::class(instance, class, self.inventory, b, 0)?;
                }
                (SemanticFormal::Const { carrier }, SemanticActual::Const { ty, value }) => {
                    let expected = self.recipe_value(instance, carrier, b, 0)?;
                    let actual = self.resolved(instance, ty, b, 0)?;
                    if actual != expected || !actual.classes.is_empty() || actual.ty != value.ty() {
                        return Err(fail(
                            "actual scope Const changes its original full carrier or bits type",
                        ));
                    }
                }
                _ => return Err(fail("actual scope changes its complete mixed formal kind")),
            }
        }
        self.requirements(instance, &declaration.requirements, b)
    }

    pub(super) fn associated_actual(
        &self,
        receiver: &Value,
        declaration: LifeDeclarationId,
        arguments: &[Argument],
        member: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        self.enter_trait_query(receiver, declaration, arguments, Some(member), b, depth)?;
        let result = (|| {
            let trait_raw = self.inventory.original_dyn(declaration, b)?;
            let trait_member = trait_raw
                .members
                .get(member as usize)
                .ok_or_else(|| fail("actual associated query selects an absent original member"))?;
            if !matches!(trait_member.kind, DynMemberKind::Type) {
                return Err(fail(
                    "actual associated type query changes its original member kind",
                ));
            }
            let provider = self.actual_provider(receiver, declaration, arguments, b, depth + 1)?;
            let original = self.inventory.definition(provider.definition, b)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                declaration: implementation,
                ..
            }) = &original.body
            else {
                return Err(fail("actual associated provider has no original impl"));
            };
            let raw = self.inventory.original_dyn(*implementation, b)?;
            let mut row = None;
            for (ordinal, candidate) in raw.members.iter().enumerate() {
                b.spend(
                    candidate
                        .name
                        .len()
                        .saturating_add(trait_member.name.len() + 1),
                    WORK,
                )?;
                if candidate.name == trait_member.name {
                    if candidate.kind != trait_member.kind {
                        return Err(fail(
                            "actual associated override changes its original member kind",
                        ));
                    }
                    row = raw
                        .associated_definitions
                        .as_ref()
                        .and_then(|rows| rows.iter().find(|row| row.member == ordinal as u32));
                }
            }
            let (context, row) = if let Some(row) = row {
                (provider, row)
            } else {
                return Err(unknown(
                    "actual trait default needs its exact receiver-bound trait scope instance",
                ));
            };
            let value: &DynAssociatedValueRecipe = row
                .value
                .as_ref()
                .ok_or_else(|| unknown("actual associated definition omits its original RHS"))?;
            let DynAssociatedValueRecipe::Type(DynTypePattern::Semantic { scope, ty }) = value
            else {
                return Err(unknown(
                    "actual associated RHS needs its complete original scoped type definition",
                ));
            };
            if *scope != context.definition {
                return Err(fail(
                    "actual associated RHS changes its original provider scope",
                ));
            }
            self.recipe_value(context, ty.as_ref(), b, depth + 1)
        })();
        self.trait_queries.borrow_mut().pop();
        result
    }
}

pub(in crate::semantic) fn original_associated(
    receiver: &Value,
    declaration: LifeDeclarationId,
    arguments: &[Argument],
    member: u32,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    original: &original::Checked,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    let checker = Check {
        module: inventory.module,
        program,
        inventory,
        original,
        scope_queries: std::cell::RefCell::new(BTreeSet::new()),
        trait_queries: std::cell::RefCell::new(Vec::new()),
    };
    checker.associated_actual(receiver, declaration, arguments, member, b, depth)
}

pub(in crate::semantic) fn original_self(
    owner: SemanticDefinitionId,
    context: &SemanticInstance,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    original: &original::Checked,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    let mut context = context;
    for _ in 0..=b.partial_depth {
        b.spend(1, WORK)?;
        if context.definition == owner {
            let checker = Check {
                module: inventory.module,
                program,
                inventory,
                original,
                scope_queries: std::cell::RefCell::new(BTreeSet::new()),
                trait_queries: std::cell::RefCell::new(Vec::new()),
            };
            return checker.original_scope_self(context, b, depth + 1);
        }
        context = inventory.instance(
            context
                .parent
                .ok_or_else(|| fail("actual Self leaves its true original lexical scope"))?,
            b,
        )?;
    }
    Err(unknown(
        "actual Self lexical substitution exceeds its bounded depth",
    ))
}
