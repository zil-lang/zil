//! Actual owning erasure preserves the original provider, whole Self and
//! complete interface key. Formation and a finite cleanup upper are necessary
//! invariants; concrete cleanup is separately reconstructed from actual code.
use super::*;
use crate::semantic::value::{self, Argument, Value};

impl Check<'_, '_> {
    fn dyn_owner_interface(
        &self,
        id: DynInterfaceId,
        b: &mut Budget,
    ) -> CompileResult<&DynInterface> {
        let mut selected = None;
        for interface in &self.module.dyn_interfaces {
            b.spend(1, WORK)?;
            if interface.id == id && selected.replace(interface).is_some() {
                return Err(fail(
                    "owning Dyn has several actual interfaces for one exact identity",
                ));
            }
        }
        let interface =
            selected.ok_or_else(|| fail("owning Dyn type selects no actual interface"))?;
        crate::dyn_metadata::recipe(&interface.application, b)?;
        crate::dyn_metadata::retained_shape(&interface.retained_life, b)?;
        if let Some(context) = &interface.context {
            crate::dyn_metadata::ty(context, b)?;
        }
        for inherited in &interface.inherited {
            crate::dyn_metadata::recipe(inherited, b)?;
        }
        for binding in &interface.associated_types {
            crate::dyn_metadata::recipe(&binding.member, b)?;
            crate::dyn_metadata::ty(&binding.ty, b)?;
            crate::dyn_metadata::shape(&binding.life, b)?;
            b.spend(binding.classes.len(), WORK)?;
        }
        for binding in &interface.associated_constants {
            crate::dyn_metadata::recipe(&binding.member, b)?;
            crate::dyn_metadata::ty(&binding.ty, b)?;
            b.spend(1, WORK)?;
        }
        crate::dyn_context::validate(self.module, interface, b)?;
        crate::dyn_provider::application(self.module, interface, b)?;
        let scope = original::declaration_scope(
            self.program,
            interface.application.declaration,
            self.inventory,
            b,
        )?;
        let declaration = self.inventory.definition(scope, b)?;
        let raw = self
            .inventory
            .original_dyn(interface.application.declaration, b)?;
        if raw.kind != DynDeclarationKind::Trait
            || !matches!(declaration.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration:id }) if id==interface.application.declaration)
            || declaration.parameters.len() != interface.application.actuals.len()
        {
            return Err(fail(
                "owning Dyn formation changes its full original trait kind, scope or application arity",
            ));
        }
        // An interface's complete public slots and original method scopes are
        // checked by dyn_tables before any actual world or site is published.
        // Never infer an associated equation or a closed supertrait from name.
        if !raw
            .direct_supertraits
            .as_ref()
            .ok_or_else(|| unknown("owning Dyn original supertrait inventory is unknown"))?
            .is_empty()
            || !raw
                .associated_definitions
                .as_ref()
                .ok_or_else(|| unknown("owning Dyn original associated inventory is unknown"))?
                .is_empty()
        {
            return Err(unknown(
                "owning Dyn formation requires its full original inherited and associated equation closure",
            ));
        }
        Ok(interface)
    }
    pub(super) fn dyn_value_type(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "owning Dyn formation exceeds its complete lexical/type depth",
            ));
        }
        let (id, owned) = match ty {
            Type::Dyn { interface } => (*interface, false),
            Type::OwnedDyn { interface } => (*interface, true),
            _ => return Ok(false),
        };
        let interface = self.dyn_owner_interface(id, b)?;
        if owned {
            crate::dyn_cleanup::metadata(interface.owning_cleanup.as_ref(), b)?;
            crate::dyn_cleanup::bound(interface, b)?;
        }
        Ok(true)
    }
    /// Reconstruct the whole physical domain, using local class labels only
    /// to form its shape. These labels never escape as occurrence evidence.
    pub(super) fn dyn_value_domain(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("Dyn value domain exceeds its complete type depth"));
        }
        match ty {
            Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                self.dyn_value_type(ty, b, depth + 1)?;
                let life = value::dyn_domain(ty, self.inventory, b)?;
                let mut classes = Vec::new();
                for ordinal in 0..life.binder_count {
                    self.push_value(&mut classes, value::Class::Local(ordinal), b)?;
                }
                crate::dyn_metadata::ty(ty, b)?;
                Ok(Some(Value::new(ty.clone(), life, classes)))
            }
            Type::ErrorUnion { error, result } => {
                let Some(result) = self.dyn_value_domain(result, b, depth + 1)? else {
                    return Ok(None);
                };
                self.zero_data(error, b, depth + 1)?;
                crate::dyn_metadata::ty(error, b)?;
                Ok(Some(value::error_union(value::scalar(error.as_ref().clone()), result, b)?))
            }
            Type::Address { mutable, inner } => {
                let Some(inner) = self.dyn_value_domain(inner, b, depth + 1)? else {
                    return Ok(None);
                };
                let outer = value::Class::Local(inner.life.binder_count);
                Ok(Some(value::address(*mutable, outer, inner, b)?))
            }
            _ => Ok(None),
        }
    }
    fn dyn_owner_arguments(
        &self,
        actuals: &[DynGenericActual],
        expected: &[Argument],
        context: &[value::Class],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if actuals.len() != expected.len() {
            return Err(fail("owning Dyn changes its original full mixed arity"));
        }
        let class = |binder: StorageBinder| -> CompileResult<&value::Class> {
            let StorageBinder::Fixed(index) = binder else {
                return Err(fail("owning Dyn stored actual escapes its fixed Context"));
            };
            context
                .get(index as usize)
                .ok_or_else(|| fail("owning Dyn class leaves its complete Context"))
        };
        for (actual, expected) in actuals.iter().zip(expected) {
            crate::dyn_metadata::recipe(actual, b)?;
            match (actual, expected) {
                (DynGenericActual::Type(ty), Argument::Type(expected)) => {
                    crate::dyn_metadata::ty(&expected.ty, b)?;
                    crate::dyn_metadata::shape(&expected.life, b)?;
                    if *ty != expected.ty || !empty(&expected.life) || !expected.classes.is_empty()
                    {
                        return Err(fail(
                            "owning Dyn plain Type loses its complete original domain",
                        ));
                    }
                    self.actual_type(ty, b)?;
                }
                (DynGenericActual::QualifiedType(actual), Argument::Type(expected)) => {
                    crate::dyn_metadata::ty(&expected.ty, b)?;
                    crate::dyn_metadata::shape(&expected.life, b)?;
                    if actual.ty != expected.ty
                        || actual.life != expected.life
                        || actual.classes.len() != expected.classes.len()
                        || actual.classes.len() != actual.life.binder_count as usize
                    {
                        return Err(fail(
                            "owning Dyn qualified Type changes its full original domain",
                        ));
                    }
                    self.actual_type(&actual.ty, b)?;
                    for (binder, expected) in actual.classes.iter().zip(&expected.classes) {
                        b.spend(1, WORK)?;
                        if class(*binder)? != expected {
                            return Err(fail(
                                "owning Dyn Type changes its exact original Context class",
                            ));
                        }
                    }
                }
                (DynGenericActual::Const(actual), Argument::Const { carrier, value }) => {
                    crate::dyn_metadata::ty(carrier, b)?;
                    if actual.ty() != *carrier || actual != value {
                        return Err(fail(
                            "owning Dyn Const changes its full carrier or scalar bits",
                        ));
                    }
                }
                (DynGenericActual::Life(actual), Argument::Life(expected))
                    if class(*actual)? == expected => {}
                _ => {
                    return Err(fail(
                        "owning Dyn changes its original mixed actual kind or class",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn dyn_sum_type(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        Ok(matches!(ty, Type::ErrorUnion { result, .. }
            if matches!(result.as_ref(), Type::Dyn { .. } | Type::OwnedDyn { .. })))
    }
    pub(super) fn dyn_owned_result(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let mut result = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After }
                    if actual == block && instruction as usize == index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            if !matches!(
                self.original
                    .intrinsic(instance, site.recipe, self.inventory, b)?,
                Some(original::sealed::Intrinsic::OwnedDyn)
            ) {
                continue;
            }
            let value = self.dyn_owned_operation(
                instance,
                site.recipe,
                function,
                block,
                index,
                actuals,
                b,
            )?;
            if result.replace(value).is_some() {
                return Err(fail(
                    "owning Dyn result has competing original source occurrences",
                ));
            }
        }
        result.ok_or_else(|| fail("owning Dyn SSA lacks its original full construction recipe"))
    }
    pub(super) fn dyn_owned_projection(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        result: ValueId,
        parent: ValueId,
        error_arm: bool,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        operations::available_before(function, (at.0, at.1, SemanticSiteEdge::Before), parent, b)?;
        let parent = self.ssa_value(instance, function, parent, b, depth + 1)?;
        self.dyn_owned_sum_child(function, result, parent, error_arm, b, depth + 1)
    }
    pub(super) fn dyn_owned_local_projection(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        result: ValueId,
        local: LocalId,
        path: &[PlaceProjection],
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let [PlaceProjection::ErrorArm(error_arm)] = path else {
            return Err(unknown(
                "Dyn local projection needs its exact complete sum arm",
            ));
        };
        operations::initialized_local_before(function, at, local, b)?;
        let parent = self.local_value(instance, function, local, at, b, depth + 1)?;
        // The actual resource CFG separately proves the selected active arm,
        // consuming slot, outstanding loans and subsequent partial cleanup.
        self.dyn_owned_sum_child(function, result, parent, *error_arm, b, depth + 1)
    }
    fn dyn_owned_sum_child(
        &self,
        function: &TypedFunction,
        result: ValueId,
        parent: Value,
        error_arm: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let Type::ErrorUnion {
            error,
            result: success,
        } = &parent.ty
        else {
            return Err(fail("Dyn arm projection has no actual complete sum"));
        };
        let child = if error_arm { error } else { success };
        if function.values.get(result.0 as usize) != Some(child.as_ref()) {
            return Err(fail(
                "Dyn arm projection changes its complete physical child",
            ));
        }
        self.zero_data(error, b, depth + 1)?;
        let life = value::dyn_domain(success, self.inventory, b)?;
        let mut classes = Vec::new();
        for child in 0..life.binder_count {
            let mut selected = None;
            for row in &parent.life.retained {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.path.as_slice()
                    == [LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))]
                    && row.class == child
                    && selected.replace(row.binder).is_some()
                {
                    return Err(fail("Dyn sum repeats an intrinsic success child"));
                }
            }
            let binder =
                selected.ok_or_else(|| fail("Dyn sum omits an intrinsic success child"))?;
            self.push_value(
                &mut classes,
                parent
                    .classes
                    .get(binder as usize)
                    .ok_or_else(|| fail("Dyn success child leaves its complete sum domain"))?
                    .clone(),
                b,
            )?;
        }
        crate::dyn_metadata::ty(error, b)?;
        crate::dyn_metadata::ty(success, b)?;
        let success = Value::new(success.as_ref().clone(), life, classes);
        let expected =
            value::error_union(value::scalar(error.as_ref().clone()), success.copy(b)?, b)?;
        if !expected.equal_paid(&parent, b)? {
            return Err(fail(
                "Dyn projection changes the complete original sum occurrence relation",
            ));
        }
        if error_arm {
            crate::dyn_metadata::ty(error, b)?;
            Ok(value::scalar(error.as_ref().clone()))
        } else {
            Ok(success)
        }
    }
    pub(super) fn dyn_owned_borrow(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        local: LocalId,
        mutable: bool,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        operations::initialized_local_before(function, at, local, b)?;
        let owner = self.local_value(instance, function, local, at, b, depth + 1)?;
        let Type::OwnedDyn { interface } = owner.ty else {
            return Err(fail("Dyn owned view changes its real owning local"));
        };
        self.dyn_value_type(&owner.ty, b, depth + 1)?;
        let (_, life, classes) = owner.into_parts(b)?;
        let borrowed = Value::new(Type::Dyn { interface }, life, classes);
        let value = value::address(
            mutable,
            value::Class::Reborrow(function.id, id),
            borrowed,
            b,
        )?;
        if function.values.get(id.0 as usize) != Some(&value.ty) {
            return Err(fail(
                "Dyn owner view changes its full actual borrowed interface",
            ));
        }
        Ok(value)
    }
    pub(super) fn dyn_owned_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let original =
            self.original
                .dyn_creation(instance, site, self.program, self.inventory, b)?;
        let Type::ErrorUnion { error, result } = &original.output.ty else {
            return Err(fail(
                "original owning Dyn construction loses its full allocation-result carrier",
            ));
        };
        if !crate::dyn_dispatch::is_create_error(error) {
            return Err(fail(
                "original owning Dyn construction changes its canonical allocation error declaration",
            ));
        }
        let Type::OwnedDyn {
            interface: expected_interface,
        } = result.as_ref()
        else {
            return Err(fail(
                "original owning Dyn construction has another successful carrier",
            ));
        };
        self.dyn_value_type(result, b, 0)?;
        let expected_borrowed = &original.interface;
        let [
            SemanticActual::Type(written),
            SemanticActual::Type(source_claim),
        ] = actuals
        else {
            return Err(fail(
                "owning Dyn construction changes its complete interface and inferred Self Type actuals",
            ));
        };
        if self.resolved(instance, written, b, 0)? != *expected_borrowed
            || self.resolved(instance, source_claim, b, 0)? != original.source
        {
            return Err(fail(
                "owning Dyn construction changes its original written interface or complete inferred Self actual",
            ));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|row| row.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("owning Dyn source call has no exact actual instruction"))?;
        let InstructionKind::MakeDynOwned {
            interface,
            vtable,
            source,
            route,
        } = instruction.kind
        else {
            return Err(fail(
                "owning Dyn source call is replaced by another actual opcode",
            ));
        };
        if interface != *expected_interface || route != DynOwnedStorage::Heap {
            return Err(fail(
                "owning Dyn construction changes its exact original interface or explicit Heap allocation route",
            ));
        }
        let interface = self.dyn_owner_interface(interface, b)?;
        let mut selected = None;
        for table in &self.module.dyn_vtables {
            b.spend(1, WORK)?;
            if table.id == vtable && selected.replace(table).is_some() {
                return Err(fail(
                    "owning Dyn construction selects an ambiguous actual vtable identity",
                ));
            }
        }
        let table = selected
            .ok_or_else(|| fail("owning Dyn construction selects no actual provider vtable"))?;
        crate::dyn_metadata::recipe_table(table, b)?;
        crate::dyn_metadata::ty(&table.self_type, b)?;
        if table.interface != interface.id
            || table.owned_storage != Some(DynOwnedStorage::Heap)
            || table.cleanup.is_some()
        {
            return Err(fail(
                "owning Dyn construction changes its exact provider interface, storage route or compiler cleanup role",
            ));
        }
        let life = table.self_life.as_ref().ok_or_else(|| {
            unknown("owning Dyn vtable omits its complete independently checked Self domain")
        })?;
        if table.self_type != original.source.ty
            || *life != original.source.life
            || table.self_classes.len() != original.source.classes.len()
        {
            return Err(fail(
                "owning Dyn vtable changes its complete original Self domain",
            ));
        }
        for (binder, source_class) in table.self_classes.iter().zip(&original.source.classes) {
            b.spend(1, WORK)?;
            let StorageBinder::Fixed(index) = binder else {
                return Err(fail(
                    "owning Dyn Self uses an invocation outside the stored Context",
                ));
            };
            if original.interface.classes.get(*index as usize) != Some(source_class) {
                return Err(fail(
                    "owning Dyn Self changes its original class-to-Context relation",
                ));
            }
        }
        // The constructor and the implementation have distinct rigid worlds.
        // The table's full Context is the explicit application map, not an
        // equation between caller and provider classes.
        let scope = DynInstantiationScope {
            fixed_binder_count: interface.retained_life.binder_count,
            invocation_binder_count: 0,
        };
        let raw_self = DynTypeValue {
            ty: table.self_type.clone(),
            life: life.clone(),
            classes: table.self_classes.clone(),
        };
        let mut worlds_checked = 0usize;
        for entry in &table.entries {
            let worlds = self.function_worlds(entry.function, b)?;
            if worlds.is_empty() {
                return Err(fail("owning Dyn provider has no complete method world"));
            }
            for world in worlds {
                let mut parent = world.parent;
                let mut provider = None;
                for _ in 0..=b.partial_depth {
                    let Some(id) = parent else {
                        break;
                    };
                    let current = self.inventory.instance(id, b)?;
                    if current.definition == original.provider {
                        provider = Some(current);
                        break;
                    }
                    parent = current.parent;
                }
                let provider = provider.ok_or_else(|| {
                    unknown("owning Dyn method omits its exact original provider ancestry")
                })?;
                let definition = self.inventory.definition(provider.definition, b)?;
                let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                    declaration,
                    trait_application,
                    ..
                }) = &definition.body
                else {
                    return Err(fail(
                        "owning Dyn provider is not its original implementation scope",
                    ));
                };
                if *declaration != table.implementation
                    || trait_application.declaration != interface.application.declaration
                {
                    return Err(fail(
                        "owning Dyn provider changes its original implementation or trait",
                    ));
                }
                self.scope_arguments(provider, b)?;
                let self_value = self.original_scope_self(provider, b, 0)?;
                let mut images = BTreeMap::new();
                self.standalone_value(scope, &raw_self, &self_value, &mut images, b)?;
                self.standalone_actuals(scope, &table.actuals, provider, &mut images, b)?;
                let trait_actuals = self.trait_actuals(provider, trait_application, b, 0)?;
                let applied =
                    self.dyn_context_arguments(&interface.application.actuals, &images, b)?;
                if !super::traits::arguments_equal(&trait_actuals, &applied, b)? {
                    return Err(fail(
                        "owning Dyn Context changes its complete provider trait application",
                    ));
                }
                worlds_checked += 1;
            }
        }
        if worlds_checked == 0 {
            return Err(unknown(
                "owning Dyn needs a complete original provider world",
            ));
        }
        self.dyn_owner_arguments(
            &table.actuals,
            &original.actuals,
            &original.interface.classes,
            b,
        )?;
        self.dyn_owner_arguments(
            &interface.application.actuals,
            &original.trait_actuals,
            &original.interface.classes,
            b,
        )?;
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            source,
            b,
        )?;
        let actual_source = self.ssa_value(instance, function, source, b, 0)?;
        if actual_source != original.source {
            return Err(fail(
                "owning Dyn actual source changes its complete original Self type or class occurrence relation",
            ));
        }
        // The original constructor has independently proved Self:Value and
        // Self:SyncDrop. Reconstruct actual formation and the concrete cleanup;
        // a borrowing Copy record is a valid erased owner just as a Package is.
        self.actual_type(&original.source.ty, b)?;
        self.nominal_inventory(&original.source.ty, b)?;
        crate::dyn_cleanup::metadata(interface.owning_cleanup.as_ref(), b)?;
        let bound = crate::dyn_cleanup::bound(interface, b)?;
        crate::dyn_cleanup::prove(self.module, &original.source.ty, bound, b)?;
        let output = instruction.result.ok_or_else(|| {
            fail("owning Dyn construction omits its complete allocation-result SSA")
        })?;
        if function.values.get(output.0 as usize) != Some(&original.output.ty) {
            return Err(fail(
                "owning Dyn result SSA changes its original allocation error/success carrier",
            ));
        }
        Ok(original.output)
    }
}
