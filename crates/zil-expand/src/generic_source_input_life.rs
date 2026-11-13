//! Elision domains come from actual Source input occurrences, never Core layout.
use super::*;
use crate::stored_types::{SignatureCandidate, SourceInputLife};
use zil_ir::LifeTypeProjection as P;

/// This paid source walk only selects producer work. Code signatures introduce
/// their own invocation domains and are not retained input data occurrences.
pub(crate) fn has_elided(ty: &TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<bool> {
    match &ty.kind {
        TypeKind::Reference { lifetime: None, .. } => {
            budget.node(ty.span, depth)?;
            Ok(true)
        }
        TypeKind::Reference { inner, .. }
        | TypeKind::Group(inner)
        | TypeKind::Array { element: inner, .. }
        | TypeKind::Optional(inner) => {
            budget.node(ty.span, depth)?;
            has_elided(inner, budget, depth + 1)
        }
        TypeKind::Tuple(fields) => {
            budget.node(ty.span, depth)?;
            for field in fields {
                if has_elided(field, budget, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        TypeKind::Named { path, .. }
            if path.len() == 1 && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce") =>
        {
            Ok(false)
        }
        TypeKind::Named { args, .. } => {
            for argument in args {
                budget.node(ty.span, depth)?;
                if let GenericArg::Type(inner) = argument
                    && has_elided(inner, budget, depth + 1)?
                {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        TypeKind::Error { error, result } => {
            budget.node(ty.span, depth)?;
            Ok(has_elided(error, budget, depth + 1)? || has_elided(result, budget, depth + 1)?)
        }
        _ => Ok(false),
    }
}

impl Engine<'_> {
    pub(super) fn elide_source_inputs(
        &mut self,
        function: &mut Function,
        candidate: &mut SignatureCandidate,
    ) -> R<bool> {
        fn input(
            engine: &mut Engine<'_>,
            ty: &mut TypeRef,
            parameter: u32,
            path: &mut Vec<P>,
            rows: &mut Vec<SourceInputLife>,
            lives: &mut Vec<String>,
            depth: usize,
        ) -> R<()> {
            engine.budget.node(ty.span, depth)?;
            match &mut ty.kind {
                TypeKind::Reference {
                    lifetime, inner, ..
                } => {
                    if lifetime.is_none() {
                        let binder = u32::try_from(engine.life_scope.len())
                            .map_err(|_| error(ty.span, "implicit input class count overflow"))?;
                        engine.budget.identity_work(
                            64usize.saturating_add(path.len().saturating_mul(8)),
                            ty.span,
                        )?;
                        // `$` cannot occur in a user identifier. The containing
                        // exact Pending function owns the domain; ordinal/path
                        // records identify its real Source input occurrence.
                        let name = format!("$input_life_{parameter}_{binder}");
                        engine.budget.text(&name, ty.span)?;
                        engine.budget.lineage_bytes(
                            std::mem::size_of::<SourceInputLife>()
                                .saturating_add(path.len().saturating_mul(std::mem::size_of::<P>()))
                                .saturating_add(name.len().saturating_mul(3)),
                            ty.span,
                        )?;
                        engine.life_scope.push(name.clone());
                        *lifetime = Some(name);
                        rows.push(SourceInputLife {
                            parameter,
                            path: path.clone(),
                            binder,
                        });
                    }
                    let name = lifetime
                        .as_ref()
                        .expect("explicit or synthesized input class");
                    engine.budget.text(name, ty.span)?;
                    engine.budget.lineage_bytes(
                        std::mem::size_of::<String>().saturating_add(name.len()),
                        ty.span,
                    )?;
                    lives.push(name.clone());
                    path.push(P::Pointee);
                    input(engine, inner, parameter, path, rows, lives, depth + 1)?;
                    path.pop();
                }
                TypeKind::Group(inner) => {
                    input(engine, inner, parameter, path, rows, lives, depth + 1)?
                }
                TypeKind::Tuple(fields) => {
                    for (index, field) in fields.iter_mut().enumerate() {
                        path.push(P::Place(zil_ir::PlaceProjection::Field(
                            u32::try_from(index)
                                .map_err(|_| error(field.span, "input Tuple ordinal overflow"))?,
                        )));
                        input(engine, field, parameter, path, rows, lives, depth + 1)?;
                        path.pop();
                    }
                }
                TypeKind::Array { element, .. } => {
                    path.push(P::Element);
                    input(engine, element, parameter, path, rows, lives, depth + 1)?;
                    path.pop();
                }
                TypeKind::Optional(inner) => {
                    path.push(P::TypeArgument(0));
                    input(engine, inner, parameter, path, rows, lives, depth + 1)?;
                    path.pop();
                }
                TypeKind::Named { path, .. }
                    if path.len() == 1 && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce") => {
                }
                TypeKind::Named { args, .. } => {
                    for (index, arg) in args.iter_mut().enumerate() {
                        match arg {
                            GenericArg::Type(inner) => {
                                path.push(P::TypeArgument(u32::try_from(index).map_err(|_| {
                                    error(ty.span, "input type argument ordinal overflow")
                                })?));
                                input(engine, inner, parameter, path, rows, lives, depth + 1)?;
                                path.pop();
                            }
                            GenericArg::Lifetime(name) => {
                                engine.budget.text(name, ty.span)?;
                                lives.push(name.clone());
                            }
                            GenericArg::Const(_) => {}
                        }
                    }
                }
                TypeKind::Error {
                    error: failure,
                    result,
                } => {
                    for (error, arm) in [(true, failure), (false, result)] {
                        path.push(P::Place(zil_ir::PlaceProjection::ErrorArm(error)));
                        input(engine, arm, parameter, path, rows, lives, depth + 1)?;
                        path.pop();
                    }
                }
                // A callable's inputs belong to its own invocation binder.
                TypeKind::Function { .. }
                | TypeKind::ForeignFunction { .. }
                | TypeKind::ForLifetime { .. }
                | TypeKind::Dyn { .. } => {}
                _ => {}
            }
            Ok(())
        }
        fn result(
            engine: &mut Engine<'_>,
            ty: &mut TypeRef,
            source: Option<&str>,
            changed: &mut bool,
            depth: usize,
        ) -> R<()> {
            engine.budget.node(ty.span, depth)?;
            match &mut ty.kind {
                TypeKind::Reference {
                    lifetime, inner, ..
                } => {
                    if lifetime.is_none() {
                        let source = source.ok_or_else(|| {
                            error(
                                ty.span,
                                "elided borrowed result has no unique input or receiver lifetime",
                            )
                        })?;
                        engine.budget.text(source, ty.span)?;
                        *lifetime = Some(source.to_owned());
                        *changed = true;
                    }
                    result(engine, inner, source, changed, depth + 1)?;
                }
                TypeKind::Group(inner)
                | TypeKind::Optional(inner)
                | TypeKind::Array { element: inner, .. } => {
                    result(engine, inner, source, changed, depth + 1)?
                }
                TypeKind::Tuple(fields) => {
                    for field in fields {
                        result(engine, field, source, changed, depth + 1)?;
                    }
                }
                TypeKind::Named { path, .. }
                    if path.len() == 1 && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce") => {
                }
                TypeKind::Named { args, .. } => {
                    for arg in args {
                        if let GenericArg::Type(inner) = arg {
                            result(engine, inner, source, changed, depth + 1)?;
                        }
                    }
                }
                TypeKind::Error {
                    error: failure,
                    result: success,
                } => {
                    result(engine, failure, source, changed, depth + 1)?;
                    result(engine, success, source, changed, depth + 1)?;
                }
                _ => {}
            }
            Ok(())
        }
        let roles = callable::template(function, &mut self.budget)?;
        let mut lives = Vec::new();
        let mut data_lives = Vec::new();
        for (index, parameter) in function.params.iter_mut().enumerate() {
            let start = lives.len();
            let callable_receiver = roles.as_ref().is_some_and(|roles| {
                roles.bindings().any(|role| {
                    callable::parameter_mode(&parameter.ty, &role.generic_name).is_some()
                })
            });
            input(
                self,
                &mut parameter.ty,
                u32::try_from(index)
                    .map_err(|_| error(parameter.span, "input parameter ordinal overflow"))?,
                &mut Vec::new(),
                &mut candidate.source_inputs,
                &mut lives,
                0,
            )?;
            if !callable_receiver {
                for name in &lives[start..] {
                    self.budget.text(name, parameter.span)?;
                    self.budget.lineage_bytes(
                        std::mem::size_of::<String>().saturating_add(name.len()),
                        parameter.span,
                    )?;
                    data_lives.push(name.clone());
                }
            }
        }
        let mut data_unique = Vec::new();
        for life in data_lives {
            self.budget
                .identity_work(data_unique.len().saturating_add(1), function.span)?;
            if !data_unique.contains(&life) {
                data_unique.push(life);
            }
        }
        let mut unique = Vec::new();
        for life in lives {
            self.budget
                .identity_work(unique.len().saturating_add(1), function.span)?;
            if !unique.contains(&life) {
                unique.push(life);
            }
        }
        let receiver = function
            .params
            .first()
            .filter(|parameter| parameter.name == "self")
            .and_then(|parameter| match &grouped(&parameter.ty).kind {
                TypeKind::Reference { lifetime, .. } => lifetime.as_deref(),
                _ => None,
            });
        // A borrowed callable receiver has its own environment domain. A
        // declared callable invocation forwards its ordinary data domain; the
        // role's complete code contract is checked separately, never inferred
        // by equating the receiver's address with a data input.
        let selected = receiver
            .or_else(|| (data_unique.len() == 1).then(|| data_unique[0].as_str()))
            .or_else(|| (unique.len() == 1).then(|| unique[0].as_str()));
        let mut changed = !candidate.source_inputs.is_empty();
        if let Some(ty) = &mut function.return_type {
            result(self, ty, selected, &mut changed, 0)?;
        }
        if changed {
            for name in &self.life_scope {
                self.budget.text(name, function.span)?;
                self.budget.lineage_bytes(
                    std::mem::size_of::<String>().saturating_add(name.len()),
                    function.span,
                )?;
            }
            candidate.source_scope = self.life_scope.clone();
        }
        Ok(changed)
    }
}

impl Engine<'_> {
    pub(super) fn rebase_input_call_result(
        &mut self,
        name: &str,
        actuals: &[Option<TypeRef>],
        span: Span,
    ) -> R<Option<TypeRef>> {
        let Some(result) = self.stored_source_results.get(name) else {
            return Ok(None);
        };
        self.budget.ty(result)?;
        let mut result = result.clone();
        let Some(parameters) = self.stored_source_parameters.get(name) else {
            return Ok(None);
        };
        if parameters.len() != actuals.len() {
            return Ok(None);
        }
        let mut pairs = Vec::new();
        for (parameter, actual) in parameters.iter().zip(actuals) {
            if let (Some(parameter), Some(actual)) = (parameter, actual) {
                let parameter = parameter
                    .application
                    .as_ref()
                    .expect("semantic parameter source");
                self.budget.ty(parameter)?;
                self.budget.ty(actual)?;
                pairs.push((parameter.clone(), actual.clone()));
            }
        }
        fn bind(
            names: &mut BTreeMap<String, String>,
            formal: &str,
            actual: &str,
            budget: &mut Budget<'_>,
            span: Span,
        ) -> R<bool> {
            budget.text(formal, span)?;
            budget.text(actual, span)?;
            budget.identity_work(names.len().saturating_add(1), span)?;
            if let Some(previous) = names.get(formal) {
                return Ok(previous == actual);
            }
            budget.lineage_bytes(
                2 * std::mem::size_of::<String>() + formal.len() + actual.len(),
                span,
            )?;
            names.insert(formal.to_owned(), actual.to_owned());
            Ok(true)
        }
        fn match_type(
            formal: &TypeRef,
            actual: &TypeRef,
            names: &mut BTreeMap<String, String>,
            budget: &mut Budget<'_>,
            depth: usize,
        ) -> R<bool> {
            budget.node(formal.span, depth)?;
            budget.node(actual.span, depth)?;
            match (&grouped(formal).kind, &grouped(actual).kind) {
                (
                    TypeKind::Reference {
                        lifetime: Some(f),
                        inner,
                        ..
                    },
                    TypeKind::Reference {
                        lifetime: Some(a),
                        inner: other,
                        ..
                    },
                ) => {
                    if !bind(names, f, a, budget, actual.span)? {
                        return Ok(false);
                    }
                    match_type(inner, other, names, budget, depth + 1)
                }
                (TypeKind::Tuple(fs), TypeKind::Tuple(xs)) if fs.len() == xs.len() => {
                    for (f, a) in fs.iter().zip(xs) {
                        if !match_type(f, a, names, budget, depth + 1)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                (
                    TypeKind::Named { path: fp, args: fs },
                    TypeKind::Named { path: ap, args: xs },
                ) if fp == ap && fs.len() == xs.len() => {
                    for (f, a) in fs.iter().zip(xs) {
                        match (f, a) {
                            (GenericArg::Lifetime(f), GenericArg::Lifetime(a)) => {
                                if !bind(names, f, a, budget, actual.span)? {
                                    return Ok(false);
                                }
                            }
                            (GenericArg::Type(f), GenericArg::Type(a))
                                if !match_type(f, a, names, budget, depth + 1)? =>
                            {
                                return Ok(false);
                            }
                            _ => {}
                        }
                    }
                    Ok(true)
                }
                (TypeKind::Array { element: f, .. }, TypeKind::Array { element: a, .. })
                | (TypeKind::Optional(f), TypeKind::Optional(a)) => {
                    match_type(f, a, names, budget, depth + 1)
                }
                (
                    TypeKind::Error {
                        error: f,
                        result: r,
                    },
                    TypeKind::Error {
                        error: a,
                        result: z,
                    },
                ) => Ok(match_type(f, a, names, budget, depth + 1)?
                    && match_type(r, z, names, budget, depth + 1)?),
                _ => Ok(true),
            }
        }
        let mut names = BTreeMap::new();
        for (formal, actual) in pairs {
            let actual = self.original_type(&actual)?;
            if !match_type(&formal, &actual, &mut names, &mut self.budget, 0)? {
                return Ok(None);
            }
        }
        // All free result classes must have an actual input substitution.
        // Missing or conflicting information stays unknown; it is not class0.
        struct Free<'a, 'b, 'l> {
            names: &'a BTreeMap<String, String>,
            budget: &'b mut Budget<'l>,
            local: Vec<String>,
            complete: bool,
        }
        impl crate::ct_selection::Visitor for Free<'_, '_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                self.budget.node(span, depth).map_err(|e| vec![e])
            }
            fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                self.budget.text(text, span).map_err(|e| vec![e])
            }
            fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
                if let TypeKind::ForLifetime { lifetime, inner } = &ty.kind {
                    self.node(ty.span, depth)?;
                    self.text(lifetime, ty.span)?;
                    self.local.push(lifetime.clone());
                    self.ty(inner, depth + 1)?;
                    self.local.pop();
                    return Ok(());
                }
                let mut check = |name: &str| {
                    if !self.local.iter().any(|bound| bound == name)
                        && !self.names.contains_key(name)
                    {
                        self.complete = false
                    }
                };
                match &ty.kind {
                    TypeKind::Reference {
                        lifetime: Some(name),
                        ..
                    } => check(name),
                    TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                        for arg in args {
                            if let GenericArg::Lifetime(name) = arg {
                                check(name)
                            }
                        }
                    }
                    _ => {}
                }
                crate::ct_selection::walk_type(self, ty, depth)
            }
        }
        use crate::ct_selection::Visitor;
        let mut free = Free {
            names: &names,
            budget: &mut self.budget,
            local: Vec::new(),
            complete: true,
        };
        free.ty(&result, 0).map_err(|mut errors| errors.remove(0))?;
        if !free.complete {
            return Ok(None);
        }
        enums::source_field_lives(&mut result, &names, &mut self.budget, 0)?;
        self.budget.text(name, span)?;
        Ok(Some(result))
    }
}

impl Engine<'_> {
    // This is the executable spelling selected by the actual concrete impl and
    // trait application, not a same-named method lookup. The normal collector
    // independently checks and emits that implementation.
    pub(super) fn inline_method_storage_key(
        &mut self,
        target: &TypeRef,
        application: Option<&TypeRef>,
        function: &Function,
    ) -> R<Option<String>> {
        let Some(host) = trait_instances::head(target) else {
            return Ok(None);
        };
        self.budget.text(host, target.span)?;
        self.budget.text(&function.name, function.span)?;
        let member = if let Some(application) = application {
            if trait_instances::head(application)
                .is_some_and(super::super::static_traits::reserved_trait)
            {
                return Ok(None);
            }
            let reference = self.traits.reference(application, &mut self.budget)?;
            trait_instances::method_name_with(
                &reference,
                &function.name,
                super::super::static_traits::scalar_self(host),
                &mut self.budget,
                function.span,
            )?
        } else {
            function.name.clone()
        };
        self.budget.identity_work(
            host.len().saturating_add(member.len()).saturating_add(1),
            function.span,
        )?;
        Ok(Some(format!("{host}.{member}")))
    }
}
