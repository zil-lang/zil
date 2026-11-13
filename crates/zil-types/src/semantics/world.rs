//! Original lifetime worlds and actual whole-type class flow. Classes here are
//! semantic identities; data targets, permissions and resource phases are not
//! manufactured by this interpreter.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeMap;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-WORLD", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
fn empty() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: Vec::new(),
        retained: Vec::new(),
        nominal_arguments: Vec::new(),
    }
}
struct World<'a, 'b> {
    program: &'a SemanticModule,
    module: &'a TypedModule,
    ledger: &'a Ledger,
    limits: &'b Limits,
    left: &'b mut usize,
}
impl<'a> World<'a, '_> {
    fn pay(&mut self, n: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original world class-flow work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        self.pay(self.program.instances.len())?;
        self.program
            .instances
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| bad("world has no actual instance"))
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(self.program.definitions.len())?;
        self.program
            .definitions
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| bad("world has no original definition"))
    }
    fn function(&mut self, id: FunctionId) -> R<&'a TypedFunction> {
        self.pay(self.module.functions.len())?;
        self.module
            .functions
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| bad("world has no actual function"))
    }
    fn child(&mut self, id: ClosureId) -> R<(&'a ClosureDefinition, &'a SemanticInstance)> {
        self.pay(
            self.module
                .closures
                .len()
                .saturating_add(self.program.instances.len()),
        )?;
        let d = self
            .module
            .closures
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("actual closure is absent"))?;
        let mut children = self.program.instances.iter().filter(|i| matches!(i.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(f)) if f == d.function));
        let child = children
            .next()
            .ok_or_else(|| bad("actual closure has no original child world"))?;
        if children.next().is_some() {
            return Err(bad("actual closure has ambiguous original child worlds"));
        }
        Ok((d, child))
    }
    fn copy(&mut self, value: &SemanticTypeValue) -> R<SemanticTypeValue> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        Ok(value.clone())
    }
    fn resolved(
        &mut self,
        ty: &Type,
        life: StorageLifeShape,
        classes: Vec<SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
        if life.binder_count as usize != classes.len() {
            return Err(bad("whole type loses its intrinsic class count"));
        }
        Ok(SemanticTypeValue::Resolved {
            ty: ty.clone(),
            life,
            classes,
        })
    }
    fn zero(&mut self, ty: &Type) -> R<SemanticTypeValue> {
        self.pay(1)?;
        match ty {
            ty if ty.is_b0_scalar() => self.resolved(ty, empty(), Vec::new()),
            Type::Tuple(fields) => {
                for field in fields {
                    self.zero(field)?;
                }
                self.resolved(ty, empty(), Vec::new())
            }
            Type::Array { element, .. } => {
                self.zero(element)?;
                self.resolved(ty, empty(), Vec::new())
            }
            _ => Err(pending(
                "actual zero-domain type requires its complete original whole-type equation",
            )),
        }
    }
    fn symbolic(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
    ) -> R<SemanticTypeRecipe> {
        if let SemanticTypeRecipe::OriginalType {
            scope: relative,
            source,
        } = recipe
        {
            let current = self.instance(scope)?.definition;
            let SemanticScopeRef::Ancestor(hops) = relative;
            let mut owner = current;
            for _ in 0..*hops {
                self.pay(1)?;
                owner = self
                    .definition(owner)?
                    .parent
                    .ok_or_else(|| bad("original query escapes lexical definition"))?;
            }
            let proof = self.ledger.original.borrow();
            return proof
                .as_ref()
                .ok_or_else(|| bad("original query lacks complete Gamma"))?
                .type_recipe(self.program, current, owner, source, self.limits, self.left);
        }
        crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
        Ok(recipe.clone())
    }
    fn field_value(&mut self, parent: &SemanticTypeValue, index: u32) -> R<SemanticTypeValue> {
        let ty = Self::type_of(parent)?;
        let field = match stored::physical(ty) {
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.get(index as usize).map(|(_, ty)| ty)
            }
            Type::Tuple(fields) => fields.get(index as usize),
            _ => None,
        }
        .ok_or_else(|| bad("actual field projection is outside its complete product"))?;
        // A zero-domain primitive child inherits no phantom class of its
        // parent. Nonzero children need their exact stored projection map.
        self.zero(field)
    }
    fn ty(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
    ) -> R<SemanticTypeValue> {
        super::materialize::type_at(
            self.program,
            self.module,
            self.ledger,
            scope,
            recipe,
            Some(hint),
            self.limits,
            self.left,
        )
    }
    fn type_of(value: &SemanticTypeValue) -> R<&Type> {
        match value {
            SemanticTypeValue::Resolved { ty, .. } => Ok(ty),
            _ => Err(bad("Core type survives world materialization")),
        }
    }
    fn get(&mut self, values: &[Option<SemanticTypeValue>], id: ValueId) -> R<SemanticTypeValue> {
        let value = values
            .get(id.0 as usize)
            .and_then(|v| v.as_ref())
            .ok_or_else(|| bad("SSA has no reaching whole-type equation"))?;
        self.copy(value)
    }
    fn local(&mut self, locals: &[Option<SemanticTypeValue>], id: LocalId) -> R<SemanticTypeValue> {
        let value = locals
            .get(id.0 as usize)
            .and_then(|v| v.as_ref())
            .ok_or_else(|| bad("local has no initialized whole-type occupant"))?;
        self.copy(value)
    }
    fn equal(&mut self, a: &SemanticTypeValue, b: &SemanticTypeValue) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        if a != b {
            return Err(bad("actual whole type or rigid original classes disagree"));
        }
        Ok(())
    }
    fn binding(
        &mut self,
        instance: &SemanticInstance,
        binder: StorageBinder,
    ) -> R<SemanticClassExpr> {
        self.pay(instance.header_embedding.len())?;
        let mut rows = instance
            .header_embedding
            .iter()
            .filter(|r| r.header == binder);
        let row = rows
            .next()
            .ok_or_else(|| bad("actual header lacks its original class image"))?;
        if rows.next().is_some() {
            return Err(bad("actual header repeats an original class image"));
        }
        crate::dyn_metadata::semantic_operand_snapshot(&row.actual, self.limits, self.left, 1)?;
        Ok(row.actual.clone())
    }
    fn capture(&mut self, instance: &SemanticInstance, index: u32) -> R<SemanticTypeValue> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(bad("capture is outside an actual function"));
        };
        let f = self.function(function)?;
        let id = f
            .environment
            .ok_or_else(|| bad("capture has no actual environment"))?;
        let (d, child) = self.child(id)?;
        if child.id != instance.id {
            return Err(bad("capture uses another child world"));
        }
        let ty = d
            .captures
            .get(index as usize)
            .ok_or_else(|| bad("capture index is outside its actual directory"))?;
        let header = d
            .storage_life
            .as_ref()
            .ok_or_else(|| pending("capture complete domain remains unknown"))?;
        if f.storage_life.as_ref() != Some(header) {
            return Err(bad("entry and environment class directories disagree"));
        }
        match ty {
            Type::BorrowedStorage { life, .. } => {
                self.pay(life.binder_count as usize + header.classes.len())?;
                let mut images = Vec::new();
                for class in 0..life.binder_count {
                    let mut rows = header.classes.iter().filter(|r| {
                        r.position.base == StorageBase::Capture(index)
                            && r.position.path.is_empty()
                            && r.position.class == class
                    });
                    let row = rows
                        .next()
                        .ok_or_else(|| bad("capture loses an intrinsic class"))?;
                    if rows.next().is_some() {
                        return Err(bad("capture repeats an intrinsic class"));
                    }
                    images.push(self.binding(instance, row.binder)?);
                }
                self.resolved(ty, life.clone(), images)
            }
            Type::Address { mutable, inner } if inner.is_b0_scalar() => {
                self.pay(header.bindings.len())?;
                let mut rows = header.bindings.iter().filter(|r| {
                    r.position.base == StorageBase::Capture(index) && r.position.path.is_empty()
                });
                let row = rows
                    .next()
                    .ok_or_else(|| bad("capture loses its outer occurrence"))?;
                if rows.next().is_some() {
                    return Err(bad("capture repeats its outer occurrence"));
                }
                let image = self.binding(instance, row.binder)?;
                self.resolved(
                    ty,
                    StorageLifeShape {
                        binder_count: 1,
                        slots: vec![StorageLifeSlot {
                            path: Vec::new(),
                            mode: if *mutable {
                                StoredBorrowMode::ExclusiveAddress
                            } else {
                                StoredBorrowMode::SharedAddress
                            },
                            binder: 0,
                        }],
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    vec![image],
                )
            }
            _ => self.zero(ty),
        }
    }
    fn address(
        &mut self,
        ty: &Type,
        image: SemanticClassExpr,
        pointee: &SemanticTypeValue,
    ) -> R<SemanticTypeValue> {
        let (
            Type::Address { mutable, inner },
            SemanticTypeValue::Resolved {
                ty: expected,
                life: child,
                classes,
            },
        ) = (ty, pointee)
        else {
            return Err(bad("address producer changes its actual constructor"));
        };
        if inner.as_ref() != expected {
            return Err(bad("address producer changes its actual pointee type"));
        }
        self.pay(classes.len().saturating_mul(2))?;
        let mut images = vec![image];
        images.extend(classes.iter().cloned());
        self.resolved(
            ty,
            StorageLifeShape {
                binder_count: child
                    .binder_count
                    .checked_add(1)
                    .ok_or_else(|| bad("address domain overflows"))?,
                slots: vec![StorageLifeSlot {
                    path: Vec::new(),
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    binder: 0,
                }],
                retained: (0..child.binder_count)
                    .map(|class| StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee],
                        class,
                        binder: class + 1,
                    })
                    .collect(),
                nominal_arguments: Vec::new(),
            },
            images,
        )
    }
    fn pointee(&mut self, value: &SemanticTypeValue) -> R<SemanticTypeValue> {
        let SemanticTypeValue::Resolved {
            ty: Type::Address { inner, .. },
            life,
            classes,
        } = value
        else {
            return Err(bad("pointee projection has no actual address"));
        };
        let shape = match inner.as_ref() {
            Type::BorrowedStorage { life, .. } => life.clone(),
            Type::Closure(id) => {
                let (definition, _) = self.child(*id)?;
                let header = definition
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| pending("closure pointee has unknown complete domain"))?;
                StorageLifeShape {
                    binder_count: header.fixed_binder_count,
                    ..empty()
                }
            }
            _ => {
                return self.zero(inner);
            }
        };
        self.pay(shape.binder_count as usize + life.retained.len())?;
        let mut images = Vec::new();
        for class in 0..shape.binder_count {
            let mut rows = life
                .retained
                .iter()
                .filter(|r| r.path.as_slice() == [LifeTypeProjection::Pointee] && r.class == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("address loses a complete pointee class"))?;
            if rows.next().is_some() {
                return Err(bad("address repeats a pointee class"));
            }
            images.push(
                classes
                    .get(row.binder as usize)
                    .ok_or_else(|| bad("address pointee image is outside its domain"))?
                    .clone(),
            );
        }
        self.resolved(inner, shape, images)
    }
    fn map_value(
        &mut self,
        value: &SemanticTypeValue,
        actuals: &[SemanticActual],
        environment: Option<(&StorageLifeContract, &SemanticInstance, &SemanticTypeValue)>,
    ) -> R<SemanticTypeValue> {
        let mut value = self.copy(value)?;
        let SemanticTypeValue::Resolved { classes, .. } = &mut value else {
            return Err(bad("Core type survives call interpretation"));
        };
        for image in classes {
            match image {
                SemanticClassExpr::Formal {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                } => {
                    let Some(SemanticActual::Life(actual)) = actuals.get(*ordinal as usize) else {
                        return Err(bad("call omits an original lifetime actual"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    *image = actual.clone();
                }
                SemanticClassExpr::IntrinsicClass {
                    operand:
                        SemanticOperand::TypeArgument {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal,
                        },
                    path,
                    class,
                } if path.is_empty() => {
                    let Some(SemanticActual::Type(SemanticTypeValue::Resolved { classes, .. })) =
                        actuals.get(*ordinal as usize)
                    else {
                        return Err(bad("call omits a whole-Type actual"));
                    };
                    let actual = classes
                        .get(*class as usize)
                        .ok_or_else(|| bad("call truncates the whole-Type actual domain"))?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    *image = actual.clone();
                }
                SemanticClassExpr::Environment { .. }
                | SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Environment(_),
                    ..
                } => {
                    let Some((
                        header,
                        child,
                        SemanticTypeValue::Resolved {
                            classes: actual_images,
                            ..
                        },
                    )) = environment
                    else {
                        return Err(bad("call reads an unsealed child environment class"));
                    };
                    let mut selected = None;
                    for row in &child.header_embedding {
                        self.pay(1)?;
                        if row.actual == *image {
                            let StorageBinder::Fixed(fixed) = row.header else {
                                continue;
                            };
                            if fixed >= header.fixed_binder_count
                                || selected.replace(fixed).is_some()
                            {
                                return Err(bad("environment class has ambiguous Fixed images"));
                            }
                        }
                    }
                    *image = actual_images
                        .get(
                            selected
                                .ok_or_else(|| bad("environment class has no actual Fixed image"))?
                                as usize,
                        )
                        .ok_or_else(|| bad("call truncates an environment's complete domain"))?
                        .clone();
                }
                _ => {
                    return Err(pending(
                        "call class requires its complete scoped invocation/outer relation",
                    ));
                }
            }
        }
        Ok(value)
    }
    fn occurrence(&mut self, f: &TypedFunction, value: ValueId) -> R<SemanticClassExpr> {
        let Type::Address { mutable, .. } = f
            .values
            .get(value.0 as usize)
            .ok_or_else(|| bad("borrow result lacks a type"))?
        else {
            return Err(bad("borrow producer result is not an address"));
        };
        Ok(SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(value),
            path: Vec::new(),
            mode: if *mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            },
        })
    }
}

impl World<'_, '_> {
    fn class(
        &mut self,
        scope: SemanticInstanceId,
        image: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticClassExpr> {
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        Ok(match image {
            SemanticClassExpr::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => match implicit.get(ordinal) {
                Some(actual) => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    actual.clone()
                }
                None => image.clone(),
            },
            SemanticClassExpr::Formal {
                scope: relative,
                ordinal,
            } => super::materialize::class_at(
                self.program,
                self.module,
                self.ledger,
                scope,
                &SemanticLifeRecipe::Formal(SemanticFormalRef {
                    scope: *relative,
                    ordinal: *ordinal,
                }),
                self.limits,
                self.left,
            )?,
            _ => image.clone(),
        })
    }
    fn same_class(
        &mut self,
        scope: SemanticInstanceId,
        a: &SemanticClassExpr,
        b: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        let a = self.class(scope, a, implicit)?;
        let b = self.class(scope, b, implicit)?;
        if a != b {
            return Err(bad("rigid original class images disagree"));
        }
        Ok(())
    }
    fn normalize(
        &mut self,
        scope: SemanticInstanceId,
        value: &SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        let mut value = self.copy(value)?;
        let SemanticTypeValue::Resolved { classes, .. } = &mut value else {
            return Err(bad("Core value survives normalization"));
        };
        for image in classes {
            *image = self.class(scope, image, implicit)?;
        }
        Ok(value)
    }
    fn same(
        &mut self,
        scope: SemanticInstanceId,
        a: &SemanticTypeValue,
        b: &SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        let a = self.normalize(scope, a, implicit)?;
        let b = self.normalize(scope, b, implicit)?;
        self.equal(&a, &b)
    }
    fn original_recipe<'a>(
        &mut self,
        original: &'a SemanticDefinition,
        id: SemanticSiteId,
    ) -> R<&'a SemanticSiteRecipe> {
        let (SemanticDefinitionBody::Function { sites, .. }
        | SemanticDefinitionBody::FunctionWithBody { sites, .. }) = &original.body
        else {
            return Err(bad("operation is outside its original function"));
        };
        self.pay(sites.len())?;
        sites
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| bad("actual operation has no original recipe"))
    }
    fn point_sites<'a>(
        &mut self,
        sites: &'a [&'a SemanticSite],
        block: BlockId,
        instruction: usize,
    ) -> R<Vec<&'a SemanticSite>> {
        self.pay(sites.len())?;
        Ok(sites.iter().copied().filter(|s| matches!(s.at, SemanticSiteAddress::Typed { block: b, instruction: i, .. } if b == block && i as usize == instruction)).collect())
    }
    fn constraints(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        function: &TypedFunction,
        sites: &[&SemanticSite],
        block: BlockId,
        instruction: usize,
        values: &[Option<SemanticTypeValue>],
        locals: &[Option<SemanticTypeValue>],
        declared: &mut BTreeMap<LocalId, SemanticTypeValue>,
        implicit: &mut BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        for site in self.point_sites(sites, block, instruction)? {
            let recipe = self.original_recipe(original, site.recipe)?;
            match (&site.action, &recipe.kind) {
                (
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(local),
                        expected,
                    },
                    SemanticSiteRecipeKind::DeclareLocal { expected: recipe },
                ) => {
                    let ty = &function
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| bad("declaration local is absent"))?
                        .ty;
                    let actual = self.ty(instance.id, recipe, ty)?;
                    self.same(instance.id, expected, &actual, implicit)?;
                    if let Some(old) = declared.insert(*local, actual) {
                        self.same(instance.id, &old, expected, implicit)?;
                    }
                }
                (
                    SemanticSiteAction::TypeUse { subject, expected },
                    SemanticSiteRecipeKind::TypeUse {
                        expected: recipe_ty,
                    },
                ) => {
                    let value = match subject {
                        SemanticOperand::Value(value) => self.get(values, *value)?,
                        SemanticOperand::Local(local) => self.local(locals, *local)?,
                        SemanticOperand::Capture(index) => self.capture(instance, *index)?,
                        _ => {
                            return Err(pending(
                                "type-use needs its complete actual operand world",
                            ));
                        }
                    };
                    let original_value = self.ty(instance.id, recipe_ty, Self::type_of(&value)?)?;
                    self.same(instance.id, expected, &original_value, implicit)?;
                    // Only the original body's exact implicit occurrence can
                    // name a freshly emitted local borrow. A signature input,
                    // Formal or already sealed environment is never unified.
                    if let (
                        SemanticTypeValue::Resolved {
                            classes: images, ..
                        },
                        SemanticTypeValue::Resolved {
                            classes: actual_images,
                            ..
                        },
                    ) = (&original_value, &value)
                    {
                        self.pay(
                            images
                                .len()
                                .saturating_add(original.implicit_lifetimes.len()),
                        )?;
                        for (expected, actual) in images.iter().zip(actual_images) {
                            if let SemanticClassExpr::Implicit {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            } = expected
                            {
                                if original.implicit_lifetimes.iter().any(|i| {
                                    i.ordinal == *ordinal
                                        && i.occurrence_path == recipe.occurrence_path
                                }) && matches!(
                                    actual,
                                    SemanticClassExpr::OuterOccurrence {
                                        operand: SemanticOperand::Value(_),
                                        ..
                                    }
                                ) {
                                    if let Some(old) = implicit.insert(*ordinal, actual.clone()) {
                                        if old != *actual {
                                            return Err(bad(
                                                "one original occurrence selects different emitted class heads",
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    self.same(instance.id, &value, &original_value, implicit)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn constructor(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        f: &TypedFunction,
        sites: &[&SemanticSite],
        block: BlockId,
        index: usize,
        result: ValueId,
        fields: &[ValueId],
        values: &[Option<SemanticTypeValue>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        let mut selected = None;
        for site in self.point_sites(sites, block, index)? {
            if let SemanticSiteAction::Construct {
                result: SemanticOperand::Value(value),
                value: claimed,
            } = &site.action
            {
                if *value != result || selected.is_some() {
                    return Err(bad(
                        "constructor selects a different or repeated actual result",
                    ));
                }
                let recipe = self.original_recipe(original, site.recipe)?;
                let SemanticSiteRecipeKind::Construct { result: recipe } = &recipe.kind else {
                    return Err(bad("constructor changes original recipe kind"));
                };
                let expected = self.ty(instance.id, recipe, &f.values[result.0 as usize])?;
                self.same(instance.id, claimed, &expected, implicit)?;
                let symbolic = self.symbolic(instance.id, recipe)?;
                let SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                } = &symbolic
                else {
                    return Err(pending(
                        "constructor needs its original compound field instantiation",
                    ));
                };
                let nominal = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal {
                    body:
                        SemanticNominalBody::Product {
                            fields: original_fields,
                        },
                    ..
                } = &nominal.body
                else {
                    return Err(pending(
                        "sum constructor needs its selected original variant world",
                    ));
                };
                let actual_fields = match stored::physical(Self::type_of(&expected)?) {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => fields,
                    _ => return Err(bad("original nominal product changes actual constructor")),
                };
                if fields.len() != original_fields.len()
                    || actual_fields.len() != original_fields.len()
                {
                    return Err(bad("constructor omits original physical fields"));
                }
                self.pay(fields.len())?;
                for ((field, recipe), (name, ty)) in
                    fields.iter().zip(original_fields).zip(actual_fields)
                {
                    if name != &recipe.name {
                        return Err(bad("constructor changes original field identity"));
                    }
                    let recipe =
                        super::instantiate::ty(&recipe.ty, arguments, self.limits, self.left)?;
                    let expected_field = self.ty(instance.id, &recipe, ty)?;
                    let value = self.get(values, *field)?;
                    self.same(instance.id, &value, &expected_field, implicit)?;
                }
                selected = Some(expected);
            }
        }
        selected.ok_or_else(|| bad("actual qualified constructor has no original constructor site"))
    }
    fn body_call(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        site: &SemanticSite,
        actuals: &[SemanticActual],
        target: SemanticDefinitionId,
    ) -> R<()> {
        if !matches!(
            self.original_recipe(original, site.recipe)?.kind,
            SemanticSiteRecipeKind::OriginalCall
        ) {
            return Err(bad("actual body call changes original operation kind"));
        }
        let recipes = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("body call lacks complete original Gamma"))?;
            let key = (original.id, site.recipe);
            if proof.call_targets.get(&key) != Some(&target) {
                return Err(bad("actual body call selects another original target"));
            }
            let recipes = proof
                .calls
                .get(&key)
                .ok_or_else(|| bad("body call lacks original fresh actuals"))?;
            if recipes.len() != actuals.len() {
                return Err(bad("body call omits mixed actuals"));
            }
            for recipe in recipes {
                crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
            }
            recipes.clone()
        };
        for (recipe, actual) in recipes.iter().zip(actuals) {
            let expected = super::materialize::actual_at(
                self.program,
                self.module,
                self.ledger,
                instance.id,
                recipe,
                self.limits,
                self.left,
            )?;
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
            if actual != &expected {
                return Err(bad("body call actuals differ from original inference"));
            }
        }
        Ok(())
    }
    fn requirements(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        actuals: Option<&[SemanticActual]>,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        self.pay(original.requirements.len())?;
        for requirement in &original.requirements {
            match requirement {
                SemanticRequirementRecipe::LifeEqual { left, right } => {
                    if let Some(actuals) = actuals {
                        // No callee equality assumption participates in the
                        // caller's proof of the same premise.
                        let select = |recipe: &SemanticLifeRecipe| -> R<SemanticClassExpr> {
                            let SemanticLifeRecipe::Formal(SemanticFormalRef {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            }) = recipe
                            else {
                                return Err(pending(
                                    "where equality needs its checked lexical selector map",
                                ));
                            };
                            let Some(SemanticActual::Life(image)) = actuals.get(*ordinal as usize)
                            else {
                                return Err(bad("where equality selects a non-Life actual"));
                            };
                            Ok(image.clone())
                        };
                        let a = select(left)?;
                        let b = select(right)?;
                        self.same_class(instance.id, &a, &b, implicit)?;
                    }
                }
                SemanticRequirementRecipe::Callable {
                    ty:
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        },
                    ..
                } => {
                    let proof = self.ledger.original.borrow();
                    if !proof
                        .as_ref()
                        .is_some_and(|proof| proof.complete.contains(&original.id))
                    {
                        return Err(bad(
                            "result callable goal lacks complete original code/capture proof",
                        ));
                    }
                    // Every real target instance, its complete header and every
                    // operation are independently interpreted by this World.
                }
                _ => {
                    return Err(pending(
                        "original where premise requires its complete rigid-role proof",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl World<'_, '_> {
    fn check_header(
        &mut self,
        instance: &SemanticInstance,
        f: &TypedFunction,
        signature: &SemanticSignatureRecipe,
    ) -> R<()> {
        let mut expected_bindings = Vec::new();
        let mut expected_classes = Vec::new();
        for (index, (local, recipe)) in f.parameters.iter().zip(&signature.parameters).enumerate() {
            let ty = &f.locals[local.0 as usize].ty;
            let value = self.ty(instance.id, &recipe.ty, ty)?;
            let SemanticTypeValue::Resolved { life, classes, .. } = value else {
                return Err(bad("header input retains Core type"));
            };
            match ty {
                Type::Address { .. }
                    if life.slots.len() == 1
                        && life.binder_count == 1
                        && classes.len() == 1
                        && life.retained.is_empty() =>
                {
                    expected_bindings.push((
                        StoragePosition {
                            base: StorageBase::Parameter(index as u32),
                            path: Vec::new(),
                        },
                        classes[0].clone(),
                    ));
                }
                Type::BorrowedStorage { .. }
                    if life.slots.is_empty() && life.retained.is_empty() =>
                {
                    for (class, image) in classes.into_iter().enumerate() {
                        expected_classes.push((
                            SignatureLifeClass {
                                base: StorageBase::Parameter(index as u32),
                                path: Vec::new(),
                                class: class as u32,
                            },
                            image,
                        ));
                    }
                }
                _ if life.binder_count == 0 && classes.is_empty() => {}
                _ => {
                    return Err(pending(
                        "input header needs complete composite occurrence/class enumeration",
                    ));
                }
            }
        }
        let result = self.ty(instance.id, &signature.result, &f.return_type)?;
        let SemanticTypeValue::Resolved { life, classes, .. } = result else {
            return Err(bad("header result retains Core type"));
        };
        if matches!(
            f.return_type,
            Type::Closure(_) | Type::BorrowedStorage { .. }
        ) {
            for (class, image) in classes.into_iter().enumerate() {
                expected_classes.push((
                    SignatureLifeClass {
                        base: StorageBase::Result,
                        path: Vec::new(),
                        class: class as u32,
                    },
                    image,
                ));
            }
        } else if life.binder_count != 0 {
            return Err(pending(
                "result header needs its complete ordinary occurrence enumeration",
            ));
        }
        if let Some(closure) = f.environment {
            let (d, child) = self.child(closure)?;
            if child.id != instance.id || d.storage_life != f.storage_life {
                return Err(bad("entry header differs from its real environment"));
            }
            for (index, ty) in d.captures.iter().enumerate() {
                let value = self.capture(instance, index as u32)?;
                let SemanticTypeValue::Resolved { life, classes, .. } = value else {
                    return Err(bad("capture header retains Core type"));
                };
                match ty {
                    Type::BorrowedStorage { .. } => {
                        for (class, image) in classes.into_iter().enumerate() {
                            expected_classes.push((
                                SignatureLifeClass {
                                    base: StorageBase::Capture(index as u32),
                                    path: Vec::new(),
                                    class: class as u32,
                                },
                                image,
                            ));
                        }
                    }
                    Type::Address { .. } if classes.len() == 1 => expected_bindings.push((
                        StoragePosition {
                            base: StorageBase::Capture(index as u32),
                            path: Vec::new(),
                        },
                        classes[0].clone(),
                    )),
                    _ if life.binder_count == 0 => {}
                    _ => {
                        return Err(pending(
                            "capture header needs full opaque/composite enumeration",
                        ));
                    }
                }
            }
        }
        let Some(header) = &f.storage_life else {
            if expected_bindings.is_empty()
                && expected_classes.is_empty()
                && instance.header_embedding.is_empty()
            {
                return Ok(());
            }
            return Err(bad("actual nonempty header class directory is unknown"));
        };
        self.pay(
            header
                .bindings
                .len()
                .saturating_add(header.classes.len())
                .saturating_add(instance.header_embedding.len()),
        )?;
        if header.bindings.len() != expected_bindings.len()
            || header.classes.len() != expected_classes.len()
            || !header.static_bindings.is_empty()
        {
            return Err(bad(
                "actual header omits or adds original class/borrow occurrences",
            ));
        }
        for (position, image) in expected_bindings {
            let mut rows = header.bindings.iter().filter(|r| r.position == position);
            let row = rows
                .next()
                .ok_or_else(|| bad("actual header omits an original borrow occurrence"))?;
            if rows.next().is_some() {
                return Err(bad("actual header repeats an original borrow occurrence"));
            }
            let actual = self.binding(instance, row.binder)?;
            self.same_class(instance.id, &actual, &image, &BTreeMap::new())?;
        }
        for (position, image) in expected_classes {
            let mut rows = header.classes.iter().filter(|r| r.position == position);
            let row = rows
                .next()
                .ok_or_else(|| bad("actual header omits an original intrinsic class"))?;
            if rows.next().is_some() {
                return Err(bad("actual header repeats an original intrinsic class"));
            }
            let actual = self.binding(instance, row.binder)?;
            self.same_class(instance.id, &actual, &image, &BTreeMap::new())?;
        }
        let fixed = instance
            .header_embedding
            .iter()
            .filter(|r| matches!(r.header, StorageBinder::Fixed(_)))
            .count();
        let invocation = instance.header_embedding.len() - fixed;
        if fixed != header.fixed_binder_count as usize
            || invocation != header.invocation_binder_count as usize
        {
            return Err(bad(
                "actual header changes its complete original binder domain",
            ));
        }
        Ok(())
    }
    fn invoke(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        sites: &[&SemanticSite],
        block: BlockId,
        index: usize,
        operand: SemanticOperand,
        value: &SemanticTypeValue,
        arguments: &[ValueId],
        values: &[Option<SemanticTypeValue>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        let value = if matches!(Self::type_of(value)?, Type::Address { .. }) {
            self.pointee(value)?
        } else {
            self.copy(value)?
        };
        let Type::Closure(id) = Self::type_of(&value)? else {
            return Err(pending(
                "borrowed callable needs its complete current pointee world",
            ));
        };
        let (d, child) = self.child(*id)?;
        let target = self.function(d.function)?;
        let target_original = self.definition(child.definition)?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &target_original.body
        else {
            return Err(bad("closure has no original code signature"));
        };
        let header = d
            .storage_life
            .as_ref()
            .ok_or_else(|| pending("called closure domain is unknown"))?;
        let mut selected = false;
        for site in self.point_sites(sites, block, index)? {
            if let SemanticSiteAction::BodyCall { actuals } = &site.action {
                if selected {
                    return Err(bad("callable has multiple original body-call sites"));
                }
                self.body_call(instance, original, site, actuals, child.definition)?;
                if !actuals.is_empty() {
                    return Err(pending(
                        "mixed callable actuals need complete child substitution",
                    ));
                }
                selected = true;
                continue;
            }
            if let SemanticSiteAction::CallableCall { callee: actual } = &site.action {
                if actual != &operand || selected {
                    return Err(bad("callable call changes its actual operand"));
                }
                let recipe = self.original_recipe(original, site.recipe)?;
                let SemanticSiteRecipeKind::CallableCall { callee: expected } = &recipe.kind else {
                    return Err(bad("callable call changes original recipe kind"));
                };
                let expected = self.ty(instance.id, expected, Self::type_of(&value)?)?;
                self.same(instance.id, &value, &expected, implicit)?;
                selected = true;
            }
        }
        if !selected {
            return Err(bad("actual callable call has no original operand site"));
        }
        if arguments.len() != signature.parameters.len()
            || target.parameters.len() != arguments.len()
        {
            return Err(bad("callable call changes original arity"));
        }
        for ((argument, recipe), local) in arguments
            .iter()
            .zip(&signature.parameters)
            .zip(&target.parameters)
        {
            let expected = self.ty(child.id, &recipe.ty, &target.locals[local.0 as usize].ty)?;
            let expected = self.map_value(&expected, &[], Some((header, child, &value)))?;
            let actual = self.get(values, *argument)?;
            self.same(instance.id, &actual, &expected, implicit)?;
        }
        let result = self.ty(child.id, &signature.result, &target.return_type)?;
        self.map_value(&result, &[], Some((header, child, &value)))
    }
    fn run(&mut self, instance: &SemanticInstance) -> R<()> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
        else {
            return Err(pending(
                "lexical scope instance requires its complete original implementation witness",
            ));
        };
        if instance.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid {
            return Err(pending(
                "partition requires full rigid-body substitution correspondence",
            ));
        }
        let f = self.function(id)?;
        let original = self.definition(instance.definition)?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &original.body
        else {
            return Err(bad("actual function changes original scope kind"));
        };
        if original.parameters.iter().any(|p| {
            matches!(
                p,
                SemanticFormal::Type { .. } | SemanticFormal::Const { .. }
            )
        }) {
            return Err(pending(
                "mixed generic body needs its complete original symbolic type/const equations",
            ));
        }
        if signature.parameters.len() != f.parameters.len()
            || signature.call_kind != f.call_kind
            || signature.result_mode != f.result_mode
        {
            return Err(bad(
                "actual function changes its complete original signature",
            ));
        }
        self.requirements(instance, original, None, &BTreeMap::new())?;
        if f.blocks.len() != 1 {
            return Err(pending(
                "class CFG requires its complete original reaching-world fixed point",
            ));
        }
        self.pay(
            self.program
                .sites
                .len()
                .saturating_add(f.locals.len())
                .saturating_add(f.values.len()),
        )?;
        let sites: Vec<_> = self
            .program
            .sites
            .iter()
            .filter(|s| s.instance == instance.id)
            .collect();
        let mut locals = vec![None; f.locals.len()];
        let mut values = vec![None; f.values.len()];
        let mut declared = BTreeMap::new();
        let mut implicit = BTreeMap::new();
        for (parameter, recipe) in f.parameters.iter().zip(&signature.parameters) {
            let ty = &f
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| bad("parameter is absent"))?
                .ty;
            let value = self.ty(instance.id, &recipe.ty, ty)?;
            locals[parameter.0 as usize] = Some(self.copy(&value)?);
            declared.insert(*parameter, value);
        }
        let expected_result = self.ty(instance.id, &signature.result, &f.return_type)?;
        self.check_header(instance, f, signature)?;
        let block = &f.blocks[0];
        for (index, instruction) in block.instructions.iter().enumerate() {
            self.constraints(
                instance,
                original,
                f,
                &sites,
                block.id,
                index,
                &values,
                &locals,
                &mut declared,
                &mut implicit,
            )?;
            self.pay(1)?;
            let ty = instruction
                .result
                .and_then(|id| f.values.get(id.0 as usize));
            let produced = match &instruction.kind {
                InstructionKind::Load(local) => Some(self.local(&locals, *local)?),
                InstructionKind::TakeLocal { local } => {
                    let value = self.local(&locals, *local)?;
                    locals[local.0 as usize] = None;
                    Some(value)
                }
                InstructionKind::Store { local, value } => {
                    let value = self.get(&values, *value)?;
                    if Self::type_of(&value)?
                        != &f
                            .locals
                            .get(local.0 as usize)
                            .ok_or_else(|| bad("store local is absent"))?
                            .ty
                    {
                        return Err(bad("store changes actual whole local type"));
                    }
                    if let Some(expected) = declared.get(local) {
                        self.same(instance.id, &value, expected, &implicit)?;
                    }
                    *locals
                        .get_mut(local.0 as usize)
                        .ok_or_else(|| bad("store local is absent"))? = Some(value);
                    None
                }
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                    *locals
                        .get_mut(local.0 as usize)
                        .ok_or_else(|| bad("activation local is absent"))? = None;
                    None
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    let inner = self.local(&locals, *local)?;
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("borrow has no result"))?;
                    let image = self.occurrence(f, result)?;
                    Some(self.address(
                        ty.ok_or_else(|| bad("borrow has no type"))?,
                        image,
                        &inner,
                    )?)
                }
                InstructionKind::ReborrowAddress { address, .. } => {
                    let value = self.get(&values, *address)?;
                    let SemanticTypeValue::Resolved {
                        mut life, classes, ..
                    } = value
                    else {
                        return Err(bad("Core reborrow type survives"));
                    };
                    let Type::Address { mutable, .. } =
                        ty.ok_or_else(|| bad("reborrow has no address type"))?
                    else {
                        return Err(bad("reborrow changes its outer constructor"));
                    };
                    let outer = life
                        .slots
                        .iter_mut()
                        .find(|row| row.path.is_empty())
                        .ok_or_else(|| bad("reborrow loses its real outer occurrence"))?;
                    outer.mode = if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    };
                    Some(self.resolved(
                        ty.ok_or_else(|| bad("reborrow has no type"))?,
                        life,
                        classes,
                    )?)
                }
                InstructionKind::ReadCapture { index } => Some(self.capture(instance, *index)?),
                InstructionKind::BorrowCapture { index, .. } => {
                    let value = self.capture(instance, *index)?;
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("capture borrow has no result"))?;
                    let image = self.occurrence(f, result)?;
                    Some(self.address(
                        ty.ok_or_else(|| bad("capture borrow has no type"))?,
                        image,
                        &value,
                    )?)
                }
                InstructionKind::ReadAddress { address } => {
                    let value = self.get(&values, *address)?;
                    Some(self.pointee(&value)?)
                }
                InstructionKind::AddressField {
                    address,
                    index: field,
                } => {
                    let parent = self.get(&values, *address)?;
                    let pointee = self.pointee(&parent)?;
                    let fields = match stored::physical(Self::type_of(&pointee)?) {
                        Type::Record { fields, .. } | Type::Package { fields, .. } => fields,
                        _ => {
                            return Err(pending(
                                "address field needs its complete actual product world",
                            ));
                        }
                    };
                    let field = &fields
                        .get(*field as usize)
                        .ok_or_else(|| bad("address field is outside actual product"))?
                        .1;
                    let value = self.zero(field)?;
                    let SemanticTypeValue::Resolved { classes, .. } = &parent else {
                        unreachable!()
                    };
                    let image = classes
                        .first()
                        .ok_or_else(|| bad("field projection loses its original head"))?
                        .clone();
                    Some(self.address(
                        ty.ok_or_else(|| bad("field address has no type"))?,
                        image,
                        &value,
                    )?)
                }
                InstructionKind::MakeAggregate { fields } => {
                    if matches!(
                        ty,
                        Some(
                            Type::BorrowedStorage { .. }
                                | Type::Record { .. }
                                | Type::Package { .. }
                        )
                    ) {
                        Some(
                            self.constructor(
                                instance,
                                original,
                                f,
                                &sites,
                                block.id,
                                index,
                                instruction
                                    .result
                                    .ok_or_else(|| bad("constructor has no result"))?,
                                fields,
                                &values,
                                &implicit,
                            )?,
                        )
                    } else {
                        Some(self.zero(ty.ok_or_else(|| bad("aggregate has no type"))?)?)
                    }
                }
                InstructionKind::MakeClosure { closure, captures } => {
                    let (d, child) = self.child(*closure)?;
                    if child.parent != Some(instance.id) || captures.len() != d.captures.len() {
                        return Err(bad(
                            "closure constructor changes its actual lexical creator/captures",
                        ));
                    }
                    let mut selected = None;
                    for site in self.point_sites(&sites, block.id, index)? {
                        if let SemanticSiteAction::Closure {
                            closure: SemanticClosureTarget::Typed(id),
                            environment,
                            ..
                        } = &site.action
                        {
                            if id != closure || selected.is_some() {
                                return Err(bad(
                                    "closure constructor changes its actual environment identity",
                                ));
                            }
                            let recipe = self.original_recipe(original, site.recipe)?;
                            let SemanticSiteRecipeKind::Closure {
                                definition,
                                environment: recipes,
                            } = &recipe.kind
                            else {
                                return Err(bad(
                                    "closure constructor changes original recipe kind",
                                ));
                            };
                            if *definition != child.definition || recipes.len() != environment.len()
                            {
                                return Err(bad("closure constructor omits original ghost slots"));
                            }
                            let expected = child
                                .environment
                                .as_ref()
                                .ok_or_else(|| bad("child ghost environment is absent"))?;
                            if environment.len() != expected.actuals.len() {
                                return Err(bad("child ghost environment omits complete actuals"));
                            }
                            let child_original = self.definition(child.definition)?;
                            if child_original.environment.len() != expected.actuals.len() {
                                return Err(bad(
                                    "original environment omits full mixed sealed slots",
                                ));
                            }
                            for (formal, claimed) in
                                child_original.environment.iter().zip(&expected.actuals)
                            {
                                let source = super::materialize::actual_at(
                                    self.program,
                                    self.module,
                                    self.ledger,
                                    instance.id,
                                    &formal.source,
                                    self.limits,
                                    self.left,
                                )?;
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    &source,
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    claimed,
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                                if &source != claimed {
                                    return Err(bad(
                                        "sealed environment actual changes its original creator-domain source",
                                    ));
                                }
                            }
                            for ((recipe, actual), claimed) in
                                recipes.iter().zip(environment).zip(&expected.actuals)
                            {
                                let value = super::materialize::actual_at(
                                    self.program,
                                    self.module,
                                    self.ledger,
                                    instance.id,
                                    recipe,
                                    self.limits,
                                    self.left,
                                )?;
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    &value,
                                    self.limits,
                                    self.left,
                                    2,
                                )?;
                                if &value != actual || &value != claimed {
                                    return Err(bad(
                                        "closure ghost slot changes its original creator actual",
                                    ));
                                }
                            }
                            selected = Some(());
                        }
                    }
                    if selected.is_none() {
                        return Err(bad("actual closure lacks its original creation site"));
                    }
                    let header = d
                        .storage_life
                        .as_ref()
                        .ok_or_else(|| pending("constructed closure full domain is unknown"))?;
                    let env = child
                        .environment
                        .as_ref()
                        .ok_or_else(|| bad("constructed closure has no sealed environment"))?;
                    let mut images = vec![None; header.fixed_binder_count as usize];
                    for binding in &child.header_embedding {
                        self.pay(1)?;
                        let StorageBinder::Fixed(fixed) = binding.header else {
                            continue;
                        };
                        let image = match &binding.actual {
                            SemanticClassExpr::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            } => {
                                let Some(SemanticActual::Life(image)) =
                                    env.actuals.get(*ordinal as usize)
                                else {
                                    return Err(bad(
                                        "constructed Fixed class selects non-Life slot",
                                    ));
                                };
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    image,
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                                image.clone()
                            }
                            SemanticClassExpr::IntrinsicClass {
                                operand: SemanticOperand::Environment(ordinal),
                                path,
                                class,
                            } if path.is_empty() => {
                                let Some(SemanticActual::Type(SemanticTypeValue::Resolved {
                                    life,
                                    classes,
                                    ..
                                })) = env.actuals.get(*ordinal as usize)
                                else {
                                    return Err(bad(
                                        "constructed Fixed class selects non-Type slot",
                                    ));
                                };
                                if life.binder_count as usize != classes.len() {
                                    return Err(bad(
                                        "constructed Type environment omits complete classes",
                                    ));
                                }
                                let image = classes.get(*class as usize).ok_or_else(|| {
                                    bad("constructed Fixed class exceeds sealed Type domain")
                                })?;
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    image,
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                                image.clone()
                            }
                            _ => {
                                return Err(pending(
                                    "constructed implicit capture requires its complete actual occurrence seal",
                                ));
                            }
                        };
                        *images
                            .get_mut(fixed as usize)
                            .ok_or_else(|| bad("Fixed image exceeds actual domain"))? = Some(image);
                    }
                    let images = images
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| bad("constructed closure omits actual Fixed images"))?;
                    let closure_value = self.resolved(
                        ty.ok_or_else(|| bad("closure constructor has no type"))?,
                        StorageLifeShape {
                            binder_count: header.fixed_binder_count,
                            ..empty()
                        },
                        images,
                    )?;
                    for (capture, source) in captures.iter().enumerate() {
                        let expected = self.capture(child, capture as u32)?;
                        let expected =
                            self.map_value(&expected, &[], Some((header, child, &closure_value)))?;
                        let actual = self.get(&values, *source)?;
                        self.same(instance.id, &actual, &expected, &implicit)?;
                    }
                    Some(closure_value)
                }
                InstructionKind::Call {
                    function: target,
                    arguments,
                } => {
                    self.pay(self.program.instances.len())?;
                    let mut candidates = self.program.instances.iter().filter(|i| matches!(i.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == *target));
                    let callee = candidates
                        .next()
                        .ok_or_else(|| bad("actual call has no original callee world"))?;
                    if candidates.next().is_some() {
                        return Err(bad("actual call has ambiguous original callee worlds"));
                    }
                    let target_function = self.function(*target)?;
                    let target_original = self.definition(callee.definition)?;
                    let (SemanticDefinitionBody::Function {
                        signature: target_signature,
                        ..
                    }
                    | SemanticDefinitionBody::FunctionWithBody {
                        signature: target_signature,
                        ..
                    }) = &target_original.body
                    else {
                        return Err(bad("call target changes original scope kind"));
                    };
                    let mut selected = None;
                    for site in self.point_sites(&sites, block.id, index)? {
                        let actuals = match &site.action {
                            SemanticSiteAction::BodyCall { actuals } => {
                                self.body_call(
                                    instance,
                                    original,
                                    site,
                                    actuals,
                                    callee.definition,
                                )?;
                                actuals
                            }
                            SemanticSiteAction::Call {
                                target: SemanticFunctionTarget::Typed(target_id),
                                actuals,
                            } => {
                                if target_id != target {
                                    return Err(bad("call changes actual callee"));
                                }
                                let recipe = self.original_recipe(original, site.recipe)?;
                                let SemanticSiteRecipeKind::Call {
                                    target: definition,
                                    actuals: recipes,
                                } = &recipe.kind
                                else {
                                    return Err(bad("call changes original recipe kind"));
                                };
                                if *definition != callee.definition
                                    || recipes.len() != actuals.len()
                                {
                                    return Err(bad("call omits original actuals"));
                                }
                                for (recipe, actual) in recipes.iter().zip(actuals) {
                                    let expected = super::materialize::actual_at(
                                        self.program,
                                        self.module,
                                        self.ledger,
                                        instance.id,
                                        recipe,
                                        self.limits,
                                        self.left,
                                    )?;
                                    crate::dyn_metadata::semantic_operand_snapshot(
                                        actual,
                                        self.limits,
                                        self.left,
                                        1,
                                    )?;
                                    crate::dyn_metadata::semantic_operand_snapshot(
                                        &expected,
                                        self.limits,
                                        self.left,
                                        1,
                                    )?;
                                    if actual != &expected {
                                        return Err(bad(
                                            "call changes original mixed actual expression",
                                        ));
                                    }
                                }
                                actuals
                            }
                            _ => continue,
                        };
                        if selected.is_some() || actuals.len() != target_original.parameters.len() {
                            return Err(bad("call has repeated or incomplete actual world"));
                        }
                        {
                            self.requirements(instance, target_original, Some(actuals), &implicit)?;
                            if arguments.len() != target_signature.parameters.len()
                                || arguments.len() != target_function.parameters.len()
                            {
                                return Err(bad("call omits actual original parameters"));
                            }
                            for ((argument, recipe), local) in arguments
                                .iter()
                                .zip(&target_signature.parameters)
                                .zip(&target_function.parameters)
                            {
                                let expected = self.ty(
                                    callee.id,
                                    &recipe.ty,
                                    &target_function.locals[local.0 as usize].ty,
                                )?;
                                let expected = self.map_value(&expected, actuals, None)?;
                                let value = self.get(&values, *argument)?;
                                self.same(instance.id, &value, &expected, &implicit)?;
                            }
                            let result = self.ty(
                                callee.id,
                                &target_signature.result,
                                &target_function.return_type,
                            )?;
                            selected = Some(self.map_value(&result, actuals, None)?);
                        }
                    }
                    Some(
                        selected
                            .ok_or_else(|| bad("actual call has no original full-actual site"))?,
                    )
                }
                InstructionKind::CallClosure { callee, arguments }
                | InstructionKind::CallClosureOnce { callee, arguments }
                | InstructionKind::CallClosureBorrowed {
                    callee, arguments, ..
                } => {
                    let value = self.get(&values, *callee)?;
                    Some(self.invoke(
                        instance,
                        original,
                        &sites,
                        block.id,
                        index,
                        SemanticOperand::Value(*callee),
                        &value,
                        arguments,
                        &values,
                        &implicit,
                    )?)
                }
                InstructionKind::CallClosureShared { callee, arguments }
                | InstructionKind::CallClosureMut { callee, arguments } => {
                    let value = self.local(&locals, *callee)?;
                    Some(self.invoke(
                        instance,
                        original,
                        &sites,
                        block.id,
                        index,
                        SemanticOperand::Local(*callee),
                        &value,
                        arguments,
                        &values,
                        &implicit,
                    )?)
                }
                InstructionKind::DropClosure { local } => {
                    let value = self.local(&locals, *local)?;
                    if !matches!(Self::type_of(&value)?, Type::Closure(_)) {
                        return Err(bad("closure drop targets another actual type"));
                    }
                    locals[local.0 as usize] = None;
                    None
                }
                InstructionKind::ExtractField { aggregate, index } => {
                    let value = self.get(&values, *aggregate)?;
                    Some(self.field_value(&value, *index)?)
                }
                InstructionKind::ReadPackageField { local, index } => {
                    let value = self.local(&locals, *local)?;
                    Some(self.field_value(&value, *index)?)
                }
                InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. } => None,
                InstructionKind::Constant(_) => {
                    Some(self.zero(ty.ok_or_else(|| bad("constant has no actual type"))?)?)
                }
                // These scalar operations preserve no lifetime or nominal
                // class. Their endpoint/operator typing already ran in Builder.
                InstructionKind::Binary { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::ConvertNumber { .. }
                    if ty.is_some_and(Type::is_b0_scalar) =>
                {
                    Some(self.zero(ty.ok_or_else(|| bad("scalar producer has no type"))?)?)
                }
                _ => {
                    return Err(pending(
                        "actual operation requires its complete original class transfer",
                    ));
                }
            };
            if let Some(result) = instruction.result {
                let value = produced
                    .ok_or_else(|| bad("actual SSA producer has no complete class equation"))?;
                if Self::type_of(&value)?
                    != f.values
                        .get(result.0 as usize)
                        .ok_or_else(|| bad("actual SSA result is absent"))?
                {
                    return Err(bad(
                        "actual SSA type changes its whole original class value",
                    ));
                }
                *values
                    .get_mut(result.0 as usize)
                    .ok_or_else(|| bad("SSA class result is absent"))? = Some(value);
            } else if produced.is_some() {
                return Err(bad("class-producing operation has no actual SSA result"));
            }
        }
        self.constraints(
            instance,
            original,
            f,
            &sites,
            block.id,
            block.instructions.len(),
            &values,
            &locals,
            &mut declared,
            &mut implicit,
        )?;
        match block.terminator {
            Terminator::Return(Some(result)) => {
                let value = self.get(&values, result)?;
                self.same(instance.id, &value, &expected_result, &implicit)?;
            }
            Terminator::Return(None) if f.return_type == Type::Unit => {}
            Terminator::Trap => {}
            _ => {
                return Err(pending(
                    "original CFG terminator needs its complete class-flow equation",
                ));
            }
        }
        Ok(())
    }
}
pub(super) fn program(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut world = World {
        program,
        module,
        ledger,
        limits,
        left,
    };
    world.pay(
        program
            .instances
            .len()
            .saturating_add(module.functions.len()),
    )?;
    let mut targets = BTreeMap::new();
    for instance in &program.instances {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(pending(
                "original scope instance needs its complete actual provider proof",
            ));
        };
        if targets.insert(function, instance.id).is_some() {
            return Err(bad("actual function has multiple original worlds"));
        }
        let original = world.definition(instance.definition)?;
        if original.parameters.len() != instance.actuals.len() {
            return Err(bad("world omits original mixed actuals"));
        }
        for (ordinal, (formal, actual)) in original
            .parameters
            .iter()
            .zip(&instance.actuals)
            .enumerate()
        {
            world.pay(1)?;
            match (formal, actual) {
                (SemanticFormal::Lifetime, SemanticActual::Life(actual)) => {
                    let expected = super::materialize::class_at(
                        program,
                        module,
                        ledger,
                        instance.id,
                        &SemanticLifeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: ordinal as u32,
                        }),
                        limits,
                        world.left,
                    )?;
                    if actual != &expected {
                        return Err(bad(
                            "rigid original world was replaced by a caller's lifetime coincidence",
                        ));
                    }
                }
                _ => {
                    return Err(pending(
                        "mixed Type/Const original world needs its complete rigid interpretation",
                    ));
                }
            }
        }
        world.run(instance)?;
    }
    let mut constructed = std::collections::BTreeSet::new();
    for function in &module.functions {
        world.pay(function.blocks.len())?;
        for block in &function.blocks {
            world.pay(block.instructions.len())?;
            for instruction in &block.instructions {
                if let InstructionKind::MakeClosure { closure, .. } = instruction.kind {
                    constructed.insert(closure);
                }
            }
        }
    }
    for definition in &module.closures {
        world.pay(1)?;
        if !constructed.contains(&definition.id) {
            return Err(bad(
                "actual closure domain has no real constructor in the complete program",
            ));
        }
    }
    for function in &module.functions {
        world.pay(1)?;
        if !targets.contains_key(&function.id) {
            return Err(bad("actual function has no complete original world"));
        }
    }
    Ok(())
}
