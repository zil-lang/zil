//! Full ordered lifetime/type arguments for actual stored nominal declarations.
use super::*;
use crate::stored_types::{self, LexicalLife};
use zil_ir::{
    LifeDeclarationId, LifeNominalActual, LifeNominalDeclaration, LifeNominalKind,
    LifeParameterKind,
};

pub(super) fn has_life(item: &Item) -> bool {
    matches!(&item.kind, ItemKind::Aggregate(a) if a.generics.iter().any(|p| matches!(p.kind, GenericParamKind::Lifetime)))
}

impl<'a> Engine<'a> {
    pub(super) fn stored_lookup_fee(&mut self, entries: usize, bytes: usize, span: Span) -> R<()> {
        self.budget.identity_work(
            entries
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
                .saturating_mul(12)
                .saturating_mul(bytes.saturating_add(1)),
            span,
        )
    }

    pub(super) fn stored_declaration(&mut self, name: &str, span: Span) -> R<bool> {
        self.stored_lookup_fee(self.templates.len(), name.len(), span)?;
        let Some(item) = self.templates.get(name) else {
            return Ok(false);
        };
        if let ItemKind::Aggregate(aggregate) = &item.kind {
            for _ in &aggregate.generics {
                self.budget.node(span, 0)?;
            }
            Ok(has_life(item))
        } else {
            Ok(false)
        }
    }

    pub(super) fn record_declaration(
        &mut self,
        name: &str,
        span: Span,
    ) -> R<Option<(&'a Item, bool)>> {
        if self.stored_nominals {
            self.stored_lookup_fee(self.templates.len(), name.len(), span)?;
        }
        let Some(item) = self.templates.get(name).copied() else {
            return Ok(None);
        };
        let ItemKind::Aggregate(aggregate) = &item.kind else {
            return Ok(None);
        };
        let stored = if self.stored_nominals {
            for _ in &aggregate.generics {
                self.budget.node(span, 0)?;
            }
            has_life(item)
        } else {
            false
        };
        Ok(Some((item, stored)))
    }
    pub(super) fn prepare_stored_headers(&mut self) -> R<()> {
        if !self.stored_nominals {
            return Ok(());
        }
        let mut original = std::mem::take(&mut self.pending);
        let mut ready = VecDeque::new();
        while let Some(mut pending) = original.pop_front() {
            if let ItemKind::Function(function) = &mut pending.item.kind {
                self.life_scope.clear();
                let mut callable_parameters = Vec::new();
                let mut callable_profile = function.where_clause.is_empty();
                for (index, parameter) in function.generics.iter().enumerate() {
                    self.budget.node(parameter.span, 0)?;
                    match parameter.kind {
                        GenericParamKind::Type(_) => callable_parameters.push(index),
                        GenericParamKind::Lifetime => {}
                        _ => callable_profile = false,
                    }
                    if matches!(parameter.kind, GenericParamKind::Lifetime) {
                        self.budget.text(&parameter.name, parameter.span)?;
                        self.life_scope.push(parameter.name.clone());
                    }
                }
                if !callable_profile {
                    callable_parameters.clear();
                }
                self.with_origin(pending.origin, |engine| {
                    if engine.stored_signature_context(function, &callable_parameters)? {
                        engine.budget.function_snapshot(function)?;
                        engine.budget.text(&function.name, function.span)?;
                        engine.stored_lookup_fee(
                            engine.signatures.len(),
                            function.name.len(),
                            function.span,
                        )?;
                        engine
                            .signatures
                            .insert(function.name.clone(), Cow::Owned(function.clone()));
                    }
                    Ok(())
                })?;
            }
            self.budget.node(pending.item.span, 0)?;
            ready.push_back(pending);
        }
        self.life_scope.clear();
        // Header specialization creates ordinary physical declaration items.
        // They share the same pending ledger and are not silently discarded.
        ready.append(&mut self.pending);
        self.pending = ready;
        Ok(())
    }

    pub(super) fn stored_signature(&mut self, function: &mut Function) -> R<bool> {
        self.stored_signature_context(function, &[])
    }
    fn stored_signature_context(
        &mut self,
        function: &mut Function,
        callable_parameters: &[usize],
    ) -> R<bool> {
        if !self.stored_nominals {
            return Ok(false);
        }
        for name in self.products.storage.functions.keys() {
            self.budget.text(name, function.span)?;
            self.budget.text(&function.name, function.span)?;
        }
        if self.products.storage.functions.contains_key(&function.name) {
            return Ok(false);
        }
        let mut occurrences = Vec::new();
        let mut stored = false;
        let mut callable_data_profile = true;
        let mut callable_receivers = 0usize;
        let mut next_elided = u32::try_from(self.life_scope.len())
            .map_err(|_| error(function.span, "lifetime scope overflow"))?;
        for (index, parameter) in function.params.iter_mut().enumerate() {
            let before = occurrences.len();
            let mut parameter_stored = false;
            let physical_witness = self.storage_header(
                &mut parameter.ty,
                zil_ir::StorageBase::Parameter(
                    u32::try_from(index)
                        .map_err(|_| error(parameter.span, "parameter ordinal overflow"))?,
                ),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut parameter_stored,
                &mut next_elided,
                0,
            )?;
            stored |= parameter_stored;
            if !callable_parameters.is_empty() {
                if callable_parameters.len() > 1 {
                    self.budget
                        .identity_work(callable_parameters.len(), parameter.span)?;
                }
                let receiver = callable_parameters.iter().find_map(|ordinal| {
                    let generic = &function.generics[*ordinal];
                    let receiver = match &parameter.ty.kind {
                        TypeKind::Named { path, args } if args.is_empty() && path.as_slice() == [generic.name.as_str()] => true,
                        TypeKind::Reference { inner, .. } => matches!(&inner.kind, TypeKind::Named {path,args}
                            if args.is_empty() && path.as_slice() == [generic.name.as_str()]),
                        _ => false,
                    };
                    if !receiver { return None; }
                    let GenericParamKind::Type(bounds) = &generic.kind else { return None; };
                    let [bound] = bounds.as_slice() else { return None; };
                    match &bound.kind {
                        TypeKind::Named {path,..} if matches!(path.as_slice(), [name] if name == "FnOnce") => Some(None),
                        TypeKind::Named {path,..} if matches!(path.as_slice(), [name] if name == "FnMut") => Some(Some(true)),
                        TypeKind::Named {path,..} if matches!(path.as_slice(), [name] if name == "Fn") => Some(Some(false)),
                        _ => None,
                    }
                });
                match &parameter.ty.kind {
                    TypeKind::Reference { mutable, inner, .. }
                        if matches!(inner.kind, TypeKind::Named { .. }) =>
                    {
                        if parameter_stored {
                            callable_data_profile &=
                                !*mutable && occurrences.len() - before == 2 && physical_witness;
                        } else {
                            callable_data_profile &= receiver == Some(Some(*mutable));
                            callable_receivers += 1;
                        }
                    }
                    _ if callable_context::scalar_syntax(&parameter.ty) => {}
                    TypeKind::Named { .. } if receiver == Some(None) && !parameter_stored => {
                        callable_receivers += 1;
                    }
                    TypeKind::Named { .. } if parameter.give && !parameter_stored => {}
                    _ => callable_data_profile = false,
                }
            }
        }
        if let Some(result) = &mut function.return_type {
            self.storage_header(
                result,
                zil_ir::StorageBase::Result,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next_elided,
                0,
            )?;
        }
        if !stored {
            return Ok(false);
        }
        let (contract, fixed, invocation) = stored_types::contract_with_sources(
            occurrences,
            Vec::new(),
            &mut self.budget,
            function.span,
        )?;
        if callable_data_profile && !callable_parameters.is_empty() {
            let selected = callable::template(function, &mut self.budget)?;
            if let Some(selected) = selected
                && callable_receivers == selected.roles.len()
            {
                self.budget
                    .identity_work(selected.roles.len(), function.span)?;
                for role in selected.roles {
                    self.prepare_callable_context(
                        function,
                        &contract,
                        &fixed,
                        &invocation,
                        role.source_generic_parameter as usize,
                        role.index,
                    )?;
                }
            }
        }
        self.budget.text(&function.name, function.span)?;
        self.stored_lookup_fee(
            self.products.storage.functions.len(),
            function.name.len(),
            function.span,
        )?;
        self.products
            .storage
            .functions
            .insert(function.name.clone(), contract);
        // All retained occurrences have been emitted before the old flat
        // function-lifetime producer sees this header. Only direct header
        // references are erased here; arbitrary body/type labels stay checked.
        fn erase(ty: &mut TypeRef) {
            match &mut ty.kind {
                TypeKind::Reference {
                    lifetime, inner, ..
                } => {
                    *lifetime = None;
                    erase(inner);
                }
                TypeKind::Group(inner) => erase(inner),
                TypeKind::Tuple(fields) => {
                    for field in fields {
                        erase(field);
                    }
                }
                _ => {}
            }
        }
        for parameter in &mut function.params {
            erase(&mut parameter.ty);
        }
        if let Some(result) = &mut function.return_type {
            erase(result);
        }
        // Retain the source scope until the body is specialized, even when a
        // caller appears before this definition. Only emitted Core is erased.
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn storage_header(
        &mut self,
        ty: &mut TypeRef,
        base: zil_ir::StorageBase,
        path: &mut Vec<zil_ir::ContractProjection>,
        type_path: &mut Vec<zil_ir::LifeTypeProjection>,
        occurrences: &mut Vec<stored_types::Occurrence>,
        stored: &mut bool,
        next_elided: &mut u32,
        depth: usize,
    ) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::Group(inner) => self.storage_header(
                inner,
                base,
                path,
                type_path,
                occurrences,
                stored,
                next_elided,
                depth + 1,
            ),
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                let life = if let Some(name) = lifetime {
                    self.source_life(name, ty.span)?
                } else {
                    let life = LexicalLife(*next_elided);
                    *next_elided = next_elided
                        .checked_add(1)
                        .ok_or_else(|| error(ty.span, "elided lifetime scope overflow"))?;
                    life
                };
                for _ in path.iter() {
                    self.budget.node(ty.span, depth)?;
                }
                occurrences.push(stored_types::Occurrence {
                    position: stored_types::OccurrencePosition::Borrow(zil_ir::StoragePosition {
                        base,
                        path: path.clone(),
                    }),
                    life,
                    fixed_input: false,
                });
                // StoragePosition paths are relative to the receiver's actual
                // pointee; crossing this outer Address is not a byte selector.
                type_path.push(zil_ir::LifeTypeProjection::Pointee);
                let result = self.storage_header(
                    inner,
                    base,
                    path,
                    type_path,
                    occurrences,
                    stored,
                    next_elided,
                    depth + 1,
                );
                type_path.pop();
                result
            }
            TypeKind::Named { path: name, args } if name.len() == 1 => {
                if !self.stored_declaration(&name[0], ty.span)? {
                    return Ok(false);
                }
                let (generated, classes) =
                    self.stored_nominal(&name[0], args, ty.span, depth + 1)?;
                for item in &self.products.storage.instances {
                    self.budget.node(ty.span, depth)?;
                    if let zil_ir::Type::BorrowedStorage { storage, .. } = &item.instance.ty
                        && let zil_ir::Type::Record { name, .. } = storage.as_ref()
                    {
                        self.budget.text(name, ty.span)?;
                    }
                }
                let instance = self.products.storage.instances.iter().find(|item|
                    matches!(&item.instance.ty, zil_ir::Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), zil_ir::Type::Record { name, .. } if name == &generated)))
                    .ok_or_else(|| error(ty.span, "stored nominal instance output absent"))?;
                let zil_ir::Type::BorrowedStorage { life, .. } = &instance.instance.ty else {
                    unreachable!()
                };
                let fixed_input = !matches!(base, zil_ir::StorageBase::Result);
                // A qualified boundary maps its local classes exactly once.
                // Its physical fields/ports are reconstructed from the actual
                // validated type, never duplicated as signature access rows.
                if classes.len() != life.binder_count as usize {
                    return Err(error(
                        ty.span,
                        "stored source class count differs from qualified type",
                    ));
                }
                for (class, source) in classes.iter().enumerate() {
                    self.budget.node(ty.span, depth)?;
                    for _ in type_path.iter() {
                        self.budget.node(ty.span, depth)?;
                    }
                    occurrences.push(stored_types::Occurrence {
                        position: stored_types::OccurrencePosition::Class(
                            zil_ir::SignatureLifeClass {
                                base,
                                path: type_path.clone(),
                                class: u32::try_from(class).map_err(|_| {
                                    error(ty.span, "signature class ordinal overflow")
                                })?,
                            },
                        ),
                        life: *source,
                        fixed_input,
                    });
                }
                let physical_witness = life.binder_count == 1
                    && matches!(life.slots.as_slice(),[slot]
                    if slot.binder==0 && slot.mode==zil_ir::StoredBorrowMode::SharedAddress);
                *stored = true;
                *name = vec![generated];
                args.clear();
                Ok(physical_witness)
            }
            TypeKind::Tuple(fields) => {
                let mut physical_witness = false;
                for (index, field) in fields.iter_mut().enumerate() {
                    let index = u32::try_from(index)
                        .map_err(|_| error(field.span, "tuple field ordinal overflow"))?;
                    path.push(zil_ir::ContractProjection::Place(
                        zil_ir::PlaceProjection::Field(index),
                    ));
                    type_path.push(zil_ir::LifeTypeProjection::Place(
                        zil_ir::PlaceProjection::Field(index),
                    ));
                    physical_witness |= self.storage_header(
                        field,
                        base,
                        path,
                        type_path,
                        occurrences,
                        stored,
                        next_elided,
                        depth + 1,
                    )?;
                    path.pop();
                    type_path.pop();
                }
                Ok(physical_witness)
            }
            _ => Ok(false),
        }
    }
    pub(super) fn source_life(&mut self, name: &str, span: Span) -> R<LexicalLife> {
        for binder in &self.life_scope {
            self.budget.text(binder, span)?;
            self.budget.text(name, span)?;
        }
        let index = self
            .life_scope
            .iter()
            .position(|binder| binder == name)
            .ok_or_else(|| error(span, "undeclared nominal lifetime actual"))?;
        Ok(LexicalLife(
            u32::try_from(index).map_err(|_| error(span, "lifetime scope overflow"))?,
        ))
    }

    pub(super) fn stored_nominal(
        &mut self,
        template: &str,
        arguments: &mut [GenericArg],
        span: Span,
        depth: usize,
    ) -> R<(String, Vec<LexicalLife>)> {
        self.budget.node(span, depth)?;
        self.budget.text(template, span)?;
        self.stored_lookup_fee(self.templates.len(), template.len(), span)?;
        let item = self
            .templates
            .get(template)
            .copied()
            .ok_or_else(|| error(span, "stored nominal declaration is absent"))?;
        let ItemKind::Aggregate(aggregate) = &item.kind else {
            return Err(error(
                span,
                "stored nominal application requires an aggregate",
            ));
        };
        for _ in &aggregate.generics {
            self.budget.node(span, depth)?;
        }
        if aggregate.generics.len() != arguments.len() {
            return Err(error(
                span,
                "stored nominal requires its full ordered argument list",
            ));
        }
        let origin = self
            .template_origin(item)?
            .ok_or_else(|| error(span, "stored nominal source provenance is absent"))?;
        let origins = self
            .origins
            .ok_or_else(|| error(span, "stored nominal source inventory is absent"))?;
        for _ in &origins.identities {
            self.budget.node(span, depth)?;
        }
        let identity = origins.identities.get(&origin.declaration).ok_or_else(|| {
            error(
                span,
                "stored nominal original declaration identity is absent",
            )
        })?;
        self.budget.text(&identity.identity.package, span)?;
        for part in identity
            .identity
            .module_path
            .iter()
            .chain(&identity.identity.declaration_path)
        {
            self.budget.text(part, span)?;
            self.budget.node(span, depth)?;
        }
        for _ in &identity.identity.generation_path {
            self.budget.node(span, depth)?;
        }
        let declaration_id = LifeDeclarationId {
            module: origin.declaration.module,
            declaration: identity.declaration,
            compiler: origin.declaration.compiler,
        };
        let stable_identity = identity.identity.clone();
        let mut substitution = BTreeMap::new();
        let mut lives = Vec::new();
        let mut actuals = Vec::new();
        let mut parameters = Vec::new();
        let mut keys = Vec::new();
        let mut local_classes = Vec::new();
        for (formal, actual) in aggregate.generics.iter().zip(arguments.iter_mut()) {
            self.budget.node(formal.span, depth)?;
            self.budget.text(&formal.name, formal.span)?;
            match (&formal.kind, actual) {
                (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                    let life = self.source_life(name, span)?;
                    for _ in &local_classes {
                        self.budget.node(span, depth)?;
                    }
                    let class = local_classes
                        .iter()
                        .position(|prior| *prior == life)
                        .unwrap_or_else(|| {
                            local_classes.push(life);
                            local_classes.len() - 1
                        });
                    // Length-delimited generic keys retain kind and every full
                    // ordinal, including phantom arguments; names do not enter.
                    self.budget.identity_work(15, span)?;
                    keys.push(format!("life:{class}"));
                    parameters.push(LifeParameterKind::Lifetime);
                    actuals.push(LifeNominalActual::Lifetime(class as u32));
                    lives.push((formal.name.clone(), life));
                }
                (GenericParamKind::Type(_), GenericArg::Type(ty)) => {
                    self.closed_stored_actual(ty, depth + 1)?;
                    self.ty(ty, depth + 1)?;
                    self.closed_stored_actual(ty, depth + 1)?;
                    self.budget.ty(ty)?;
                    let key = type_key(ty, &mut self.budget)?;
                    keys.push(key);
                    parameters.push(LifeParameterKind::Type);
                    actuals.push(LifeNominalActual::Type(super::super::type_ref(ty)));
                    substitution.insert(formal.name.clone(), ty.clone());
                }
                (GenericParamKind::Const(domain), actual) => {
                    // normalize inspects the formal domain before it reads the
                    // value. Reserve that actual domain traversal first; the
                    // later Core type construction has its own reservation.
                    self.budget.ty(domain)?;
                    let carrier = consts::argument(actual, Some(formal), &mut self.budget)?;
                    let normalized = consts::normalize(formal, &carrier, &mut self.budget)?;
                    let value = consts::normalized_value(&normalized, &mut self.budget)?;
                    keys.push(type_key(&normalized, &mut self.budget)?);
                    self.budget.ty(domain)?;
                    parameters.push(LifeParameterKind::Const(super::super::type_ref(domain)));
                    actuals.push(LifeNominalActual::Const(value));
                    substitution.insert(formal.name.clone(), normalized);
                }
                _ => {
                    return Err(error(
                        span,
                        "stored nominal actual kind differs from its declared full ordinal",
                    ));
                }
            }
        }
        self.with_origin(Some(origin), |engine| {
            engine.traits.check_arguments(
                &aggregate.generics,
                &substitution,
                (
                    &trait_bounds::Bounds::new(),
                    &BTreeSet::new(),
                    callable::StoreCallbacks::None,
                ),
                &mut engine.budget,
                span,
                None,
            )
        })?;
        // Reconstructing classes on every use is deliberate: the cached shape
        // does not bind a new caller's lexical lifetime numbers.
        let key = (template.to_owned(), keys);
        let key_bytes = key.1.iter().try_fold(template.len(), |size, part| {
            size.checked_add(part.len().saturating_add(1))
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Expand,
                        "stored instance key size overflow",
                        Some(span),
                    )
                })
        })?;
        self.stored_lookup_fee(self.instances.len(), key_bytes, span)?;
        let existing = self.instances.get(&key);
        if let Some(name) = existing {
            self.budget.text(name, span)?;
        }
        let existing = existing.cloned();
        let generated = if let Some(existing) = &existing {
            self.budget.text(existing, span)?;
            existing.clone()
        } else {
            if self.instances.len() >= self.limits.max_functions.min(512) {
                return Err(Diagnostic::limit(
                    Phase::Expand,
                    "stored nominal instance budget exceeded",
                    Some(span),
                ));
            }
            self.budget
                .identity_work(template.len().saturating_add(19), span)?;
            format!("$stored_{}_{}", template, self.instances.len())
        };
        self.budget.substituted_item(item, &substitution)?;
        consts::prepay_scopes(item, &substitution, &mut self.budget)?;
        let mut instance = item.clone();
        substitute_item(&mut instance, &substitution)?;
        let ItemKind::Aggregate(physical) = &mut instance.kind else {
            unreachable!()
        };
        physical.name.clone_from(&generated);
        for field in &mut physical.fields {
            self.ty(&mut field.ty, depth + 1)?;
        }
        let declaration = LifeNominalDeclaration {
            id: declaration_id,
            identity: stable_identity,
            kind: LifeNominalKind::Record,
            parameters,
        };
        let produced = stored_types::instantiate(
            physical,
            declaration,
            actuals,
            &lives,
            &mut self.budget,
            span,
        )?;
        let classes = produced.classes;
        if existing.is_none() {
            physical.generics.clear();
            self.stored_lookup_fee(self.instances.len(), key_bytes, span)?;
            self.budget.text(&generated, span)?;
            self.instances.insert(key, generated.clone());
            self.products
                .storage
                .instances
                .push(stored_types::Instance {
                    classes: Vec::new(),
                    ..produced
                });
            self.enqueue(instance, Some(origin))?;
        }
        Ok((generated, classes))
    }

    fn closed_stored_actual(&mut self, ty: &TypeRef, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Reference { .. } => {
                return Err(unsupported(
                    ty.span,
                    "borrowed Type actual requires its joint scoped metadata relation",
                ));
            }
            TypeKind::Named { path, args } => {
                for part in path {
                    self.budget.text(part, ty.span)?;
                }
                if path.len() == 1 {
                    for instance in &self.products.storage.instances {
                        self.budget.node(ty.span, depth)?;
                        if let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                            && let zil_ir::Type::Record { name, .. } = storage.as_ref()
                        {
                            self.budget.text(name, ty.span)?;
                            self.budget.text(&path[0], ty.span)?;
                            if name == &path[0] {
                                return Err(unsupported(
                                    ty.span,
                                    "qualified nominal Type actual requires joint class substitution",
                                ));
                            }
                        }
                    }
                }
                for arg in args {
                    self.budget.node(ty.span, depth)?;
                    match arg {
                        GenericArg::Lifetime(_) => {
                            return Err(unsupported(
                                ty.span,
                                "lifetime-bearing Type actual requires joint class substitution",
                            ));
                        }
                        GenericArg::Type(inner) => self.closed_stored_actual(inner, depth + 1)?,
                        GenericArg::Const(value) => {
                            self.budget.const_expression_depth(value, depth + 1)?
                        }
                    }
                }
            }
            TypeKind::Group(inner) | TypeKind::Optional(inner) => {
                self.closed_stored_actual(inner, depth + 1)?
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) | TypeKind::Impl(fields) => {
                for field in fields {
                    self.closed_stored_actual(field, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => self.closed_stored_actual(element, depth + 1)?,
            TypeKind::Error { error, result } => {
                self.closed_stored_actual(error, depth + 1)?;
                self.closed_stored_actual(result, depth + 1)?;
            }
            // This owns its invocation relation. The earlier lexical walker
            // rejects references to a lifetime outside this ForLifetime scope.
            TypeKind::ForLifetime { .. } => self.budget.ty_depth(ty, depth)?,
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for parameter in params {
                    self.closed_stored_actual(parameter, depth + 1)?;
                }
                self.closed_stored_actual(result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}
