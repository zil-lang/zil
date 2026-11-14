//! Interpret original function instances and their actual local captured heads.
//! Empty metadata never selects a proof: the complete type, original signature,
//! creator, occurrence and operation inventories must independently agree.
use super::*;
mod abstract_calls;
mod borrow_places;
mod borrows;
mod buffers;
mod callable_roles;
mod capabilities;
mod capture_creators;
mod capture_heads;
mod capture_origins;
mod capture_places;
mod class_origins;
mod classes;
mod descriptions;
mod dyn_owners;
mod dyn_tables;
mod function_values;
mod mutex;
mod nominal_fields;
mod operations;
mod parameter_classes;
mod pattern_places;
mod protocol_values;
mod registration;
mod registration_observe;
mod registration_start;
mod resources;
mod slice_borrows;
mod slices;
mod stores;
mod structural;
mod tasks;
mod traits;
mod worlds;
pub(super) use traits::{original_associated, original_self};

fn unknown(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::unsupported(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
fn empty(life: &StorageLifeShape) -> bool {
    life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
}
struct Check<'a, 'i> {
    module: &'a TypedModule,
    program: &'a SemanticModule,
    inventory: &'i Inventory<'a>,
    original: &'i original::Checked,
    scope_queries: std::cell::RefCell<BTreeSet<ClosureId>>,
    trait_queries: std::cell::RefCell<Vec<traits::Query>>,
}
impl Check<'_, '_> {
    fn ordinary_type(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        self.ordinary_type_at(ty, b, 0)
    }
    fn ordinary_type_at(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("complete actual type exceeds its bounded depth"));
        }
        if ty.is_b0_scalar()
            || *ty == Type::ForeignHandle
            || *ty == crate::generic_mutex::error_type()
            || crate::dyn_dispatch::is_create_error(ty)
        {
            return Ok(());
        }
        if self.dyn_value_type(ty, b, depth + 1)? {
            return Ok(());
        }
        if self.protocol_value_type(ty, b, depth + 1)? {
            return Ok(());
        }
        match ty {
            Type::Managed(inner) | Type::Desc(inner) => match inner.as_ref() {
                Type::Mutex { payload } => return self.mutex_payload(payload, b),
                Type::ByteBuffer => {
                    b.spend(self.module.resources.len(), WORK)?;
                    let mut count = 0;
                    for resource in &self.module.resources {
                        crate::dyn_metadata::ty(&resource.payload, b)?;
                        if resource.payload == Type::ByteBuffer
                            || resource.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
                        {
                            if resource.payload != Type::ByteBuffer
                                || resource.name != zil_ir::BYTE_BUFFER_RESOURCE_NAME
                                || resource.teardown.is_some()
                            {
                                return Err(fail(
                                    "semantic ByteBuffer carrier has another sealed allocator or cleanup definition",
                                ));
                            }
                            count += 1;
                        }
                    }
                    if count != 1 {
                        return Err(fail(
                            "semantic ByteBuffer carrier lacks its exact unique sealed definition",
                        ));
                    }
                    return Ok(());
                }
                Type::OwnedStore { element } => {
                    self.ordinary_type_at(element, b, depth + 1)?;
                    crate::dyn_metadata::ty(inner, b)?;
                    let owner = Type::Managed(inner.clone());
                    return crate::ownership_store::ordinary_owned(self.module, &owner, b);
                }
                Type::Registration { .. } | Type::AsyncRegistration { .. } => {
                    crate::dyn_metadata::ty(inner, b)?;
                    return self.resource_zero(&Type::Managed(inner.clone()), b, depth + 1);
                }
                Type::Task { .. } => {
                    return self.resource_zero(&Type::Managed(inner.clone()), b, depth + 1);
                }
                Type::Record { .. } | Type::BorrowedStorage { .. } => {
                    return self.resource_nominal(inner, b, depth + 1);
                }
                _ => {
                    return Err(unknown(
                        "semantic resource carrier needs its full original environment, Drop and retained scope inventory",
                    ));
                }
            },
            Type::Address { inner, .. } => {
                return self.ordinary_type_at(inner, b, depth + 1);
            }
            Type::MutexGuard {
                payload,
                retained_life,
            } => {
                self.mutex_payload(payload, b)?;
                if retained_life.binder_count != 1
                    || !retained_life.retained.is_empty()
                    || !retained_life.nominal_arguments.is_empty()
                    || !matches!(retained_life.ports.as_slice(),[port] if port.leaf == (Type::Mutex {payload:payload.clone()}) && port.mode == StoredBorrowMode::SharedAddress && port.binder == 0)
                {
                    return Err(fail(
                        "actual Mutex guard loses its complete canonical retained head",
                    ));
                }
                return Ok(());
            }
            Type::Slice { element } => {
                return self.ordinary_type_at(element, b, depth + 1);
            }
            Type::ErrorUnion { error, result } => {
                self.ordinary_type_at(error, b, depth + 1)?;
                return self.ordinary_type_at(result, b, depth + 1);
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.ordinary_type_at(parameter, b, depth + 1)?;
                }
                return self.ordinary_type_at(result, b, depth + 1);
            }
            Type::Array { element, .. } => {
                return self.ordinary_type_at(element, b, depth + 1);
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.ordinary_type_at(field, b, depth + 1)?;
                }
                return Ok(());
            }
            Type::Record { .. }
            | Type::Package { .. }
            | Type::Enum { .. }
            | Type::BorrowedStorage { .. } => {
                return self.nominal_inventory(ty, b);
            }
            _ => {}
        }
        let Type::Closure(id) = ty else {
            return Err(unknown(
                "semantic persistent or compound type needs complete original class substitution",
            ));
        };
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let definition = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == *id)
            .ok_or_else(|| fail("actual closure inventory identity differs"))?;
        // The complete actual directory also binds every active implicit
        // captured head to its original name occurrence and creator operand.
        self.capture_directory(*id, b)?;
        b.spend(
            definition
                .captures
                .len()
                .saturating_add(definition.parameters.len()),
            WORK,
        )?;
        for ty in definition
            .parameters
            .iter()
            .chain(std::iter::once(&definition.result))
        {
            // Classification preserves each complete input/result type. The
            // original header and role theorem separately check all classes;
            // classifying a reference never grants a borrow or fixed domain.
            self.ordinary_type_at(ty, b, depth + 1)?;
        }
        let entry = self
            .module
            .functions
            .get(definition.function.0 as usize)
            .filter(|entry| entry.id == definition.function && entry.environment == Some(*id))
            .ok_or_else(|| fail("actual closure type has another definition's entry"))?;
        if entry.return_type != definition.result
            || entry.parameters.len() != definition.parameters.len()
        {
            return Err(fail("actual closure entry loses its complete signature"));
        }
        for (local, ty) in entry.parameters.iter().zip(&definition.parameters) {
            if entry.locals.get(local.0 as usize).map(|local| &local.ty) != Some(ty) {
                return Err(fail("actual closure parameter differs from its directory"));
            }
        }
        Ok(())
    }
    fn value_type(&self, value: &SemanticTypeValue, b: &mut Budget) -> CompileResult<Type> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(fail("Core type survived into executable semantic context"));
        };
        crate::dyn_metadata::shape(life, b)?;
        if classes.len() != life.binder_count as usize {
            return Err(fail("semantic value omits a complete class occurrence"));
        }
        match ty {
            Type::Address { .. } if original::strings::is_byte_type(ty) && empty(life) => {}
            Type::BorrowedStorage {
                life: intrinsic, ..
            } if intrinsic == life => {}
            Type::Address { .. } if life.binder_count == 1 => {}
            Type::MutexGuard { .. }
                if life.binder_count == 1
                    && life.slots.is_empty()
                    && life.retained.is_empty()
                    && life.nominal_arguments.is_empty() => {}
            Type::Desc(inner) => {
                crate::dyn_metadata::ty(inner, b)?;
                self.resource_zero(&Type::Managed(inner.clone()), b, 0)?;
                if life.binder_count != 1
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || !matches!(life.slots.as_slice(),[slot] if slot.path.is_empty() && slot.binder==0 && slot.mode==StoredBorrowMode::WritableDescription)
                {
                    return Err(fail(
                        "semantic description loses its complete weak outer occurrence",
                    ));
                }
            }
            Type::Address { mutable, inner }
                if matches!(inner.as_ref(), Type::MutexGuard { .. }) =>
            {
                self.ordinary_type(inner, b)?;
                let [row] = life.retained.as_slice() else {
                    return Err(fail(
                        "semantic guard address omits its intrinsic Pointee class",
                    ));
                };
                if row.path.as_slice() != [LifeTypeProjection::Pointee]
                    || row.class != 0
                    || row.binder >= life.binder_count
                {
                    return Err(fail(
                        "semantic guard address changes its exact intrinsic class selector",
                    ));
                }
                crate::dyn_metadata::ty(inner, b)?;
                let expected = value::address(
                    *mutable,
                    value::Class::Local(0),
                    value::Value {
                        ty: inner.as_ref().clone(),
                        life: StorageLifeShape {
                            binder_count: 1,
                            slots: Vec::new(),
                            retained: Vec::new(),
                            nominal_arguments: Vec::new(),
                        },
                        classes: vec![value::Class::Local(row.binder)],
                    },
                    b,
                )?;
                if expected.life != *life {
                    return Err(fail(
                        "semantic guard address changes its complete outer/Pointee embedding",
                    ));
                }
            }
            Type::Closure(id) => {
                let definition = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|definition| definition.id == *id)
                    .ok_or_else(|| fail("semantic closure type has another actual identity"))?;
                let count = definition
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("semantic closure has no complete intrinsic directory"))?
                    .fixed_binder_count;
                if life.binder_count != count
                    || !life.slots.is_empty()
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                {
                    return Err(fail(
                        "semantic closure value loses its opaque complete domain",
                    ));
                }
            }
            _ if empty(life) => {}
            _ => {
                return Err(unknown(
                    "semantic value needs its full type/class occurrence reconstruction",
                ));
            }
        }
        self.actual_type(ty, b)?;
        Ok(ty.clone())
    }
    fn actual_type(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        if let Type::Address { inner, .. } = ty {
            // Physical classification preserves the complete pointee type.
            // Header/SSA class relations and the ordinary loan/epoch kernels
            // independently prove every actual address operation.
            b.spend(1, WORK)?;
            self.ordinary_type(inner, b)
        } else {
            self.ordinary_type(ty, b)
        }
    }
    fn ancestor<'b>(
        &'b self,
        mut instance: &'b SemanticInstance,
        scope: SemanticScopeRef,
        b: &mut Budget,
    ) -> CompileResult<&'b SemanticInstance> {
        let SemanticScopeRef::Ancestor(depth) = scope;
        if depth as usize > b.partial_depth {
            return Err(fail("semantic lexical ancestor exceeds its finite bound"));
        }
        for _ in 0..depth {
            instance = self.inventory.instance(
                instance
                    .parent
                    .ok_or_else(|| fail("semantic lexical ancestor is absent"))?,
                b,
            )?;
        }
        Ok(instance)
    }
    fn ty(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        Ok(self.recipe_value(instance, recipe, b, depth)?.ty)
    }
    fn ty_closed(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(fail("original semantic type exceeds its finite bound"));
        }
        match recipe {
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() || *ty == Type::Unit => {
                Ok(ty.clone())
            }
            SemanticTypeRecipe::Exact(value) if empty(&value.life) && value.classes.is_empty() => {
                self.ordinary_type(&value.ty, b)?;
                if let Type::Closure(id) = &value.ty
                    && !self.capture_directory(*id, b)?.is_empty()
                {
                    return Err(fail(
                        "exact closure recipe erases its actual intrinsic head domain",
                    ));
                }
                Ok(value.ty.clone())
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let mut actuals = Vec::new();
                for argument in arguments {
                    let SemanticActualRecipe::Type(recipe) = argument else {
                        return Err(unknown(
                            "original nominal application needs complete mixed constant and lifetime substitution",
                        ));
                    };
                    let ty = self.ty(instance, recipe, b, depth + 1)?;
                    b.spend(actuals.len().saturating_add(1), WORK)?;
                    actuals
                        .try_reserve(1)
                        .map_err(|_| fail("original nominal actual allocation failed"))?;
                    actuals.push(ty);
                }
                original::nominal::actual(*definition, &actuals, self.inventory, b, depth + 1)
            }
            SemanticTypeRecipe::Tuple(fields) => {
                if fields.is_empty() {
                    return Ok(Type::Unit);
                }
                let mut actuals = Vec::new();
                for field in fields {
                    let ty = self.ty(instance, field, b, depth + 1)?;
                    b.spend(actuals.len().saturating_add(1), WORK)?;
                    actuals
                        .try_reserve(1)
                        .map_err(|_| fail("original product actual allocation failed"))?;
                    actuals.push(ty);
                }
                Ok(Type::Tuple(actuals))
            }
            SemanticTypeRecipe::Formal(reference) => {
                let instance = self.ancestor(instance, reference.scope, b)?;
                let definition = self.inventory.definition(instance.definition, b)?;
                if !matches!(
                    definition.parameters.get(reference.ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("type recipe changes an original mixed formal's kind"));
                }
                match instance.actuals.get(reference.ordinal as usize) {
                    Some(SemanticActual::Type(value)) => self.value_type(value, b),
                    _ => Err(fail(
                        "type recipe has no complete actual at its original ordinal",
                    )),
                }
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                let instance = self.ancestor(instance, *scope, b)?;
                let definition = self.inventory.definition(instance.definition, b)?;
                if !matches!(
                    definition
                        .environment
                        .get(*ordinal as usize)
                        .map(|formal| &formal.kind),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail(
                        "type environment selector changes its checked slot kind",
                    ));
                }
                match instance
                    .environment
                    .as_ref()
                    .and_then(|environment| environment.actuals.get(*ordinal as usize))
                {
                    Some(SemanticActual::Type(value)) => self.value_type(value, b),
                    _ => Err(fail("type environment has no sealed actual type")),
                }
            }
            SemanticTypeRecipe::OriginalType { scope, source } => {
                let ty = self.original.source_type(
                    instance,
                    *scope,
                    source,
                    self.program,
                    self.inventory,
                    b,
                )?;
                self.actual_type(&ty, b)?;
                Ok(ty)
            }
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment,
            } => {
                let mut found = None;
                for candidate in &self.program.instances {
                    b.spend(1, WORK)?;
                    if candidate.definition != *definition || candidate.parent != Some(instance.id)
                    {
                        continue;
                    }
                    let Some(actual_environment) = &candidate.environment else {
                        continue;
                    };
                    if !self.actual_vectors(
                        instance,
                        arguments,
                        &candidate.actuals,
                        b,
                        depth + 1,
                    )? || !self.actual_vectors(
                        instance,
                        environment,
                        &actual_environment.actuals,
                        b,
                        depth + 1,
                    )? {
                        continue;
                    }
                    let SemanticClosureTarget::Typed(id) = actual_environment.closure else {
                        return Err(fail("closure type retains an unresolved original target"));
                    };
                    if found.is_some_and(|previous| previous != id) {
                        return Err(fail(
                            "original closure application has ambiguous concrete identities",
                        ));
                    }
                    found = Some(id);
                }
                let ty =
                    Type::Closure(found.ok_or_else(|| {
                        fail("original closure type has no exact actual instance")
                    })?);
                self.ordinary_type(&ty, b)?;
                Ok(ty)
            }
            _ => Err(unknown(
                "original semantic type constructor requires complete class and nominal evaluation",
            )),
        }
    }
    fn actual_vectors(
        &self,
        context: &SemanticInstance,
        recipes: &[SemanticActualRecipe],
        actuals: &[SemanticActual],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        if depth > b.partial_depth {
            return Err(fail("original actual recipe exceeds its finite bound"));
        }
        b.spend(recipes.len().saturating_add(actuals.len()), WORK)?;
        if recipes.len() != actuals.len() {
            return Ok(false);
        }
        for (recipe, actual) in recipes.iter().zip(actuals) {
            let expected = match recipe {
                SemanticActualRecipe::Type(recipe) => crate::semantic::value::Argument::Type(
                    self.recipe_value(context, recipe, b, depth + 1)?,
                ),
                SemanticActualRecipe::Life(recipe) => crate::semantic::value::Argument::Life(
                    self.life(context, recipe, b, depth + 1)?,
                ),
                _ => {
                    return Err(unknown(
                        "original constant actual needs its complete carrier-aware expression proof",
                    ));
                }
            };
            if self.actual_value(context, actual, b, depth + 1)? != expected {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn requirements(
        &self,
        context: &SemanticInstance,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(requirements.len(), WORK)?;
        for requirement in requirements {
            match requirement {
                SemanticRequirementRecipe::Structural { ty, bound } => {
                    let ty = self.ty(context, ty, b, 0)?;
                    let predicate = match bound {
                        DynStructuralBound::Copy => original::Predicate::Copy,
                        DynStructuralBound::Value => original::Predicate::Value,
                        DynStructuralBound::SyncDrop => original::Predicate::SyncDrop,
                        DynStructuralBound::StoreElement => original::Predicate::StoreElement,
                    };
                    if !self.structural_actual(&ty, predicate, b, 0)? {
                        return Err(fail(
                            "actual type fails its original structural requirement",
                        ));
                    }
                }
                SemanticRequirementRecipe::Builtin { ty, trait_kind } => {
                    if !matches!(
                        trait_kind,
                        SemanticBuiltinTrait::Copy
                            | SemanticBuiltinTrait::Value
                            | SemanticBuiltinTrait::SyncDrop
                            | SemanticBuiltinTrait::StoreElement
                    ) {
                        self.capability_requirement(context, ty, *trait_kind, b)?;
                        continue;
                    }
                    let ty = self.ty(context, ty, b, 0)?;
                    let predicate = match trait_kind {
                        SemanticBuiltinTrait::Copy => original::Predicate::Copy,
                        SemanticBuiltinTrait::Value => original::Predicate::Value,
                        SemanticBuiltinTrait::SyncDrop => original::Predicate::SyncDrop,
                        SemanticBuiltinTrait::StoreElement => original::Predicate::StoreElement,
                        _ => {
                            return Err(unknown(
                                "sealed semantic requirement needs its complete provider and execution proof",
                            ));
                        }
                    };
                    if !self.structural_actual(&ty, predicate, b, 0)? {
                        return Err(fail("actual type fails its original sealed requirement"));
                    }
                }
                SemanticRequirementRecipe::Callable {
                    ty,
                    kind,
                    signature,
                } => {
                    self.callable_requirement(context, ty, *kind, signature, b)?;
                }
                SemanticRequirementRecipe::TypeEqual { left, right } => {
                    if self.recipe_value(context, left, b, 0)?
                        != self.recipe_value(context, right, b, 0)?
                    {
                        return Err(fail(
                            "actual type specialization does not satisfy its original equality",
                        ));
                    }
                }
                SemanticRequirementRecipe::Trait { ty, application } => {
                    let receiver = self.recipe_value(context, ty, b, 0)?;
                    let arguments = self.trait_actuals(context, application, b, 0)?;
                    self.actual_provider(&receiver, application.declaration, &arguments, b, 0)?;
                }
                _ => {
                    return Err(unknown(
                        "original semantic premise needs independent caller and rigid-definition proof",
                    ));
                }
            }
        }
        Ok(())
    }
    fn function_instance(
        &self,
        target: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<&SemanticInstance> {
        self.shared_zero_view(target, b)
    }
    fn instances(&self, b: &mut Budget) -> CompileResult<()> {
        for instance in &self.program.instances {
            let definition = self.inventory.definition(instance.definition, b)?;
            if matches!(instance.target, SemanticInstanceTarget::Scope)
                && matches!(definition.body, SemanticDefinitionBody::Scope(_))
            {
                self.scope_arguments(instance, b)?;
                self.original_scope_self(instance, b, 0)?;
                continue;
            }
            if matches!(instance.target, SemanticInstanceTarget::Scope) {
                if !matches!(definition.body, SemanticDefinitionBody::Nominal { .. })
                    || instance.parent.is_some()
                    || instance.environment.is_some()
                    || instance.self_type.is_some()
                    || !instance.header_embedding.is_empty()
                    || !matches!(
                        instance.lifetime_instantiation,
                        SemanticLifetimeInstantiation::Rigid
                    )
                {
                    return Err(unknown(
                        "original scope instance needs complete lexical, receiver and class proof",
                    ));
                }
                if instance.actuals.len() != definition.parameters.len() {
                    return Err(fail("nominal scope omits full mixed actuals"));
                }
                let mut actuals = Vec::new();
                for (formal, actual) in definition.parameters.iter().zip(&instance.actuals) {
                    let (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) =
                        (formal, actual)
                    else {
                        return Err(unknown(
                            "nominal scope needs its complete constant and lifetime actual proof",
                        ));
                    };
                    let ty = self.value_type(value, b)?;
                    self.requirements(instance, requirements, b)?;
                    b.spend(actuals.len().saturating_add(1), WORK)?;
                    actuals
                        .try_reserve(1)
                        .map_err(|_| fail("nominal scope allocation failed"))?;
                    actuals.push(ty);
                }
                self.requirements(instance, &definition.requirements, b)?;
                original::nominal::actual(definition.id, &actuals, self.inventory, b, 0)?;
                continue;
            }
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                return Err(unknown(
                    "nominal and impl semantic contexts need their complete original evaluation",
                ));
            };
            let function = self.inventory.function(target, b)?;
            match (definition.kind, function.environment, &instance.environment) {
                (SemanticDefinitionKind::Function, None, None) => {}
                (SemanticDefinitionKind::Method, None, None) => {
                    let parent = self.inventory.instance(
                        instance
                            .parent
                            .ok_or_else(|| fail("actual method has no true lexical Self scope"))?,
                        b,
                    )?;
                    let expected = self.original_scope_self(parent, b, 0)?;
                    let actual = instance.self_type.as_ref().ok_or_else(|| {
                        fail("actual method omits its complete original Self value")
                    })?;
                    if self.resolved(instance, actual, b, 0)? != expected {
                        return Err(fail(
                            "actual method Self differs from its true original parent scope",
                        ));
                    }
                }
                (SemanticDefinitionKind::Closure, Some(actual), Some(environment))
                    if environment.closure == SemanticClosureTarget::Typed(actual) => {}
                _ => {
                    return Err(unknown(
                        "original function kind requires its exact entry and creator environment proof",
                    ));
                }
            }
            if !matches!(
                instance.lifetime_instantiation,
                SemanticLifetimeInstantiation::Rigid
            ) || instance.self_type.is_some()
                && definition.kind != SemanticDefinitionKind::Method
            {
                return Err(unknown(
                    "semantic nonempty header or partition needs its complete rigid class proof",
                ));
            }
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                protocol_contracts,
                foreign,
                resume_bound,
                ..
            } = &definition.body
            else {
                return Err(fail(
                    "actual function is relabelled as another original kind",
                ));
            };
            b.spend(
                protocol_contracts
                    .len()
                    .saturating_add(function.protocol_contracts.len()),
                WORK,
            )?;
            for declaration in foreign.iter().chain(function.foreign.iter()) {
                b.spend(
                    declaration.abi.len().saturating_add(
                        declaration.symbol.len() + declaration.noescape_parameters.len() + 1,
                    ),
                    WORK,
                )?;
            }
            if *foreign != function.foreign || *resume_bound != function.resume_bound {
                return Err(fail(
                    "actual function changes its original foreign or resume declaration",
                ));
            }
            if *protocol_contracts != function.protocol_contracts {
                return Err(fail(
                    "actual protocol table differs from its complete original declaration",
                ));
            }
            if signature.bound_lifetimes != 0 {
                return Err(unknown(
                    "original function binder frames require scoped class interpretation",
                ));
            }
            if instance.actuals.len() != definition.parameters.len() {
                return Err(fail("original formal vector is incomplete"));
            }
            for (ordinal, (formal, actual)) in definition
                .parameters
                .iter()
                .zip(&instance.actuals)
                .enumerate()
            {
                match (formal, actual) {
                    (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) => {
                        let value = self.resolved(instance, value, b, 0)?;
                        self.actual_type(&value.ty, b)?;
                        self.requirements(instance, requirements, b)?;
                    }
                    (SemanticFormal::Lifetime, SemanticActual::Life(actual)) => {
                        if !matches!(actual,SemanticClassExpr::Formal {scope:SemanticScopeRef::Ancestor(0),ordinal:actual} if *actual as usize==ordinal)
                        {
                            return Err(fail(
                                "actual rigid Life changes its full original formal ordinal",
                            ));
                        }
                        crate::semantic::value::class(instance, actual, self.inventory, b, 0)?;
                    }
                    (SemanticFormal::Const { carrier }, SemanticActual::Const { ty, value }) => {
                        let expected = self.recipe_value(instance, carrier, b, 0)?;
                        let actual = self.resolved(instance, ty, b, 0)?;
                        if expected != actual
                            || !actual.classes.is_empty()
                            || actual.ty != value.ty()
                        {
                            return Err(fail(
                                "actual Const formal changes its original complete carrier",
                            ));
                        }
                    }
                    _ => {
                        return Err(fail(
                            "actual original mixed argument changes its formal kind",
                        ));
                    }
                }
            }
            if !definition.environment.is_empty() {
                let parent = self.inventory.instance(
                    instance.parent.ok_or_else(|| {
                        fail("semantic environment has no actual creator context")
                    })?,
                    b,
                )?;
                let actual = instance
                    .environment
                    .as_ref()
                    .ok_or_else(|| fail("semantic environment is omitted"))?;
                if definition.environment.len() != actual.actuals.len() {
                    return Err(fail("semantic environment omits an original slot"));
                }
                for (formal, value) in definition.environment.iter().zip(&actual.actuals) {
                    // Both the kind and source are interpreted in the real
                    // parent, before the child may read this checked slot.
                    match (&formal.kind, value) {
                        (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) => {
                            self.value_type(value, b)?;
                            self.requirements(parent, requirements, b)?;
                            let SemanticActualRecipe::Type(source) = &formal.source else {
                                return Err(fail(
                                    "type environment source changes its original kind",
                                ));
                            };
                            if self.recipe_value(parent, source, b, 0)?
                                != self.resolved(parent, value, b, 0)?
                            {
                                return Err(fail(
                                    "actual environment differs from its original creator source",
                                ));
                            }
                        }
                        _ => {
                            return Err(unknown(
                                "semantic life/const environment needs complete sealed creator proof",
                            ));
                        }
                    }
                }
            } else if instance
                .environment
                .as_ref()
                .is_some_and(|environment| !environment.actuals.is_empty())
            {
                return Err(fail(
                    "actual environment invents an undeclared semantic slot",
                ));
            }
            self.requirements(instance, &definition.requirements, b)?;
            b.spend(
                function
                    .parameters
                    .len()
                    .saturating_add(function.values.len())
                    .saturating_add(function.locals.len()),
                WORK,
            )?;
            if function.parameters.len() != signature.parameters.len()
                || function.call_kind != signature.call_kind
            {
                return Err(fail(
                    "actual function header changes its original signature",
                ));
            }
            for (parameter, parameter_recipe) in
                function.parameters.iter().zip(&signature.parameters)
            {
                let actual = function
                    .locals
                    .get(parameter.0 as usize)
                    .ok_or_else(|| fail("actual parameter local is absent"))?;
                let expected = self.ty(instance, &parameter_recipe.ty, b, 0)?;
                if !matches!(
                    parameter_recipe.mode,
                    PassingMode::Infer | PassingMode::Copy
                ) && !matches!(
                    (&expected, parameter_recipe.mode),
                    (Type::Address { mutable: false, .. }, PassingMode::Borrow)
                        | (Type::Address { mutable: true, .. }, PassingMode::BorrowMut)
                        | (Type::Managed(_), PassingMode::Give | PassingMode::Move)
                        | (Type::Desc(_), PassingMode::Describe)
                ) {
                    return Err(unknown(
                        "semantic parameter mode needs an independently checked resource relation",
                    ));
                }
                self.actual_type(&expected, b)?;
                if actual.ty != expected {
                    return Err(fail(
                        "actual parameter changes its complete original nominal substitution",
                    ));
                }
            }
            if function.return_type != self.ty(instance, &signature.result, b, 0)?
                || !(function.result_mode == signature.result_mode
                    || signature.result_mode == PassingMode::Infer)
            {
                return Err(fail(
                    "actual result differs from the original instantiated signature",
                ));
            }
            self.ordinary_type(&function.return_type, b)?;
            self.header_relation(instance, function, signature, b)?;
            for ty in function
                .locals
                .iter()
                .map(|local| &local.ty)
                .chain(&function.values)
            {
                self.actual_type(ty, b)?;
            }
            if let Some(closure) = function.environment {
                self.capture_directory(closure, b)?;
            }
        }
        for function in &self.module.functions {
            if self.function_worlds(function.id, b)?.is_empty() {
                return Err(fail("actual function has no complete original world"));
            }
        }
        Ok(())
    }
}

pub(super) fn check(
    module: &TypedModule,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    original: &original::Checked,
    b: &mut Budget,
) -> CompileResult<()> {
    let checker = Check {
        module,
        program,
        inventory,
        original,
        scope_queries: std::cell::RefCell::new(BTreeSet::new()),
        trait_queries: std::cell::RefCell::new(Vec::new()),
    };
    checker.dyn_tables(b)?;
    checker.instances(b)?;
    checker.sites(b)?;
    checker.constructor_class_sources(b)?;
    // Publish only a completed intrinsic-domain result. Raw Some(0), an
    // absent environment, or scalar-looking entry ABI alone is not evidence.
    let mut zero = BTreeSet::new();
    for closure in &module.closures {
        b.spend(1, WORK)?;
        if !closure
            .storage_life
            .as_ref()
            .is_some_and(|life| life.fixed_binder_count == 0)
        {
            continue;
        }
        checker.ordinary_type(&Type::Closure(closure.id), b)?;
        if !checker.capture_directory(closure.id, b)?.is_empty() {
            return Err(fail(
                "zero closure directory contains a nonzero or unknown actual capture",
            ));
        }
        for capture in &closure.captures {
            checker.zero_data(capture, b, 0)?;
        }
        b.lookup(zero.len(), WORK)?;
        zero.insert(closure.id);
    }
    for id in zero {
        b.lookup(b.semantic_zero_closures.len(), WORK)?;
        b.semantic_zero_closures.insert(id);
    }
    for closure in &module.closures {
        for capture in &closure.captures {
            if matches!(capture, Type::BorrowedStorage { storage, life }
                if matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. } | Type::Tuple(_))
                    && life.slots.is_empty() && storage.is_copy_value())
            {
                crate::stored_flow::qualification(module, capture, inventory.limits, b)?;
            }
        }
        // instances()+sites() above proved the actual entry, complete intrinsic
        // directory, real constructors and original class images. Preserve the
        // nonzero domain; this table is never a zero/Copy/loan certificate.
        b.lookup(b.semantic_closures_proven.len(), WORK)?;
        b.semantic_closures_proven.insert(closure.id);
    }
    for declaration in &module.dyn_declarations {
        // The original checker proved all definitions (also unused bodies),
        // exact raw scope links, full mixed premises, member conformance and
        // associated RHS before instances/sites above proved their actuals.
        // This only prevents the legacy runtime-only inventory from treating
        // a complete original declaration as an executable interface table.
        b.lookup(b.semantic_declarations_proven.len(), WORK)?;
        b.semantic_declarations_proven.insert(declaration.id);
    }
    // No partial table or witness graph is visible to a later kernel. The
    // complete original table bridge, every function world and all actual
    // semantic operations above succeeded before these exact IDs are sealed.
    for table in &module.dyn_vtables {
        b.lookup(b.semantic_dyn_tables_proven.len(), WORK)?;
        if !b.semantic_dyn_tables_proven.insert(table.id) {
            return Err(fail("completed dynamic table identity is repeated"));
        }
    }
    // Actual operation/SSA/cleanup checks still run in the ordinary kernels.
    // No semantic context manufactures a target, loan, epoch or effect.
    Ok(())
}
