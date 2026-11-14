//! Exact original Return endpoints into already interpreted whole-type domains.
//! Projection copies indices; it never searches by type or caller image.
use super::domains::Domain;
use super::*;
use crate::semantics::original::TaskReturnSources;

impl Eval<'_, '_, '_> {
    fn endpoint_recipe(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        step: LifeTypeProjection,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.node(depth)?;
        let child = match (recipe, step) {
            (
                SemanticTypeRecipe::OriginalType {
                    scope: selector,
                    source,
                },
                _,
            ) => {
                let expanded = self.original_type(scope, *selector, source, depth + 1)?;
                return self.endpoint_recipe(scope, &expanded, step, depth + 1);
            }
            (SemanticTypeRecipe::Qualified { storage, .. }, _) => {
                return self.endpoint_recipe(scope, storage, step, depth + 1);
            }
            (
                SemanticTypeRecipe::Tuple(fields),
                LifeTypeProjection::Place(PlaceProjection::Field(i)),
            ) => fields.get(i as usize),
            (
                SemanticTypeRecipe::Address { inner, .. }
                | SemanticTypeRecipe::Desc { inner, .. }
                | SemanticTypeRecipe::Managed(inner)
                | SemanticTypeRecipe::Mutex(inner)
                | SemanticTypeRecipe::Registration(inner)
                | SemanticTypeRecipe::AsyncRegistration(inner),
                LifeTypeProjection::Pointee,
            ) => Some(inner.as_ref()),
            (
                SemanticTypeRecipe::Array { element, .. },
                LifeTypeProjection::Place(PlaceProjection::Field(_)),
            )
            | (
                SemanticTypeRecipe::OwnedStore(element) | SemanticTypeRecipe::Slice(element),
                LifeTypeProjection::Element,
            ) => Some(element.as_ref()),
            (SemanticTypeRecipe::Task { result, .. }, LifeTypeProjection::Result) => {
                Some(result.as_ref())
            }
            _ => None,
        };
        child
            .map(|child| {
                crate::dyn_metadata::semantic_operand_snapshot(child, self.limits, self.left, 1)?;
                Ok(child.clone())
            })
            .transpose()
    }
    fn endpoint_type<'t>(
        &mut self,
        mut ty: &'t Type,
        step: LifeTypeProjection,
        depth: usize,
    ) -> R<&'t Type> {
        self.node(depth)?;
        while let Type::BorrowedStorage { storage, .. }
        | Type::Opaque {
            hidden: storage, ..
        } = ty
        {
            self.fee(1)?;
            ty = storage;
        }
        let child = match (ty, step) {
            (Type::Tuple(fields), LifeTypeProjection::Place(PlaceProjection::Field(i))) => {
                fields.get(i as usize)
            }
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                LifeTypeProjection::Place(PlaceProjection::Field(i)),
            ) => fields.get(i as usize).map(|(_, ty)| ty),
            (
                Type::Enum { variants, .. },
                LifeTypeProjection::Place(PlaceProjection::Variant(p)),
            ) => variants
                .get(p.variant as usize)
                .and_then(|(_, fields)| fields.get(p.index as usize)),
            (
                Type::ErrorUnion { error, result },
                LifeTypeProjection::Place(PlaceProjection::ErrorArm(is_error)),
            ) => Some(if is_error {
                error.as_ref()
            } else {
                result.as_ref()
            }),
            (
                Type::Array { element, length },
                LifeTypeProjection::Place(PlaceProjection::Field(i)),
            ) if i < *length => Some(element.as_ref()),
            (
                Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner),
                LifeTypeProjection::Pointee,
            ) => Some(inner.as_ref()),
            (
                Type::Mutex { payload } | Type::MutexGuard { payload, .. },
                LifeTypeProjection::Pointee,
            ) => Some(payload.as_ref()),
            (
                Type::Registration { environment } | Type::AsyncRegistration { environment },
                LifeTypeProjection::Pointee,
            ) => Some(environment.as_ref()),
            (
                Type::OwnedStore { element } | Type::Slice { element },
                LifeTypeProjection::Element,
            ) => Some(element.as_ref()),
            (Type::Task { result, .. }, LifeTypeProjection::Result) => Some(result.as_ref()),
            _ => None,
        };
        child.ok_or_else(|| {
            bad("original Task Return endpoint leaves its exact complete operand type")
        })
    }
    fn endpoint_direct(
        &mut self,
        life: &StorageLifeShape,
        path: &[LifeTypeProjection],
    ) -> R<Option<Vec<u32>>> {
        self.fee(
            life.retained
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == path)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Ok(None);
        }
        self.fee(
            rows.len()
                .saturating_mul(rows.len().saturating_add(1).ilog2() as usize + 1),
        )?;
        rows.sort_by_key(|row| row.class);
        let mut result = Vec::new();
        for (class, row) in rows.iter().enumerate() {
            self.fee(result.len().saturating_add(1))?;
            if row.class as usize != class
                || row.binder >= life.binder_count
                || result.contains(&row.binder)
            {
                return Err(bad(
                    "Task Return endpoint has a noncanonical or collapsed whole-child map",
                ));
            }
            result.push(row.binder);
        }
        Ok(Some(result))
    }
    fn endpoint_map(
        &mut self,
        scope: SemanticInstanceId,
        recipe: Option<&SemanticTypeRecipe>,
        ty: &Type,
        life: &StorageLifeShape,
        path: &[LifeTypeProjection],
        depth: usize,
    ) -> R<Vec<u32>> {
        self.node(depth)?;
        if path.is_empty() {
            self.fee(life.binder_count as usize)?;
            return Ok((0..life.binder_count).collect());
        }
        if let Some(map) = self.endpoint_direct(life, path)? {
            return Ok(map);
        }
        let step = path[0];
        let child_ty = self.endpoint_type(ty, step, depth + 1)?;
        let child_recipe = recipe
            .map(|recipe| self.endpoint_recipe(scope, recipe, step, depth + 1))
            .transpose()?
            .flatten();
        let child_life = if let Some(recipe) = &child_recipe {
            let SemanticTypeValue::Resolved {
                ty: actual, life, ..
            } = self.ty(scope, recipe, Some(child_ty), depth + 1)?
            else {
                unreachable!()
            };
            inference::charge_type_snapshot(child_ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(&actual, Span::default(), self.limits, self.left, 1)?;
            if actual != *child_ty {
                return Err(bad(
                    "Task Return projection changes its full original child type",
                ));
            }
            life
        } else if let Type::BorrowedStorage { life, .. } = child_ty {
            self.copy_shape(life)?
        } else {
            return Err(unknown(
                "Task Return endpoint awaits its original complete child-domain recipe",
            ));
        };
        let prefix = vec![step];
        let map = if let Some(map) = self.endpoint_direct(life, &prefix)? {
            if map.len() != child_life.binder_count as usize {
                return Err(bad("Task Return prefix omits a complete child index"));
            }
            map
        } else {
            self.fee(child_life.binder_count as usize)?;
            let mut map = vec![None; child_life.binder_count as usize];
            if let LifeTypeProjection::Place(place) = step {
                for slot in &child_life.slots {
                    let mut full = vec![place];
                    self.fee(slot.path.len().saturating_add(1))?;
                    full.extend_from_slice(&slot.path);
                    self.fee(
                        life.slots
                            .len()
                            .saturating_mul(full.len().saturating_add(1)),
                    )?;
                    let mut rows = life
                        .slots
                        .iter()
                        .filter(|row| row.path == full && row.mode == slot.mode);
                    let row = rows.next().ok_or_else(|| {
                        unknown("Task Return physical child is absent from its full parent map")
                    })?;
                    if rows.next().is_some() {
                        return Err(bad(
                            "Task Return physical child has multiple parent occurrences",
                        ));
                    }
                    Self::endpoint_put(&mut map, slot.binder, row.binder, life.binder_count)?;
                }
            }
            for child in &child_life.retained {
                let mut full = prefix.clone();
                self.fee(child.path.len().saturating_add(1))?;
                full.extend_from_slice(&child.path);
                self.fee(
                    life.retained
                        .len()
                        .saturating_mul(full.len().saturating_add(1)),
                )?;
                let mut rows = life
                    .retained
                    .iter()
                    .filter(|row| row.path == full && row.class == child.class);
                let row = rows.next().ok_or_else(|| {
                    unknown("Task Return retained child has no complete parent projection")
                })?;
                if rows.next().is_some() {
                    return Err(bad(
                        "Task Return retained child has repeated parent projections",
                    ));
                }
                Self::endpoint_put(&mut map, child.binder, row.binder, life.binder_count)?;
            }
            for child in &child_life.nominal_arguments {
                let mut full = prefix.clone();
                self.fee(child.path.len().saturating_add(1))?;
                full.extend_from_slice(&child.path);
                self.fee(
                    life.nominal_arguments
                        .len()
                        .saturating_mul(full.len().saturating_add(1)),
                )?;
                let mut rows = life.nominal_arguments.iter().filter(|row| {
                    row.path == full
                        && row.declaration == child.declaration
                        && row.argument == child.argument
                });
                let row = rows.next().ok_or_else(|| {
                    unknown("Task Return nominal child has no complete parent occurrence")
                })?;
                if rows.next().is_some() {
                    return Err(bad(
                        "Task Return nominal child has repeated parent occurrences",
                    ));
                }
                Self::endpoint_put(&mut map, child.binder, row.binder, life.binder_count)?;
            }
            map.into_iter().collect::<Option<Vec<_>>>().ok_or_else(|| {
                unknown("Task Return child projection omits an intrinsic or phantom index")
            })?
        };
        for (i, binder) in map.iter().enumerate() {
            self.fee(i.saturating_add(1))?;
            if map[..i].contains(binder) {
                return Err(bad(
                    "Task Return child projection collapses complete indices",
                ));
            }
        }
        if path.len() == 1 {
            return Ok(map);
        }
        let child = self.endpoint_map(
            scope,
            child_recipe.as_ref(),
            child_ty,
            &child_life,
            &path[1..],
            depth + 1,
        )?;
        child
            .into_iter()
            .map(|class| {
                map.get(class as usize).copied().ok_or_else(|| {
                    bad("Task Return nested projection leaves its complete child domain")
                })
            })
            .collect()
    }
    fn endpoint_put(map: &mut [Option<u32>], child: u32, parent: u32, count: u32) -> R<()> {
        let target = map
            .get_mut(child as usize)
            .ok_or_else(|| bad("Task Return class leaves its full child domain"))?;
        if parent >= count || target.is_some_and(|old| old != parent) {
            return Err(bad("Task Return child changes its exact parent class"));
        }
        *target = Some(parent);
        Ok(())
    }
    pub(super) fn task_return_class(
        &mut self,
        scope: SemanticInstanceId,
        signature: &SemanticSignatureRecipe,
        sources: &[TaskReturnSources],
        inputs: &[Domain],
        input_maps: &[Vec<u32>],
        output: &Domain,
        class: u32,
    ) -> R<u32> {
        self.fee(sources.len())?;
        if sources.is_empty() {
            return Err(unknown(
                "Task result has no exact original Return source relation",
            ));
        }
        let SemanticTypeValue::Resolved {
            ty: result_ty,
            life: result_life,
            ..
        } = &output.value
        else {
            unreachable!()
        };
        let mut common = None;
        for statement in sources {
            let rows = statement.sources.as_ref().ok_or_else(|| {
                unknown(&format!(
                    "Task result has an unresolved original Return source at {:?}",
                    statement.statement
                ))
            })?;
            self.fee(rows.len())?;
            let mut selected = None;
            for row in rows {
                let result_map = self.endpoint_map(
                    scope,
                    Some(&signature.result),
                    result_ty,
                    result_life,
                    &row.result_path,
                    0,
                )?;
                let input = inputs.get(row.parameter as usize).ok_or_else(|| {
                    bad("Task Return endpoint selects an absent actual parameter")
                })?;
                let SemanticTypeValue::Resolved {
                    ty: input_ty,
                    life: input_life,
                    ..
                } = &input.value
                else {
                    unreachable!()
                };
                let recipe = signature
                    .parameters
                    .get(row.parameter as usize)
                    .ok_or_else(|| {
                        bad("Task Return endpoint selects an absent original parameter")
                    })?;
                let input_map = self.endpoint_map(
                    scope,
                    Some(&recipe.ty),
                    input_ty,
                    input_life,
                    &row.input_path,
                    0,
                )?;
                if row.class.is_none() && result_map.len() != input_map.len() {
                    return Err(bad(
                        "Task Return whole-child edge changes its complete domain size",
                    ));
                }
                self.fee(if row.class.is_some() {
                    1
                } else {
                    result_map.len()
                })?;
                let positions = if let Some(k) = row.class {
                    vec![k as usize]
                } else {
                    (0..result_map.len()).collect()
                };
                for k in positions {
                    let result = *result_map.get(k).ok_or_else(|| {
                        bad("Task Return source selects a nonexistent result child class")
                    })?;
                    if result != class {
                        continue;
                    }
                    let input = *input_map.get(k).ok_or_else(|| {
                        bad("Task Return source selects a nonexistent input child class")
                    })?;
                    let binder = *input_maps
                        .get(row.parameter as usize)
                        .and_then(|map| map.get(input as usize))
                        .ok_or_else(|| {
                            bad("Task Return source leaves the exact Task input-domain map")
                        })?;
                    if selected.is_some_and(|old| old != binder) {
                        return Err(unknown(
                            "Task Return result has different exact input-domain sources",
                        ));
                    }
                    selected = Some(binder);
                }
            }
            let binder = selected.ok_or_else(|| {
                unknown(&format!(
                    "Task result class {class} has no complete source at original Return {:?}",
                    statement.statement
                ))
            })?;
            if common.is_some_and(|old| old != binder) {
                return Err(unknown(
                    "Task result class has different complete sources across original Returns",
                ));
            }
            common = Some(binder);
        }
        common.ok_or_else(|| unknown("Task result original source relation is incomplete"))
    }
}
