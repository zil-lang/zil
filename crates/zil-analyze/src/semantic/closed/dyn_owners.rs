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
        if interface.context.is_some()
            || interface.retained_life.binder_count != 0
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
            || !interface.retained_life.nominal_arguments.is_empty()
            || !interface.application.actuals.is_empty()
            || !interface.inherited.is_empty()
            || !interface.associated_types.is_empty()
            || !interface.associated_constants.is_empty()
        {
            return Err(unknown(
                "owning Dyn formation needs its complete original mixed, inherited, associated and Context class domain",
            ));
        }
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
            || !declaration.parameters.is_empty()
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
    fn dyn_owner_arguments(
        &self,
        actuals: &[DynGenericActual],
        expected: &[Argument],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if actuals.len() != expected.len() {
            return Err(fail(
                "owning Dyn table changes its provider's complete original mixed arity",
            ));
        }
        for (actual, expected) in actuals.iter().zip(expected) {
            crate::dyn_metadata::recipe(actual, b)?;
            match (actual, expected) {
                (DynGenericActual::Type(ty), Argument::Type(expected)) => {
                    crate::dyn_metadata::ty(&expected.ty, b)?;
                    crate::dyn_metadata::shape(&expected.life, b)?;
                    b.spend(expected.classes.len(), WORK)?;
                    if !empty(&expected.life) || !expected.classes.is_empty() {
                        return Err(unknown(
                            "owning Dyn plain Type actual requires its full original nonzero intrinsic domain",
                        ));
                    }
                    self.zero_data(ty, b, 0)?;
                    if *ty != expected.ty {
                        return Err(fail(
                            "owning Dyn table changes a complete original provider Type actual",
                        ));
                    }
                }
                (DynGenericActual::QualifiedType(actual), Argument::Type(expected)) => {
                    if !empty(&expected.life)
                        || !expected.classes.is_empty()
                        || !actual.classes.is_empty()
                    {
                        return Err(unknown(
                            "owning Dyn qualified Type actual requires its full original class-to-Context embedding",
                        ));
                    }
                    self.zero_data(&actual.ty, b, 0)?;
                    if actual.ty != expected.ty || actual.life != expected.life {
                        return Err(fail(
                            "owning Dyn table changes its complete original qualified Type actual",
                        ));
                    }
                }
                (DynGenericActual::Const(actual), Argument::Const { carrier, value }) => {
                    crate::dyn_metadata::ty(carrier, b)?;
                    if actual.ty() != *carrier || actual != value {
                        return Err(fail(
                            "owning Dyn table changes an original Const carrier or exact scalar bits",
                        ));
                    }
                }
                (DynGenericActual::Life(_), Argument::Life(_)) => {
                    return Err(unknown(
                        "owning Dyn lifetime actual needs its full original private-to-Context scope embedding",
                    ));
                }
                _ => {
                    return Err(fail(
                        "owning Dyn table changes a full-ordinal original mixed actual kind",
                    ));
                }
            }
        }
        Ok(())
    }
    fn dyn_owner_shape(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<usize> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "owning Dyn concrete source exceeds its complete cleanup/type depth",
            ));
        }
        crate::dyn_metadata::ty(ty, b)?;
        if ty.is_b0_scalar() {
            return Ok(0);
        }
        match ty {
            Type::Named(_) => Err(fail(
                "owning Dyn source retains an unresolved Named declaration",
            )),
            Type::BorrowedStorage { storage, life } => {
                self.nominal_inventory(ty, b)?;
                if !empty(life) {
                    return Err(unknown(
                        "owning Dyn source requires its full original retained target and class relation",
                    ));
                }
                self.dyn_owner_shape(storage, b, depth + 1)
            }
            Type::Managed(payload) => {
                self.actual_type(ty, b)?;
                match payload.as_ref() {
                    Type::ByteBuffer => Ok(1),
                    Type::OwnedStore { element } => {
                        self.dyn_owner_shape(element, b, depth + 1)?;
                        Ok(1)
                    }
                    Type::Record { fields, .. } => {
                        // The resource's payload is not a separate nominal
                        // application. actual_type above reconstructs its
                        // original Resource definition and full payload; only
                        // genuine nested field applications need an inventory.
                        for (_, field) in fields {
                            if self.dyn_owner_shape(field, b, depth + 1)? != 0 {
                                return Err(unknown(
                                    "owning Dyn resource payload needs its complete nested cleanup theorem",
                                ));
                            }
                        }
                        Ok(1)
                    }
                    _ => Err(unknown(
                        "owning Dyn source has no complete original synchronous concrete resource cleanup family",
                    )),
                }
            }
            Type::Record { fields, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, field) in fields {
                    if self.dyn_owner_shape(field, b, depth + 1)? != 0 {
                        return Err(unknown(
                            "owning Dyn Copy record companion contains an owning cleanup leaf",
                        ));
                    }
                }
                Ok(0)
            }
            Type::Package { fields, .. } => {
                self.nominal_inventory(ty, b)?;
                let mut owners = 0usize;
                for (_, field) in fields {
                    owners = owners
                        .checked_add(self.dyn_owner_shape(field, b, depth + 1)?)
                        .ok_or_else(|| fail("owning Dyn concrete cleanup leaf count overflows"))?;
                }
                if owners == 0 {
                    return Err(unknown(
                        "owning Dyn Package source lacks a complete real managed cleanup leaf",
                    ));
                }
                Ok(owners)
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if self.dyn_owner_shape(field, b, depth + 1)? != 0 {
                        return Err(unknown(
                            "owning Dyn tuple companion needs its complete owning aggregate cleanup profile",
                        ));
                    }
                }
                Ok(0)
            }
            Type::Enum { variants, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, fields) in variants {
                    for field in fields {
                        if self.dyn_owner_shape(field, b, depth + 1)? != 0 {
                            return Err(unknown(
                                "owning Dyn sum companion needs its complete selected-tag cleanup profile",
                            ));
                        }
                    }
                }
                Ok(0)
            }
            Type::Array { element, length } => {
                if *length == 0 || *length as u128 > b.helper_nodes as u128 {
                    return Err(unknown(
                        "owning Dyn array companion needs its complete original finite extent cleanup profile",
                    ));
                }
                if self.dyn_owner_shape(element, b, depth + 1)? != 0 {
                    return Err(unknown(
                        "owning Dyn array companion contains an owning element outside its complete cleanup profile",
                    ));
                }
                Ok(0)
            }
            _ => Err(unknown(
                "owning Dyn source requires its complete original self-contained storage and synchronous cleanup theorem",
            )),
        }
    }
    #[allow(clippy::too_many_arguments)]
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
        if !empty(&original.output.life)
            || !original.output.classes.is_empty()
            || !empty(&original.source.life)
            || !original.source.classes.is_empty()
        {
            return Err(unknown(
                "owning Dyn construction needs its complete original nonzero Self/Context class and retained-target embedding",
            ));
        }
        self.dyn_value_type(result, b, 0)?;
        let expected_borrowed = value::scalar(Type::Dyn {
            interface: *expected_interface,
        });
        let [
            SemanticActual::Type(written),
            SemanticActual::Type(source_claim),
        ] = actuals
        else {
            return Err(fail(
                "owning Dyn construction changes its complete interface and inferred Self Type actuals",
            ));
        };
        if self.resolved(instance, written, b, 0)? != expected_borrowed
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
        if !empty(life) || !table.self_classes.is_empty() || !table.ports.is_empty() {
            return Err(unknown(
                "owning Dyn table needs its complete original Self-to-Context domain and retained target transport",
            ));
        }
        if table.self_type != original.source.ty {
            return Err(fail(
                "owning Dyn vtable changes the independently checked original complete Self type",
            ));
        }
        let provider = self.actual_provider(
            &original.source,
            interface.application.declaration,
            &[],
            b,
            0,
        )?;
        let definition = self.inventory.definition(provider.definition, b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration, .. }) =
            &definition.body
        else {
            return Err(fail(
                "owning Dyn actual provider has no real original implementation scope",
            ));
        };
        if provider.definition != original.provider || *declaration != table.implementation {
            return Err(fail(
                "owning Dyn construction selects another original provider scope or implementation declaration",
            ));
        }
        let mut provider_actuals = Vec::new();
        for actual in &provider.actuals {
            self.push_value(
                &mut provider_actuals,
                self.actual_value(provider, actual, b, 0)?,
                b,
            )?;
        }
        if provider_actuals != original.actuals {
            return Err(fail(
                "owning Dyn provider changes its complete independent original mixed specialization",
            ));
        }
        self.dyn_owner_arguments(&table.actuals, &original.actuals, b)?;
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
        // A closed source must be a real complete owning Package with an
        // independently checked managed leaf, not a same-sized tuple or claim.
        let physical = match &original.source.ty {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            ty => ty,
        };
        if !matches!(physical, Type::Package { .. }) {
            return Err(unknown(
                "owning Dyn closed construction requires its complete original owning Package profile",
            ));
        }
        self.dyn_owner_shape(&original.source.ty, b, 0)?;
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
