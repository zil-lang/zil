//! A sealed class proxy is defined by its original parent-domain source. This
//! transports class identity only; concrete Type actuals add no Gamma premise,
//! and a parent's unscoped SSA/local occurrence cannot become a child source.
use std::collections::BTreeSet;
use super::*;

fn rebase(world: &mut World<'_, '_>, image: &SemanticClassExpr, hops: u32) -> R<SemanticClassExpr> {
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    let mut image = image.clone();
    let scope = match &mut image {
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
            return Err(pending(
                "sealed parent class needs its complete scoped original occurrence transport",
            ));
        }
    };
    let SemanticScopeRef::Ancestor(depth) = scope;
    *depth = depth
        .checked_add(hops)
        .ok_or_else(|| bad("sealed class lexical depth overflows"))?;
    Ok(image)
}
fn source(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    ordinal: u32,
) -> R<(SemanticInstanceId, SemanticActual)> {
    let instance = world.instance(scope)?;
    let original = world.definition(instance.definition)?;
    let environment = instance
        .environment
        .as_ref()
        .ok_or_else(|| bad("sealed class has no actual environment"))?;
    world.pay(
        original
            .environment
            .len()
            .saturating_add(environment.actuals.len()),
    )?;
    if original.environment.len() != environment.actuals.len() {
        return Err(bad(
            "sealed class environment changes the complete mixed slot inventory",
        ));
    }
    let formal = original
        .environment
        .get(ordinal as usize)
        .ok_or_else(|| bad("sealed class selects an absent original slot"))?;
    let actual = environment
        .actuals
        .get(ordinal as usize)
        .ok_or_else(|| bad("sealed class selects an absent actual slot"))?;
    let parent = world.instance(
        instance
            .parent
            .ok_or_else(|| bad("sealed class has no real creator instance"))?,
    )?;
    if original.parent != Some(parent.definition) {
        return Err(bad("sealed class changes its true original lexical parent"));
    }
    let SemanticClosureTarget::Typed(closure) = environment.closure else {
        return Err(bad("sealed class retains a Core closure target"));
    };
    let (_, child) = world.child(closure)?;
    if child.id != scope {
        return Err(bad("sealed class names another actual child environment"));
    }
    let expected = super::super::super::materialize::actual_in_view(
        world.program,
        &world.view,
        world.ledger,
        parent.id,
        &formal.source,
        world.limits,
        world.left,
    )?;
    crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(&expected, world.limits, world.left, 1)?;
    if actual != &expected {
        return Err(bad(
            "sealed class changes its complete original parent-domain source",
        ));
    }
    if !matches!(
        (&formal.kind, &expected),
        (SemanticFormal::Type { .. }, SemanticActual::Type(_))
            | (SemanticFormal::Lifetime, SemanticActual::Life(_))
    ) {
        return Err(bad("sealed class changes its original Type/Life slot kind"));
    }
    Ok((parent.id, expected))
}
struct Normalize {
    active: BTreeSet<(SemanticInstanceId, u32)>,
}
impl Normalize {
    fn image(
        &mut self,
        world: &mut World<'_, '_>,
        scope: SemanticInstanceId,
        image: &SemanticClassExpr,
        depth: usize,
    ) -> R<SemanticClassExpr> {
        world.pay(1)?;
        if depth > world.limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "sealed class source depth exceeded",
                None,
            ));
        }
        match image {
            SemanticClassExpr::OuterOccurrence {
                operand: SemanticOperand::Environment(ordinal),
                path,
                mode,
            } => {
                let proxy = super::environment_outer(world, scope, *ordinal, path, *mode)?;
                self.image(world, scope, &proxy, depth + 1)
            }
            SemanticClassExpr::IntrinsicClass {
                operand: SemanticOperand::Environment(ordinal),
                path,
                class,
            } => self.slot(world, scope, *ordinal, path, Some(*class), 1, depth + 1),
            SemanticClassExpr::Environment {
                scope: SemanticScopeRef::Ancestor(hops),
                ordinal,
            } => {
                if *hops as usize > world.limits.max_call_depth {
                    return Err(bad(
                        "sealed Life environment ancestry exceeds its original domain",
                    ));
                }
                let mut owner = scope;
                for _ in 0..*hops {
                    let row = world.instance(owner)?;
                    let original = world.definition(row.definition)?;
                    let parent = row.parent.ok_or_else(|| {
                        bad("sealed Life environment leaves the actual lexical chain")
                    })?;
                    let parent = world.instance(parent)?;
                    if original.parent != Some(parent.definition) {
                        return Err(bad("sealed Life environment changes an original ancestor"));
                    }
                    owner = parent.id;
                }
                let count = hops
                    .checked_add(1)
                    .ok_or_else(|| bad("sealed Life environment ancestry overflows"))?;
                self.slot(world, owner, *ordinal, &[], None, count, depth + 1)
            }
            SemanticClassExpr::Formal {
                scope: selector,
                ordinal,
            } => super::super::super::materialize::class_in_view(
                world.program,
                &world.view,
                world.ledger,
                scope,
                &SemanticLifeRecipe::Formal(SemanticFormalRef {
                    scope: *selector,
                    ordinal: *ordinal,
                }),
                world.limits,
                world.left,
            ),
            _ => {
                crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
                Ok(image.clone())
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn slot(
        &mut self,
        world: &mut World<'_, '_>,
        scope: SemanticInstanceId,
        ordinal: u32,
        path: &[LifeTypeProjection],
        class: Option<u32>,
        hops: u32,
        depth: usize,
    ) -> R<SemanticClassExpr> {
        world.pay(index::search(self.active.len()).saturating_add(1))?;
        if !self.active.insert((scope, ordinal)) {
            return Err(bad("sealed class sources are cyclic"));
        }
        let result = (|| {
            let (parent, actual) = source(world, scope, ordinal)?;
            let image = match (class, actual) {
                (None, SemanticActual::Life(image)) => image,
                (Some(class), SemanticActual::Type(value)) => {
                    world.pay(path.len().saturating_add(1))?;
                    if path.len() > world.limits.max_call_depth {
                        return Err(bad("sealed Type class projection is excessive"));
                    }
                    let mut value = Rc::new(value);
                    for part in path {
                        value = match part {
                            LifeTypeProjection::Place(part) => {
                                projection::place(world, &value, *part)?
                            }
                            LifeTypeProjection::Pointee => projection::pointee(world, &value)?,
                            _ => {
                                return Err(pending(
                                    "sealed Type class needs its complete original selected projection",
                                ));
                            }
                        };
                    }
                    let SemanticTypeValue::Resolved { life, classes, .. } = value.as_ref() else {
                        return Err(bad("sealed Type class source remains unresolved"));
                    };
                    if classes.len() != life.binder_count as usize {
                        return Err(bad("sealed Type class source truncates its full domain"));
                    }
                    let image = classes.get(class as usize).ok_or_else(|| {
                        bad("sealed Type class is outside its full original domain")
                    })?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        image,
                        world.limits,
                        world.left,
                        1,
                    )?;
                    image.clone()
                }
                _ => {
                    return Err(bad(
                        "sealed class selector changes its original mixed slot kind",
                    ));
                }
            };
            let image = self.image(world, parent, &image, depth + 1)?;
            rebase(world, &image, hops)
        })();
        self.active.remove(&(scope, ordinal));
        result
    }
}
pub(super) fn normalize(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    image: &SemanticClassExpr,
) -> R<SemanticClassExpr> {
    Normalize {
        active: BTreeSet::new(),
    }
    .image(world, scope, image, 0)
}
