//! Full ordered lifetime/type arguments for actual stored nominal declarations.
use super::*;
use crate::stored_types::{self, LexicalLife};
use zil_ir::{
    LifeDeclarationId, LifeNominalActual, LifeNominalDeclaration, LifeNominalKind,
    LifeParameterKind,
};

pub(super) fn has_life(item: &Item) -> bool {
    match &item.kind {
        ItemKind::Aggregate(a) => a
            .generics
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime)),
        ItemKind::Enum(e) => e
            .generics
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime)),
        _ => false,
    }
}

impl<'a> Engine<'a> {
    fn capture_actual_life(
        &mut self,
        name: &mut String,
        span: Span,
        captured: &mut Vec<(String, LexicalLife)>,
    ) -> R<()> {
        let class = self.source_life(name, span)?;
        let nonce = self.next_nominal_capture;
        self.next_nominal_capture = nonce.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "source class capture identity exhausted",
                Some(span),
            )
        })?;
        // `$` is a distinct source token, never part of a source identifier.
        // The label is only an index into the class captured before erasure.
        self.budget.identity_work(32, span)?;
        let token = format!("$captured_life_{nonce}");
        self.budget.text(&token, span)?;
        self.budget
            .lineage_bytes(std::mem::size_of::<(String, LexicalLife)>(), span)?;
        captured.push((token.clone(), class));
        *name = token;
        Ok(())
    }

    fn capture_actual_type(
        &mut self,
        ty: &mut TypeRef,
        captured: &mut Vec<(String, LexicalLife)>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for arg in args {
                    match arg {
                        GenericArg::Lifetime(name) => {
                            self.capture_actual_life(name, ty.span, captured)?
                        }
                        GenericArg::Type(inner) => {
                            self.capture_actual_type(inner, captured, depth + 1)?
                        }
                        GenericArg::Const(_) => {}
                    }
                }
            }
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(name) = lifetime {
                    self.capture_actual_life(name, ty.span, captured)?;
                }
                self.capture_actual_type(inner, captured, depth + 1)?;
            }
            TypeKind::Group(inner) | TypeKind::Optional(inner) => {
                self.capture_actual_type(inner, captured, depth + 1)?
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) | TypeKind::Impl(fields) => {
                for field in fields {
                    self.capture_actual_type(field, captured, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => {
                self.capture_actual_type(element, captured, depth + 1)?
            }
            TypeKind::Error { error, result } => {
                self.capture_actual_type(error, captured, depth + 1)?;
                self.capture_actual_type(result, captured, depth + 1)?;
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for parameter in params {
                    self.capture_actual_type(parameter, captured, depth + 1)?;
                }
                self.capture_actual_type(result, captured, depth + 1)?;
            }
            TypeKind::ForLifetime { .. } => {
                return Err(unsupported(
                    ty.span,
                    "qualified Type actual with nested lifetime binder needs its own source scope",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    // The original and physical field, both source maps, both projection paths,
    // and the recursion budget remain distinct proof inputs at this boundary.
    #[allow(clippy::too_many_arguments)]
    fn stored_physical_field(
        &mut self,
        physical: &mut TypeRef,
        original: &TypeRef,
        formals: &[GenericParam],
        qualified_formals: &BTreeMap<usize, stored_types::QualifiedActual>,
        path: &mut Vec<zil_ir::LifeTypeProjection>,
        direct: &mut BTreeMap<Vec<zil_ir::LifeTypeProjection>, stored_types::QualifiedActual>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(physical.span, depth)?;
        if let TypeKind::Group(inner) = &mut physical.kind {
            return self.stored_physical_field(
                inner,
                original,
                formals,
                qualified_formals,
                path,
                direct,
                depth + 1,
            );
        }
        if let TypeKind::Group(inner) = &original.kind {
            return self.stored_physical_field(
                physical,
                inner,
                formals,
                qualified_formals,
                path,
                direct,
                depth + 1,
            );
        }
        if let TypeKind::Named { path: name, args } = &original.kind
            && name.len() == 1
            && args.is_empty()
        {
            for (ordinal, formal) in formals.iter().enumerate() {
                self.budget.text(&formal.name, original.span)?;
                self.budget.text(&name[0], original.span)?;
                if formal.name == name[0] && qualified_formals.contains_key(&ordinal) {
                    return Ok(());
                }
            }
        }
        match (&mut physical.kind, &original.kind) {
            (TypeKind::Tuple(fields), TypeKind::Tuple(source)) if fields.len() == source.len() => {
                for (index, (field, original)) in fields.iter_mut().zip(source).enumerate() {
                    let step = zil_ir::PlaceProjection::Field(
                        u32::try_from(index)
                            .map_err(|_| error(field.span, "tuple field ordinal overflow"))?,
                    );
                    path.push(zil_ir::LifeTypeProjection::Place(step));
                    self.stored_physical_field(
                        field,
                        original,
                        formals,
                        qualified_formals,
                        path,
                        direct,
                        depth + 1,
                    )?;
                    path.pop();
                }
                Ok(())
            }
            (
                TypeKind::Array { element, .. },
                TypeKind::Array {
                    element: source, ..
                },
            ) => {
                path.push(zil_ir::LifeTypeProjection::Element);
                self.stored_physical_field(
                    element,
                    source,
                    formals,
                    qualified_formals,
                    path,
                    direct,
                    depth + 1,
                )?;
                path.pop();
                Ok(())
            }
            _ => {
                // Rebuild the application from the original field and the
                // original Type actual syntax. The substituted physical name
                // is only a consistency witness; it never supplies classes.
                self.budget.substituted_type_depth(
                    original,
                    &self.nominal_source_actuals,
                    depth,
                )?;
                let mut source = original.clone();
                Substitution {
                    substitutions: &self.nominal_source_actuals,
                }
                .ty(&mut source);
                if matches!(source.kind, TypeKind::Named { .. })
                    && self.qualified_actual_syntax(&source, depth + 1)?
                {
                    let (
                        TypeKind::Named {
                            path: source_name,
                            args: source_args,
                        },
                        TypeKind::Named {
                            path: physical_name,
                            args: physical_args,
                        },
                    ) = (&original.kind, &physical.kind)
                    else {
                        return Err(unsupported(
                            physical.span,
                            "compound scoped physical field lacks complete source relation",
                        ));
                    };
                    for name in source_name {
                        self.budget.text(name, original.span)?;
                    }
                    for name in physical_name {
                        self.budget.text(name, physical.span)?;
                    }
                    if source_name != physical_name || source_args.len() != physical_args.len() {
                        return Err(error(
                            physical.span,
                            "physical field differs from original scoped application",
                        ));
                    }
                    let actual = self
                        .qualified_type_actual(&mut source, depth + 1)?
                        .ok_or_else(|| {
                            unsupported(
                                physical.span,
                                "qualified field lacks actual scoped nominal",
                            )
                        })?;
                    self.budget.lineage_bytes(
                        std::mem::size_of::<(
                            Vec<zil_ir::LifeTypeProjection>,
                            stored_types::QualifiedActual,
                        )>()
                        .saturating_add(
                            path.len()
                                .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>()),
                        ),
                        physical.span,
                    )?;
                    self.budget.identity_work(
                        direct
                            .len()
                            .saturating_add(1)
                            .saturating_mul(path.len().saturating_add(1)),
                        physical.span,
                    )?;
                    direct.insert(path.clone(), actual);
                    self.budget.ty(&source)?;
                    *physical = source;
                    return Ok(());
                }
                self.ty(physical, depth + 1)
            }
        }
    }
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
        if let ItemKind::Aggregate(_) | ItemKind::Enum(_) = &item.kind {
            for _ in params(item) {
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
                self.source_enum_return = None;
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
                pending.source_enum_return = self.source_enum_return.take();
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
        if callable_data_profile && callable_receivers == callable_parameters.len() {
            // Preserve the original unique-binder path and its paid template
            // schedule. Only repeated runtime positions need an extra selector.
            for &parameter in callable_parameters {
                self.prepare_callable_context(
                    function,
                    &contract,
                    &fixed,
                    &invocation,
                    parameter,
                    None,
                )?;
            }
        } else if callable_data_profile && !callable_parameters.is_empty() {
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
                        Some(role.index),
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
                if !self.stored_application(&name[0], args, ty.span, depth + 1)? {
                    return Ok(false);
                }
                if matches!(base, zil_ir::StorageBase::Result) && type_path.is_empty() {
                    self.budget.text(&name[0], ty.span)?;
                    self.stored_lookup_fee(self.enums.len(), name[0].len(), ty.span)?;
                    if self.enums.contains_key(&name[0]) {
                        // A return body must see the actual source application,
                        // including its caller-life Type actuals. Capture it only
                        // after the stored proof is known, before name erasure.
                        self.budget.node(ty.span, depth)?;
                        self.budget.lineage_bytes(
                            std::mem::size_of::<TypeRef>()
                                .saturating_add(std::mem::size_of::<String>())
                                .saturating_add(
                                    args.len().saturating_mul(std::mem::size_of::<GenericArg>()),
                                ),
                            ty.span,
                        )?;
                        for argument in args.iter() {
                            match argument {
                                GenericArg::Type(actual) => self.budget.ty(actual)?,
                                GenericArg::Lifetime(life) => self.budget.text(life, ty.span)?,
                                GenericArg::Const(value) => self.budget.const_expression(value)?,
                            }
                        }
                        self.source_enum_return = Some(TypeRef {
                            span: ty.span,
                            kind: TypeKind::Named {
                                path: name.clone(),
                                args: args.clone(),
                            },
                        });
                    }
                }
                let (generated, classes) =
                    self.stored_nominal(&name[0], args, ty.span, depth + 1)?;
                for item in &self.products.storage.instances {
                    self.budget.node(ty.span, depth)?;
                    if let zil_ir::Type::BorrowedStorage { storage, .. } = &item.instance.ty
                        && let zil_ir::Type::Record { name, .. } | zil_ir::Type::Enum { name, .. } =
                            storage.as_ref()
                    {
                        self.budget.text(name, ty.span)?;
                    }
                }
                let instance = self.products.storage.instances.iter().find(|item|
                    matches!(&item.instance.ty, zil_ir::Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), zil_ir::Type::Record { name, .. } | zil_ir::Type::Enum { name, .. } if name == &generated)))
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
            TypeKind::Array { element, .. } => {
                let before = occurrences.len();
                type_path.push(zil_ir::LifeTypeProjection::Element);
                let witness = self.storage_header(
                    element,
                    base,
                    path,
                    type_path,
                    occurrences,
                    stored,
                    next_elided,
                    depth + 1,
                );
                type_path.pop();
                let witness = witness?;
                self.budget
                    .identity_work(occurrences.len().saturating_sub(before), ty.span)?;
                if witness
                    || occurrences[before..].iter().any(|row| {
                        matches!(row.position, stored_types::OccurrencePosition::Borrow(_))
                    })
                {
                    return Err(unsupported(
                        ty.span,
                        "array signature physical borrow needs an actual element place projection",
                    ));
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    pub(super) fn source_life(&mut self, name: &str, span: Span) -> R<LexicalLife> {
        for (token, _) in &self.nominal_captured_lives {
            self.budget.text(token, span)?;
            self.budget.text(name, span)?;
        }
        if let Some((_, life)) = self
            .nominal_captured_lives
            .iter()
            .find(|(token, _)| token == name)
        {
            return Ok(*life);
        }
        for (formal, _) in &self.nominal_life_bindings {
            self.budget.text(formal, span)?;
            self.budget.text(name, span)?;
        }
        if let Some((_, life)) = self
            .nominal_life_bindings
            .iter()
            .find(|(formal, _)| formal == name)
        {
            return Ok(*life);
        }
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
        let formals = match &item.kind {
            ItemKind::Aggregate(a) => a.generics.as_slice(),
            ItemKind::Enum(e) => e.generics.as_slice(),
            _ => {
                return Err(error(
                    span,
                    "stored nominal application requires an actual declaration",
                ));
            }
        };
        if let ItemKind::Aggregate(aggregate) = &item.kind
            && !aggregate.methods.is_empty()
        {
            return Err(unsupported(
                span,
                "generic resource or methods require capability resolution",
            ));
        }
        for _ in formals {
            self.budget.node(span, depth)?;
        }
        if formals.len() != arguments.len() {
            return Err(error(
                span,
                "stored nominal requires its full ordered argument list",
            ));
        }
        let origin = self
            .template_origin(item)?
            .ok_or_else(|| error(span, "stored nominal source provenance is absent"))?;
        let (declaration_id, stable_identity) = if template == options::TEMPLATE {
            options::storage_identity(item, origin, &mut self.budget)?
        } else {
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
            (declaration_id, stable_identity)
        };
        let mut substitution = BTreeMap::new();
        let mut lives = Vec::new();
        let mut actuals = Vec::new();
        let mut parameters = Vec::new();
        let mut keys = Vec::new();
        let mut local_classes = Vec::new();
        let mut qualified_actuals = BTreeMap::new();
        let mut source_actuals = BTreeMap::new();
        let mut captured_lives = Vec::new();
        for (ordinal, (formal, actual)) in formals.iter().zip(arguments.iter_mut()).enumerate() {
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
                    self.budget.ty(ty)?;
                    self.budget.text(&formal.name, ty.span)?;
                    self.budget
                        .lineage_bytes(std::mem::size_of::<(String, TypeRef)>(), ty.span)?;
                    let mut source = ty.clone();
                    self.capture_actual_type(&mut source, &mut captured_lives, depth + 1)?;
                    source_actuals.insert(formal.name.clone(), source);
                    if let Some(actual) = self.qualified_type_actual(ty, depth + 1)? {
                        if matches!(actual.ty, zil_ir::Type::Address { mutable: true, .. }) {
                            return Err(unsupported(
                                ty.span,
                                "mutable address Type actual needs its affine consumer",
                            ));
                        }
                        if matches!(actual.ty, zil_ir::Type::Address { .. })
                            && template != options::TEMPLATE
                        {
                            return Err(unsupported(
                                ty.span,
                                "address Type actual needs an independently authenticated formal occurrence",
                            ));
                        }
                        let class_key =
                            self.joint_class_key(&actual.classes, &mut local_classes, span)?;
                        let physical_key = type_key(ty, &mut self.budget)?;
                        self.budget.identity_work(
                            physical_key.len().saturating_add(class_key.len()),
                            span,
                        )?;
                        keys.push(format!("{physical_key}[{class_key}]"));
                        parameters.push(LifeParameterKind::Type);
                        stored_types::pay_type(&actual.ty, &mut self.budget, span, depth + 1)?;
                        actuals.push(LifeNominalActual::Type(actual.ty.clone()));
                        self.budget.ty(ty)?;
                        substitution.insert(formal.name.clone(), ty.clone());
                        self.budget.lineage_bytes(
                            std::mem::size_of::<(usize, stored_types::QualifiedActual)>(),
                            span,
                        )?;
                        qualified_actuals.insert(ordinal, actual);
                        continue;
                    }
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
                formals,
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
        for (name, _) in &lives {
            self.budget.text(name, span)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<(String, LexicalLife)>(), span)?;
        }
        self.budget.lineage_bytes(
            captured_lives
                .len()
                .saturating_mul(std::mem::size_of::<(String, LexicalLife)>()),
            span,
        )?;
        let previous_lives = std::mem::replace(&mut self.nominal_life_bindings, lives.clone());
        let previous_source = std::mem::replace(&mut self.nominal_source_actuals, source_actuals);
        let capture_mark = self.nominal_captured_lives.len();
        self.nominal_captured_lives.extend(captured_lives);
        let mut direct_fields = BTreeMap::new();
        let field_result = (|| -> R<()> {
            match (&mut instance.kind, &item.kind) {
                (ItemKind::Aggregate(physical), ItemKind::Aggregate(source)) => {
                    physical.name.clone_from(&generated);
                    for (index, (field, original)) in
                        physical.fields.iter_mut().zip(&source.fields).enumerate()
                    {
                        let step = zil_ir::PlaceProjection::Field(
                            u32::try_from(index)
                                .map_err(|_| error(field.span, "stored field ordinal overflow"))?,
                        );
                        self.stored_physical_field(
                            &mut field.ty,
                            &original.ty,
                            formals,
                            &qualified_actuals,
                            &mut vec![zil_ir::LifeTypeProjection::Place(step)],
                            &mut direct_fields,
                            depth + 1,
                        )?;
                    }
                }
                (ItemKind::Enum(physical), ItemKind::Enum(source)) => {
                    physical.name.clone_from(&generated);
                    if physical.variants.len() != source.variants.len() {
                        return Err(error(span, "stored enum variant count differs"));
                    }
                    for (variant_index, (variant, original)) in physical
                        .variants
                        .iter_mut()
                        .zip(&source.variants)
                        .enumerate()
                    {
                        if variant.name != original.name {
                            return Err(error(variant.span, "stored enum variant order differs"));
                        }
                        let variant_ordinal = u32::try_from(variant_index)
                            .map_err(|_| error(variant.span, "stored variant ordinal overflow"))?;
                        match (&mut variant.fields, &original.fields) {
                            (VariantFields::Unit, VariantFields::Unit) => {}
                            (VariantFields::Tuple(fields), VariantFields::Tuple(source_fields))
                                if fields.len() == source_fields.len() =>
                            {
                                for (index, (field, original)) in
                                    fields.iter_mut().zip(source_fields).enumerate()
                                {
                                    let step = zil_ir::PlaceProjection::Variant(
                                        zil_ir::VariantProjection {
                                            variant: variant_ordinal,
                                            index: u32::try_from(index).map_err(|_| {
                                                error(
                                                    field.span,
                                                    "stored variant field ordinal overflow",
                                                )
                                            })?,
                                        },
                                    );
                                    self.stored_physical_field(
                                        &mut field.ty,
                                        &original.ty,
                                        formals,
                                        &qualified_actuals,
                                        &mut vec![zil_ir::LifeTypeProjection::Place(step)],
                                        &mut direct_fields,
                                        depth + 1,
                                    )?;
                                }
                            }
                            (
                                VariantFields::Record(fields),
                                VariantFields::Record(source_fields),
                            ) if fields.len() == source_fields.len() => {
                                for (index, (field, original)) in
                                    fields.iter_mut().zip(source_fields).enumerate()
                                {
                                    if field.name != original.name {
                                        return Err(error(
                                            field.span,
                                            "stored variant record field order differs",
                                        ));
                                    }
                                    let step = zil_ir::PlaceProjection::Variant(
                                        zil_ir::VariantProjection {
                                            variant: variant_ordinal,
                                            index: u32::try_from(index).map_err(|_| {
                                                error(
                                                    field.span,
                                                    "stored variant field ordinal overflow",
                                                )
                                            })?,
                                        },
                                    );
                                    self.stored_physical_field(
                                        &mut field.ty,
                                        &original.ty,
                                        formals,
                                        &qualified_actuals,
                                        &mut vec![zil_ir::LifeTypeProjection::Place(step)],
                                        &mut direct_fields,
                                        depth + 1,
                                    )?;
                                }
                            }
                            _ => {
                                return Err(error(
                                    variant.span,
                                    "stored enum variant physical fields differ",
                                ));
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        self.nominal_life_bindings = previous_lives;
        self.nominal_source_actuals = previous_source;
        self.nominal_captured_lives.truncate(capture_mark);
        field_result?;
        let declaration = LifeNominalDeclaration {
            id: declaration_id,
            identity: stable_identity,
            kind: if matches!(item.kind, ItemKind::Enum(_)) {
                LifeNominalKind::Enum
            } else {
                LifeNominalKind::Record
            },
            parameters,
        };
        let produced = match (&instance.kind, &item.kind) {
            (ItemKind::Aggregate(physical), ItemKind::Aggregate(original)) => {
                stored_types::instantiate_joint_with_fields(
                    physical,
                    declaration,
                    actuals,
                    &lives,
                    Some((original, &qualified_actuals)),
                    &direct_fields,
                    &mut self.budget,
                    span,
                )?
            }
            (ItemKind::Enum(physical), ItemKind::Enum(original)) => {
                stored_types::instantiate_enum_joint(
                    physical,
                    original,
                    declaration,
                    actuals,
                    &lives,
                    &qualified_actuals,
                    &direct_fields,
                    &mut self.budget,
                    span,
                )?
            }
            _ => unreachable!(),
        };
        let classes = produced.classes;
        self.record_inherent_instance(&generated, template, &substitution, span)?;
        if existing.is_none() {
            if let ItemKind::Aggregate(physical) = &instance.kind {
                self.stored_lookup_fee(self.stored_record_fields.len(), generated.len(), span)?;
                self.budget.text(&generated, span)?;
                self.budget.lineage_bytes(
                    physical
                        .fields
                        .len()
                        .saturating_mul(std::mem::size_of::<(String, TypeRef)>()),
                    span,
                )?;
                let mut fields = Vec::new();
                for field in &physical.fields {
                    self.budget.text(&field.name, field.span)?;
                    self.budget.ty(&field.ty)?;
                    fields.push((field.name.clone(), field.ty.clone()));
                }
                self.stored_record_fields.insert(generated.clone(), fields);
            }
            // Caller pattern elaboration needs the family and payload layout,
            // not the callee's lexical life name. This private layout hint
            // grants no relation: the actual qualified type and its complete
            // catalog/signature remain independent kernel inputs.
            if template == options::TEMPLATE
                && matches!(
                    qualified_actuals.get(&0).map(|actual| &actual.ty),
                    Some(zil_ir::Type::Address { mutable: false, .. })
                )
            {
                for argument in arguments.iter() {
                    if let GenericArg::Type(ty) = argument {
                        self.budget.ty(ty)?;
                    }
                }
                self.budget.text(template, span)?;
                self.budget.text(&generated, span)?;
                let mut hint = arguments.to_vec();
                if let [GenericArg::Type(ty)] = hint.as_mut_slice() {
                    let mut ty = ty;
                    loop {
                        match &mut ty.kind {
                            TypeKind::Group(inner) => ty = inner,
                            TypeKind::Reference { lifetime, .. } => {
                                *lifetime = None;
                                break;
                            }
                            _ => break,
                        }
                    }
                }
                self.instance_types.insert(
                    generated.clone(),
                    TypeRef {
                        span,
                        kind: TypeKind::Named {
                            path: vec![template.to_owned()],
                            args: hint,
                        },
                    },
                );
            }
            match &mut instance.kind {
                ItemKind::Aggregate(physical) => physical.generics.clear(),
                ItemKind::Enum(physical) => physical.generics.clear(),
                _ => unreachable!(),
            }
            self.traits
                .register_copy_shape(&instance, &mut self.budget)?;
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

    pub(super) fn closed_stored_actual(&mut self, ty: &TypeRef, depth: usize) -> R<()> {
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
                            && let zil_ir::Type::Record { name, .. }
                            | zil_ir::Type::Enum { name, .. } = storage.as_ref()
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
