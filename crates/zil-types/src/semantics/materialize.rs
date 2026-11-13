//! Resolve Core-only operands against the actual emitted function and value.
//! Resulting operands remain untrusted. This is not the original W, body class
//! flow, environment seal or rigid/partition proof at the publication boundary.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeSet;

fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-MATERIALIZE", Span::default(), message)
}
fn unknown(message: &str) -> Diagnostic {
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
fn resolved(
    ty: Type,
    life: StorageLifeShape,
    classes: Vec<SemanticClassExpr>,
) -> SemanticTypeValue {
    SemanticTypeValue::Resolved { ty, life, classes }
}
struct Eval<'a, 'b> {
    input: &'a SemanticModule,
    module: &'a TypedModule,
    ledger: &'a Ledger,
    limits: &'b Limits,
    left: &'b mut usize,
    active: BTreeSet<(SemanticInstanceId, u32)>,
}
impl<'a> Eval<'a, '_> {
    fn fee(&mut self, count: usize) -> R<()> {
        *self.left = self.left.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "semantic type materialization work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "semantic type materialization depth exceeded",
                None,
            ));
        }
        self.fee(1)
    }
    fn instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        self.fee(self.input.instances.len())?;
        self.input
            .instances
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| bad("materialized operand has no real semantic instance"))
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.fee(self.input.definitions.len())?;
        self.input
            .definitions
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| bad("materialized operand lost its original declaration"))
    }
    fn ancestor(
        &mut self,
        mut id: SemanticInstanceId,
        hops: SemanticScopeRef,
        depth: usize,
    ) -> R<SemanticInstanceId> {
        let SemanticScopeRef::Ancestor(hops) = hops;
        for step in 0..hops as usize {
            self.node(depth.saturating_add(step))?;
            id = self
                .instance(id)?
                .parent
                .ok_or_else(|| bad("materialized type crosses its lexical root"))?;
        }
        Ok(id)
    }
    fn function(&mut self, scope: SemanticInstanceId) -> R<&'a TypedFunction> {
        let instance = self.instance(scope)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
        else {
            return Err(bad(
                "runtime type operand is outside its actual emitted function",
            ));
        };
        self.fee(self.module.functions.len())?;
        self.module
            .functions
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| bad("materialized type has no actual Typed function"))
    }
    fn copy(&mut self, value: &SemanticTypeValue) -> R<SemanticTypeValue> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        Ok(value.clone())
    }
    fn physical(&mut self, ty: &Type) -> R<Type> {
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
        Ok(ty.clone())
    }
    fn operand(
        &mut self,
        scope: SemanticInstanceId,
        operand: &SemanticOperand,
    ) -> R<SemanticOperand> {
        if let SemanticOperand::CoreExpression {
            expression,
            occurrence: 0,
        } = operand
        {
            let f = self.function(scope)?;
            self.fee(self.ledger.frames.len().saturating_add(1).ilog2() as usize + 1)?;
            let frame = self
                .ledger
                .frames
                .get(&f.id)
                .ok_or_else(|| bad("type operand has no actual emitter frame"))?;
            self.fee(frame.expressions.len().saturating_add(1).ilog2() as usize + 1)?;
            let row = frame
                .expressions
                .get(expression)
                .ok_or_else(|| bad("type operand expression was not emitted"))?;
            row.value
                .map(SemanticOperand::Value)
                .or_else(|| row.place.clone())
                .ok_or_else(|| bad("type operand expression has no actual value or place"))
        } else if matches!(operand, SemanticOperand::CoreExpression { .. }) {
            Err(bad("type operand occurrence is not a real Core mint"))
        } else {
            Ok(operand.clone())
        }
    }
    fn class(
        &mut self,
        scope: SemanticInstanceId,
        class: &SemanticClassExpr,
    ) -> R<SemanticClassExpr> {
        crate::dyn_metadata::semantic_operand_snapshot(class, self.limits, self.left, 1)?;
        let mut class = class.clone();
        if let SemanticClassExpr::OuterOccurrence { operand, .. }
        | SemanticClassExpr::IntrinsicClass { operand, .. } = &mut class
        {
            *operand = self.operand(scope, operand)?;
        }
        Ok(class)
    }
    fn recipe_operand(&mut self, operand: &SemanticRecipeOperand) -> R<SemanticOperand> {
        self.fee(1)?;
        Ok(match operand {
            SemanticRecipeOperand::TypeFormal(formal) => SemanticOperand::TypeArgument {
                scope: formal.scope,
                ordinal: formal.ordinal,
            },
            SemanticRecipeOperand::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => SemanticOperand::Environment(*ordinal),
            SemanticRecipeOperand::Parameter(index) => SemanticOperand::Parameter(*index),
            SemanticRecipeOperand::Receiver => SemanticOperand::Receiver,
            SemanticRecipeOperand::Environment { .. } => {
                return Err(unknown(
                    "ancestor Type environment requires its checked sealed whole-type map",
                ));
            }
        })
    }
    fn life(
        &mut self,
        scope: SemanticInstanceId,
        life: &SemanticLifeRecipe,
    ) -> R<SemanticClassExpr> {
        self.fee(1)?;
        Ok(match life {
            SemanticLifeRecipe::Formal(reference) => SemanticClassExpr::Formal {
                scope: reference.scope,
                ordinal: reference.ordinal,
            },
            SemanticLifeRecipe::Implicit { scope, ordinal } => SemanticClassExpr::Implicit {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Environment { scope, ordinal } => SemanticClassExpr::Environment {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Bound { depth, index } => SemanticClassExpr::Bound {
                depth: *depth,
                index: *index,
            },
            SemanticLifeRecipe::Invocation(index) => {
                let instance = self.instance(scope)?;
                self.fee(instance.header_embedding.len())?;
                let mut rows = instance
                    .header_embedding
                    .iter()
                    .filter(|row| row.header == StorageBinder::Invocation(*index));
                let row = rows.next().ok_or_else(|| {
                    unknown("invocation type needs its complete actual header embedding")
                })?;
                if rows.next().is_some() {
                    return Err(bad("invocation header embedding is ambiguous"));
                }
                self.class(scope, &row.actual)?
            }
            SemanticLifeRecipe::OuterOccurrence {
                operand,
                path,
                mode,
            } => {
                self.fee(path.len())?;
                SemanticClassExpr::OuterOccurrence {
                    operand: self.recipe_operand(operand)?,
                    path: path.clone(),
                    mode: *mode,
                }
            }
            SemanticLifeRecipe::IntrinsicClass {
                operand,
                path,
                class,
            } => {
                self.fee(path.len())?;
                SemanticClassExpr::IntrinsicClass {
                    operand: self.recipe_operand(operand)?,
                    path: path.clone(),
                    class: *class,
                }
            }
        })
    }
    fn type_actual(
        &mut self,
        scope: SemanticInstanceId,
        ordinal: u32,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        self.fee(self.active.len().saturating_add(1).ilog2() as usize + 1)?;
        if !self.active.insert((scope, ordinal)) {
            return Err(bad(
                "whole-type actual depends cyclically on its own asserted class table",
            ));
        }
        let instance = self.instance(scope)?;
        let value = instance
            .actuals
            .get(ordinal as usize)
            .ok_or_else(|| bad("whole-type actual omits its original full ordinal"))?;
        let value = match value {
            SemanticActual::Type(value) => self.value(scope, value, None, depth + 1)?,
            SemanticActual::CoreRecipe { scope, recipe } => match recipe.as_ref() {
                SemanticActualRecipe::Type(recipe) => self.ty(*scope, recipe, None, depth + 1)?,
                _ => return Err(bad("Type formal selects a non-Type actual")),
            },
            _ => return Err(bad("Type formal selects a non-Type actual")),
        };
        self.active.remove(&(scope, ordinal));
        let SemanticTypeValue::Resolved { ty, life, .. } = value else {
            unreachable!()
        };
        self.fee(life.binder_count as usize)?;
        let classes = (0..life.binder_count)
            .map(|class| SemanticClassExpr::IntrinsicClass {
                operand: SemanticOperand::TypeArgument {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                },
                path: Vec::new(),
                class,
            })
            .collect();
        Ok(resolved(ty, life, classes))
    }
    fn value(
        &mut self,
        owner: SemanticInstanceId,
        value: &SemanticTypeValue,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        match value {
            SemanticTypeValue::Resolved { ty, life, classes } => {
                if life.binder_count as usize != classes.len() {
                    return Err(bad("whole-type class directory omits intrinsic classes"));
                }
                if let Some(hint) = hint {
                    inference::charge_type_snapshot(
                        ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    inference::charge_type_snapshot(
                        hint,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if ty != hint {
                        return Err(bad(
                            "resolved type does not match its actual emitted operand",
                        ));
                    }
                }
                let mut value = self.copy(value)?;
                if let SemanticTypeValue::Resolved { ty, life, classes } = &mut value {
                    if let Type::BorrowedStorage { life: actual, .. } = ty {
                        if actual != life {
                            return Err(bad(
                                "resolved type domain contradicts its actual qualified type",
                            ));
                        }
                    }
                    for class in classes {
                        if let SemanticClassExpr::OuterOccurrence { operand, .. }
                        | SemanticClassExpr::IntrinsicClass { operand, .. } = class
                        {
                            *operand = self.operand(owner, operand)?;
                        }
                    }
                }
                Ok(value)
            }
            SemanticTypeValue::Core { scope, source } => match source {
                SemanticCoreTypeSource::OriginalRecipe(recipe) => {
                    self.ty(*scope, recipe, hint, depth + 1)
                }
                SemanticCoreTypeSource::Expression {
                    module,
                    function,
                    expression,
                    occurrence: 0,
                    recipe,
                } => {
                    let f = self.function(*scope)?;
                    self.fee(self.ledger.frames.len().saturating_add(1).ilog2() as usize + 1)?;
                    let frame = self
                        .ledger
                        .frames
                        .get(&f.id)
                        .ok_or_else(|| bad("inferred type has no actual emitter owner"))?;
                    if frame.origin.module != *module || frame.origin.creator != *function {
                        return Err(bad("inferred type selects another actual Core owner"));
                    }
                    self.fee(frame.expressions.len().saturating_add(1).ilog2() as usize + 1)?;
                    let row = frame
                        .expressions
                        .get(expression)
                        .ok_or_else(|| bad("inferred whole-type expression was not emitted"))?;
                    let actual = if let Some(value) = row.value {
                        f.values.get(value.0 as usize)
                    } else {
                        match row.place.as_ref() {
                            Some(SemanticOperand::Local(local)) => {
                                f.locals.get(local.0 as usize).map(|l| &l.ty)
                            }
                            Some(SemanticOperand::Capture(index)) => f
                                .environment
                                .and_then(|id| self.module.closures.iter().find(|d| d.id == id))
                                .and_then(|d| d.captures.get(*index as usize)),
                            _ => None,
                        }
                    }
                    .ok_or_else(|| {
                        bad("inferred type expression has no actual complete value/place type")
                    })?;
                    if let Some(hint) = hint {
                        inference::charge_type_snapshot(
                            actual,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                        inference::charge_type_snapshot(
                            hint,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if actual != hint {
                            return Err(bad("inferred type and actual site type disagree"));
                        }
                    }
                    self.ty(*scope, recipe, Some(actual), depth + 1)
                }
                SemanticCoreTypeSource::Expression { .. } => {
                    Err(bad("inferred whole-type occurrence is not a real mint"))
                }
            },
        }
    }
    fn constant(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticConstRecipe,
        carrier: &Type,
        depth: usize,
    ) -> R<Constant> {
        self.node(depth)?;
        let result = match recipe {
            SemanticConstRecipe::Value(value) => {
                self.fee(match value {
                    Constant::WideString(units) => units.len().saturating_add(1),
                    _ => 1,
                })?;
                value.clone()
            }
            SemanticConstRecipe::IntegerLiteral(text) => {
                self.fee(text.len().saturating_mul(3).saturating_add(1))?;
                let (text, negative) = text
                    .strip_prefix('-')
                    .map_or((text.as_str(), false), |value| (value, true));
                integer(text, Some(carrier), negative, Span::default())?
            }
            SemanticConstRecipe::FloatLiteral(text) => {
                self.fee(text.len().saturating_mul(3).saturating_add(1))?;
                parse_float_literal(text, Some(carrier), Span::default())
                    .map_err(|mut errors| errors.remove(0))?
            }
            SemanticConstRecipe::Formal(reference) => {
                let owner = self.ancestor(scope, reference.scope, depth + 1)?;
                let actual = self
                    .instance(owner)?
                    .actuals
                    .get(reference.ordinal as usize)
                    .ok_or_else(|| bad("const actual omits its original full ordinal"))?;
                let actual = self.actual(owner, actual, depth + 1)?;
                let SemanticActual::Const { value, .. } = actual else {
                    return Err(bad("Const formal selects a different actual kind"));
                };
                value
            }
            _ => {
                return Err(unknown(
                    "semantic const expression needs its complete carrier evaluation",
                ));
            }
        };
        if result.ty() != *carrier {
            return Err(bad(
                "semantic constant differs from its resolved actual carrier",
            ));
        }
        Ok(result)
    }
    fn nominal(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        args: &[SemanticActualRecipe],
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        let original = self.definition(definition)?;
        let SemanticDefinitionBody::Nominal {
            declaration, kind, ..
        } = &original.body
        else {
            return Err(bad("nominal type recipe selects another original kind"));
        };
        if args.len() != original.parameters.len() {
            return Err(bad("nominal actuals omit original mixed positions"));
        }
        self.fee(args.len().saturating_mul(2))?;
        let mut actuals = Vec::new();
        let mut images = Vec::new();
        for (argument, formal) in args.iter().zip(&original.parameters) {
            match (argument, formal) {
                (SemanticActualRecipe::Life(life), SemanticFormal::Lifetime) => {
                    let image = self.life(scope, life)?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &image,
                        self.limits,
                        self.left,
                        images.len().saturating_add(1),
                    )?;
                    let ordinal = images
                        .iter()
                        .position(|old| old == &image)
                        .unwrap_or(images.len());
                    if ordinal == images.len() {
                        images.push(image);
                    }
                    actuals.push(LifeNominalActual::Lifetime(
                        u32::try_from(ordinal)
                            .map_err(|_| bad("nominal class ordinal overflows"))?,
                    ));
                }
                (SemanticActualRecipe::Type(recipe), SemanticFormal::Type { .. }) => {
                    let SemanticTypeValue::Resolved { ty, life, .. } =
                        self.ty(scope, recipe, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    if life.binder_count != 0 {
                        return Err(unknown(
                            "nominal whole-Type actual requires its complete retained class composition",
                        ));
                    }
                    actuals.push(LifeNominalActual::Type(ty));
                }
                (SemanticActualRecipe::Const { carrier, value }, SemanticFormal::Const { .. }) => {
                    let SemanticTypeValue::Resolved { ty, life, .. } =
                        self.ty(scope, carrier, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    if life.binder_count != 0 {
                        return Err(bad("const carrier retains a free semantic domain"));
                    }
                    actuals.push(LifeNominalActual::Const(self.constant(
                        scope,
                        value,
                        &ty,
                        depth + 1,
                    )?));
                }
                _ => {
                    return Err(bad(
                        "nominal actual kind differs at an original mixed position",
                    ));
                }
            }
        }
        self.fee(self.module.life_instances.len())?;
        let mut found = None;
        for instance in &self.module.life_instances {
            if instance.declaration != *declaration {
                continue;
            }
            inference::charge_type_snapshot(
                &instance.ty,
                Span::default(),
                self.limits,
                self.left,
                1,
            )?;
            self.fee(instance.arguments.len().saturating_add(actuals.len()))?;
            for argument in instance.arguments.iter().chain(actuals.iter()) {
                match argument {
                    LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. } => {
                        inference::charge_type_snapshot(
                            ty,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                    }
                    LifeNominalActual::Const(Constant::WideString(units)) => {
                        self.fee(units.len())?
                    }
                    _ => self.fee(1)?,
                }
                if let LifeNominalActual::QualifiedType { life, .. } = argument {
                    self.fee(
                        life.slots
                            .len()
                            .saturating_add(life.retained.len())
                            .saturating_add(life.nominal_arguments.len()),
                    )?;
                    for path in life
                        .slots
                        .iter()
                        .map(|row| row.path.len())
                        .chain(life.retained.iter().map(|row| row.path.len()))
                        .chain(life.nominal_arguments.iter().map(|row| row.path.len()))
                    {
                        self.fee(path)?;
                    }
                }
            }
            if instance.arguments != actuals {
                continue;
            }
            if let Some(hint) = hint {
                inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
                if &instance.ty != hint {
                    continue;
                }
            }
            if found.replace(&instance.ty).is_some() {
                return Err(bad(
                    "nominal declaration and actuals select ambiguous complete types",
                ));
            }
        }
        let ty = found.ok_or_else(|| {
            unknown("nominal recipe needs its actual complete constructor or declaration instance")
        })?;
        let physical = stored::physical(ty);
        let actual_kind = match physical {
            Type::Record { .. } => LifeNominalKind::Record,
            Type::Package { .. } => LifeNominalKind::Package,
            Type::Enum { .. } => LifeNominalKind::Enum,
            Type::Managed(_) => LifeNominalKind::Resource,
            _ => return Err(bad("nominal recipe has another actual type constructor")),
        };
        if actual_kind != *kind {
            return Err(bad("nominal recipe changes its original ownership kind"));
        }
        let life = match ty {
            Type::BorrowedStorage { life, .. } => {
                if !life.slots.is_empty() || !life.retained.is_empty() {
                    return Err(unknown(
                        "nominal data classes require full stored and opaque path composition",
                    ));
                }
                self.fee(
                    life.nominal_arguments
                        .len()
                        .saturating_add(life.binder_count as usize),
                )?;
                let mut classes = vec![None; life.binder_count as usize];
                for row in &life.nominal_arguments {
                    self.fee(row.path.len().saturating_add(1))?;
                    if !row.path.is_empty() || row.declaration != *declaration {
                        return Err(unknown(
                            "nested nominal class requires its exact original type-use traversal",
                        ));
                    }
                    let Some(SemanticActualRecipe::Life(recipe)) = args.get(row.argument as usize)
                    else {
                        return Err(bad(
                            "nominal phantom row selects a non-Life original actual",
                        ));
                    };
                    let image = self.life(scope, recipe)?;
                    let target = classes
                        .get_mut(row.binder as usize)
                        .ok_or_else(|| bad("nominal phantom binder exceeds its actual domain"))?;
                    if target.as_ref().is_some_and(|old| old != &image) {
                        return Err(unknown(
                            "fresh nominal canonicalization requires the original exact lifetime equations",
                        ));
                    }
                    *target = Some(image);
                }
                let classes: Vec<_> =
                    classes.into_iter().collect::<Option<_>>().ok_or_else(|| {
                        bad("nominal complete domain contains an unaccounted phantom class")
                    })?;
                let value = resolved(self.physical(ty)?, life.clone(), classes);
                return Ok(value);
            }
            _ if images.is_empty() => empty(),
            _ => {
                return Err(bad(
                    "nominal phantom domain was erased from the actual Typed type",
                ));
            }
        };
        Ok(resolved(self.physical(ty)?, life, Vec::new()))
    }
    fn ty(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        let value = match recipe {
            SemanticTypeRecipe::Atom(ty) => resolved(self.physical(ty)?, empty(), Vec::new()),
            SemanticTypeRecipe::Exact(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    value.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                let mut classes = Vec::new();
                for image in &value.classes {
                    classes.push(self.life(scope, image)?);
                }
                resolved(self.physical(&value.ty)?, value.life.clone(), classes)
            }
            SemanticTypeRecipe::Formal(reference) => {
                let owner = self.ancestor(scope, reference.scope, depth + 1)?;
                let mut value = self.type_actual(owner, reference.ordinal, depth + 1)?;
                if let SemanticTypeValue::Resolved { classes, .. } = &mut value {
                    for image in classes {
                        if let SemanticClassExpr::IntrinsicClass {
                            operand: SemanticOperand::TypeArgument { scope, .. },
                            ..
                        } = image
                        {
                            *scope = reference.scope;
                        }
                    }
                }
                value
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => self.nominal(scope, *definition, arguments, hint, depth + 1)?,
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                let SemanticTypeValue::Resolved {
                    ty: inner,
                    life: inner_life,
                    classes: inner_classes,
                } = self.ty(scope, inner, None, depth + 1)?
                else {
                    unreachable!()
                };
                let is_address = matches!(recipe, SemanticTypeRecipe::Address { .. });
                let mutable = match recipe {
                    SemanticTypeRecipe::Address { mutable, .. } => *mutable,
                    _ => true,
                };
                if matches!(lifetime, SemanticOuterLifeRecipe::Static) && mutable {
                    return Err(bad("Static outer occurrence cannot grant mutable access"));
                }
                let mut classes = Vec::new();
                let mut life = empty();
                if let SemanticOuterLifeRecipe::Class(image) = lifetime {
                    classes.push(self.life(scope, image)?);
                    life.slots.push(StorageLifeSlot {
                        path: Vec::new(),
                        binder: 0,
                        mode: if is_address {
                            if mutable {
                                StoredBorrowMode::ExclusiveAddress
                            } else {
                                StoredBorrowMode::SharedAddress
                            }
                        } else {
                            StoredBorrowMode::WritableDescription
                        },
                    });
                }
                let offset = classes.len() as u32;
                self.fee(inner_classes.len().saturating_mul(2))?;
                for (class, image) in inner_classes.into_iter().enumerate() {
                    life.retained.push(StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee],
                        class: class as u32,
                        binder: offset + class as u32,
                    });
                    classes.push(image);
                }
                if inner_life.binder_count as usize + offset as usize != classes.len() {
                    return Err(bad("pointee complete class inventory is inconsistent"));
                }
                life.binder_count = u32::try_from(classes.len())
                    .map_err(|_| bad("outer class inventory overflows"))?;
                resolved(
                    if is_address {
                        Type::Address {
                            mutable,
                            inner: Box::new(inner),
                        }
                    } else {
                        Type::Desc(Box::new(inner))
                    },
                    life,
                    classes,
                )
            }
            SemanticTypeRecipe::Slice(inner) => {
                let SemanticTypeValue::Resolved {
                    ty,
                    life: child,
                    classes,
                } = self.ty(scope, inner, None, depth + 1)?
                else {
                    unreachable!()
                };
                self.fee(classes.len())?;
                let mut life = empty();
                life.binder_count = child.binder_count;
                life.retained = (0..child.binder_count)
                    .map(|class| StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Element],
                        class,
                        binder: class,
                    })
                    .collect();
                resolved(
                    Type::Slice {
                        element: Box::new(ty),
                    },
                    life,
                    classes,
                )
            }
            SemanticTypeRecipe::Tuple(items) => {
                self.fee(items.len())?;
                let mut fields = Vec::new();
                for item in items {
                    let SemanticTypeValue::Resolved { ty, life, .. } =
                        self.ty(scope, item, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    if life.binder_count != 0 {
                        return Err(unknown(
                            "compound type needs its full child class composition",
                        ));
                    }
                    fields.push(ty);
                }
                resolved(Type::Tuple(fields), empty(), Vec::new())
            }
            SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::OwnedStore(inner) => {
                let SemanticTypeValue::Resolved { ty, life, classes } =
                    self.ty(scope, inner, None, depth + 1)?
                else {
                    unreachable!()
                };
                if life.binder_count != 0 {
                    return Err(unknown(
                        "owner carrier requires its complete child domain projection",
                    ));
                }
                resolved(
                    if matches!(recipe, SemanticTypeRecipe::Managed(_)) {
                        Type::Managed(Box::new(ty))
                    } else {
                        Type::OwnedStore {
                            element: Box::new(ty),
                        }
                    },
                    life,
                    classes,
                )
            }
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment,
            } => {
                let declaration = self.definition(*definition)?;
                if arguments.len() != declaration.parameters.len()
                    || environment.len() != declaration.environment.len()
                {
                    return Err(bad(
                        "closure recipe omits original mixed arguments or ghost slots",
                    ));
                }
                let Some(Type::Closure(id)) = hint else {
                    return Err(unknown(
                        "closure recipe needs its actual emitted environment identity",
                    ));
                };
                self.fee(
                    self.module
                        .closures
                        .len()
                        .saturating_add(self.input.instances.len()),
                )?;
                let closure = self
                    .module
                    .closures
                    .iter()
                    .find(|row| row.id == *id)
                    .ok_or_else(|| bad("whole closure type has no actual definition"))?;
                let mut children = self.input.instances.iter().filter(|child| child.definition == *definition && matches!(child.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(f)) if f == closure.function));
                let child = children
                    .next()
                    .ok_or_else(|| bad("closure recipe has no matching actual original child"))?;
                if children.next().is_some() {
                    return Err(bad("closure recipe has ambiguous actual child worlds"));
                }
                let life = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("whole closure type has an unknown actual domain"))?;
                self.fee(life.fixed_binder_count as usize + child.header_embedding.len())?;
                let mut images = vec![None; life.fixed_binder_count as usize];
                for row in &child.header_embedding {
                    let StorageBinder::Fixed(fixed) = row.header else {
                        continue;
                    };
                    let SemanticClassExpr::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    } = row.actual
                    else {
                        return Err(unknown(
                            "whole closure implicit head needs its actual occurrence image at this use",
                        ));
                    };
                    let recipe = environment
                        .get(ordinal as usize)
                        .ok_or_else(|| bad("closure Fixed image selects an absent ghost slot"))?;
                    let actual = self.recipe_actual(scope, recipe, depth + 1)?;
                    let SemanticActual::Life(image) = actual else {
                        return Err(bad("closure Fixed image selects a non-Life ghost slot"));
                    };
                    let slot = images
                        .get_mut(fixed as usize)
                        .ok_or_else(|| bad("closure Fixed embedding exceeds its full domain"))?;
                    if slot.replace(image).is_some() {
                        return Err(bad("closure Fixed embedding repeats a class"));
                    }
                }
                let images = images
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| {
                        unknown("whole closure type is missing an actual Fixed image")
                    })?;
                resolved(
                    Type::Closure(*id),
                    StorageLifeShape {
                        binder_count: life.fixed_binder_count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    images,
                )
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                let original = self.definition(*definition)?;
                if arguments.len() != original.parameters.len() {
                    return Err(bad(
                        "opaque application omits original mixed actual positions",
                    ));
                }
                let value = self.ty(scope, hidden, hint, depth + 1)?;
                let SemanticTypeValue::Resolved { classes, .. } = &value else {
                    unreachable!()
                };
                // An opaque nominal occurrence cannot disappear merely because
                // its physical hidden representation currently has no leaf.
                self.fee(arguments.len().saturating_add(classes.len()))?;
                for argument in arguments {
                    let actual = self.recipe_actual(scope, argument, depth + 1)?;
                    match actual {
                        SemanticActual::Life(image) if classes.contains(&image) => {}
                        SemanticActual::Type(SemanticTypeValue::Resolved {
                            life,
                            classes: images,
                            ..
                        }) if life.binder_count == 0 && images.is_empty() => {}
                        SemanticActual::Const {
                            ty:
                                SemanticTypeValue::Resolved {
                                    life,
                                    classes: images,
                                    ..
                                },
                            ..
                        } if life.binder_count == 0 && images.is_empty() => {}
                        _ => {
                            return Err(unknown(
                                "opaque full original domain needs additional persistent hidden class occurrences",
                            ));
                        }
                    }
                }
                value
            }
            _ => {
                return Err(unknown(
                    "semantic type recipe requires its complete actual class materializer",
                ));
            }
        };
        if let Some(hint) = hint {
            let SemanticTypeValue::Resolved { ty, .. } = &value else {
                unreachable!()
            };
            inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            if ty != hint {
                return Err(bad(
                    "original type recipe differs from its actual emitted complete type",
                ));
            }
        }
        Ok(value)
    }
    fn recipe_actual(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticActualRecipe,
        depth: usize,
    ) -> R<SemanticActual> {
        crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
        self.actual(
            scope,
            &SemanticActual::CoreRecipe {
                scope,
                recipe: Box::new(recipe.clone()),
            },
            depth,
        )
    }
    fn actual(
        &mut self,
        owner: SemanticInstanceId,
        actual: &SemanticActual,
        depth: usize,
    ) -> R<SemanticActual> {
        self.node(depth)?;
        Ok(match actual {
            SemanticActual::Type(value) => {
                SemanticActual::Type(self.value(owner, value, None, depth + 1)?)
            }
            SemanticActual::Const { ty, value } => {
                let ty = self.value(owner, ty, None, depth + 1)?;
                let SemanticTypeValue::Resolved {
                    ty: carrier, life, ..
                } = &ty
                else {
                    unreachable!()
                };
                if value.ty() != *carrier || life.binder_count != 0 {
                    return Err(bad("const actual has another complete carrier"));
                }
                self.fee(match value {
                    Constant::WideString(units) => units.len().saturating_add(1),
                    _ => 1,
                })?;
                SemanticActual::Const {
                    ty,
                    value: value.clone(),
                }
            }
            SemanticActual::Life(image) => SemanticActual::Life(self.class(owner, image)?),
            SemanticActual::CoreRecipe { scope, recipe } => match recipe.as_ref() {
                SemanticActualRecipe::Type(recipe) => {
                    SemanticActual::Type(self.ty(*scope, recipe, None, depth + 1)?)
                }
                SemanticActualRecipe::Life(recipe) => {
                    SemanticActual::Life(self.life(*scope, recipe)?)
                }
                SemanticActualRecipe::Const { carrier, value } => {
                    let ty = self.ty(*scope, carrier, None, depth + 1)?;
                    let SemanticTypeValue::Resolved {
                        ty: carrier, life, ..
                    } = &ty
                    else {
                        unreachable!()
                    };
                    if life.binder_count != 0 {
                        return Err(bad("const recipe has a free-class carrier"));
                    }
                    let value = self.constant(*scope, value, carrier, depth + 1)?;
                    SemanticActual::Const { ty, value }
                }
            },
        })
    }
}

/// Materialize every holding point, including unused instance actuals and
/// declaration constraints. Original recipes remain present for the subsequent
/// independent checks; no residual Core operand is interpreted as an empty one.
pub(super) fn program(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    crate::dyn_metadata::semantic_snapshot(program, limits, left, 1)?;
    let input = program.clone();
    let mut eval = Eval {
        input: &input,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
    };
    eval.fee(program.instances.len().saturating_add(program.sites.len()))?;
    for instance in &mut program.instances {
        eval.fee(instance.actuals.len())?;
        for (ordinal, actual) in instance.actuals.iter_mut().enumerate() {
            let mut value = eval.actual(instance.id, actual, 0)?;
            if let SemanticActual::Type(SemanticTypeValue::Resolved { life, classes, .. }) =
                &mut value
            {
                // Actual Type class N is intrinsic. Callee code never imports
                // caller SSA or recursively consults its own asserted images.
                eval.fee(life.binder_count as usize)?;
                *classes = (0..life.binder_count)
                    .map(|class| SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::TypeArgument {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: ordinal as u32,
                        },
                        path: Vec::new(),
                        class,
                    })
                    .collect();
            }
            *actual = value;
        }
        if let Some(environment) = &mut instance.environment {
            let creator = instance
                .parent
                .ok_or_else(|| bad("actual ghost environment has no real lexical creator"))?;
            eval.fee(environment.actuals.len())?;
            for value in &mut environment.actuals {
                *value = eval.actual(creator, value, 0)?;
            }
        }
        if instance.self_type.is_some() {
            return Err(unknown(
                "actual Self requires the complete method/receiver and parent impl materializer",
            ));
        }
        eval.fee(instance.header_embedding.len())?;
        for binding in &mut instance.header_embedding {
            binding.actual = eval.class(instance.id, &binding.actual)?;
        }
    }
    for site in &mut program.sites {
        let f = eval.function(site.instance)?;
        match &mut site.action {
            SemanticSiteAction::Construct {
                result: SemanticOperand::Value(result),
                value,
            } => {
                let hint = f
                    .values
                    .get(result.0 as usize)
                    .ok_or_else(|| bad("constructor type materialization lost its actual SSA"))?;
                *value = eval.value(site.instance, value, Some(hint), 0)?;
            }
            SemanticSiteAction::Construct { .. } => {
                return Err(bad("constructor result was not relocated to actual SSA"));
            }
            SemanticSiteAction::Closure { environment, .. } => {
                eval.fee(environment.len())?;
                for value in environment {
                    *value = eval.actual(site.instance, value, 0)?;
                }
            }
            SemanticSiteAction::TypeUse { subject, expected } => {
                let hint = match subject {
                    SemanticOperand::Value(value) => f.values.get(value.0 as usize),
                    SemanticOperand::Local(local) => {
                        f.locals.get(local.0 as usize).map(|local| &local.ty)
                    }
                    SemanticOperand::Capture(index) => f
                        .environment
                        .and_then(|id| eval.module.closures.iter().find(|d| d.id == id))
                        .and_then(|d| d.captures.get(*index as usize)),
                    SemanticOperand::Parameter(index) => f
                        .parameters
                        .get(*index as usize)
                        .and_then(|local| f.locals.get(local.0 as usize))
                        .map(|local| &local.ty),
                    _ => None,
                }
                .ok_or_else(|| {
                    unknown("type-use requires its actual value or declared local type")
                })?;
                *expected = eval.value(site.instance, expected, Some(hint), 0)?;
            }
            SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Typed(local),
                expected,
            } => {
                let hint = f
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| bad("declaration constraint names an absent actual local"))?;
                *expected = eval.value(site.instance, expected, Some(&hint.ty), 0)?;
            }
            SemanticSiteAction::Call { actuals, .. } => {
                eval.fee(actuals.len())?;
                for actual in actuals {
                    *actual = eval.actual(site.instance, actual, 0)?;
                }
            }
            SemanticSiteAction::CallableCall { .. } => {}
            SemanticSiteAction::DeclareLocal { .. } => {
                return Err(bad("Core declaration target survived actual relocation"));
            }
        }
    }
    Ok(())
}

/// Interpret an original type in an exact emitted instance. The caller still
/// proves the original world and the actual body's class flow independently.
pub(super) fn type_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticTypeRecipe,
    hint: Option<&Type>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeValue> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
    }
    .ty(scope, recipe, hint, 0)
}
pub(super) fn actual_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticActualRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticActual> {
    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
    let actual = SemanticActual::CoreRecipe {
        scope,
        recipe: Box::new(recipe.clone()),
    };
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
    }
    .actual(scope, &actual, 0)
}
