//! Link EVERY actual nominal catalogue application to its original field and
//! variant recipes, including unused applications. Physical equality and class
//! equations are separate: equal concrete callers do not add original premises.
use crate::*;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-NOMINAL", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
struct Check<'a, 'b> {
    program: &'a SemanticModule,
    module: &'a TypedModule,
    limits: &'b Limits,
    left: &'b mut usize,
    active: Vec<usize>,
    complete: std::collections::BTreeSet<usize>,
}
impl<'a> Check<'a, '_> {
    fn node(&mut self, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "nominal original application work exceeded",
                None,
            )
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "nominal original application depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn pay(&mut self, n: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "nominal original catalogue work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn shape_work(&mut self, life: &StorageLifeShape) -> R<()> {
        self.pay(1)?;
        for slot in &life.slots {
            self.node(slot.path.len())?;
            self.pay(slot.path.len())?;
        }
        for row in &life.retained {
            self.node(row.path.len())?;
            self.pay(row.path.len())?;
        }
        for row in &life.nominal_arguments {
            self.node(row.path.len())?;
            self.pay(row.path.len())?;
        }
        Ok(())
    }
    fn same(&mut self, a: &Type, b: &Type) -> R<()> {
        inference::charge_type_snapshot(a, Span::default(), self.limits, self.left, 1)?;
        inference::charge_type_snapshot(b, Span::default(), self.limits, self.left, 1)?;
        if a != b {
            return Err(bad(
                "actual nominal field changes its original instantiated whole type",
            ));
        }
        Ok(())
    }
    fn original(&mut self, declaration: LifeDeclarationId) -> R<Option<&'a SemanticDefinition>> {
        self.pay(self.program.definitions.len())?;
        let mut found = None;
        for definition in &self.program.definitions {
            if matches!(&definition.body,SemanticDefinitionBody::Nominal { declaration:actual,.. } if *actual==declaration)
            {
                if found.replace(definition).is_some() {
                    return Err(bad(
                        "nominal application has multiple original declaration recipes",
                    ));
                }
            }
        }
        Ok(found)
    }
    fn instance(&mut self, ty: &Type) -> R<&'a LifeNominalInstance> {
        self.pay(self.module.life_instances.len())?;
        let mut found: Option<&LifeNominalInstance> = None;
        for row in &self.module.life_instances {
            inference::charge_type_snapshot(&row.ty, Span::default(), self.limits, self.left, 1)?;
            if &row.ty != ty {
                continue;
            }
            if let Some(prior) = found {
                self.pay(row.arguments.len().saturating_add(prior.arguments.len()))?;
                for actual in row.arguments.iter().chain(&prior.arguments) {
                    match actual {
                        LifeNominalActual::Type(ty) => inference::charge_type_snapshot(
                            ty,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?,
                        LifeNominalActual::QualifiedType { ty, life } => {
                            inference::charge_type_snapshot(
                                ty,
                                Span::default(),
                                self.limits,
                                self.left,
                                1,
                            )?;
                            self.shape_work(life)?;
                        }
                        LifeNominalActual::Const(Constant::WideString(units)) => {
                            self.pay(units.len())?
                        }
                        _ => {}
                    }
                }
                if row.declaration != prior.declaration || row.arguments != prior.arguments {
                    return Err(bad(
                        "one exact field type has conflicting original nominal applications",
                    ));
                }
            } else {
                found = Some(row);
            }
        }
        found.ok_or_else(|| pending("nominal field has no complete actual original application"))
    }
    fn shape(instance: &LifeNominalInstance) -> Option<&StorageLifeShape> {
        if let Type::BorrowedStorage { life, .. } = &instance.ty {
            Some(life)
        } else {
            None
        }
    }
    fn domain(&mut self, ty: &Type, depth: usize) -> R<u32> {
        self.node(depth)?;
        match ty {
            Type::BorrowedStorage { life, .. } => Ok(life.binder_count),
            ty if ty.is_b0_scalar() => Ok(0),
            Type::ByteBuffer
            | Type::ForeignHandle
            | Type::CWideString
            | Type::SyncCell(_)
            | Type::TaskI32 => Ok(0),
            Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } => {
                let row = self.instance(ty)?;
                // The mandatory whole-module ordinary proof establishes zero
                // for plain entries. It is not inferred from visible fields.
                if matches!(row.ty, Type::BorrowedStorage { .. }) {
                    return Err(bad("plain field selects a qualified application"));
                }
                Ok(0)
            }
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Mutex { payload: inner } => self.domain(inner, depth + 1),
            Type::Tuple(fields) => {
                for field in fields {
                    if self.domain(field, depth + 1)? != 0 {
                        return Err(pending(
                            "nonzero compound field requires its complete qualified boundary",
                        ));
                    }
                }
                Ok(0)
            }
            Type::Array { element, .. } | Type::Slice { element } => {
                if self.domain(element, depth + 1)? != 0 {
                    return Err(pending(
                        "nonzero element field requires its complete qualified boundary",
                    ));
                }
                Ok(0)
            }
            Type::ErrorUnion { error, result } => {
                if self.domain(error, depth + 1)? != 0 || self.domain(result, depth + 1)? != 0 {
                    return Err(pending(
                        "nonzero error field requires its complete qualified boundary",
                    ));
                }
                Ok(0)
            }
            _ => Err(pending(
                "nominal field requires a complete opaque/callable domain directory",
            )),
        }
    }
    fn projected(
        &mut self,
        owner: &LifeNominalInstance,
        path: &[LifeTypeProjection],
        class: u32,
    ) -> R<u32> {
        let life =
            Self::shape(owner).ok_or_else(|| bad("plain nominal omits a nonzero field domain"))?;
        self.pay(life.retained.len().saturating_add(path.len()))?;
        let mut found = None;
        for row in &life.retained {
            self.pay(row.path.len())?;
            if row.path == path && row.class == class {
                if found.replace(row.binder).is_some() {
                    return Err(bad("nominal repeats a field-to-parent class equation"));
                }
            }
        }
        let binder = found
            .ok_or_else(|| pending("nominal field has no complete parent class projection"))?;
        if binder >= life.binder_count {
            return Err(bad("nominal field class exceeds its actual parent domain"));
        }
        Ok(binder)
    }
    fn type_images(
        &mut self,
        owner: &LifeNominalInstance,
        ordinal: u32,
        ty: &Type,
        depth: usize,
    ) -> R<Vec<u32>> {
        let count = self.domain(ty, depth + 1)?;
        self.pay(count as usize)?;
        let mut images = Vec::new();
        for class in 0..count {
            let prefix = [LifeTypeProjection::TypeArgument(ordinal)];
            if let Some(life) = Self::shape(owner) {
                let mut direct = None;
                for row in &life.retained {
                    self.pay(row.path.len().saturating_add(1))?;
                    if row.path == prefix && row.class == class {
                        if direct.replace(row.binder).is_some() {
                            return Err(bad("Type actual has repeated parent class projections"));
                        }
                    }
                }
                if let Some(image) = direct {
                    if image >= life.binder_count {
                        return Err(bad("Type actual maps outside parent domain"));
                    }
                    images.push(image);
                    continue;
                }
                if let Type::BorrowedStorage { life: child, .. } = ty {
                    let mut image = None;
                    for occurrence in child.nominal_arguments.iter().filter(|r| r.binder == class) {
                        for parent in &life.nominal_arguments {
                            self.pay(
                                parent
                                    .path
                                    .len()
                                    .saturating_add(occurrence.path.len())
                                    .saturating_add(2),
                            )?;
                            if parent.path.first()
                                == Some(&LifeTypeProjection::TypeArgument(ordinal))
                                && parent.path[1..] == occurrence.path
                                && parent.declaration == occurrence.declaration
                                && parent.argument == occurrence.argument
                            {
                                if image.is_some_and(|old| old != parent.binder) {
                                    return Err(bad(
                                        "one Type intrinsic class has inconsistent original parent images",
                                    ));
                                }
                                image = Some(parent.binder);
                            }
                        }
                    }
                    if let Some(image) = image {
                        if image >= life.binder_count {
                            return Err(bad("Type nominal class maps outside parent domain"));
                        }
                        images.push(image);
                        continue;
                    }
                }
            }
            return Err(pending(
                "Type actual omits an original full-domain class equation",
            ));
        }
        // A whole Type's intrinsic positions remain distinct even when its
        // outside lifetime images happen to be equal. Only an already smaller
        // actual whole Type may have a smaller domain.
        for (index, image) in images.iter().enumerate() {
            self.pay(index)?;
            if images[..index].contains(image) {
                return Err(bad(
                    "nominal Type argument collapses distinct intrinsic classes",
                ));
            }
        }
        Ok(images)
    }
    fn life(
        &mut self,
        owner: &LifeNominalInstance,
        recipe: &SemanticLifeRecipe,
        depth: usize,
    ) -> R<u32> {
        self.node(depth)?;
        match recipe {
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                let Some(LifeNominalActual::Lifetime(class)) =
                    owner.arguments.get(*ordinal as usize)
                else {
                    return Err(bad(
                        "original field lifetime selects another full actual kind",
                    ));
                };
                let count = Self::shape(owner).map_or(0, |s| s.binder_count);
                if *class >= count {
                    return Err(bad(
                        "original field lifetime disappeared from actual nominal domain",
                    ));
                }
                Ok(*class)
            }
            SemanticLifeRecipe::IntrinsicClass {
                operand: SemanticRecipeOperand::TypeFormal(reference),
                path,
                class,
            } if reference.scope == SemanticScopeRef::Ancestor(0) && path.is_empty() => {
                let Some(LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. }) =
                    owner.arguments.get(reference.ordinal as usize)
                else {
                    return Err(bad("field intrinsic class selects a non-Type actual"));
                };
                self.type_images(owner, reference.ordinal, ty, depth + 1)?
                    .get(*class as usize)
                    .copied()
                    .ok_or_else(|| bad("field intrinsic class exceeds the complete Type actual"))
            }
            _ => Err(pending(
                "field lifetime requires its complete checked lexical environment",
            )),
        }
    }
    fn constant(
        &mut self,
        owner: &LifeNominalInstance,
        recipe: &SemanticConstRecipe,
        depth: usize,
    ) -> R<Constant> {
        self.node(depth)?;
        match recipe {
            SemanticConstRecipe::Value(value) => {
                if let Constant::WideString(units) = value {
                    self.pay(units.len())?;
                }
                Ok(value.clone())
            }
            SemanticConstRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                let Some(LifeNominalActual::Const(value)) = owner.arguments.get(*ordinal as usize)
                else {
                    return Err(bad("field Const selects another full actual kind"));
                };
                if let Constant::WideString(units) = value {
                    self.pay(units.len())?;
                }
                Ok(value.clone())
            }
            SemanticConstRecipe::IntegerLiteral(text) => {
                self.pay(text.len().saturating_mul(3))?;
                integer(text, Some(&Type::Usize), false, Span::default())
            }
            _ => Err(pending(
                "field Const requires its complete original symbolic evaluation",
            )),
        }
    }
    fn length(value: Constant) -> Option<u32> {
        match value {
            Constant::I8(n) => u32::try_from(n).ok(),
            Constant::I16(n) => u32::try_from(n).ok(),
            Constant::I32(n) => u32::try_from(n).ok(),
            Constant::I64(n) => u32::try_from(n).ok(),
            Constant::Isize(n) => u32::try_from(n).ok(),
            Constant::U8(n) => Some(n as u32),
            Constant::U16(n) => Some(n as u32),
            Constant::U32(n) => Some(n),
            Constant::U64(n) => u32::try_from(n).ok(),
            Constant::Usize(n) => u32::try_from(n).ok(),
            _ => None,
        }
    }
    fn types(
        &mut self,
        owner: &LifeNominalInstance,
        recipe: &SemanticTypeRecipe,
        actual: &Type,
        path: &[LifeTypeProjection],
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match (recipe, actual) {
            (SemanticTypeRecipe::Atom(expected), _) => self.same(expected, actual),
            (SemanticTypeRecipe::Exact(value), _) => {
                self.same(&value.ty, actual)?;
                self.shape_work(&value.life)?;
                self.pay(value.classes.len())?;
                let count = self.domain(actual, depth + 1)?;
                if value.life.binder_count != count || value.classes.len() != count as usize {
                    return Err(bad(
                        "exact nominal field changes its complete intrinsic domain",
                    ));
                }
                if let Type::BorrowedStorage { life, .. } = actual {
                    self.shape_work(life)?;
                    if &value.life != life {
                        return Err(bad(
                            "exact nominal field changes its intrinsic qualification",
                        ));
                    }
                    for (class, image) in value.classes.iter().enumerate() {
                        let expected = self.life(owner, image, depth + 1)?;
                        if self.projected(owner, path, class as u32)? != expected {
                            return Err(bad(
                                "exact nominal field changes its original class equation",
                            ));
                        }
                    }
                } else if count != 0
                    || !value.life.slots.is_empty()
                    || !value.life.retained.is_empty()
                    || !value.life.nominal_arguments.is_empty()
                {
                    return Err(pending(
                        "exact outer field requires its full original occurrence projection",
                    ));
                }
                Ok(())
            }
            (SemanticTypeRecipe::Formal(reference), _)
                if reference.scope == SemanticScopeRef::Ancestor(0) =>
            {
                let Some(
                    LifeNominalActual::Type(expected)
                    | LifeNominalActual::QualifiedType { ty: expected, .. },
                ) = owner.arguments.get(reference.ordinal as usize)
                else {
                    return Err(bad("field whole-Type formal selects another actual kind"));
                };
                self.same(expected, actual)?;
                let images = self.type_images(owner, reference.ordinal, expected, depth + 1)?;
                for (class, image) in images.into_iter().enumerate() {
                    if self.projected(owner, path, class as u32)? != image {
                        return Err(bad(
                            "field changes its whole-Type actual's intrinsic class equation",
                        ));
                    }
                }
                Ok(())
            }
            (
                SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                },
                _,
            ) => {
                self.pay(self.program.definitions.len())?;
                let original = self
                    .program
                    .definitions
                    .iter()
                    .find(|d| d.id == *definition)
                    .ok_or_else(|| bad("nested field original declaration is absent"))?;
                let SemanticDefinitionBody::Nominal { declaration, .. } = &original.body else {
                    return Err(bad(
                        "nested nominal field changes original declaration kind",
                    ));
                };
                let child = self.instance(actual)?;
                if child.declaration != *declaration || child.arguments.len() != arguments.len() {
                    return Err(bad(
                        "field changes original nested declaration or full mixed actuals",
                    ));
                }
                for (ordinal, (expected, actual)) in
                    arguments.iter().zip(&child.arguments).enumerate()
                {
                    self.node(depth + 1)?;
                    match (expected, actual) {
                        (
                            SemanticActualRecipe::Type(expected),
                            LifeNominalActual::Type(actual)
                            | LifeNominalActual::QualifiedType { ty: actual, .. },
                        ) => {
                            // Child Type argument identity is checked whole; any
                            // nonzero images also keep their explicit path.
                            self.pay(path.len().saturating_add(1))?;
                            let mut p = path.to_vec();
                            p.push(LifeTypeProjection::TypeArgument(ordinal as u32));
                            self.types(owner, expected, actual, &p, depth + 1)?;
                        }
                        (
                            SemanticActualRecipe::Life(expected),
                            LifeNominalActual::Lifetime(class),
                        ) => {
                            if self.projected(owner, path, *class)?
                                != self.life(owner, expected, depth + 1)?
                            {
                                return Err(bad(
                                    "nested field switches an original lifetime actual",
                                ));
                            }
                        }
                        (
                            SemanticActualRecipe::Const { value, .. },
                            LifeNominalActual::Const(actual),
                        ) => {
                            let expected = self.constant(owner, value, depth + 1)?;
                            self.pay(1)?;
                            if &expected != actual {
                                return Err(bad("nested field switches its original Const actual"));
                            }
                        }
                        _ => return Err(bad("nested field changes a full mixed actual kind")),
                    }
                }
                self.row(child, depth + 1)
            }
            (SemanticTypeRecipe::Tuple(expected), Type::Tuple(actual))
                if expected.len() == actual.len() =>
            {
                for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
                    self.pay(path.len().saturating_add(1))?;
                    let mut p = path.to_vec();
                    p.push(LifeTypeProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    self.types(owner, expected, actual, &p, depth + 1)?;
                }
                Ok(())
            }
            (
                SemanticTypeRecipe::Array { element, length },
                Type::Array {
                    element: actual,
                    length: actual_length,
                },
            ) => {
                if Self::length(self.constant(owner, length, depth + 1)?) != Some(*actual_length) {
                    return Err(bad("field array changes its original Const length"));
                }
                self.pay(path.len().saturating_add(1))?;
                let mut p = path.to_vec();
                p.push(LifeTypeProjection::Element);
                self.types(owner, element, actual, &p, depth + 1)
            }
            (SemanticTypeRecipe::Slice(inner), Type::Slice { element: actual })
            | (SemanticTypeRecipe::OwnedStore(inner), Type::OwnedStore { element: actual }) => {
                self.pay(path.len().saturating_add(1))?;
                let mut p = path.to_vec();
                p.push(LifeTypeProjection::Element);
                self.types(owner, inner, actual, &p, depth + 1)
            }
            (SemanticTypeRecipe::Managed(inner), Type::Managed(actual))
            | (SemanticTypeRecipe::Mutex(inner), Type::Mutex { payload: actual })
            | (
                SemanticTypeRecipe::Registration(inner),
                Type::Registration {
                    environment: actual,
                },
            )
            | (
                SemanticTypeRecipe::AsyncRegistration(inner),
                Type::AsyncRegistration {
                    environment: actual,
                },
            ) => {
                self.pay(path.len().saturating_add(1))?;
                let mut p = path.to_vec();
                p.push(LifeTypeProjection::Pointee);
                self.types(owner, inner, actual, &p, depth + 1)
            }
            (
                SemanticTypeRecipe::Address {
                    mutable,
                    lifetime,
                    inner,
                },
                Type::Address {
                    mutable: actual_mode,
                    inner: actual,
                },
            ) => {
                if mutable != actual_mode {
                    return Err(bad("field changes original address permission mode"));
                }
                self.outer(
                    owner,
                    lifetime,
                    path,
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    depth + 1,
                )?;
                self.pay(path.len().saturating_add(1))?;
                let mut p = path.to_vec();
                p.push(LifeTypeProjection::Pointee);
                self.types(owner, inner, actual, &p, depth + 1)
            }
            (SemanticTypeRecipe::Desc { lifetime, inner }, Type::Desc(actual)) => {
                self.outer(
                    owner,
                    lifetime,
                    path,
                    StoredBorrowMode::WritableDescription,
                    depth + 1,
                )?;
                self.pay(path.len().saturating_add(1))?;
                let mut p = path.to_vec();
                p.push(LifeTypeProjection::Pointee);
                self.types(owner, inner, actual, &p, depth + 1)
            }
            (
                SemanticTypeRecipe::ErrorUnion { error, result },
                Type::ErrorUnion {
                    error: actual_error,
                    result: actual_result,
                },
            ) => {
                for (is_error, expected, actual) in
                    [(true, error, actual_error), (false, result, actual_result)]
                {
                    self.pay(path.len().saturating_add(1))?;
                    let mut p = path.to_vec();
                    p.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(
                        is_error,
                    )));
                    self.types(owner, expected, actual, &p, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(pending(
                "nominal actual field needs its complete original type/opaque/context equation",
            )),
        }
    }
    fn outer(
        &mut self,
        owner: &LifeNominalInstance,
        expected: &SemanticOuterLifeRecipe,
        path: &[LifeTypeProjection],
        mode: StoredBorrowMode,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        let SemanticOuterLifeRecipe::Class(expected) = expected else {
            return Err(pending(
                "nominal Static occurrence needs complete constructor/write obligations",
            ));
        };
        let expected = self.life(owner, expected, depth + 1)?;
        let mut physical_path = Vec::new();
        self.pay(path.len())?;
        for step in path {
            let LifeTypeProjection::Place(place) = step else {
                return Err(pending(
                    "nested pointee occurrence needs its complete child class projection",
                ));
            };
            physical_path.push(*place);
        }
        let shape = Self::shape(owner)
            .ok_or_else(|| bad("plain field erased a declared borrow occurrence"))?;
        self.pay(shape.slots.len())?;
        let mut found = None;
        for slot in &shape.slots {
            self.pay(slot.path.len())?;
            if slot.path == physical_path {
                if found.replace(slot).is_some() {
                    return Err(bad("nominal repeats an actual field borrow occurrence"));
                }
            }
        }
        let slot = found.ok_or_else(|| bad("nominal omits an original field borrow occurrence"))?;
        if slot.mode != mode || slot.binder != expected {
            return Err(bad(
                "nominal field switches original lifetime or permission mode",
            ));
        }
        Ok(())
    }
    fn actual_type(
        &mut self,
        owner: &LifeNominalInstance,
        recipe: &SemanticTypeRecipe,
    ) -> R<&'a Type> {
        if let SemanticTypeRecipe::Formal(reference) = recipe {
            if reference.scope == SemanticScopeRef::Ancestor(0) {
                let row = self.instance(&owner.ty)?;
                if let Some(
                    LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. },
                ) = row.arguments.get(reference.ordinal as usize)
                {
                    return Ok(ty);
                }
            }
        }
        Err(pending(
            "nominal where premise requires its complete symbolic actual type evaluator",
        ))
    }
    fn copy_type(&mut self, ty: &Type, depth: usize) -> R<bool> {
        self.node(depth)?;
        Ok(match ty {
            Type::Address { mutable: false, .. } => true,
            Type::FunctionPointer { .. }
            | Type::BorrowedFunctionPointer { .. }
            | Type::ForeignCallback { .. } => true,
            Type::BorrowedStorage { storage, life } => {
                stored::shape(ty, self.left, self.limits.max_call_depth, Span::default())?;
                !life
                    .slots
                    .iter()
                    .any(|r| r.mode != StoredBorrowMode::SharedAddress)
                    && self.copy_type(storage, depth + 1)?
            }
            Type::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.copy_type(field, depth + 1)?;
                }
                all
            }
            Type::Array { element, .. } => self.copy_type(element, depth + 1)?,
            Type::Record { fields, .. } => {
                let mut all = true;
                for (_, field) in fields {
                    all &= self.copy_type(field, depth + 1)?;
                }
                all
            }
            Type::Enum { variants, .. } => {
                let mut all = true;
                for (_, fields) in variants {
                    for field in fields {
                        all &= self.copy_type(field, depth + 1)?;
                    }
                }
                all
            }
            Type::ErrorUnion { error, result } => {
                self.copy_type(error, depth + 1)? && self.copy_type(result, depth + 1)?
            }
            other => other.is_copy_value(),
        })
    }
    fn value_type(&mut self, ty: &Type, depth: usize) -> R<bool> {
        self.node(depth)?;
        Ok(match ty {
            ty if ty.is_b0_scalar() => true,
            Type::Address { .. }
            | Type::FunctionPointer { .. }
            | Type::BorrowedFunctionPointer { .. }
            | Type::ForeignCallback { .. }
            | Type::ForeignHandle
            | Type::CWideString => true,
            Type::BorrowedStorage { storage, .. } => {
                stored::shape(ty, self.left, self.limits.max_call_depth, Span::default())?;
                self.value_type(storage, depth + 1)?
            }
            Type::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.value_type(field, depth + 1)?;
                }
                all
            }
            Type::Array { element, .. } => self.value_type(element, depth + 1)?,
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                let mut all = true;
                for (_, field) in fields {
                    all &= if let Type::Managed(_) = field {
                        resources::supported(field)
                    } else {
                        self.value_type(field, depth + 1)?
                    };
                }
                all
            }
            Type::Enum { variants, .. } => {
                let mut all = true;
                for (_, fields) in variants {
                    for field in fields {
                        all &= if let Type::Managed(_) = field {
                            resources::supported(field)
                        } else {
                            self.value_type(field, depth + 1)?
                        };
                    }
                }
                all
            }
            Type::ErrorUnion { error, result } => {
                self.value_type(error, depth + 1)? && self.value_type(result, depth + 1)?
            }
            _ => false,
        })
    }
    fn requirements(
        &mut self,
        owner: &LifeNominalInstance,
        definition: &SemanticDefinition,
        depth: usize,
    ) -> R<()> {
        for requirement in
            definition
                .requirements
                .iter()
                .chain(
                    definition
                        .parameters
                        .iter()
                        .flat_map(|formal| match formal {
                            SemanticFormal::Type { requirements } => requirements.as_slice(),
                            _ => &[],
                        }),
                )
        {
            self.node(depth)?;
            match requirement {
                SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind: SemanticBuiltinTrait::Copy,
                }
                | SemanticRequirementRecipe::Structural {
                    ty,
                    bound: DynStructuralBound::Copy,
                } => {
                    let ty = self.actual_type(owner, ty)?;
                    if !self.copy_type(ty, depth + 1)? {
                        return Err(bad(
                            "actual nominal does not satisfy its complete original Copy premise",
                        ));
                    }
                }
                SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind: SemanticBuiltinTrait::Value,
                } => {
                    let ty = self.actual_type(owner, ty)?;
                    if !self.value_type(ty, depth + 1)? {
                        return Err(bad(
                            "actual nominal does not satisfy its original Value premise",
                        ));
                    }
                }
                SemanticRequirementRecipe::LifeEqual { left, right } => {
                    if self.life(owner, left, depth + 1)? != self.life(owner, right, depth + 1)? {
                        return Err(bad(
                            "nominal actuals do not satisfy original lifetime equality",
                        ));
                    }
                }
                _ => {
                    return Err(pending(
                        "nominal where requirement needs its complete original structural/trait/role proof",
                    ));
                }
            }
        }
        Ok(())
    }
    fn row(&mut self, instance: &'a LifeNominalInstance, depth: usize) -> R<()> {
        self.node(depth)?;
        let key = instance as *const LifeNominalInstance as usize;
        self.pay(
            self.active
                .len()
                .saturating_add(self.complete.len().saturating_add(1).ilog2() as usize),
        )?;
        if self.complete.contains(&key) {
            return Ok(());
        }
        if self.active.contains(&key) {
            return Err(bad("nominal original application equations form a cycle"));
        }
        let Some(original) = self.original(instance.declaration)? else {
            // These two descriptors are derived later by the actual Task
            // constructor, not a source nominal declaration. The task pass
            // independently checked their exact ordered input/context shape.
            if matches!(instance.declaration,id if id==zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD || id==zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE)
            {
                return Ok(());
            }
            return Err(pending(
                "complete semantic program omits an actual original nominal definition",
            ));
        };
        let SemanticDefinitionBody::Nominal { kind, body, .. } = &original.body else {
            unreachable!()
        };
        if original.parameters.len() != instance.arguments.len() {
            return Err(bad(
                "nominal original application omits complete mixed actuals",
            ));
        }
        self.requirements(instance, original, depth + 1)?;
        self.active.push(key);
        match (body, stored::physical(&instance.ty)) {
            (
                SemanticNominalBody::Product { fields: expected },
                Type::Record { fields: actual, .. } | Type::Package { fields: actual, .. },
            ) => {
                if !matches!(
                    (kind, stored::physical(&instance.ty)),
                    (
                        LifeNominalKind::Record | LifeNominalKind::Resource,
                        Type::Record { .. }
                    ) | (LifeNominalKind::Package, Type::Package { .. })
                ) || expected.len() != actual.len()
                {
                    return Err(bad("nominal product changes original kind or field arity"));
                }
                for (index, (expected, (name, actual))) in expected.iter().zip(actual).enumerate() {
                    self.pay(name.len().saturating_add(expected.name.len()))?;
                    if name != &expected.name {
                        return Err(bad("nominal changes original field identity/order"));
                    }
                    self.types(
                        instance,
                        &expected.ty,
                        actual,
                        &[LifeTypeProjection::Place(PlaceProjection::Field(
                            index as u32,
                        ))],
                        depth + 1,
                    )?;
                }
            }
            (
                SemanticNominalBody::Enum { variants: expected },
                Type::Enum {
                    variants: actual, ..
                },
            ) => {
                if *kind != LifeNominalKind::Enum || expected.len() != actual.len() {
                    return Err(bad("nominal changes original enum alternatives"));
                }
                for (variant, (expected, (name, fields))) in expected.iter().zip(actual).enumerate()
                {
                    self.pay(name.len().saturating_add(expected.name.len()))?;
                    if name != &expected.name || fields.len() != expected.fields.len() {
                        return Err(bad("nominal changes original enum tag or payload arity"));
                    }
                    for (index, (expected, actual)) in
                        expected.fields.iter().zip(fields).enumerate()
                    {
                        self.types(
                            instance,
                            expected,
                            actual,
                            &[LifeTypeProjection::Place(PlaceProjection::Variant(
                                VariantProjection {
                                    variant: variant as u32,
                                    index: index as u32,
                                },
                            ))],
                            depth + 1,
                        )?;
                    }
                }
            }
            _ => return Err(bad("nominal application changes original physical kind")),
        }
        self.active.pop();
        self.complete.insert(key);
        Ok(())
    }
}
pub(super) fn program(
    program: &SemanticModule,
    module: &TypedModule,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut check = Check {
        program,
        module,
        limits,
        left,
        active: Vec::new(),
        complete: std::collections::BTreeSet::new(),
    };
    check.pay(module.life_instances.len())?;
    for instance in &module.life_instances {
        check.row(instance, 0)?;
    }
    Ok(())
}
