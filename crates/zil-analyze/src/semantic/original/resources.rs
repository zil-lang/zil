//! An original resource's cleanup entry is selected by its authored builtin
//! implementation, never by a runtime teardown name or a matching layout.
use super::*;

pub(in crate::semantic) fn ordinary_drop_absent(
    definition: SemanticDefinitionId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    for owner in inventory.definitions.values() {
        b.spend(1, WORK)?;
        if matches!(&owner.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{
            self_type:SemanticTypeRecipe::Nominal{definition:target,..},trait_kind:SemanticBuiltinTrait::Drop,
        }) if *target==definition)
        {
            return Err(unknown(
                "ordinary nominal Drop requires its complete value teardown and non-Copy theorem",
            ));
        }
    }
    Ok(())
}

pub(in crate::semantic) fn drop_method(
    resource: SemanticDefinitionId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Option<SemanticDefinitionId>> {
    let declaration = inventory.definition(resource, b)?;
    if !matches!(
        declaration.body,
        SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Resource,
            ..
        }
    ) {
        return Err(fail("original cleanup owner is not a resource declaration"));
    }
    let mut owner = None;
    for definition in inventory.definitions.values() {
        b.spend(1, WORK)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl {
            self_type,
            trait_kind: SemanticBuiltinTrait::Drop,
        }) = &definition.body
        else {
            continue;
        };
        let SemanticTypeRecipe::Nominal {
            definition: target,
            arguments,
        } = self_type
        else {
            return Err(unknown(
                "original Drop Self needs its complete nominal provider pattern",
            ));
        };
        if *target != resource {
            continue;
        }
        // Generic provider matching must prove the original where-context;
        // a physical specialization cannot supply these premises.
        if !arguments.is_empty()
            || !definition.parameters.is_empty()
            || !declaration.parameters.is_empty()
            || !definition.requirements.is_empty()
            || definition.parent.is_some()
            || !definition.environment.is_empty()
        {
            return Err(unknown(
                "original generic resource Drop needs its complete provider substitution and premises",
            ));
        }
        if owner.replace(*definition).is_some() {
            return Err(fail("original resource has multiple Drop implementations"));
        }
    }
    let Some(owner) = owner else {
        return Ok(None);
    };
    let mut entry = None;
    for method in inventory.definitions.values() {
        b.spend(1, WORK)?;
        if method.parent != Some(owner.id) {
            continue;
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            receiver_parameter,
            protocol_contracts,
            foreign,
            body,
            ..
        } = &method.body
        else {
            return Err(fail(
                "original resource Drop implementation contains a nonmethod member",
            ));
        };
        if method.kind != SemanticDefinitionKind::Method
            || method.identity.declaration_path.last().map(String::as_str) != Some("drop")
            || entry.replace(method.id).is_some()
            || !method.parameters.is_empty()
            || !method.environment.is_empty()
            || !method.requirements.is_empty()
            || foreign.is_some()
            || !protocol_contracts.is_empty()
            || !matches!(body, SemanticOriginalFunctionBody::Defined(_))
            || signature.call_kind != CallKind::Direct
            || signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0
            || parameters.len() != 1
            || *receiver_parameter != Some(0)
            || signature.parameters.len() != 1
            || signature.result != SemanticTypeRecipe::Atom(Type::Unit)
            || !matches!(
                signature.result_mode,
                PassingMode::Infer | PassingMode::Copy
            )
            || !matches!(
                signature.parameters[0].mode,
                PassingMode::Infer | PassingMode::Borrow
            )
            || !matches!(&signature.parameters[0].ty,
                SemanticTypeRecipe::Desc { inner, .. }
                if matches!(inner.as_ref(), SemanticTypeRecipe::SelfType { scope: SemanticScopeRef::Ancestor(1) }))
        {
            return Err(fail(
                "original resource Drop changes its receiver, result, execution or cleanup entry contract",
            ));
        }
    }
    entry
        .map(Some)
        .ok_or_else(|| fail("original resource Drop implementation has no complete method body"))
}

impl Solver<'_, '_> {
    pub(super) fn resource_payload(&self, term: Term, b: &mut Budget) -> CompileResult<bool> {
        let Kind::Nominal { definition, .. } = self.state.arena[self.type_root(term, b)?] else {
            return Ok(false);
        };
        Ok(matches!(
            self.inventory.definition(definition, b)?.body,
            SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Resource,
                ..
            }
        ))
    }
}
