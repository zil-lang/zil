//! Private original selectors accompanying the existing whole-type evaluator.
//! A joint domain compares these identities before looking at caller images.
use super::*;
use std::rc::Rc;
#[cfg(test)]
#[path = "materialize_domain_tests.rs"]
mod tests;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Selector {
    Outer {
        owner: SemanticInstanceId,
        life: SemanticLifeRecipe,
    },
    Intrinsic {
        owner: SemanticInstanceId,
        ty: Rc<SemanticTypeRecipe>,
        boundary: Boundary,
        occurrence: Option<Rc<Occurrence>>,
        class: u32,
    },
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum OccurrenceRoot {
    Recipe,
    Original(SemanticScopeRef, SemanticOriginalTypeSource),
    TaskParameter(u32),
    TaskResult,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Occurrence {
    pub root: OccurrenceRoot,
    pub path: Vec<u32>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Boundary {
    Whole,
    Pointee,
    Element,
}
pub(super) struct Domain {
    pub value: SemanticTypeValue,
    pub selectors: Vec<Selector>,
}
impl Eval<'_, '_, '_> {
    fn selector_snapshot(&mut self, selector: &Selector) -> R<()> {
        match selector {
            Selector::Outer { life, .. } => {
                crate::dyn_metadata::semantic_operand_snapshot(life, self.limits, self.left, 1)
            }
            Selector::Intrinsic { ty, occurrence, .. } => {
                if let Some(occurrence) = occurrence {
                    self.occurrence_snapshot(occurrence)?;
                }
                crate::dyn_metadata::semantic_operand_snapshot(
                    ty.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )
            }
        }
    }
    fn occurrence_snapshot(&mut self, occurrence: &Occurrence) -> R<()> {
        self.fee(occurrence.path.len().saturating_add(1))?;
        if occurrence.path.len() > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original type boundary occurrence depth exceeded",
                None,
            ));
        }
        if let OccurrenceRoot::Original(_, source) = &occurrence.root {
            crate::dyn_metadata::semantic_operand_snapshot(source, self.limits, self.left, 1)?;
        }
        Ok(())
    }
    pub(super) fn outer_selector(
        &mut self,
        scope: SemanticInstanceId,
        life: &SemanticLifeRecipe,
    ) -> R<Selector> {
        crate::dyn_metadata::semantic_operand_snapshot(life, self.limits, self.left, 1)?;
        let (owner, life) = match life {
            SemanticLifeRecipe::Formal(reference) => {
                let owner = self.ancestor(scope, reference.scope, 0)?;
                let SemanticClassExpr::Formal { ordinal, .. } =
                    self.canonical_formal(scope, reference)?
                else {
                    unreachable!()
                };
                (
                    owner,
                    SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    }),
                )
            }
            SemanticLifeRecipe::Implicit {
                scope: selector,
                ordinal,
            } => (
                self.ancestor(scope, *selector, 0)?,
                SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: *ordinal,
                },
            ),
            SemanticLifeRecipe::Environment {
                scope: selector,
                ordinal,
            } => (
                self.ancestor(scope, *selector, 0)?,
                SemanticLifeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: *ordinal,
                },
            ),
            _ => (scope, life.clone()),
        };
        Ok(Selector::Outer { owner, life })
    }
    pub(super) fn type_domain(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<Domain> {
        self.type_domain_at(
            scope,
            recipe,
            hint,
            depth,
            &Occurrence {
                root: OccurrenceRoot::Recipe,
                path: Vec::new(),
            },
        )
    }
    pub(super) fn type_domain_at(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
        occurrence: &Occurrence,
    ) -> R<Domain> {
        if let SemanticTypeRecipe::OriginalType {
            scope: selector,
            source,
        } = recipe
        {
            self.node(depth)?;
            let expanded = self.original_type(scope, *selector, source, depth + 1)?;
            if let Some(value) =
                self.original_result_boundary(scope, *selector, source, hint, depth + 1)?
            {
                return self.freeze_domain(scope, &expanded, Boundary::Whole, value, occurrence);
            }
            if matches!(occurrence.root, OccurrenceRoot::Recipe) {
                crate::dyn_metadata::semantic_operand_snapshot(source, self.limits, self.left, 1)?;
                let origin = Occurrence {
                    root: OccurrenceRoot::Original(*selector, source.clone()),
                    path: occurrence.path.clone(),
                };
                return self.type_domain_at(scope, &expanded, hint, depth + 1, &origin);
            }
            return self.type_domain_at(scope, &expanded, hint, depth + 1, occurrence);
        }
        if let SemanticTypeRecipe::Tuple(items) = recipe {
            return self.tuple_domain(scope, items, depth, occurrence);
        }
        let value = self.ty_raw(scope, recipe, hint, depth)?;
        if let SemanticTypeRecipe::Address {
            lifetime, inner, ..
        }
        | SemanticTypeRecipe::Desc { lifetime, inner } = recipe
        {
            let SemanticTypeValue::Resolved { life, .. } = &value else {
                unreachable!()
            };
            let count = life.binder_count;
            let mut selectors = Vec::new();
            if let SemanticOuterLifeRecipe::Class(lifetime) = lifetime {
                selectors.push(self.outer_selector(scope, lifetime)?);
            }
            let offset = selectors.len() as u32;
            if count < offset {
                return Err(bad("outer type domain omits its original occurrence"));
            }
            let child_count = count - offset;
            self.fee(child_count as usize)?;
            if child_count != 0 {
                crate::dyn_metadata::semantic_operand_snapshot(
                    inner.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                let ty = Rc::new(inner.as_ref().clone());
                self.occurrence_snapshot(occurrence)?;
                let occurrence = Rc::new(occurrence.clone());
                for class in 0..child_count {
                    selectors.push(Selector::Intrinsic {
                        owner: scope,
                        ty: ty.clone(),
                        boundary: Boundary::Pointee,
                        occurrence: Some(occurrence.clone()),
                        class,
                    });
                }
            }
            return Ok(Domain { value, selectors });
        }
        match recipe {
            SemanticTypeRecipe::Formal(reference) => {
                let owner = self.ancestor(scope, reference.scope, depth + 1)?;
                let recipe = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                self.freeze_domain(owner, &recipe, Boundary::Whole, value, occurrence)
            }
            SemanticTypeRecipe::Environment {
                scope: selector,
                ordinal,
            } => {
                let owner = self.ancestor(scope, *selector, depth + 1)?;
                let recipe = SemanticTypeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: *ordinal,
                };
                self.freeze_domain(owner, &recipe, Boundary::Whole, value, occurrence)
            }
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                self.freeze_domain(scope, inner, Boundary::Pointee, value, occurrence)
            }
            SemanticTypeRecipe::OwnedStore(element)
            | SemanticTypeRecipe::Slice(element)
            | SemanticTypeRecipe::Array { element, .. } => {
                self.freeze_domain(scope, element, Boundary::Element, value, occurrence)
            }
            _ => self.freeze_domain(scope, recipe, Boundary::Whole, value, occurrence),
        }
    }
    fn freeze_domain(
        &mut self,
        owner: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        boundary: Boundary,
        value: SemanticTypeValue,
        occurrence: &Occurrence,
    ) -> R<Domain> {
        let SemanticTypeValue::Resolved { life, classes, .. } = &value else {
            return Err(bad("private full domain retains a Core type"));
        };
        if life.binder_count as usize != classes.len() {
            return Err(bad("private full domain omits intrinsic indices"));
        }
        self.fee(classes.len())?;
        let mut selectors = Vec::new();
        if life.binder_count != 0 {
            crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
            let ty = Rc::new(recipe.clone());
            let occurrence = if boundary == Boundary::Whole {
                None
            } else {
                self.occurrence_snapshot(occurrence)?;
                Some(Rc::new(occurrence.clone()))
            };
            for class in 0..life.binder_count {
                selectors.push(Selector::Intrinsic {
                    owner,
                    ty: ty.clone(),
                    boundary,
                    occurrence: occurrence.clone(),
                    class,
                });
            }
        }
        Ok(Domain { value, selectors })
    }
    pub(super) fn join_domain(
        &mut self,
        selectors: &mut Vec<Selector>,
        classes: &mut Vec<SemanticClassExpr>,
        child: &Domain,
    ) -> R<Vec<u32>> {
        let SemanticTypeValue::Resolved {
            life,
            classes: images,
            ..
        } = &child.value
        else {
            return Err(bad("joint domain retains an unresolved child type"));
        };
        if child.selectors.len() != life.binder_count as usize
            || child.selectors.len() != images.len()
        {
            return Err(bad("joint domain omits a complete child index map"));
        }
        let mut map = Vec::new();
        for (selector, image) in child.selectors.iter().zip(images) {
            self.fee(selectors.len().saturating_add(map.len()).saturating_add(1))?;
            let index = self.intern_selector(selectors, classes, selector, image)?;
            if map.contains(&index) {
                return Err(bad("joint domain collapses two frozen child indices"));
            }
            map.push(index);
        }
        Ok(map)
    }
    pub(super) fn find_selector(
        &mut self,
        selectors: &[Selector],
        classes: &[SemanticClassExpr],
        selector: &Selector,
        image: &SemanticClassExpr,
    ) -> R<Option<u32>> {
        self.fee(selectors.len().saturating_add(1))?;
        if selectors.len() != classes.len() {
            return Err(bad("joint domain selector/image arity changed"));
        }
        let mut found = None;
        for (index, prior) in selectors.iter().enumerate() {
            self.selector_snapshot(selector)?;
            self.selector_snapshot(prior)?;
            if prior == selector {
                if found.replace(index as u32).is_some() {
                    return Err(bad("joint original selector is duplicated"));
                }
                crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(
                    &classes[index],
                    self.limits,
                    self.left,
                    1,
                )?;
                if &classes[index] != image {
                    return Err(bad(
                        "one original joint selector has inconsistent actual images",
                    ));
                }
            }
        }
        Ok(found)
    }
    pub(super) fn intern_selector(
        &mut self,
        selectors: &mut Vec<Selector>,
        classes: &mut Vec<SemanticClassExpr>,
        selector: &Selector,
        image: &SemanticClassExpr,
    ) -> R<u32> {
        if let Some(index) = self.find_selector(selectors, classes, selector, image)? {
            return Ok(index);
        }
        if selectors.len() >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "joint original domain node limit exceeded",
                None,
            ));
        }
        let index =
            u32::try_from(selectors.len()).map_err(|_| bad("joint original domain overflows"))?;
        self.selector_snapshot(selector)?;
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        selectors.push(selector.clone());
        classes.push(image.clone());
        Ok(index)
    }
    fn tuple_domain(
        &mut self,
        scope: SemanticInstanceId,
        items: &[SemanticTypeRecipe],
        depth: usize,
        occurrence: &Occurrence,
    ) -> R<Domain> {
        self.node(depth)?;
        self.fee(items.len())?;
        let mut children = Vec::new();
        for (ordinal, item) in items.iter().enumerate() {
            let ordinal =
                u32::try_from(ordinal).map_err(|_| bad("tuple child ordinal overflows"))?;
            self.occurrence_snapshot(occurrence)?;
            let mut child_occurrence = occurrence.clone();
            child_occurrence.path.push(ordinal);
            children.push(self.type_domain_at(scope, item, None, depth + 1, &child_occurrence)?);
        }
        self.compose_tuple(children)
    }

    pub(super) fn compose_tuple(&mut self, children: Vec<Domain>) -> R<Domain> {
        self.fee(children.len())?;
        if children.is_empty() {
            return Ok(Domain {
                value: resolved(Type::Unit, empty(), Vec::new()),
                selectors: Vec::new(),
            });
        }
        let mut fields = Vec::new();
        let mut life = empty();
        let mut classes = Vec::new();
        let mut selectors = Vec::new();
        for (ordinal, child) in children.into_iter().enumerate() {
            let ordinal =
                u32::try_from(ordinal).map_err(|_| bad("tuple child ordinal overflows"))?;
            let map = self.join_domain(&mut selectors, &mut classes, &child)?;
            let SemanticTypeValue::Resolved {
                ty, life: child, ..
            } = child.value
            else {
                unreachable!()
            };
            let mapped = |class: u32| {
                map.get(class as usize)
                    .copied()
                    .ok_or_else(|| bad("tuple child projection leaves its complete domain"))
            };
            for slot in child.slots {
                self.fee(slot.path.len().saturating_add(2))?;
                let mut path = vec![PlaceProjection::Field(ordinal)];
                path.extend(slot.path);
                life.slots.push(StorageLifeSlot {
                    path,
                    binder: mapped(slot.binder)?,
                    mode: slot.mode,
                });
            }
            if matches!(
                ty,
                Type::BorrowedStorage { .. }
                    | Type::Closure(_)
                    | Type::MutexGuard { .. }
                    | Type::Task { .. }
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
            ) {
                self.fee(map.len())?;
                for (class, binder) in map.iter().enumerate() {
                    life.retained.push(StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Place(PlaceProjection::Field(ordinal))],
                        class: class as u32,
                        binder: *binder,
                    });
                }
            } else {
                for row in child.retained {
                    self.fee(row.path.len().saturating_add(2))?;
                    let mut path = vec![LifeTypeProjection::Place(PlaceProjection::Field(ordinal))];
                    path.extend(row.path);
                    life.retained.push(StorageRetainedBinding {
                        path,
                        class: row.class,
                        binder: mapped(row.binder)?,
                    });
                }
            }
            for row in child.nominal_arguments {
                self.fee(row.path.len().saturating_add(2))?;
                let mut path = vec![LifeTypeProjection::Place(PlaceProjection::Field(ordinal))];
                path.extend(row.path);
                life.nominal_arguments.push(NominalLifeArgument {
                    path,
                    binder: mapped(row.binder)?,
                    declaration: row.declaration,
                    argument: row.argument,
                });
            }
            fields.push(ty);
        }
        life.binder_count = u32::try_from(selectors.len())
            .map_err(|_| bad("tuple joint original domain overflows"))?;
        Ok(Domain {
            value: resolved(Type::Tuple(fields), life, classes),
            selectors,
        })
    }
}
