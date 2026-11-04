//! Bounded ordinary Copy binders. Symbolic checking stays inside the producer;
//! executable artifacts contain only concrete, independently checked CT types.
use super::*;

fn direct_name(ty: &s::TypeRef) -> Option<&str> {
    match &ty.kind {
        s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => Some(&path[0]),
        _ => None,
    }
}
impl<'a> Compiler<'a> {
    pub(super) fn source_type_cost(&mut self, ty: &s::TypeRef, depth: usize) -> R<()> {
        self.budget.depth(depth, ty.span)?;
        self.budget.node(ty.span)?;
        match &ty.kind {
            s::TypeKind::Named { path, args } => {
                for name in path {
                    self.budget.text(name, ty.span)?;
                }
                for argument in args {
                    match argument {
                        s::GenericArg::Lifetime(name) => self.budget.text(name, ty.span)?,
                        s::GenericArg::Type(inner) => self.source_type_cost(inner, depth + 1)?,
                        s::GenericArg::Const(value) => {
                            self.source_expression_copy(value, depth + 1)?
                        }
                    }
                }
            }
            s::TypeKind::Group(inner)
            | s::TypeKind::ForLifetime { inner, .. }
            | s::TypeKind::Reference { inner, .. } => self.source_type_cost(inner, depth + 1)?,
            s::TypeKind::Function { params, result } => {
                for parameter in params {
                    self.source_type_cost(parameter, depth + 1)?;
                }
                self.source_type_cost(result, depth + 1)?;
            }

            s::TypeKind::Tuple(fields) | s::TypeKind::Union(fields) => {
                for field in fields {
                    self.source_type_cost(field, depth + 1)?;
                }
            }
            s::TypeKind::Array { element, length } => {
                self.source_type_cost(element, depth + 1)?;
                if let Some(length) = length {
                    self.budget.node(length.span)?;
                    match &length.kind {
                        s::ExprKind::Literal(s::Literal::Integer(text)) => {
                            self.budget.text(text, length.span)?
                        }
                        _ => {
                            return Err(error(
                                length.span,
                                "CT array constructor argument requires a fixed integer length",
                            ));
                        }
                    }
                }
            }
            // This helper only copies explicit constructor type arguments.
            // Other ordinary shapes are accepted by ty() without AST copying.
            _ => {
                return Err(error(
                    ty.span,
                    "CT constructor type argument needs a named or tuple type",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn generic_names(
        &mut self,
        parameters: &[s::GenericParam],
        clauses: &[s::BoundClause],
        span: Span,
    ) -> R<Vec<String>> {
        let mut names: Vec<String> = Vec::new();
        let mut copy = Vec::new();
        for parameter in parameters {
            self.budget.node(parameter.span)?;
            self.budget.text(&parameter.name, parameter.span)?;
            for name in &names {
                self.budget
                    .work(parameter.name.len().min(name.len()) + 1, parameter.span)?;
            }
            if names.contains(&parameter.name) {
                return Err(error(parameter.span, "duplicate CT generic type binder"));
            }
            let s::GenericParamKind::Type(bounds) = &parameter.kind else {
                return Err(error(
                    parameter.span,
                    "CT generics require ordinary Copy type binders",
                ));
            };
            let mut has_copy = false;
            for bound in bounds {
                self.budget.work(1, bound.span)?;
                if direct_name(bound) != Some("Copy") {
                    return Err(error(bound.span, "CT generic bound must be ordinary Copy"));
                }
                has_copy = true;
            }
            names.push(parameter.name.clone());
            copy.push(has_copy);
        }
        for clause in clauses {
            self.budget.work(names.len() + 1, clause.span)?;
            let s::BoundSubject::Type(subject) = &clause.subject else {
                return Err(error(clause.span, "CT where requires a direct type binder"));
            };
            if let Some(name) = direct_name(subject) {
                self.budget.text(name, subject.span)?;
            }
            let index = direct_name(subject)
                .and_then(|name| names.iter().position(|n| n == name))
                .ok_or_else(|| {
                    error(
                        clause.span,
                        "CT where subject is not this function's type binder",
                    )
                })?;
            if clause.lifetime_bound.is_some() || clause.bounds.is_empty() {
                return Err(error(clause.span, "CT where requires ordinary Copy"));
            }
            for bound in &clause.bounds {
                self.budget.work(1, bound.span)?;
                if direct_name(bound) != Some("Copy") {
                    return Err(error(
                        bound.span,
                        "CT generic where bound must be ordinary Copy",
                    ));
                }
                copy[index] = true;
            }
        }
        if copy.iter().any(|has_copy| !has_copy) {
            return Err(error(span, "CT generic type binder needs a Copy bound"));
        }
        Ok(names)
    }
    pub(super) fn binding(&mut self, name: &str, span: Span) -> R<Option<CtTy>> {
        for (actual, ty) in &self.bindings {
            self.budget.work(1, span)?;
            if actual == name {
                self.budget.copy_ty(ty, 0, span)?;
                return Ok(Some(ty.clone()));
            }
        }
        Ok(None)
    }
    pub(super) fn is_symbolic(&self, ty: &CtTy) -> bool {
        matches!(ty, CtTy::Record(id) if (u32::MAX - id.0) < self.symbolic.len() as u32)
    }
    pub(super) fn ordinary_copy(&mut self, ty: &CtTy, depth: usize, span: Span) -> R<()> {
        self.budget.depth(depth, span)?;
        if self.is_symbolic(ty) {
            return Ok(());
        }
        match ty {
            CtTy::Scalar(_) => Ok(()),
            CtTy::Tuple(fields) | CtTy::AnonymousSum(fields) => {
                for field in fields {
                    self.ordinary_copy(field, depth + 1, span)?;
                }
                Ok(())
            }
            CtTy::Array { element, .. } => self.ordinary_copy(element, depth + 1, span),
            CtTy::Record(id) => {
                let fields = &self.records[id.0 as usize].fields;
                for field in fields {
                    self.budget.copy_ty(field, 0, span)?;
                }
                let fields = fields.clone();
                for field in &fields {
                    self.ordinary_copy(field, depth + 1, span)?;
                }
                Ok(())
            }
            CtTy::Enum(id) => {
                let variants = &self.enums[id.0 as usize].variants;
                for fields in variants {
                    for field in fields {
                        self.budget.copy_ty(field, 0, span)?;
                    }
                }
                let variants = variants.clone();
                for fields in &variants {
                    for field in fields {
                        self.ordinary_copy(field, depth + 1, span)?;
                    }
                }
                Ok(())
            }
            _ => Err(error(
                span,
                "CT generic actual type is not an ordinary Copy value",
            )),
        }
    }
    pub(super) fn instance_bindings(
        &mut self,
        declaration: usize,
        arguments: &[CtTy],
    ) -> R<Vec<(String, CtTy)>> {
        let function = self.declarations[declaration].function;
        if function.generics.is_empty() {
            return Ok(Vec::new());
        }
        let names =
            self.generic_names(&function.generics, &function.where_clause, function.span)?;
        self.bind_arguments(names, arguments, function.span)
    }
    pub(super) fn bind_arguments(
        &mut self,
        names: Vec<String>,
        arguments: &[CtTy],
        span: Span,
    ) -> R<Vec<(String, CtTy)>> {
        if names.len() != arguments.len() {
            return Err(error(span, "CT generic type argument count differs"));
        }
        let mut bindings = Vec::new();
        for (name, ty) in names.into_iter().zip(arguments) {
            self.ordinary_copy(ty, 0, span)?;
            self.budget.node(span)?;
            self.budget.copy_ty(ty, 0, span)?;
            bindings.push((name, ty.clone()));
        }
        Ok(bindings)
    }
    pub(super) fn enqueue_instance(
        &mut self,
        declaration: usize,
        arguments: &[CtTy],
    ) -> R<CtFunctionId> {
        if self.declarations[declaration].function.generics.is_empty() {
            if !arguments.is_empty() {
                return Err(error(
                    self.declarations[declaration].item.span,
                    "nongeneric CT function takes no type arguments",
                ));
            }
            return self.enqueue(declaration);
        }
        let span = self.declarations[declaration].item.span;
        self.ensure_generic_definition(declaration)?;
        // Validate before cache lookup: a hit cannot bypass the actual bound.
        self.instance_bindings(declaration, arguments)?;
        for i in 0..self.generic_selected.len() {
            self.budget.work(1, span)?;
            let (actual, args, id) = &self.generic_selected[i];
            for ty in args.iter().chain(arguments) {
                self.budget.ty(ty, 0, span)?;
            }
            if *actual == declaration && args == arguments {
                return Ok(*id);
            }
        }
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.functions,
            1,
            span,
            "CT source function-instance budget exhausted",
        )?;
        for ty in arguments {
            self.budget.copy_ty(ty, 0, span)?;
            self.budget.copy_ty(ty, 0, span)?;
        }
        self.budget.node(span)?;
        let id = CtFunctionId(self.functions.len() as u32);
        self.functions.push(None);
        self.generic_selected
            .push((declaration, arguments.to_vec(), id));
        self.budget.node(span)?;
        self.pending
            .push_back((declaration, PendingTypes::Data(arguments.to_vec()), id));
        Ok(id)
    }
    pub(super) fn instance_header(
        &mut self,
        declaration: usize,
        arguments: &[CtTy],
    ) -> R<(Vec<CtTy>, CtTy)> {
        let bindings = self.instance_bindings(declaration, arguments)?;
        let old = std::mem::replace(&mut self.bindings, bindings);
        let result = self.header(declaration);
        self.bindings = old;
        result
    }
    pub(super) fn has_unbound<T>(
        &mut self,
        ty: &s::TypeRef,
        names: &[String],
        slots: &[Option<T>],
        depth: usize,
    ) -> R<bool> {
        self.budget.depth(depth, ty.span)?;
        Ok(match &ty.kind {
            s::TypeKind::Named { path, args } => {
                if path.len() == 1 {
                    self.budget.work(names.len(), ty.span)?;
                    if let Some(i) = names.iter().position(|n| n == &path[0]) {
                        return Ok(slots[i].is_none());
                    }
                }
                let mut found = false;
                for arg in args {
                    if let s::GenericArg::Type(ty) = arg {
                        found |= self.has_unbound(ty, names, slots, depth + 1)?;
                    }
                }
                found
            }
            s::TypeKind::Tuple(fields) | s::TypeKind::Union(fields) => {
                let mut found = false;
                for field in fields {
                    found |= self.has_unbound(field, names, slots, depth + 1)?;
                }
                found
            }
            s::TypeKind::Group(inner)
            | s::TypeKind::Array { element: inner, .. }
            | s::TypeKind::ForLifetime { inner, .. }
            | s::TypeKind::Reference { inner, .. } => {
                self.has_unbound(inner, names, slots, depth + 1)?
            }
            s::TypeKind::Function { params, result } => {
                let mut found = self.has_unbound(result, names, slots, depth + 1)?;
                for parameter in params {
                    found |= self.has_unbound(parameter, names, slots, depth + 1)?;
                }
                found
            }
            s::TypeKind::Error { error, result } => {
                self.has_unbound(error, names, slots, depth + 1)?
                    || self.has_unbound(result, names, slots, depth + 1)?
            }
            _ => false,
        })
    }
    pub(super) fn partial_type(
        &mut self,
        module: usize,
        pattern: &s::TypeRef,
        names: &[String],
        slots: &[Option<CtTy>],
        depth: usize,
    ) -> R<Option<CtTy>> {
        if self.has_unbound(pattern, names, slots, depth)? {
            return Ok(None);
        }
        let mut bindings = Vec::new();
        for (name, ty) in names.iter().zip(slots) {
            if let Some(ty) = ty {
                self.budget.text(name, pattern.span)?;
                self.budget.copy_ty(ty, 0, pattern.span)?;
                self.budget.node(pattern.span)?;
                bindings.push((name.clone(), ty.clone()));
            }
        }
        let old = std::mem::replace(&mut self.bindings, bindings);
        let result = self.ty(module, pattern, depth + 1);
        self.bindings = old;
        result.map(Some)
    }
    /// Infer using the declaration's syntax and concrete schema arguments. This
    /// never inspects fields to invent an operation on a Copy binder.
    pub(super) fn infer(
        &mut self,
        module: usize,
        pattern: &s::TypeRef,
        actual: &CtTy,
        names: &[String],
        slots: &mut [Option<CtTy>],
        depth: usize,
    ) -> R<()> {
        self.budget.depth(depth, pattern.span)?;
        if let Some(name) = direct_name(pattern) {
            self.budget.work(names.len(), pattern.span)?;
            if let Some(index) = names.iter().position(|n| n == name) {
                self.budget.ty(actual, 0, pattern.span)?;
                if let Some(old) = &slots[index] {
                    self.budget.ty(old, 0, pattern.span)?;
                    if old != actual {
                        return Err(error(
                            pattern.span,
                            "inconsistent CT generic type inference",
                        ));
                    }
                } else {
                    self.budget.copy_ty(actual, 0, pattern.span)?;
                    slots[index] = Some(actual.clone());
                }
                return Ok(());
            }
        }
        match (&pattern.kind, actual) {
            (s::TypeKind::Group(inner), _) => {
                self.infer(module, inner, actual, names, slots, depth + 1)?
            }
            (s::TypeKind::Tuple(fields), CtTy::Tuple(types)) if fields.len() == types.len() => {
                for (field, ty) in fields.iter().zip(types) {
                    self.infer(module, field, ty, names, slots, depth + 1)?;
                }
            }
            (s::TypeKind::Array { element, .. }, CtTy::Array { element: ty, .. }) => {
                self.infer(module, element, ty, names, slots, depth + 1)?
            }
            (s::TypeKind::Named { path, args }, CtTy::Record(id))
                if !args.is_empty() && !self.is_symbolic(actual) =>
            {
                let (owner, item) = self.named_item(module, path, pattern.span)?;
                let (actual_owner, aggregate) = self.record_sources[id.0 as usize];
                if owner != actual_owner
                    || !matches!(&item.kind,s::ItemKind::Aggregate(a) if std::ptr::eq(a,aggregate))
                {
                    return Err(error(pattern.span, "CT generic nominal type differs"));
                }
                let schema_args = &self.records[id.0 as usize].arguments;
                for argument in schema_args {
                    self.budget.node(pattern.span)?;
                    if let CtSchemaArgument::Type(graph) = argument {
                        self.budget.metadata(graph, pattern.span)?;
                    }
                }
                let schema_args = schema_args.clone();
                let mut types = Vec::new();
                for argument in &schema_args {
                    let CtSchemaArgument::Type(graph) = argument else {
                        return Err(error(
                            pattern.span,
                            "CT generic scalar inference requires an exact scalar parameter",
                        ));
                    };
                    types.push(self.metadata_data_type(graph, depth + 1, pattern.span)?);
                }
                if args.len() != types.len() {
                    return Err(error(pattern.span, "CT generic nominal arity differs"));
                }
                for (arg, ty) in args.iter().zip(&types) {
                    let s::GenericArg::Type(arg) = arg else {
                        return Err(error(
                            pattern.span,
                            "CT generic value parameters are unsupported",
                        ));
                    };
                    self.infer(module, arg, ty, names, slots, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn check_generic_definitions(&mut self) -> R<Vec<CtSymbolId>> {
        self.checking_generics = true;
        let mut definitions = Vec::new();
        for index in 0..self.declarations.len() {
            let d = self.declarations[index];
            if d.function.generics.is_empty() || !self.ct_only_function(d.module, d.function)? {
                continue;
            }
            self.ensure_generic_definition(index)?;
            self.budget.node(d.item.span)?;
            definitions.push(self.symbol(d.module, d.item.span)?);
        }
        self.drain_symbolic()?;
        self.checking_generics = false;
        // No symbolic type, body, quote or schema is published. All actual work
        // remains spent, including instance slots and discarded temporary data.
        self.selected.clear();
        self.generic_selected.clear();
        self.generic_metadata_selected.clear();
        self.functions.clear();
        self.constants.clear();
        self.records.clear();
        self.record_sources.clear();
        self.record_arguments.clear();
        self.enums.clear();
        self.enum_sources.clear();
        self.templates.clear();
        self.symbolic.clear();
        Ok(definitions)
    }
    fn drain_symbolic(&mut self) -> R<()> {
        while let Some((declaration, arguments, id)) = self.pending.pop_front() {
            let PendingTypes::Data(arguments) = arguments else {
                return Err(error(
                    self.declarations[declaration].item.span,
                    "closed metadata entered symbolic definition queue",
                ));
            };
            self.bindings = self.instance_bindings(declaration, &arguments)?;
            cfg::function(self, id, declaration)?;
            self.bindings.clear();
        }
        Ok(())
    }
    pub(super) fn ensure_generic_definition(&mut self, declaration: usize) -> R<()> {
        let d = self.declarations[declaration];
        self.budget
            .work(self.checked_generics.len() + 1, d.item.span)?;
        if self.checked_generics.contains(&declaration) {
            return Ok(());
        }
        if self.checking_generics {
            let names =
                self.generic_names(&d.function.generics, &d.function.where_clause, d.item.span)?;
            self.budget.node(d.item.span)?;
            self.checked_generics.push(declaration);
            let mut arguments = Vec::new();
            for _ in names {
                self.budget.node(d.item.span)?;
                let ty = CtTy::Record(CtRecordSchemaId(u32::MAX - self.symbolic.len() as u32));
                self.budget.copy_ty(&ty, 0, d.item.span)?;
                self.symbolic.push(ty.clone());
                arguments.push(ty);
            }
            self.enqueue_instance(declaration, &arguments)?;
            return Ok(());
        }
        // A generic pure-value helper can first be reached from a concrete CT
        // function. Check its bound independently before any monomorphized body
        // can accidentally grant scalar/field capabilities.
        let mut scratch = Self::unindexed(self.modules, self.budget.limits);
        for d in &self.declarations {
            self.budget.node(d.item.span)?;
        }
        scratch.declarations.extend_from_slice(&self.declarations);
        scratch.checking_generics = true;
        std::mem::swap(&mut scratch.budget, &mut self.budget);
        let result = scratch
            .ensure_generic_definition(declaration)
            .and_then(|()| scratch.drain_symbolic());
        std::mem::swap(&mut scratch.budget, &mut self.budget);
        result?;
        for checked in scratch.checked_generics {
            self.budget
                .work(self.checked_generics.len() + 1, d.item.span)?;
            if !self.checked_generics.contains(&checked) {
                self.budget.node(d.item.span)?;
                self.checked_generics.push(checked);
            }
        }
        Ok(())
    }
}
