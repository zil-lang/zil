//! Whole retained/storage shape recipes. These construct untrusted metadata;
//! actual nominal, header and CFG checks independently validate the result.
use super::*;
impl Eval<'_, '_> {
    pub(super) fn copy_shape(&mut self, life: &StorageLifeShape) -> R<StorageLifeShape> {
        self.fee(1)?;
        for slot in &life.slots {
            self.fee(slot.path.len().saturating_add(1))?;
        }
        for row in &life.retained {
            self.fee(row.path.len().saturating_add(1))?;
        }
        for row in &life.nominal_arguments {
            self.fee(row.path.len().saturating_add(1))?;
        }
        Ok(life.clone())
    }
    fn shape_class(&mut self, domain: &[SemanticClassExpr], image: &SemanticClassExpr) -> R<u32> {
        self.fee(domain.len())?;
        let mut found = None;
        for (index, actual) in domain.iter().enumerate() {
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
            if actual == image && found.replace(index as u32).is_some() {
                return Err(unknown(
                    "shape class needs an unambiguous original intrinsic selector",
                ));
            }
        }
        found.ok_or_else(|| bad("shape class is absent from its complete domain"))
    }
    fn shape_sequence(
        &mut self,
        scope: SemanticInstanceId,
        sequence: &SemanticClassSequenceRecipe,
        depth: usize,
    ) -> R<Vec<SemanticClassExpr>> {
        self.node(depth)?;
        self.fee(sequence.parts.len())?;
        let mut result = Vec::new();
        for part in &sequence.parts {
            match part {
                SemanticClassSequencePart::One(image) => result.push(self.life(scope, image)?),
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    self.fee(path.len())?;
                    if !path.is_empty() {
                        return Err(unknown(
                            "shape domain path needs its full child class projection",
                        ));
                    }
                    let SemanticTypeValue::Resolved { life, classes, .. } =
                        self.ty(scope, ty, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    if classes.len() != life.binder_count as usize {
                        return Err(bad("shape sequence omits whole-Type intrinsic classes"));
                    }
                    self.fee(classes.len())?;
                    // All positions survive, including distinct intrinsic
                    // classes whose current caller images happen to coincide.
                    result.extend(classes);
                }
            }
        }
        Ok(result)
    }
    fn shape_retained(
        &mut self,
        scope: SemanticInstanceId,
        domain: &[SemanticClassExpr],
        recipes: &[SemanticRetainedPart],
        depth: usize,
    ) -> R<Vec<StorageRetainedBinding>> {
        self.node(depth)?;
        self.fee(recipes.len())?;
        let mut result = Vec::new();
        for recipe in recipes {
            let (path, parents) = match recipe {
                SemanticRetainedPart::One {
                    path,
                    child_class,
                    parent,
                } => {
                    let image = self.life(scope, parent)?;
                    let binder = self.shape_class(domain, &image)?;
                    self.fee(path.len() + 1)?;
                    result.push(StorageRetainedBinding {
                        path: path.clone(),
                        class: *child_class,
                        binder,
                    });
                    continue;
                }
                SemanticRetainedPart::All { path, parents } => {
                    (path, self.shape_sequence(scope, parents, depth + 1)?)
                }
            };
            for (class, image) in parents.into_iter().enumerate() {
                let binder = self.shape_class(domain, &image)?;
                self.fee(path.len() + 1)?;
                result.push(StorageRetainedBinding {
                    path: path.clone(),
                    class: class as u32,
                    binder,
                });
            }
        }
        Ok(result)
    }
    fn shape_nominal(
        &mut self,
        scope: SemanticInstanceId,
        domain: &[SemanticClassExpr],
        recipes: &[SemanticNominalPart],
        depth: usize,
    ) -> R<Vec<NominalLifeArgument>> {
        self.node(depth)?;
        self.fee(recipes.len())?;
        let mut result = Vec::new();
        for recipe in recipes {
            match recipe {
                SemanticNominalPart::One {
                    path,
                    declaration,
                    argument,
                    class,
                } => {
                    let image = self.life(scope, class)?;
                    let binder = self.shape_class(domain, &image)?;
                    self.fee(path.len() + 1)?;
                    result.push(NominalLifeArgument {
                        path: path.clone(),
                        declaration: *declaration,
                        argument: *argument,
                        binder,
                    });
                }
                SemanticNominalPart::FromType { path, ty, classes } => {
                    let SemanticTypeValue::Resolved { life, .. } =
                        self.ty(scope, ty, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    let images = self.shape_sequence(scope, classes, depth + 1)?;
                    if images.len() != life.binder_count as usize {
                        return Err(bad(
                            "nominal shape projection changes its full child domain",
                        ));
                    }
                    for row in life.nominal_arguments {
                        let image = images.get(row.binder as usize).ok_or_else(|| {
                            bad("nominal shape child class is outside its domain")
                        })?;
                        let binder = self.shape_class(domain, image)?;
                        self.fee(path.len().saturating_add(row.path.len()).saturating_add(1))?;
                        let mut full = path.clone();
                        full.extend(row.path);
                        result.push(NominalLifeArgument {
                            path: full,
                            declaration: row.declaration,
                            argument: row.argument,
                            binder,
                        });
                    }
                }
            }
        }
        Ok(result)
    }
    pub(super) fn storage_shape(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticStorageShapeRecipe,
        depth: usize,
    ) -> R<(StorageLifeShape, Vec<SemanticClassExpr>)> {
        self.node(depth)?;
        let classes = self.shape_sequence(scope, &recipe.domain, depth + 1)?;
        let binder_count =
            u32::try_from(classes.len()).map_err(|_| bad("storage shape domain overflows"))?;
        self.fee(recipe.slots.len())?;
        let mut slots = Vec::new();
        for row in &recipe.slots {
            let image = self.life(scope, &row.class)?;
            let binder = self.shape_class(&classes, &image)?;
            self.fee(row.path.len() + 1)?;
            slots.push(StorageLifeSlot {
                path: row.path.clone(),
                binder,
                mode: row.mode,
            });
        }
        let retained = self.shape_retained(scope, &classes, &recipe.retained, depth + 1)?;
        let nominal_arguments = self.shape_nominal(scope, &classes, &recipe.nominal, depth + 1)?;
        Ok((
            StorageLifeShape {
                binder_count,
                slots,
                retained,
                nominal_arguments,
            },
            classes,
        ))
    }
    pub(super) fn retained_shape(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<(RetainedLifeShape, Vec<SemanticClassExpr>)> {
        self.node(depth)?;
        let classes = self.shape_sequence(scope, &recipe.domain, depth + 1)?;
        let binder_count =
            u32::try_from(classes.len()).map_err(|_| bad("retained shape domain overflows"))?;
        self.fee(recipe.ports.len())?;
        let mut ports = Vec::new();
        for part in &recipe.ports {
            match part {
                SemanticPortPart::One { leaf, mode, class } => {
                    let SemanticTypeValue::Resolved { ty: leaf, .. } =
                        self.ty(scope, leaf, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    let image = self.life(scope, class)?;
                    let binder = self.shape_class(&classes, &image)?;
                    ports.push(RetainedBorrowPort {
                        leaf,
                        mode: *mode,
                        binder,
                    });
                }
                SemanticPortPart::FromType {
                    ty,
                    classes: images,
                } => {
                    let SemanticTypeValue::Resolved { ty, life, .. } =
                        self.ty(scope, ty, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    let images = self.shape_sequence(scope, images, depth + 1)?;
                    if images.len() != life.binder_count as usize {
                        return Err(bad("port shape projection changes its full child domain"));
                    }
                    let child = match ty {
                        Type::Task { retained_life, .. }
                        | Type::MutexGuard { retained_life, .. } => retained_life,
                        _ => {
                            return Err(unknown(
                                "port projection requires its actual complete opaque carrier shape",
                            ));
                        }
                    };
                    self.fee(child.ports.len())?;
                    for port in child.ports {
                        let image = images
                            .get(port.binder as usize)
                            .ok_or_else(|| bad("port child class is outside its whole domain"))?;
                        let binder = self.shape_class(&classes, image)?;
                        ports.push(RetainedBorrowPort { binder, ..port });
                    }
                }
            }
        }
        let retained = self.shape_retained(scope, &classes, &recipe.retained, depth + 1)?;
        let nominal_arguments = self.shape_nominal(scope, &classes, &recipe.nominal, depth + 1)?;
        Ok((
            RetainedLifeShape {
                binder_count,
                ports,
                retained,
                nominal_arguments,
            },
            classes,
        ))
    }
}
