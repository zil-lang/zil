//! Joint nominal Type actuals. Physical names locate a produced item; actual
//! source lifetime classes come from this specialization, never from its name.
use super::*;
use crate::stored_types::{self, LexicalLife};

impl Engine<'_> {
    pub(super) fn stored_application(
        &mut self,
        name: &str,
        arguments: &[GenericArg],
        span: Span,
        depth: usize,
    ) -> R<bool> {
        if name == "Task" || name == "OwnedDyn" {
            // The retained Task envelope is a builtin qualified carrier.
            // Its Context is checked by the Task header/recipe consumer.
            return Ok(false);
        }
        if self.stored_declaration(name, span)? {
            return Ok(true);
        }
        for argument in arguments {
            self.budget.node(span, depth)?;
            if let GenericArg::Type(ty) = argument
                && self.qualified_actual_syntax(ty, depth + 1)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn qualified_actual_syntax(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Reference {
                lifetime: Some(_), ..
            } => Ok(true),
            TypeKind::Named { path, args } => {
                if path.len() == 1 {
                    for instance in &self.products.storage.instances {
                        self.budget.node(ty.span, depth)?;
                        if let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                            && let zil_ir::Type::Record { name, .. }
                            | zil_ir::Type::Enum { name, .. }
                            | zil_ir::Type::Package { name, .. } = storage.as_ref()
                        {
                            self.budget.text(name, ty.span)?;
                            self.budget.text(&path[0], ty.span)?;
                            if name == &path[0] {
                                return Ok(true);
                            }
                        }
                    }
                }
                for argument in args {
                    self.budget.node(ty.span, depth)?;
                    match argument {
                        GenericArg::Lifetime(_) => return Ok(true),
                        GenericArg::Type(inner)
                            if self.qualified_actual_syntax(inner, depth + 1)? =>
                        {
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
                Ok(false)
            }
            TypeKind::Group(inner) | TypeKind::Optional(inner) => {
                self.qualified_actual_syntax(inner, depth + 1)
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    if self.qualified_actual_syntax(field, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            TypeKind::Array { element, .. } => self.qualified_actual_syntax(element, depth + 1),
            TypeKind::Error { error, result } => Ok(self
                .qualified_actual_syntax(error, depth + 1)?
                || self.qualified_actual_syntax(result, depth + 1)?),
            _ => Ok(false),
        }
    }

    pub(super) fn qualified_type_actual(
        &mut self,
        ty: &mut TypeRef,
        depth: usize,
    ) -> R<Option<stored_types::QualifiedActual>> {
        if let Some(actual) = self.prepare_owned_dyn_context(ty, depth + 1)? {
            return Ok(Some(actual));
        }
        if !self.qualified_actual_syntax(ty, depth)? {
            return Ok(None);
        }
        if let TypeKind::Group(inner) = &mut ty.kind {
            return self.qualified_type_actual(inner, depth + 1);
        }
        if let TypeKind::Reference {
            lifetime: Some(name),
            mutable,
            inner,
        } = &mut ty.kind
        {
            let class = self.source_life(name, ty.span)?;
            self.closed_stored_actual(inner, depth + 1)?;
            self.ty(inner, depth + 1)?;
            self.closed_stored_actual(inner, depth + 1)?;
            self.budget.text(name, ty.span)?;
            self.budget.ty(inner)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<LexicalLife>(), ty.span)?;
            return Ok(Some(stored_types::QualifiedActual {
                name: name.clone(),
                ty: zil_ir::Type::Address {
                    mutable: *mutable,
                    inner: Box::new(super::super::type_ref(inner)),
                },
                classes: vec![class],
            }));
        }
        let TypeKind::Named { path, args } = &mut ty.kind else {
            return Err(unsupported(
                ty.span,
                "compound qualified Type actual needs its full joint metadata mapping",
            ));
        };
        if path.len() != 1 || !self.templates.contains_key(&path[0]) || args.is_empty() {
            return Err(unsupported(
                ty.span,
                "qualified Type actual lost its source lifetime application",
            ));
        }
        let (name, classes) = self.stored_nominal(&path[0], args, ty.span, depth + 1)?;
        let mut actual = None;
        for instance in &self.products.storage.instances {
            self.budget.node(ty.span, depth)?;
            if let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                && let zil_ir::Type::Record { name: physical, .. }
                | zil_ir::Type::Enum { name: physical, .. }
                | zil_ir::Type::Package { name: physical, .. } = storage.as_ref()
            {
                self.budget.text(physical, ty.span)?;
                self.budget.text(&name, ty.span)?;
                if physical == &name {
                    stored_types::pay_type(
                        &instance.instance.ty,
                        &mut self.budget,
                        ty.span,
                        depth + 1,
                    )?;
                    actual = Some(instance.instance.ty.clone());
                }
            }
        }
        let actual =
            actual.ok_or_else(|| error(ty.span, "actual qualified Type instance absent"))?;
        self.budget.text(&name, ty.span)?;
        *path = vec![name.clone()];
        args.clear();
        Ok(Some(stored_types::QualifiedActual {
            name,
            ty: actual,
            classes,
        }))
    }

    pub(super) fn joint_class_key(
        &mut self,
        classes: &[LexicalLife],
        joint: &mut Vec<LexicalLife>,
        span: Span,
    ) -> R<String> {
        let mut key = String::new();
        for class in classes {
            self.budget
                .identity_work(joint.len().saturating_add(24), span)?;
            let index = if let Some(index) = joint.iter().position(|prior| prior == class) {
                index
            } else {
                self.budget
                    .lineage_bytes(std::mem::size_of::<LexicalLife>(), span)?;
                joint.push(*class);
                joint.len() - 1
            };
            key.push_str(&format!("class:{index};"));
        }
        Ok(key)
    }
}

impl Engine<'_> {
    pub(super) fn semantic_source_actual(
        &mut self,
        ty: &TypeRef,
        domain: &str,
    ) -> R<Option<SemanticActual>> {
        if self.life_scope.is_empty() || !self.qualified_actual_syntax(ty, 0)? {
            return Ok(None);
        }
        let mut classes = Vec::new();
        self.semantic_free_classes(ty, &mut Vec::new(), &mut classes, 0)?;
        self.budget.ty(ty)?;
        self.budget.text(domain, ty.span)?;
        Ok(Some(SemanticActual {
            application: Some(ty.clone()),
            classes,
            domain: domain.to_owned(),
        }))
    }
    fn semantic_free_classes(
        &mut self,
        ty: &TypeRef,
        bound: &mut Vec<String>,
        classes: &mut Vec<LexicalLife>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(name) = lifetime {
                    self.semantic_free_life(name, ty.span, bound, classes)?;
                }
                self.semantic_free_classes(inner, bound, classes, depth + 1)?;
            }
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for arg in args {
                    match arg {
                        GenericArg::Lifetime(name) => {
                            self.semantic_free_life(name, ty.span, bound, classes)?
                        }
                        GenericArg::Type(inner) => {
                            self.semantic_free_classes(inner, bound, classes, depth + 1)?
                        }
                        GenericArg::Const(_) => {}
                    }
                }
            }
            TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::Array { element: inner, .. } => {
                self.semantic_free_classes(inner, bound, classes, depth + 1)?
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                let mark = bound.len();
                self.budget.text(lifetime, ty.span)?;
                bound.push(lifetime.clone());
                self.semantic_free_classes(inner, bound, classes, depth + 1)?;
                bound.truncate(mark);
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    self.semantic_free_classes(field, bound, classes, depth + 1)?;
                }
            }
            TypeKind::Error { error, result } => {
                self.semantic_free_classes(error, bound, classes, depth + 1)?;
                self.semantic_free_classes(result, bound, classes, depth + 1)?;
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for param in params {
                    self.semantic_free_classes(param, bound, classes, depth + 1)?;
                }
                self.semantic_free_classes(result, bound, classes, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn semantic_free_life(
        &mut self,
        name: &str,
        span: Span,
        bound: &[String],
        classes: &mut Vec<LexicalLife>,
    ) -> R<()> {
        for local in bound {
            self.budget.text(local, span)?;
            self.budget.text(name, span)?;
        }
        if !bound.iter().any(|local| local == name) {
            let class = self.source_life(name, span)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<LexicalLife>(), span)?;
            classes.push(class);
        }
        Ok(())
    }
    pub(super) fn bind_semantic_source(&mut self, pattern: &Pattern, env: &mut Env) -> R<()> {
        match &pattern.kind {
            PatternKind::Name(name) => {
                let Some(source) = env
                    .bindings
                    .get(name)
                    .and_then(|binding| binding.ty.as_ref())
                else {
                    return Ok(());
                };
                let Some(actual) = self.semantic_source_actual(source, &env.domain)? else {
                    return Ok(());
                };
                self.budget.ty(source)?;
                let mut physical = source.clone();
                self.ty(&mut physical, 0)?;
                let binding = env.bindings.get_mut(name).expect("actual binding");
                binding.ty = Some(physical);
                binding.semantic = Some(actual);
            }
            PatternKind::Tuple(fields) => {
                for field in fields {
                    self.bind_semantic_source(field, env)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
