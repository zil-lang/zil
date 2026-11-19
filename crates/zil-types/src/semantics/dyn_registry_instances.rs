//! Actual method proposals are formed after ordinary type resolution. Original
//! method/parent identities select each emission; matching physical Self never
//! selects a provider or supplies a missing lexical actual.
use super::*;
#[path = "dyn_registry_witnesses.rs"]
mod witnesses;

struct Domain {
    fixed: Vec<SemanticClassExpr>,
    invocation: Vec<SemanticClassExpr>,
}
fn push<T>(rows: &mut Vec<T>, value: T, limits: &Limits, left: &mut usize) -> R<()> {
    if rows.len() >= limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "Dyn actual proposal exceeds node limit",
            None,
        ));
    }
    provider_fee(
        left,
        1 + if rows.len() == rows.capacity() {
            rows.len()
        } else {
            0
        },
    )?;
    rows.try_reserve(1).map_err(|_| {
        Diagnostic::limit(Phase::Types, "Dyn actual proposal allocation failed", None)
    })?;
    rows.push(value);
    Ok(())
}
fn shifted(
    class: &SemanticClassExpr,
    hops: u32,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticClassExpr> {
    crate::dyn_metadata::semantic_operand_snapshot(class, limits, left, 1)?;
    let mut class = class.clone();
    if hops != 0 {
        let scope = match &mut class {
            SemanticClassExpr::Formal { scope, .. }
            | SemanticClassExpr::Implicit { scope, .. }
            | SemanticClassExpr::Environment { scope, .. } => scope,
            SemanticClassExpr::IntrinsicClass {
                operand: SemanticOperand::TypeArgument { scope, .. },
                ..
            }
            | SemanticClassExpr::OuterOccurrence {
                operand: SemanticOperand::TypeArgument { scope, .. },
                ..
            } => scope,
            _ => {
                return Err(unknown(
                    "Dyn ancestor actual needs its original sealed class relocation",
                ));
            }
        };
        let SemanticScopeRef::Ancestor(depth) = scope;
        *depth = depth
            .checked_add(hops)
            .ok_or_else(|| bad("Dyn lexical class depth overflows"))?;
    }
    Ok(class)
}
impl Domain {
    fn position(
        &self,
        image: &SemanticClassExpr,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<StorageBinder>> {
        let mut found = None;
        for (fixed, rows) in [(true, &self.fixed), (false, &self.invocation)] {
            for (index, candidate) in rows.iter().enumerate() {
                crate::dyn_metadata::semantic_operand_snapshot(candidate, limits, left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
                if candidate == image {
                    let index = u32::try_from(index)
                        .map_err(|_| bad("Dyn lexical class ordinal overflows"))?;
                    if found
                        .replace(if fixed {
                            StorageBinder::Fixed(index)
                        } else {
                            StorageBinder::Invocation(index)
                        })
                        .is_some()
                    {
                        return Err(bad("Dyn lexical class has ambiguous coordinates"));
                    }
                }
            }
        }
        Ok(found)
    }
    fn bind(
        &mut self,
        image: SemanticClassExpr,
        invocation: bool,
        limits: &Limits,
        left: &mut usize,
    ) -> R<StorageBinder> {
        if let Some(position) = self.position(&image, limits, left)? {
            return Ok(position);
        }
        let rows = if invocation {
            &mut self.invocation
        } else {
            &mut self.fixed
        };
        let ordinal =
            u32::try_from(rows.len()).map_err(|_| bad("Dyn lexical class ordinal overflows"))?;
        push(rows, image, limits, left)?;
        Ok(if invocation {
            StorageBinder::Invocation(ordinal)
        } else {
            StorageBinder::Fixed(ordinal)
        })
    }
    fn get(
        &self,
        image: &SemanticClassExpr,
        limits: &Limits,
        left: &mut usize,
    ) -> R<StorageBinder> {
        self.position(image, limits, left)?
            .ok_or_else(|| bad("Dyn actual class is outside its complete lexical domain"))
    }
    fn value(
        &self,
        value: &SemanticTypeValue,
        hops: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<DynTypeValue> {
        crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(bad("Dyn actual retains an unresolved whole type"));
        };
        if classes.len() != life.binder_count as usize {
            return Err(bad("Dyn actual loses its full intrinsic domain"));
        }
        let mut images = Vec::new();
        for image in classes {
            push(
                &mut images,
                self.get(&shifted(image, hops, limits, left)?, limits, left)?,
                limits,
                left,
            )?;
        }
        Ok(DynTypeValue {
            ty: ty.clone(),
            life: life.clone(),
            classes: images,
        })
    }
    fn actual(
        &self,
        actual: &SemanticActual,
        hops: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<DynGenericActual> {
        Ok(match actual {
            SemanticActual::Type(value) => {
                let value = self.value(value, hops, limits, left)?;
                if value.life.binder_count == 0
                    && value.life.slots.is_empty()
                    && value.life.retained.is_empty()
                    && value.life.nominal_arguments.is_empty()
                {
                    DynGenericActual::Type(value.ty)
                } else {
                    DynGenericActual::QualifiedType(value)
                }
            }
            SemanticActual::Life(image) => DynGenericActual::Life(self.get(
                &shifted(image, hops, limits, left)?,
                limits,
                left,
            )?),
            SemanticActual::Const { ty, value } => {
                let carrier = self.value(ty, hops, limits, left)?;
                if carrier.life.binder_count != 0
                    || !carrier.classes.is_empty()
                    || carrier.ty != value.ty()
                {
                    return Err(bad("Dyn Const actual changes its complete carrier"));
                }
                crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                DynGenericActual::Const(value.clone())
            }
            _ => return Err(bad("Dyn actual retains an unresolved mixed argument")),
        })
    }
    fn actuals(
        &self,
        row: &SemanticInstance,
        hops: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Vec<DynGenericActual>> {
        let mut result = Vec::new();
        for actual in &row.actuals {
            let actual = self.actual(actual, hops, limits, left)?;
            push(&mut result, actual, limits, left)?;
        }
        Ok(result)
    }
}
fn exact_instance<'a>(
    program: &'a SemanticModule,
    id: SemanticInstanceId,
    left: &mut usize,
) -> R<&'a SemanticInstance> {
    provider_fee(left, program.instances.len())?;
    let mut rows = program.instances.iter().filter(|row| row.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("Dyn lexical instance is absent"))?;
    if rows.next().is_some() {
        return Err(bad("Dyn lexical instance identity is ambiguous"));
    }
    Ok(row)
}
fn exact_definition<'a>(
    program: &'a SemanticModule,
    id: SemanticDefinitionId,
    left: &mut usize,
) -> R<&'a SemanticDefinition> {
    provider_fee(left, program.definitions.len())?;
    let mut rows = program.definitions.iter().filter(|row| row.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("Dyn original definition is absent"))?;
    if rows.next().is_some() {
        return Err(bad("Dyn original definition identity is ambiguous"));
    }
    Ok(row)
}

fn domain_for(
    program: &SemanticModule,
    instance: SemanticInstanceId,
    limits: &Limits,
    left: &mut usize,
) -> R<Domain> {
    let mut domain = Domain {
        fixed: Vec::new(),
        invocation: Vec::new(),
    };
    let mut chain = Vec::new();
    let mut next = Some(instance);
    while let Some(id) = next {
        if chain.len() >= limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Dyn lexical instance depth exceeded",
                None,
            ));
        }
        provider_fee(left, chain.len())?;
        if chain.contains(&id) {
            return Err(bad("Dyn lexical instance chain is cyclic"));
        }
        let row = exact_instance(program, id, left)?;
        push(&mut chain, id, limits, left)?;
        next = row.parent;
    }
    for (hops, id) in chain.iter().enumerate().rev() {
        let row = exact_instance(program, *id, left)?;
        let owner = exact_definition(program, row.definition, left)?;
        if owner.parent
            != row
                .parent
                .map(|id| exact_instance(program, id, left).map(|row| row.definition))
                .transpose()?
            || owner.parameters.len() != row.actuals.len()
        {
            return Err(bad(
                "Dyn lexical actuals change the complete original frame",
            ));
        }
        let hops = u32::try_from(hops).map_err(|_| bad("Dyn lexical depth overflows"))?;
        for (formal, actual) in owner.parameters.iter().zip(&row.actuals) {
            match (formal, actual) {
                (
                    SemanticFormal::Type { .. },
                    SemanticActual::Type(SemanticTypeValue::Resolved { life, classes, .. }),
                ) if classes.len() == life.binder_count as usize => {
                    for image in classes {
                        domain.bind(shifted(image, hops, limits, left)?, false, limits, left)?;
                    }
                }
                (SemanticFormal::Lifetime, SemanticActual::Life(image)) => {
                    domain.bind(shifted(image, hops, limits, left)?, false, limits, left)?;
                }
                (SemanticFormal::Const { .. }, SemanticActual::Const { .. }) => {}
                _ => {
                    return Err(bad(
                        "Dyn lexical actual changes an original mixed formal kind",
                    ));
                }
            }
        }
    }
    Ok(domain)
}

pub(super) fn complete(
    program: &SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut proposals = Vec::new();
    provider_fee(left, module.dyn_declarations.len())?;
    for (di, declaration) in module.dyn_declarations.iter().enumerate() {
        let Some(definitions) = &declaration.method_definitions else {
            continue;
        };
        provider_fee(left, definitions.len())?;
        for (mi, definition) in definitions.iter().enumerate() {
            let original = exact_scope(
                program,
                SemanticDynOwner::MethodDefinition(definition.id),
                left,
            )?;
            identity_work(&definition.identity, left)?;
            identity_work(&original.identity, left)?;
            if original.kind != SemanticDefinitionKind::Method
                || definition.identity != original.identity
            {
                return Err(bad(
                    "Dyn actual proposal changes its original method identity",
                ));
            }
            exact_signature_recipe(&definition.signature, original, left)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                receiver_parameter: Some(receiver_parameter),
                ..
            } = &original.body
            else {
                continue;
            };
            provider_fee(left, program.instances.len())?;
            for instance in program
                .instances
                .iter()
                .filter(|row| row.definition == original.id)
            {
                let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(target)) =
                    instance.target
                else {
                    continue;
                };
                provider_fee(left, module.functions.len())?;
                let mut functions = module
                    .functions
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.id == target);
                let (fi, function) = functions
                    .next()
                    .ok_or_else(|| bad("Dyn original instance has no actual emitted function"))?;
                if functions.next().is_some() {
                    return Err(bad("Dyn actual function identity is ambiguous"));
                }
                let frame = ledger
                    .frames
                    .get(&target)
                    .ok_or_else(|| bad("Dyn actual method lacks its emitter frame"))?;
                if frame.parent.is_some() || frame.origin.closure.is_some() {
                    return Err(bad("Dyn method target is a closure entry"));
                }
                let parent = exact_instance(
                    program,
                    instance
                        .parent
                        .ok_or_else(|| bad("Dyn method lacks its actual lexical parent"))?,
                    left,
                )?;
                if original.parent != Some(parent.definition)
                    || !matches!(parent.target, SemanticInstanceTarget::Scope)
                {
                    return Err(bad("Dyn actual method changes its original lexical parent"));
                }
                let local = function
                    .parameters
                    .get(*receiver_parameter as usize)
                    .and_then(|local| function.locals.get(local.0 as usize))
                    .ok_or_else(|| bad("Dyn receiver has no actual parameter local"))?;
                let Type::Address { inner, .. } = &local.ty else {
                    return Err(bad("Dyn method receiver is not an actual address"));
                };
                // Only the declared address of a Managed Self borrows the
                // manager slot while selecting its payload as the impl subject.
                let parameter = signature.parameters.get(*receiver_parameter as usize)
                    .ok_or_else(|| bad("Dyn receiver ordinal exceeds its original signature"))?;
                let managed_receiver = matches!(&parameter.ty,
                    SemanticTypeRecipe::Address { inner, .. } if matches!(inner.as_ref(),
                        SemanticTypeRecipe::Managed(subject) if matches!(subject.as_ref(),
                            SemanticTypeRecipe::SelfType { scope: SemanticScopeRef::Ancestor(1) })));
                let subject = match (inner.as_ref(), managed_receiver) {
                    (Type::Managed(payload), true) => payload.as_ref(),
                    (_, false) => inner.as_ref(),
                    _ => return Err(bad("resource method address omits its declared manager head")),
                };
                let value = crate::semantics::materialize::type_at(
                    program,
                    module,
                    ledger,
                    instance.id,
                    &SemanticTypeRecipe::SelfType {
                        scope: SemanticScopeRef::Ancestor(1),
                    },
                    Some(subject),
                    limits,
                    left,
                )?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, limits, left, 1)?;
                if instance.self_type.as_ref() != Some(&value)
                    || !matches!(&value, SemanticTypeValue::Resolved { ty, .. } if ty == subject)
                {
                    return Err(bad(
                        "Dyn method Self differs from its independently materialized original scope",
                    ));
                }
                let mut domain = domain_for(program, instance.id, limits, left)?;
                let SemanticTypeValue::Resolved { classes, .. } = &value else {
                    unreachable!()
                };
                for image in classes {
                    domain.bind(shifted(image, 0, limits, left)?, false, limits, left)?;
                }
                let header = function
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| bad("Dyn actual method has no complete header"))?;
                let mut embedding = Vec::new();
                for (fixed, count) in [
                    (true, header.fixed_binder_count),
                    (false, header.invocation_binder_count),
                ] {
                    for ordinal in 0..count {
                        let binder = if fixed {
                            StorageBinder::Fixed(ordinal)
                        } else {
                            StorageBinder::Invocation(ordinal)
                        };
                        provider_fee(left, instance.header_embedding.len())?;
                        let mut rows = instance
                            .header_embedding
                            .iter()
                            .filter(|row| row.header == binder);
                        let row = rows
                            .next()
                            .ok_or_else(|| bad("Dyn header omits an original binder image"))?;
                        if rows.next().is_some() {
                            return Err(bad("Dyn header repeats an original binder image"));
                        }
                        // The definition's implicit directory also contains
                        // body-local borrow/observation occurrences. Its call
                        // domain is instead the actual complete header map;
                        // full lexical actuals (including phantom N) remain
                        // in the Fixed domain above.
                        let image = domain.bind(
                            shifted(&row.actual, 0, limits, left)?,
                            !fixed,
                            limits,
                            left,
                        )?;
                        push(
                            &mut embedding,
                            DynLifeEmbedding {
                                from: binder,
                                to: image,
                            },
                            limits,
                            left,
                        )?;
                    }
                }
                if embedding.len() != instance.header_embedding.len() {
                    return Err(bad("Dyn header adds an out-of-domain binder image"));
                }
                let receiver = domain.value(&value, 0, limits, left)?;
                let declaration_actuals = domain.actuals(parent, 1, limits, left)?;
                let method_actuals = domain.actuals(instance, 0, limits, left)?;
                let actual = DynFunctionInstance {
                    scope: DynInstantiationScope {
                        fixed_binder_count: u32::try_from(domain.fixed.len())
                            .map_err(|_| bad("Dyn Fixed domain overflows"))?,
                        invocation_binder_count: u32::try_from(domain.invocation.len())
                            .map_err(|_| bad("Dyn Invocation domain overflows"))?,
                    },
                    header_embedding: embedding,
                    definition: definition.id,
                    receiver,
                    declaration_actuals,
                    method_actuals,
                };
                crate::dyn_metadata::semantic_operand_snapshot(&actual, limits, left, 1)?;
                let specialization = DynDefinitionSpecialization {
                    function: DynFunctionTarget::Typed(target),
                    receiver: actual.receiver.clone(),
                    declaration_actuals: actual.declaration_actuals.clone(),
                    method_actuals: actual.method_actuals.clone(),
                };
                push(
                    &mut proposals,
                    (di, mi, fi, actual, specialization, frame.origin),
                    limits,
                    left,
                )?;
            }
        }
    }
    let mut seen = BTreeMap::new();
    for (di, mi, fi, actual, specialization, origin) in proposals {
        let function = &mut module.functions[fi];
        provider_fee(left, seen.len().saturating_add(1).ilog2() as usize + 1)?;
        // Several original callers can select one emitted method. Rebuild
        // each complete instantiation above and compare it below even when
        // the callable already has its one physical registry entry.
        if seen.insert(function.id, (di, mi)).is_some_and(|entry| entry != (di, mi)) {
            return Err(bad("Dyn emitted method selects distinct original registry entries"));
        }
        if let Some(old) = &function.dyn_instance {
            crate::dyn_metadata::semantic_operand_snapshot(old, limits, left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, limits, left, 1)?;
            if old != &actual {
                return Err(bad(
                    "Dyn proposed function changes its complete original instantiation",
                ));
            }
        } else {
            function.dyn_instance = Some(actual);
        }
        let definition = &mut module.dyn_declarations[di]
            .method_definitions
            .as_mut()
            .ok_or_else(|| bad("Dyn original method directory disappeared"))?[mi];
        let mut found = false;
        for old in &definition.instances {
            crate::dyn_metadata::semantic_operand_snapshot(old, limits, left, 1)?;
            let target = match old.function {
                DynFunctionTarget::Typed(id) => id == function.id,
                DynFunctionTarget::Core { module, source } => {
                    module == origin.module && source == origin.creator
                }
            };
            if !target {
                continue;
            }
            if found
                || old.receiver != specialization.receiver
                || old.declaration_actuals != specialization.declaration_actuals
                || old.method_actuals != specialization.method_actuals
            {
                return Err(bad(
                    "Dyn proposed specialization changes the complete actual vector",
                ));
            }
            found = true;
        }
        if !found {
            push(&mut definition.instances, specialization, limits, left)?;
        }
    }
    witnesses::complete(program, module, ledger, limits, left)
}
