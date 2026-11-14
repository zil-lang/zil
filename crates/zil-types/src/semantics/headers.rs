//! Proposals from the actual emitted capture access, not from capture order or
//! equality of concrete types. Publication still requires the whole body proof.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
#[path = "header_captures.rs"]
mod captures;
pub(super) use captures::populate as capture_domains;

fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-HEADER", Span::default(), message)
}
fn pay(left: &mut usize, work: usize) -> R<()> {
    *left = left.checked_sub(work).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "semantic header reconstruction work exceeded",
            None,
        )
    })?;
    Ok(())
}
#[derive(Clone, Copy)]
enum Origin {
    Head(u32),
    Data(u32),
}

pub(super) fn program(
    program: &mut SemanticModule,
    module: &TypedModule,
    left: &mut usize,
) -> R<BTreeSet<SemanticInstanceId>> {
    let mut complete = BTreeSet::new();
    pay(
        left,
        program
            .instances
            .len()
            .saturating_add(program.definitions.len()),
    )?;
    for instance in &mut program.instances {
        let Some(environment) = &instance.environment else {
            continue;
        };
        let SemanticClosureTarget::Typed(closure) = environment.closure else {
            return Err(bad("Core closure survived relocation"));
        };
        pay(
            left,
            module
                .closures
                .len()
                .saturating_add(module.functions.len())
                .saturating_add(program.definitions.len()),
        )?;
        let definition = module
            .closures
            .iter()
            .find(|d| d.id == closure)
            .ok_or_else(|| bad("actual environment definition is absent"))?;
        let function = module
            .functions
            .iter()
            .find(|f| f.id == definition.function)
            .ok_or_else(|| bad("actual environment entry is absent"))?;
        let original = program
            .definitions
            .iter()
            .find(|d| d.id == instance.definition)
            .ok_or_else(|| bad("closure lost its original scope"))?;
        let Some(life) = definition.storage_life.as_ref() else {
            continue;
        };
        if function.storage_life.as_ref() != Some(life) {
            return Err(bad("actual closure and entry class directories disagree"));
        }
        // Other capture families retain their complete input proposals for the
        // general original-domain solver. They are not guessed by this pass.
        if life.invocation_binder_count != 0
            || !life.classes.is_empty()
            || !life.static_bindings.is_empty()
            || function.blocks.len() != 1
        {
            continue;
        }
        let (SemanticDefinitionBody::Function { sites, .. }
        | SemanticDefinitionBody::FunctionWithBody { sites, .. }) = &original.body
        else {
            return Err(bad("closure original has no body"));
        };
        let mut proposal = BTreeMap::new();
        for implicit in &original.implicit_lifetimes {
            pay(left, sites.len().saturating_add(program.sites.len()))?;
            let mut recipes = sites.iter().filter(|s| {
                s.occurrence_path == implicit.occurrence_path
                    && matches!(s.kind, SemanticSiteRecipeKind::TypeUse { .. })
            });
            let Some(recipe) = recipes.next() else {
                continue;
            };
            if recipes.next().is_some() {
                return Err(bad("implicit occurrence selects multiple original reads"));
            }
            let mut actuals = program
                .sites
                .iter()
                .filter(|s| s.instance == instance.id && s.recipe == recipe.id);
            let Some(site) = actuals.next() else {
                return Err(bad("implicit occurrence has no actual emitted read"));
            };
            if actuals.next().is_some() {
                return Err(bad("implicit occurrence has multiple actual reads"));
            }
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge: SemanticSiteEdge::Before,
            } = site.at
            else {
                return Err(bad("implicit read has no actual end boundary"));
            };
            let SemanticSiteAction::TypeUse {
                subject: SemanticOperand::Value(subject),
                ..
            } = site.action
            else {
                return Err(bad("implicit read has no actual result"));
            };
            let body = &function.blocks[0];
            if block != body.id || instruction as usize > body.instructions.len() {
                return Err(bad("implicit read leaves its actual block"));
            }
            pay(
                left,
                function
                    .locals
                    .len()
                    .saturating_add(function.values.len())
                    .saturating_add(instruction as usize),
            )?;
            let mut locals = vec![None; function.locals.len()];
            let mut values = vec![None; function.values.len()];
            for op in body.instructions.iter().take(instruction as usize) {
                let origin = match op.kind {
                    InstructionKind::ReadCapture { index } => Some(Origin::Head(index)),
                    InstructionKind::Load(local) => *locals
                        .get(local.0 as usize)
                        .ok_or_else(|| bad("capture read has invalid hidden local"))?,
                    InstructionKind::ReadAddress { address } => {
                        match values.get(address.0 as usize).and_then(|r| *r) {
                            Some(Origin::Head(index)) => Some(Origin::Data(index)),
                            _ => None,
                        }
                    }
                    InstructionKind::Store { local, value } => {
                        let origin = *values
                            .get(value.0 as usize)
                            .ok_or_else(|| bad("hidden capture store has invalid value"))?;
                        *locals
                            .get_mut(local.0 as usize)
                            .ok_or_else(|| bad("hidden capture store has invalid local"))? = origin;
                        None
                    }
                    _ => None,
                };
                if let Some(value) = op.result {
                    *values
                        .get_mut(value.0 as usize)
                        .ok_or_else(|| bad("capture read has invalid result"))? = origin;
                }
            }
            let Some(Origin::Data(capture)) = values.get(subject.0 as usize).and_then(|r| *r)
            else {
                continue;
            };
            let Some(Type::Address {
                mutable: false,
                inner,
            }) = definition.captures.get(capture as usize)
            else {
                continue;
            };
            if !inner.is_b0_scalar() {
                continue;
            }
            pay(left, life.bindings.len())?;
            let mut bindings = life.bindings.iter().filter(|b| {
                b.position.base == StorageBase::Capture(capture) && b.position.path.is_empty()
            });
            let Some(binding) = bindings.next() else {
                return Err(bad("actual capture head has no complete fixed binding"));
            };
            if bindings.next().is_some() {
                return Err(bad("actual capture head has multiple fixed bindings"));
            }
            let StorageBinder::Fixed(fixed) = binding.binder else {
                return Err(bad("persistent captured head is not Fixed"));
            };
            let image = SemanticClassExpr::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: implicit.ordinal,
            };
            if let Some(old) = proposal.insert(fixed, image.clone()) {
                if old != image {
                    return Err(bad(
                        "one Fixed domain is assigned different original implicit inputs",
                    ));
                }
            }
        }
        if proposal.len() != life.fixed_binder_count as usize {
            continue;
        }
        pay(
            left,
            definition
                .captures
                .len()
                .saturating_add(life.bindings.len()),
        )?;
        let mut captures = 0;
        let mut finite = true;
        for (index, ty) in definition.captures.iter().enumerate() {
            match ty {
                Type::Address {
                    mutable: false,
                    inner,
                } if inner.is_b0_scalar() => {
                    captures += 1;
                    pay(left, life.bindings.len())?;
                    let count = life.bindings.iter().filter(|b| b.position.base == StorageBase::Capture(index as u32) && b.position.path.is_empty() && matches!(b.binder, StorageBinder::Fixed(k) if proposal.contains_key(&k))).count();
                    if count != 1 {
                        finite = false;
                    }
                }
                ty if ty.is_b0_scalar() => {}
                _ => finite = false,
            }
        }
        if !finite || captures != life.bindings.len() {
            continue;
        }
        pay(
            left,
            proposal
                .len()
                .saturating_add(instance.header_embedding.len()),
        )?;
        let expected: Vec<_> = proposal
            .into_iter()
            .map(|(fixed, actual)| SemanticHeaderBinding {
                header: StorageBinder::Fixed(fixed),
                actual,
            })
            .collect();
        if instance.header_embedding.is_empty() {
            instance.header_embedding = expected;
        } else if instance.header_embedding != expected {
            return Err(bad(
                "claimed class embedding changes its original actual capture occurrence",
            ));
        }
        complete.insert(instance.id);
    }
    Ok(complete)
}
