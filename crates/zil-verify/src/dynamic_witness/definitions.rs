//! Exact original-definition specialization links. Function-local binder
//! domains stay local here; table Context substitutions are a separate proof.
use super::*;

fn kinds(formals: &[LifeParameterKind], actuals: &[DynGenericActual], b: &mut Budget) -> Check<()> {
    b.spend(formals.len().saturating_add(actuals.len()))?;
    if formals.len() != actuals.len() {
        return Err(bad(
            "method specialization omits complete original ordered actuals",
        ));
    }
    for (formal, actual) in formals.iter().zip(actuals) {
        if !matches!(
            (formal, actual),
            (
                LifeParameterKind::Type,
                DynGenericActual::Type(_) | DynGenericActual::QualifiedType(_)
            ) | (LifeParameterKind::Lifetime, DynGenericActual::Life(_))
                | (LifeParameterKind::Const(_), DynGenericActual::Const(_))
        ) {
            return Err(bad(
                "method specialization actual differs from its original formal kind",
            ));
        }
    }
    Ok(())
}

pub(super) fn check(module: &TypedModule, b: &mut Budget) -> Check<()> {
    if module.dyn_declarations.len() > b.limits.max_nodes
        || module.functions.len() > b.limits.max_functions
    {
        return Err(limit("dynamic definition inventory exceeds selected bound"));
    }
    b.spend(
        module
            .dyn_declarations
            .len()
            .saturating_add(module.functions.len()),
    )?;
    let mut definitions = Vec::new();
    for declaration in &module.dyn_declarations {
        let rows = declaration
            .method_definitions
            .as_ref()
            .ok_or_else(|| bad("original method definitions are Unknown"))?;
        b.spend(rows.len())?;
        for row in rows {
            dynamic_metadata::recipe(row, b)?;
            definitions.push((declaration, row));
        }
    }
    for function in &module.functions {
        if let Some(instance) = &function.dyn_instance {
            dynamic_metadata::recipe(instance, b)?;
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut identities = std::collections::BTreeSet::new();
    let mut targets = std::collections::BTreeSet::new();
    for (owner, definition) in &definitions {
        b.spend(2)?;
        if !ids.insert(definition.id) || !identities.insert(&definition.identity) {
            return Err(bad("original method definition identity is duplicated"));
        }
        let Some(DynMemberDeclaration {
            kind: DynMemberKind::Method(method_formals),
            ..
        }) = owner.members.get(definition.member as usize)
        else {
            return Err(bad(
                "method specialization owner has no original method formal scope",
            ));
        };
        for instance in &definition.instances {
            b.spend(1)?;
            kinds(&owner.parameters, &instance.declaration_actuals, b)?;
            kinds(method_formals, &instance.method_actuals, b)?;
            let DynFunctionTarget::Typed(id) = instance.function else {
                return Err(bad(
                    "Typed method specialization retains an unresolved Core target",
                ));
            };
            if !targets.insert(id) {
                return Err(bad(
                    "actual function is claimed by multiple original specializations",
                ));
            }
            b.spend(module.functions.len())?;
            let actual = module
                .functions
                .get(id.0 as usize)
                .ok_or_else(|| bad("original specialization actual function is absent"))?;
            if actual.id != id {
                return Err(bad(
                    "original specialization target has a noncanonical FunctionId",
                ));
            }
            let original = actual.dyn_instance.as_ref().ok_or_else(|| {
                bad("actual specialization has no complete original instance record")
            })?;
            // Both sides are prepaid complete typed values in THIS function's
            // own Fixed/Invocation domains. No Context ordinal is compared here.
            dynamic_metadata::recipe(original, b)?;
            dynamic_metadata::recipe(instance, b)?;
            super::scopes::function(module, actual, original, b)?;
            if owner.kind == DynDeclarationKind::Impl {
                let pattern = owner
                    .self_pattern
                    .as_ref()
                    .ok_or_else(|| bad("original implementation Self recipe is Unknown"))?;
                let mut arguments = matching::Arguments {
                    declaration: &original.declaration_actuals,
                    method: &original.method_actuals,
                    invocation: &original.header_embedding,
                    implicit: vec![
                        None;
                        owner
                            .implicit_lifetimes
                            .as_ref()
                            .ok_or_else(|| bad(
                                "original implicit occurrence directory is Unknown"
                            ))?
                            .len()
                    ],
                };
                matching::ty(
                    module,
                    pattern,
                    &original.receiver,
                    &original.receiver,
                    &mut arguments,
                    b,
                )?;
            }
            if original.definition != definition.id
                || original.receiver != instance.receiver
                || original.declaration_actuals != instance.declaration_actuals
                || original.method_actuals != instance.method_actuals
            {
                return Err(bad(
                    "actual function's full instance differs from its original exact specialization target",
                ));
            }
        }
    }
    for function in &module.functions {
        if let Some(instance) = &function.dyn_instance {
            b.spend(definitions.len().saturating_add(1))?;
            if !ids.contains(&instance.definition) || !targets.contains(&function.id) {
                return Err(bad(
                    "function instance is not listed by its unique original definition",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn signature() -> DynSignatureRecipe {
        DynSignatureRecipe {
            parameters: vec![],
            result: DynTypePattern::Concrete(Type::I32),
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
            invocation_binder_count: 0,
            contract: DynContractRecipe {
                requires: Some(vec![]),
                guarantees: Some(vec![]),
                hold: Some(vec![]),
                call: Some(DynCallAccess {
                    reads: vec![],
                    writes: vec![],
                }),
                result: Some(DynResultSource::Value),
                cleanup: Some(DynCleanupRecipe {
                    environment: false,
                    positions: vec![],
                    access: DynCallAccess {
                        reads: vec![],
                        writes: vec![],
                    },
                    effects: vec![],
                }),
                effects: Some(vec![]),
                resume: Some(DynResumeBound::Local),
                threads: Some(DynThreadRequirements {
                    transfer: false,
                    share: false,
                }),
            },
        }
    }
    fn fixture() -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        let id = DynDefinitionId {
            module: ModuleId(0),
            source: NodeId(100),
        };
        let receiver = DynTypeValue {
            ty: Type::I32,
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![],
        };
        let instance = DynFunctionInstance {
            scope: DynInstantiationScope {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
            },
            header_embedding: vec![],
            definition: id,
            receiver: receiver.clone(),
            declaration_actuals: vec![],
            method_actuals: vec![],
        };
        module.functions[0].dyn_instance = Some(instance);
        module.dyn_declarations = vec![DynDeclaration {
            implicit_lifetimes: Some(vec![]),
            id: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(1),
                compiler: false,
            },
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec![],
                declaration_path: vec!["Read".into()],
                generation_path: vec![],
            },
            kind: DynDeclarationKind::Trait,
            parameters: vec![],
            members: vec![DynMemberDeclaration {
                name: "read".into(),
                kind: DynMemberKind::Method(vec![]),
            }],
            self_pattern: None,
            implemented_trait: None,
            direct_supertraits: Some(vec![]),
            requirements: Some(vec![]),
            associated_definitions: Some(vec![]),
            method_signatures: Some(vec![DynMethodRecipe {
                member: 0,
                requirements: vec![],
                signature: signature(),
                default_definition: Some(id),
            }]),
            method_definitions: Some(vec![DynDefinitionDeclaration {
                id,
                identity: LifeDeclarationIdentity {
                    package: "test".into(),
                    module_path: vec![],
                    declaration_path: vec!["Read".into(), "read".into()],
                    generation_path: vec![],
                },
                member: 0,
                scope: DynDefinitionScope::TraitDefault,
                signature: signature(),
                requirements: vec![],
                instances: vec![DynDefinitionSpecialization {
                    function: DynFunctionTarget::Typed(module.functions[0].id),
                    receiver,
                    declaration_actuals: vec![],
                    method_actuals: vec![],
                }],
            }]),
        }];
        module
    }
    fn run(module: &TypedModule) -> Check<()> {
        let limits = Limits::default();
        let raw = check(module, &mut Budget::new(&limits));
        let (_, plan) = crate::tests::baseline();
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = check(&decoded, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref().map(|_| ()).map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map(|_| ()).map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn original_exact_function_target_cannot_be_replaced_by_a_matching_instance_tag() {
        let module = fixture();
        assert!(run(&module).is_ok());
        let mut raw = module.clone();
        let mut other = raw.functions[0].clone();
        other.id = FunctionId(1);
        other.dyn_instance = None;
        raw.functions.push(other);
        raw.dyn_declarations[0].method_definitions.as_mut().unwrap()[0].instances[0].function =
            DynFunctionTarget::Typed(FunctionId(1));
        assert!(run(&raw).is_err());
        let mut raw = module.clone();
        raw.functions[0]
            .dyn_instance
            .as_mut()
            .unwrap()
            .definition
            .source = NodeId(101);
        assert!(run(&raw).is_err());
        let mut raw = module.clone();
        raw.functions[0]
            .dyn_instance
            .as_mut()
            .unwrap()
            .method_actuals
            .push(DynGenericActual::Const(Constant::I32(1)));
        assert!(run(&raw).is_err());
        raw.dyn_declarations[0].method_definitions.as_mut().unwrap()[0].instances[0]
            .method_actuals
            .push(DynGenericActual::Const(Constant::I32(1)));
        assert!(run(&raw).unwrap_err().message.contains("ordered actuals"));
    }
    #[test]
    fn typed_specialization_requires_one_complete_local_domain_and_no_core_relocation() {
        let module = fixture();
        let mut raw = module.clone();
        let definition = &mut raw.dyn_declarations[0].method_definitions.as_mut().unwrap()[0];
        definition.instances.push(definition.instances[0].clone());
        assert!(run(&raw).is_err());
        let mut raw = module.clone();
        raw.dyn_declarations[0].method_definitions.as_mut().unwrap()[0].instances[0].function =
            DynFunctionTarget::Core {
                module: ModuleId(0),
                source: NodeId(0),
            };
        assert!(run(&raw).is_err());
        let mut raw = module.clone();
        let mut other = raw.functions[0].clone();
        other.id = FunctionId(1);
        raw.functions.push(other);
        assert!(run(&raw).is_err());
    }
    #[test]
    fn complete_original_scope_and_target_work_has_one_fresh_raw_wire_boundary() {
        let module = fixture();
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        super::super::declaration_graph(&module, &mut budget).unwrap();
        let exact = limits.max_analysis_steps - budget.remaining;
        let (_, plan) = crate::tests::baseline();
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for actual in [&module, &decoded] {
            for _ in 0..2 {
                let exact_limits = Limits {
                    max_analysis_steps: exact,
                    ..limits.clone()
                };
                let mut exact_budget = Budget::new(&exact_limits);
                super::super::declaration_graph(actual, &mut exact_budget).unwrap();
                assert_eq!(exact_budget.remaining, 0);
                let low_limits = Limits {
                    max_analysis_steps: exact - 1,
                    ..limits.clone()
                };
                let error = super::super::declaration_graph(actual, &mut Budget::new(&low_limits))
                    .unwrap_err();
                assert_eq!(error.code, "E-RESOURCE-LIMIT");
            }
        }
    }
}
